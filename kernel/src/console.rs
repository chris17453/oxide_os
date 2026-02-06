//! Console I/O and keyboard handling for the OXIDE kernel.

use arch_x86_64 as arch;
use arch_x86_64::serial;
use core::fmt::Write;

/// Push any escape sequence bytes to the input subsystem
///
/// Routes the escape sequence to both VT input and console device
fn push_escape_sequence(seq: &[u8]) {
    #[cfg(feature = "debug-console")]
    {
        let mut w = serial::SerialWriter;
        let _ = write!(w, "[ESCAPE] Pushing {} bytes to VT+console: ", seq.len());
        for &byte in seq {
            let _ = write!(w, "{:02x} ", byte);
        }
        let _ = write!(w, "\n");
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

/// Strip ANSI/CSI escape sequences from output for cleaner serial debug logs
#[cfg(feature = "debug-tty-read")]
fn strip_ansi_escapes(data: &[u8]) -> alloc::vec::Vec<u8> {
    extern crate alloc;
    use alloc::vec::Vec;

    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        if i + 1 < data.len() && data[i] == 0x1B {
            // ESC
            // Check for CSI sequence: ESC [
            if data[i + 1] == b'[' {
                // Skip until we find the end of CSI sequence (letter A-Z, a-z)
                i += 2;
                while i < data.len() {
                    let c = data[i];
                    i += 1;
                    if (c >= b'A' && c <= b'Z') || (c >= b'a' && c <= b'z') {
                        break;
                    }
                }
                continue;
            }
            // Check for other escape sequences: ESC ?
            else if data[i + 1] == b'?' {
                // Skip ESC ? sequences
                i += 2;
                while i < data.len() {
                    let c = data[i];
                    i += 1;
                    if c == b'h' || c == b'l' {
                        break;
                    }
                }
                continue;
            }
        }

        result.push(data[i]);
        i += 1;
    }

    result
}

/// Console write function for syscalls
///
/// Writes to serial and terminal emulator (if initialized).
pub fn console_write(data: &[u8]) {
    // Write to serial for debugging (filter escape sequences for clean debug output)
    #[cfg(feature = "debug-tty-read")]
    {
        extern crate alloc;
        use alloc::vec::Vec;
        let filtered = strip_ansi_escapes(data);
        let mut writer = serial::SerialWriter;
        for &byte in &filtered {
            let _ = writer.write_char(byte as char);
        }
    }
    #[cfg(not(feature = "debug-tty-read"))]
    {
        let mut writer = serial::SerialWriter;
        for &byte in data {
            let _ = writer.write_char(byte as char);
        }
    }

    // Write to terminal emulator for ANSI-processed framebuffer output (unfiltered - needs escapes!)
    if terminal::is_initialized() {
        terminal::write(data);
    } else if fb::is_initialized() {
        // Fallback to basic fb console before terminal is ready
        for &byte in data {
            fb::putchar(byte as char);
        }
    }
}

/// Terminal tick callback - called at ~30 FPS from timer interrupt
pub fn terminal_tick() {
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
        // NeonRoot: Debug output MUST use lock-free serial writes here.
        // terminal_tick runs in timer interrupt context — if the main code
        // holds the COM1 lock during a writeln!, taking it again deadlocks
        // and escalates to a triple fault. write_str_unsafe bypasses the lock.
        if byte < 32 || byte > 126 {
            unsafe {
                arch::serial::write_str_unsafe("[SERIAL] Got 0x");
                let hi = (byte >> 4) & 0xF;
                let lo = byte & 0xF;
                arch::serial::write_byte_unsafe(if hi < 10 { b'0' + hi } else { b'a' + hi - 10 });
                arch::serial::write_byte_unsafe(if lo < 10 { b'0' + lo } else { b'a' + lo - 10 });
                arch::serial::write_byte_unsafe(b'\n');
            }
        }

        // Route serial input to VT subsystem — /dev/console delegates to
        // the active VT device, so only one push path is needed.
        if let Some(manager) = vt::get_manager() {
            manager.push_input(byte);
        }
    }

    // Process mouse events from input subsystem
    // Mouse device is typically device 1 (keyboard is device 0)
    if fb::mouse_initialized() {
        let mut total_dx: i32 = 0;
        let mut total_dy: i32 = 0;
        let mut wheel_delta: i32 = 0;
        let has_mouse_mode = terminal::is_initialized() && terminal::has_mouse_mode();

        // Track button state for escape sequence generation and selection
        static mut MOUSE_BUTTONS: u8 = 0;
        static mut LEFT_PRESSED: bool = false;
        static mut MIDDLE_PRESSED: bool = false;

        // Drain all mouse events from input device 1
        if let Some(mouse_handle) = input::get_device(1) {
            while let Some(event) = mouse_handle.pop_event() {
                debug_mouse_unsafe!("M");
                match event.event_type() {
                    input::EventType::Rel => {
                        if event.code == input::REL_X {
                            total_dx += event.value;
                        } else if event.code == input::REL_Y {
                            total_dy += event.value;
                        } else if event.code == input::REL_WHEEL {
                            wheel_delta += event.value;
                        }
                    }
                    input::EventType::Key => {
                        // Map button codes to terminal button numbers
                        let (btn, bit) = match event.code {
                            0x110 => (0u8, 0x01u8), // BTN_LEFT
                            0x112 => (1, 0x04),     // BTN_MIDDLE
                            0x111 => (2, 0x02),     // BTN_RIGHT
                            _ => continue,
                        };
                        let pressed = event.value != 0;
                        unsafe {
                            if pressed {
                                MOUSE_BUTTONS |= bit;
                            } else {
                                MOUSE_BUTTONS &= !bit;
                            }

                            // Track left and middle buttons separately for selection/paste
                            if event.code == 0x110 {
                                // Left button
                                if pressed && !LEFT_PRESSED {
                                    // Left press - start selection if no mouse mode active
                                    LEFT_PRESSED = true;
                                    if !has_mouse_mode {
                                        if let Some((mx, my)) = fb::mouse_position() {
                                            terminal::start_selection(mx, my);
                                        }
                                    }
                                } else if !pressed && LEFT_PRESSED {
                                    // Left release - finish selection
                                    LEFT_PRESSED = false;
                                    if !has_mouse_mode {
                                        terminal::finish_selection();
                                    }
                                }
                            } else if event.code == 0x112 {
                                // Middle button - paste on press
                                if pressed && !MIDDLE_PRESSED {
                                    MIDDLE_PRESSED = true;
                                    if !has_mouse_mode {
                                        let paste_data = terminal::paste_clipboard();
                                        for &byte in &paste_data {
                                            if let Some(manager) = vt::get_manager() {
                                                manager.push_input(byte);
                                            }
                                        }
                                    }
                                } else if !pressed {
                                    MIDDLE_PRESSED = false;
                                }
                            }
                        }

                        // Generate escape sequence for button press/release if in mouse mode
                        if has_mouse_mode {
                            if let Some((mx, my)) = fb::mouse_position() {
                                let esc_btn = if pressed { btn } else { 3 }; // 3 = release
                                if let Some(seq) =
                                    terminal::mouse_event(esc_btn, mx, my, pressed, false)
                                {
                                    push_mouse_escape(&seq);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Move graphical cursor
        if total_dx != 0 || total_dy != 0 {
            fb::mouse_move(total_dx, total_dy);

            // Update selection if left button held (no mouse mode)
            unsafe {
                if LEFT_PRESSED && !has_mouse_mode {
                    if let Some((mx, my)) = fb::mouse_position() {
                        terminal::update_selection(mx, my);
                    }
                }
            }

            // Generate motion escape sequences if terminal wants them (mouse mode)
            if has_mouse_mode {
                if let Some((mx, my)) = fb::mouse_position() {
                    let held_btn = unsafe {
                        if MOUSE_BUTTONS & 0x01 != 0 {
                            0u8
                        }
                        // Left
                        else if MOUSE_BUTTONS & 0x04 != 0 {
                            1
                        }
                        // Middle
                        else if MOUSE_BUTTONS & 0x02 != 0 {
                            2
                        }
                        // Right
                        else {
                            3
                        } // No button
                    };
                    if let Some(seq) = terminal::mouse_event(held_btn, mx, my, true, true) {
                        push_mouse_escape(&seq);
                    }
                }
            }
        }

        // Handle mouse wheel scrolling
        // — EchoFrame: Scroll wheel now works in two modes:
        // 1. When mouse mode is OFF: scroll terminal history (default behavior)
        // 2. When mouse mode is ON: send escape sequences to app (e.g., vim, less)
        if wheel_delta != 0 {
            if has_mouse_mode {
                // App has requested mouse tracking - send wheel as escape sequences
                if let Some((mx, my)) = fb::mouse_position() {
                    let btn = if wheel_delta > 0 { 64u8 } else { 65u8 };
                    let clicks = wheel_delta.unsigned_abs();
                    for _ in 0..clicks {
                        if let Some(seq) = terminal::mouse_event(btn, mx, my, true, false) {
                            push_mouse_escape(&seq);
                        }
                    }
                }
            } else {
                // No mouse mode - scroll terminal view directly (3 lines per wheel click)
                let scroll_lines = (wheel_delta.unsigned_abs() as usize) * 3;
                if wheel_delta > 0 {
                    terminal::scroll_up(scroll_lines);
                } else {
                    terminal::scroll_down(scroll_lines);
                }
            }
        }
    }

    // Drive the active display: prefer terminal emulator; disable legacy fb cursor to avoid double cursor
    if terminal::is_initialized() {
        // Erase mouse cursor before terminal render to avoid it getting baked into
        // the save buffer or painted over by dirty row redraws — SoftGlyph
        fb::mouse_erase();

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

        // Redraw mouse cursor on top of freshly rendered terminal content — SoftGlyph
        fb::mouse_draw();
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

    // Enable access to user pages (STAC - Supervisor-Mode Access Prevention Clear)
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    for &byte in data {
        let _ = writer.write_char(byte as char);
    }

    // Disable access to user pages (CLAC - Supervisor-Mode Access Prevention Clear)
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }
}

/// Console write function for devfs (legacy fallback)
///
/// Writes to both serial and framebuffer (if initialized).
/// Used before terminal emulator is initialized.
/// NOTE: data may point to user memory, so we need STAC/CLAC for SMAP
pub fn console_write_bytes(data: &[u8]) {
    // Enable access to user pages (STAC)
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    // Write to serial (filter escape sequences for clean debug output)
    #[cfg(feature = "debug-tty-read")]
    {
        extern crate alloc;
        let filtered = strip_ansi_escapes(data);
        let mut writer = serial::SerialWriter;
        for &byte in &filtered {
            let _ = writer.write_char(byte as char);
        }
    }
    #[cfg(not(feature = "debug-tty-read"))]
    {
        let mut writer = serial::SerialWriter;
        for &byte in data {
            let _ = writer.write_char(byte as char);
        }
    }

    // Write to framebuffer console if available (legacy path - unfiltered, needs escapes!)
    if fb::is_initialized() && !terminal::is_initialized() {
        for &byte in data {
            fb::putchar(byte as char);
        }
    }

    // Disable access to user pages (CLAC)
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }
}

