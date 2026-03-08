//! Console I/O and keyboard handling for the OXIDE kernel.

use crate::arch;
use crate::arch::serial;
use core::fmt::Write;

/// Push any escape sequence bytes to the input subsystem
///
/// Routes the escape sequence to both VT input and console device
fn push_escape_sequence(seq: &[u8]) {
    #[cfg(feature = "debug-console")]
    {
        // — PatchBay: Debug output goes to os_log → console now
        use core::fmt::Write;
        let _ = os_log::write_str("[ESCAPE] Pushing ");
        let _ = os_log::write_u64(seq.len() as u64);
        let _ = os_log::write_str(" bytes to VT+console: ");
        for &byte in seq {
            let _ = os_log::write_byte(byte);
        }
        let _ = os_log::write_str("\n");
    }

    if let Some(manager) = vt::get_manager() {
        for &byte in seq {
            manager.push_input(byte);
        }
    }
}

/// Push mouse escape sequence bytes to the input subsystem
///
/// Routes the escape sequence to both VT input and console device so
/// applications reading from either path receive mouse events.
fn push_mouse_escape(seq: &[u8]) {
    push_escape_sequence(seq);
}

/// Console write function for syscalls and boot messages.
///
/// — GraveShift: Routes to the focused VT's terminal emulator via write_vt().
/// Each VT has its own TerminalEmulator and backing framebuffer now.
/// Serial was the original sin — every byte of userspace output crawled through
/// the UART byte-by-byte, 28,800 COM1 spinlock cycles per curses frame.
/// Debug output has its own path (os_log, debug_*! macros). Stdout is not debug.
pub fn console_write(data: &[u8]) {
    if terminal::is_initialized() {
        let vt = compositor::focused_vt();
        terminal::write_vt(vt, data);
        // — NeonRoot: mark the focused VT dirty so compositor blits our pixels to hardware
        compositor::mark_dirty(vt);
    } else if fb::is_initialized() {
        // — GraveShift: fallback to basic fb console before terminal is ready
        for &byte in data {
            fb::putchar(byte as char);
        }
    }
}

/// Terminal tick callback — called at ~100 Hz from timer ISR.
/// — GraveShift: polls input devices, processes mouse/keyboard events,
/// then hands off to compositor::tick() for dirty-checking and GPU flush.
#[allow(static_mut_refs)]
pub fn terminal_tick() {
    // — PatchBay: Track terminal tick for performance monitoring
    perf::counters().record_terminal_tick();

    // ═══════════════════════════════════════════════════════════════════════════
    // 🔥 DISABLED: DUPLICATE PROCESSING PATH (The Glitch in the Matrix) 🔥
    // ═══════════════════════════════════════════════════════════════════════════
    //
    // **THE OLD WAY (Broken AF):**
    //
    // Timer tick would poll i8042, call process_scancode(), track modifiers
    // (SHIFT/CTRL/ALT) in static vars, convert to ASCII, push to VT.
    //
    // **THE PROBLEM:**
    //
    // PS/2 IRQ handler ALSO does this (ps2/lib.rs). Same scancode, processed
    // twice, modifier state tracked in TWO places, characters generated twice.
    //
    // Result: Keyboard jitter, dropped keys, duplicate keys, modifier desync.
    // User types "hello", gets "hheelllloo" or "heo" depending on race conditions.
    //
    // **THE FIX:**
    //
    // PS/2 IRQ handler is now the ONLY path. It:
    // 1. Processes scancode → keycode
    // 2. Tracks modifiers (one source of truth)
    // 3. Converts to ASCII/escape sequences
    // 4. Pushes to console callback → VT manager
    //
    // This timer polling is DELETED. IRQ path handles everything.
    //
    // **WHAT ABOUT QEMU `sendkey`?**
    //
    // If you're using QEMU `-display none` and sendkey doesn't fire IRQ1,
    // then your QEMU is cursed. Fix your VM, don't hack the kernel.
    //
    // ═══════════════════════════════════════════════════════════════════════════
    //
    // Code left here as archaeological evidence of the bad old days.
    // Do not uncomment unless you enjoy pain.
    //
    // while let Some(scancode) = unsafe { arch::poll_keyboard() } {
    //     if let Some(byte) = process_scancode(scancode) {  // ⚠️ DUPLICATE!
    //         if let Some(manager) = vt::get_manager() {
    //             manager.push_input(byte);
    //         }
    //     }
    // }
    //
    // ═══════════════════════════════════════════════════════════════════════════

    // Also process serial port input (for -serial stdio in QEMU)
    // SAFETY: We use the lock-free serial read here because terminal_tick
    // runs in interrupt context (timer interrupt). Using the locking version
    // would deadlock if process-context code holds the COM1 lock.
    while let Some(byte) = unsafe { arch::serial_read_unsafe() } {
        // — NeonRoot: Route serial input to VT subsystem. No echo back to serial —
        // that was spewing hex dumps of every Ctrl+C and escape sequence into ISR context.
        if let Some(manager) = vt::get_manager() {
            manager.push_input(byte);
        }
    }

    // Drain VirtIO input devices (keyboard/mouse) if we're running on a
    // configuration where PCI interrupts aren't wired yet.
    virtio_input::poll();

    // — InputShade: Process mouse/tablet events from ALL input devices.
    // QEMU uses virtio-tablet-pci (EV_ABS — absolute coordinates), NOT
    // virtio-mouse-pci (EV_REL — relative deltas). The old code only handled
    // EV_REL from hardcoded device 1, so the cursor never moved. Now we drain
    // all devices and handle both ABS and REL events. The tablet is finally alive.
    if compositor::mouse_initialized() {
        let mut total_dx: i32 = 0;
        let mut total_dy: i32 = 0;
        let mut abs_x: Option<i32> = None;
        let mut abs_y: Option<i32> = None;
        let mut wheel_delta: i32 = 0;
        let has_mouse_mode = terminal::is_initialized() && terminal::has_mouse_mode();

        // — InputShade: button state for escape sequence generation
        static mut MOUSE_BUTTONS: u8 = 0;
        // — GlassSignal: shift key state for horizontal scroll
        static mut SHIFT_HELD: bool = false;

        // — InputShade: Drain mouse/tablet events from all registered input devices.
        // Device order depends on PCI enumeration — don't hardcode indices.
        let device_count = input::device_count();
        for dev_idx in 0..device_count {
            let handle = match input::try_get_device(dev_idx) {
                Some(h) => h,
                None => continue,
            };
            while let Some(event) = handle.try_pop_event() {
                match event.event_type() {
                    input::EventType::Rel => {
                        debug_mouse_unsafe!("R");
                        if event.code == input::REL_X {
                            total_dx += event.value;
                        } else if event.code == input::REL_Y {
                            total_dy += event.value;
                        } else if event.code == input::REL_WHEEL {
                            wheel_delta += event.value;
                        }
                    }
                    input::EventType::Abs => {
                        // — InputShade: Tablet sends absolute coords (0..32767).
                        debug_mouse_unsafe!("A");
                        if event.code == input::ABS_X {
                            abs_x = Some(event.value);
                        } else if event.code == input::ABS_Y {
                            abs_y = Some(event.value);
                        }
                    }
                    input::EventType::Key => {
                        // — GlassSignal: track shift state for horizontal scroll
                        if event.code == 0x2A || event.code == 0x36 { // KEY_LEFTSHIFT / KEY_RIGHTSHIFT
                            unsafe { SHIFT_HELD = event.value != 0; }
                            continue;
                        }

                        // Map button codes to compositor button + terminal escape button
                        let (comp_btn, esc_btn, bit) = match event.code {
                            0x110 => (compositor::MouseButton::Left, 0u8, 0x01u8),
                            0x112 => (compositor::MouseButton::Middle, 1u8, 0x04u8),
                            0x111 => (compositor::MouseButton::Right, 2u8, 0x02u8),
                            _ => continue,
                        };
                        let pressed = event.value != 0;

                        // — InputShade: intercept clicks for virtual keyboard overlay.
                        if event.code == 0x110 && vkbd::is_visible() {
                            if pressed {
                                if let Some((mx, my)) = compositor::mouse_position() {
                                    if let Some((bytes, len)) = vkbd::handle_tap(mx, my) {
                                        for i in 0..len {
                                            if let Some(manager) = vt::get_manager() {
                                                manager.push_input(bytes[i]);
                                            }
                                        }
                                        continue;
                                    }
                                }
                            } else {
                                vkbd::handle_release();
                            }
                        }

                        unsafe { if pressed { MOUSE_BUTTONS |= bit; } else { MOUSE_BUTTONS &= !bit; } }

                        // — GlassSignal: route button events through compositor event system.
                        // Compositor does hit-testing (scrollbar, border, content) and returns
                        // what we should do. No geometry math here — that's the compositor's job.
                        if let Some((mx, my)) = compositor::mouse_position() {
                            let action = if pressed {
                                compositor::handle_mouse_press(comp_btn, mx, my)
                            } else {
                                compositor::handle_mouse_release(comp_btn, mx, my)
                            };

                            match action {
                                compositor::MouseAction::Consumed => {
                                    // — GlassSignal: compositor ate it (scrollbar, border)
                                }
                                compositor::MouseAction::ForwardToTerminal { vt: _ } => {
                                    // — GlassSignal: content area — handle selection or mouse mode
                                    if has_mouse_mode {
                                        let eb = if pressed { esc_btn } else { 3 };
                                        if let Some(seq) = terminal::mouse_event(eb, mx, my, pressed, false) {
                                            push_mouse_escape(&seq);
                                        }
                                    } else if event.code == 0x110 {
                                        if pressed {
                                            terminal::start_selection(mx, my);
                                        } else {
                                            terminal::finish_selection();
                                        }
                                    } else if event.code == 0x112 && pressed {
                                        // — InputShade: middle-click paste
                                        let paste_data = terminal::paste_clipboard();
                                        for &byte in &paste_data {
                                            if let Some(manager) = vt::get_manager() {
                                                manager.push_input(byte);
                                            }
                                        }
                                    }
                                }
                                compositor::MouseAction::Nothing => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // — InputShade: Handle absolute positioning (tablet) — scale from
        // input range (0..32767) to screen pixels.
        let mut cursor_moved = false;
        if abs_x.is_some() || abs_y.is_some() {
            if let Some((screen_w, screen_h)) = compositor::screen_dimensions() {
                let px = abs_x
                    .map(|ax| ((ax as i64) * screen_w as i64 / 32768) as i32)
                    .unwrap_or_else(|| compositor::mouse_position().map(|(x, _)| x).unwrap_or(0));
                let py = abs_y
                    .map(|ay| ((ay as i64) * screen_h as i64 / 32768) as i32)
                    .unwrap_or_else(|| compositor::mouse_position().map(|(_, y)| y).unwrap_or(0));
                compositor::mouse_set_position(px, py);
                cursor_moved = true;
            }
        }

        // Move graphical cursor (relative mode — traditional mouse)
        if total_dx != 0 || total_dy != 0 {
            compositor::mouse_move(total_dx, total_dy);
            cursor_moved = true;
        }

        // — GlassSignal: route mouse motion through compositor event system.
        // During scrollbar drag, compositor handles proportional scrolling.
        // During content press, we forward to terminal selection or mouse mode.
        if cursor_moved {
            if let Some((mx, my)) = compositor::mouse_position() {
                let action = compositor::handle_mouse_move(mx, my);
                match action {
                    compositor::MouseAction::Consumed => {
                        // — GlassSignal: scrollbar drag — compositor handled it
                    }
                    compositor::MouseAction::ForwardToTerminal { vt: _ } => {
                        if has_mouse_mode {
                            let held_btn = unsafe {
                                if MOUSE_BUTTONS & 0x01 != 0 { 0u8 }
                                else if MOUSE_BUTTONS & 0x04 != 0 { 1 }
                                else if MOUSE_BUTTONS & 0x02 != 0 { 2 }
                                else { 3 }
                            };
                            if let Some(seq) = terminal::mouse_event(held_btn, mx, my, true, true) {
                                push_mouse_escape(&seq);
                            }
                        } else {
                            terminal::update_selection(mx, my);
                        }
                    }
                    compositor::MouseAction::Nothing => {
                        // — GlassSignal: idle motion — could do hover effects later
                    }
                }
            }
        }

        // — GlassSignal: mouse wheel — compositor handles shift+wheel (horizontal).
        // Normal wheel goes to terminal scroll or mouse mode escape sequences.
        if wheel_delta != 0 {
            let shift = unsafe { SHIFT_HELD };
            if let Some((mx, my)) = compositor::mouse_position() {
                let action = compositor::handle_mouse_wheel(wheel_delta, mx, my, shift);
                match action {
                    compositor::MouseAction::Consumed => {
                        // — GlassSignal: shift+wheel horizontal scroll handled by compositor
                    }
                    _ => {
                        // — EchoFrame: normal wheel — scroll history or mouse mode escape
                        if has_mouse_mode {
                            let btn = if wheel_delta > 0 { 64u8 } else { 65u8 };
                            let clicks = wheel_delta.unsigned_abs();
                            for _ in 0..clicks {
                                if let Some(seq) = terminal::mouse_event(btn, mx, my, true, false) {
                                    push_mouse_escape(&seq);
                                }
                            }
                        } else {
                            let scroll_lines = (wheel_delta.unsigned_abs() as usize) * 3;
                            if wheel_delta > 0 {
                                terminal::scroll_up(scroll_lines);
                            } else {
                                terminal::scroll_down(scroll_lines);
                            }
                        }
                    }
                }
            }
        }
    }

    // Drive the active display: prefer terminal emulator; disable legacy fb cursor to avoid double cursor
    if terminal::is_initialized() {
        // Blink at ~2.5Hz (every 12 frames at 30 FPS)
        static mut BLINK_TICKS: u8 = 0;
        unsafe {
            BLINK_TICKS = BLINK_TICKS.wrapping_add(1);
            if BLINK_TICKS >= 12 {
                BLINK_TICKS = 0;
                terminal::toggle_cursor_blink();
            } else {
                // Still render pending output without toggling blink state
                terminal::tick();
            }
        }

        // — NeonRoot: compositor blits dirty VT backing buffers → hardware fb,
        // draws vkbd overlay, then mouse cursor as the final layer. — SoftGlyph
        compositor::tick();
    } else if fb::is_initialized() {
        // Fallback pre-terminal: allow fb console cursor only when terminal is not active
        fb::blink_cursor();
    }
}

/// Serial-only write function for devfs
///
/// Writes only to serial port for raw debug output.
/// NOTE: data may point to user memory, so we need STAC/CLAC for SMAP
pub fn serial_write_bytes(data: &[u8]) {
    let mut writer = serial::SerialWriter;

    // — SableWire: Enable access to user pages (SMAP bypass)
    unsafe { crate::arch::user_access_begin(); }

    for &byte in data {
        let _ = writer.write_char(byte as char);
    }

    // — SableWire: Revoke access to user pages (SMAP re-engaged)
    unsafe { crate::arch::user_access_end(); }
}

/// Console write function for devfs (legacy fallback)
///
/// — GraveShift: Early-boot fallback only. Once VT is wired up, ConsoleDevice
/// delegates to VtDevice and this path is dead. No serial — the UART was never
/// meant to echo every byte of user I/O.
/// NOTE: data may point to user memory, so we need STAC/CLAC for SMAP
pub fn console_write_bytes(data: &[u8]) {
    // — SableWire: Enable access to user pages (SMAP bypass)
    unsafe { crate::arch::user_access_begin(); }

    // — GraveShift: route to focused VT's terminal emulator
    if terminal::is_initialized() {
        let vt = compositor::focused_vt();
        terminal::write_vt(vt, data);
        compositor::mark_dirty(vt);
    } else if fb::is_initialized() {
        for &byte in data {
            fb::putchar(byte as char);
        }
    }

    // — SableWire: Revoke access to user pages (SMAP re-engaged)
    unsafe { crate::arch::user_access_end(); }
}

// ═══════════════════════════════════════════════════════════════════
//  ISR-SAFE CONSOLE OUTPUT (REPLACES SERIAL)
// ═══════════════════════════════════════════════════════════════════

/// Write a byte to serial COM1 (ISR-safe, no locks)
///
/// — GraveShift: ISR debug output goes to the UART wire, period.
/// push_input() is for keyboard scancodes — not debug spew.
/// The old code shoved every byte of `[APIC-CAL]` into the VT input ring,
/// getty read it as a username, failed auth three times, and the kernel
/// "logged itself in." Never again.
///
/// # Safety
/// Safe to call from any context including ISRs. Bounded spin on UART THRE.
pub unsafe fn write_byte_unsafe(byte: u8) {
    unsafe {
        arch::serial::write_byte_unsafe(byte);
    }
}

/// Write a string to serial COM1 (ISR-safe, no locks)
///
/// — GraveShift: Debug output belongs on the wire, not in the VT input ring.
/// See docs/agents/isr-output-serial-only.md for the full horror story.
///
/// # Safety
/// Safe to call from any context including ISRs. Bounded spin on UART THRE.
pub unsafe fn write_str_unsafe(s: &str) {
    unsafe {
        arch::serial::write_str_unsafe(s);
    }
}
