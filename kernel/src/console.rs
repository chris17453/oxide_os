//! Console I/O and keyboard handling for the OXIDE kernel.

use arch_x86_64 as arch;
use arch_x86_64::serial;
use core::fmt::Write;

/// Push any escape sequence bytes to the input subsystem
///
/// Routes the escape sequence to both VT input and console device
fn push_escape_sequence(seq: &[u8]) {
    let mut w = serial::SerialWriter;
    let _ = write!(w, "[ESCAPE] Pushing {} bytes to VT+console: ", seq.len());
    for &byte in seq {
        let _ = write!(w, "{:02x} ", byte);
    }
    let _ = write!(w, "\n");

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

/// Console write function for syscalls
///
/// Writes to serial and terminal emulator (if initialized).
pub fn console_write(data: &[u8]) {
    // Write to serial for debugging
    let mut writer = serial::SerialWriter;
    for &byte in data {
        let _ = writer.write_char(byte as char);
    }

    // Write to terminal emulator for ANSI-processed framebuffer output
    if terminal::is_initialized() {
        terminal::write(data);
    } else if fb::is_initialized() {
        // Fallback to basic fb console before terminal is ready
        for &byte in data {
            fb::putchar(byte as char);
        }
    }
}

/// Terminal tick callback - called at 30 FPS from timer interrupt
pub fn terminal_tick() {
    // Process keyboard scancodes from PS/2 keyboard (IRQ-driven buffer)
    // Input flows through VT push_input() only — /dev/console delegates
    // to the active VT device, so there's no need to push to both.
    while let Some(scancode) = arch::get_scancode() {
        if let Some(byte) = process_scancode(scancode) {
            if let Some(manager) = vt::get_manager() {
                manager.push_input(byte);
            }
        }
    }

    // Also poll i8042 directly as fallback (handles cases where IRQ1
    // doesn't fire, e.g., QEMU sendkey with -display none)
    // SAFETY: We're in timer interrupt context; no concurrent i8042 access.
    while let Some(scancode) = unsafe { arch::poll_keyboard() } {
        if let Some(byte) = process_scancode(scancode) {
            if let Some(manager) = vt::get_manager() {
                manager.push_input(byte);
            }
        }
    }

    // Also process serial port input (for -serial stdio in QEMU)
    // SAFETY: We use the lock-free serial read here because terminal_tick
    // runs in interrupt context (timer interrupt). Using the locking version
    // would deadlock if process-context code holds the COM1 lock.
    while let Some(byte) = unsafe { arch::serial_read_unsafe() } {
        // Debug: log serial input
        if byte < 32 || byte > 126 {
            let mut w = serial::SerialWriter;
            let _ = write!(w, "[SERIAL] Got 0x{:02x}\n", byte);
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

        // Track button state for escape sequence generation
        static mut MOUSE_BUTTONS: u8 = 0;

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
                        if has_mouse_mode {
                            // Map button codes to terminal button numbers
                            let (btn, bit) = match event.code {
                                0x110 => (0u8, 0x01u8), // BTN_LEFT
                                0x112 => (1, 0x04),      // BTN_MIDDLE
                                0x111 => (2, 0x02),      // BTN_RIGHT
                                _ => continue,
                            };
                            let pressed = event.value != 0;
                            unsafe {
                                if pressed {
                                    MOUSE_BUTTONS |= bit;
                                } else {
                                    MOUSE_BUTTONS &= !bit;
                                }
                            }

                            // Generate escape sequence for button press/release
                            if let Some((mx, my)) = fb::mouse_position() {
                                let esc_btn = if pressed { btn } else { 3 }; // 3 = release
                                if let Some(seq) = terminal::mouse_event(esc_btn, mx, my, pressed, false) {
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

            // Generate motion escape sequences if terminal wants them
            if has_mouse_mode {
                if let Some((mx, my)) = fb::mouse_position() {
                    let held_btn = unsafe {
                        if MOUSE_BUTTONS & 0x01 != 0 { 0u8 }      // Left
                        else if MOUSE_BUTTONS & 0x04 != 0 { 1 }    // Middle
                        else if MOUSE_BUTTONS & 0x02 != 0 { 2 }    // Right
                        else { 3 } // No button
                    };
                    if let Some(seq) = terminal::mouse_event(held_btn, mx, my, true, true) {
                        push_mouse_escape(&seq);
                    }
                }
            }
        }

        // Generate wheel escape sequences
        if wheel_delta != 0 && has_mouse_mode {
            if let Some((mx, my)) = fb::mouse_position() {
                let btn = if wheel_delta > 0 { 64u8 } else { 65u8 };
                let clicks = wheel_delta.unsigned_abs();
                for _ in 0..clicks {
                    if let Some(seq) = terminal::mouse_event(btn, mx, my, true, false) {
                        push_mouse_escape(&seq);
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
        // Ensure any pending render happens at least once per tick
        terminal::tick();
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

    // Write to serial
    let mut writer = serial::SerialWriter;
    for &byte in data {
        let _ = writer.write_char(byte as char);
    }

    // Write to framebuffer console if available (legacy path)
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

// ============================================================================
// Keyboard Processing
// ============================================================================

/// Track modifier state for basic scancode decoding
static mut SHIFT_PRESSED: bool = false;
static mut CTRL_PRESSED: bool = false;
static mut ALT_PRESSED: bool = false;
static mut EXTENDED_SCANCODE: bool = false;

/// Process a single PS/2 scancode and return an ASCII byte (if any)
fn process_scancode(scancode: u8) -> Option<u8> {
    unsafe {
        // Handle extended prefix
        if scancode == 0xE0 {
            EXTENDED_SCANCODE = true;
            return None;
        }

        let is_release = scancode & 0x80 != 0;
        let code = scancode & 0x7F;

        // Update modifier state
        match code {
            0x2A | 0x36 => {
                // Shift
                SHIFT_PRESSED = !is_release;
                EXTENDED_SCANCODE = false;
                return None;
            }
            0x1D => {
                // Ctrl
                CTRL_PRESSED = !is_release;
                EXTENDED_SCANCODE = false;
                return None;
            }
            0x38 => {
                // Alt
                ALT_PRESSED = !is_release;
                EXTENDED_SCANCODE = false;
                return None;
            }
            _ => {}
        }

        // Check for Ctrl+Alt+Fn (VT switching)
        if !is_release && CTRL_PRESSED && ALT_PRESSED {
            match code {
                0x3B..=0x40 => {
                    // F1-F6 keys (scancodes 0x3B-0x40)
                    let vt_num = (code - 0x3B) as usize;
                    if let Some(manager) = vt::get_manager() {
                        manager.switch_to(vt_num);
                        let mut writer = serial::SerialWriter;
                        let _ = write!(writer, "[VT] Switched to tty{}\n", vt_num + 1);
                    }
                    EXTENDED_SCANCODE = false;
                    return None;
                }
                _ => {}
            }
        }

        // Ignore key releases for non-modifiers
        if is_release {
            EXTENDED_SCANCODE = false;
            return None;
        }

        // Clear extended flag after handling the follow-up byte
        let _extended = EXTENDED_SCANCODE;
        EXTENDED_SCANCODE = false;

        // Handle extended keys (arrow keys, etc.)
        if _extended {
            match code {
                0x48 => {
                    // UP arrow: ESC [ A
                    let mut w = serial::SerialWriter;
                    let _ = write!(w, "[KB] UP arrow detected, sending ESC[A\n");
                    push_escape_sequence(b"\x1b[A");
                    return None;
                }
                0x50 => {
                    // DOWN arrow: ESC [ B
                    let mut w = serial::SerialWriter;
                    let _ = write!(w, "[KB] DOWN arrow detected, sending ESC[B\n");
                    push_escape_sequence(b"\x1b[B");
                    return None;
                }
                0x4B => {
                    // LEFT arrow: ESC [ D (or Ctrl+LEFT: ESC [ 1 ; 5 D)
                    let mut w = serial::SerialWriter;
                    if CTRL_PRESSED {
                        let _ = write!(w, "[KB] Ctrl+LEFT detected\n");
                        push_escape_sequence(b"\x1b[1;5D");
                    } else {
                        let _ = write!(w, "[KB] LEFT arrow detected\n");
                        push_escape_sequence(b"\x1b[D");
                    }
                    return None;
                }
                0x4D => {
                    // RIGHT arrow: ESC [ C (or Ctrl+RIGHT: ESC [ 1 ; 5 C)
                    let mut w = serial::SerialWriter;
                    if CTRL_PRESSED {
                        let _ = write!(w, "[KB] Ctrl+RIGHT detected\n");
                        push_escape_sequence(b"\x1b[1;5C");
                    } else {
                        let _ = write!(w, "[KB] RIGHT arrow detected\n");
                        push_escape_sequence(b"\x1b[C");
                    }
                    return None;
                }
                0x47 => {
                    // Home key: ESC [ H
                    push_escape_sequence(b"\x1b[H");
                    return None;
                }
                0x4F => {
                    // End key: ESC [ F
                    push_escape_sequence(b"\x1b[F");
                    return None;
                }
                0x53 => {
                    // Delete key: ESC [ 3 ~
                    push_escape_sequence(b"\x1b[3~");
                    return None;
                }
                _ => {
                    // Other extended keys: ignore for now
                    return None;
                }
            }
        }

        // Map scancode to ASCII
        if let Some(ch) = scancode_to_ascii(code, SHIFT_PRESSED) {
            // Ctrl modifies alphabetic characters into control codes
            if CTRL_PRESSED && ch.is_ascii_alphabetic() {
                return Some((ch.to_ascii_lowercase() as u8 - b'a' + 1) as u8);
            }
            return Some(ch as u8);
        }
    }
    None
}

/// Convert PS/2 set 1 scancode to ASCII using US layout with Shift support
fn scancode_to_ascii(scancode: u8, shift: bool) -> Option<char> {
    match scancode {
        // Numbers
        0x02 => Some(if shift { '!' } else { '1' }),
        0x03 => Some(if shift { '@' } else { '2' }),
        0x04 => Some(if shift { '#' } else { '3' }),
        0x05 => Some(if shift { '$' } else { '4' }),
        0x06 => Some(if shift { '%' } else { '5' }),
        0x07 => Some(if shift { '^' } else { '6' }),
        0x08 => Some(if shift { '&' } else { '7' }),
        0x09 => Some(if shift { '*' } else { '8' }),
        0x0A => Some(if shift { '(' } else { '9' }),
        0x0B => Some(if shift { ')' } else { '0' }),
        0x0C => Some(if shift { '_' } else { '-' }),
        0x0D => Some(if shift { '+' } else { '=' }),

        // Letters
        0x10 => Some(if shift { 'Q' } else { 'q' }),
        0x11 => Some(if shift { 'W' } else { 'w' }),
        0x12 => Some(if shift { 'E' } else { 'e' }),
        0x13 => Some(if shift { 'R' } else { 'r' }),
        0x14 => Some(if shift { 'T' } else { 't' }),
        0x15 => Some(if shift { 'Y' } else { 'y' }),
        0x16 => Some(if shift { 'U' } else { 'u' }),
        0x17 => Some(if shift { 'I' } else { 'i' }),
        0x18 => Some(if shift { 'O' } else { 'o' }),
        0x19 => Some(if shift { 'P' } else { 'p' }),
        0x1E => Some(if shift { 'A' } else { 'a' }),
        0x1F => Some(if shift { 'S' } else { 's' }),
        0x20 => Some(if shift { 'D' } else { 'd' }),
        0x21 => Some(if shift { 'F' } else { 'f' }),
        0x22 => Some(if shift { 'G' } else { 'g' }),
        0x23 => Some(if shift { 'H' } else { 'h' }),
        0x24 => Some(if shift { 'J' } else { 'j' }),
        0x25 => Some(if shift { 'K' } else { 'k' }),
        0x26 => Some(if shift { 'L' } else { 'l' }),
        0x2C => Some(if shift { 'Z' } else { 'z' }),
        0x2D => Some(if shift { 'X' } else { 'x' }),
        0x2E => Some(if shift { 'C' } else { 'c' }),
        0x2F => Some(if shift { 'V' } else { 'v' }),
        0x30 => Some(if shift { 'B' } else { 'b' }),
        0x31 => Some(if shift { 'N' } else { 'n' }),
        0x32 => Some(if shift { 'M' } else { 'm' }),

        // Punctuation
        0x1A => Some(if shift { '{' } else { '[' }),
        0x1B => Some(if shift { '}' } else { ']' }),
        0x27 => Some(if shift { ':' } else { ';' }),
        0x28 => Some(if shift { '"' } else { '\'' }),
        0x29 => Some(if shift { '~' } else { '`' }),
        0x2B => Some(if shift { '|' } else { '\\' }),
        0x33 => Some(if shift { '<' } else { ',' }),
        0x34 => Some(if shift { '>' } else { '.' }),
        0x35 => Some(if shift { '?' } else { '/' }),

        // Whitespace and control
        0x39 => Some(' '),
        0x0F => Some('\t'),
        0x1C => Some('\n'),
        0x0E => Some('\x08'),

        _ => None,
    }
}
