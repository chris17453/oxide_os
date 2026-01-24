//! Console I/O and keyboard handling for the OXIDE kernel.

use arch_x86_64 as arch;
use arch_x86_64::serial;
use core::fmt::Write;

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
    // Process keyboard scancodes
    while let Some(scancode) = arch::get_scancode() {
        if let Some(byte) = process_scancode(scancode) {
            // Push character to console input buffer (handles both stdin and display)
            devfs::console_push_char(byte);
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

/// Console read function for syscalls
pub fn console_read(buf: &mut [u8]) -> usize {
    // Read from keyboard buffer (PS/2) or serial port
    // NOTE: No echo here - the application (shell) handles echoing
    let mut count = 0;
    for byte in buf.iter_mut() {
        // Poll for input from either keyboard buffer or serial
        loop {
            // First check keyboard buffer (PS/2 console input)
            if devfs::console_has_input() {
                // Read from console (keyboard) buffer via VFS-like mechanism
                // The console_push_str pushed bytes, we need to pop them
                // But we can't directly pop - we use a temp buffer approach
                let mut temp = [0u8; 1];
                // Read one byte from console device
                if let Ok(vnode) = vfs::mount::GLOBAL_VFS.lookup("/dev/console") {
                    if let Ok(n) = vnode.read(0, &mut temp) {
                        if n > 0 {
                            let b = temp[0];
                            *byte = b;
                            count += 1;
                            // Return on newline for line-buffered input
                            if b == b'\n' || b == b'\r' {
                                if b == b'\r' {
                                    *byte = b'\n';
                                }
                                return count;
                            }
                            break;
                        }
                    }
                }
            }

            // Fallback to serial port (for serial console / QEMU)
            if let Some(b) = serial::read_byte() {
                *byte = b;
                count += 1;
                // Return on newline for line-buffered input
                if b == b'\n' || b == b'\r' {
                    if b == b'\r' {
                        // Convert CR to LF
                        *byte = b'\n';
                    }
                    return count;
                }
                break;
            }

            // Use HLT instruction to wait for interrupt instead of busy-spin
            // Note: This doesn't yield to other processes (scheduler only preempts user mode)
            // TODO: Implement proper blocking I/O with wait queues
            unsafe {
                core::arch::asm!("hlt", options(nomem, nostack));
            }
        }
    }
    count
}

/// Serial-only write function for devfs
///
/// Writes only to serial port for raw debug output.
pub fn serial_write_bytes(data: &[u8]) {
    let mut writer = serial::SerialWriter;
    for &byte in data {
        let _ = writer.write_char(byte as char);
    }
}

/// Console write function for devfs (legacy fallback)
///
/// Writes to both serial and framebuffer (if initialized).
/// Used before terminal emulator is initialized.
pub fn console_write_bytes(data: &[u8]) {
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
}

// ============================================================================
// Keyboard Processing
// ============================================================================

/// Track modifier state for basic scancode decoding
static mut SHIFT_PRESSED: bool = false;
static mut CTRL_PRESSED: bool = false;
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
                SHIFT_PRESSED = !is_release;
                EXTENDED_SCANCODE = false;
                return None;
            }
            0x1D => {
                CTRL_PRESSED = !is_release;
                EXTENDED_SCANCODE = false;
                return None;
            }
            _ => {}
        }

        // Ignore key releases for non-modifiers
        if is_release {
            EXTENDED_SCANCODE = false;
            return None;
        }

        // Clear extended flag after handling the follow-up byte
        let _extended = EXTENDED_SCANCODE;
        EXTENDED_SCANCODE = false;

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
