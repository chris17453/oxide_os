//! Input handling for Doom
//!
//! Reads keyboard input from stdin (tty device).
//! -- InputShade: Input systems + device interaction

use libc::read;

// fcntl constants
const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: u32 = 0o4000;

/// Input state tracking
pub struct InputState {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    strafe_left: bool,
    strafe_right: bool,
    fire: bool,
    use_key: bool,
    quit: bool,
}

impl InputState {
    /// Create a new input state
    pub fn new() -> Self {
        // Set stdin to non-blocking
        let flags = unsafe { libc::fcntl::fcntl(0, F_GETFL, 0) };
        unsafe { libc::fcntl::fcntl(0, F_SETFL, (flags as u32 | O_NONBLOCK) as u64) };

        InputState {
            forward: false,
            backward: false,
            left: false,
            right: false,
            strafe_left: false,
            strafe_right: false,
            fire: false,
            use_key: false,
            quit: false,
        }
    }

    /// Update input state by reading from stdin
    /// -- InputShade: Raw input processing, no middleware latency
    pub fn update(&mut self) {
        // Reset one-shot actions
        self.fire = false;
        self.use_key = false;

        let mut buf = [0u8; 16];
        loop {
            let n = read(0, &mut buf);
            if n <= 0 {
                break;
            }

            // Parse input (ANSI escape sequences for arrow keys)
            let mut i = 0;
            while i < n as usize {
                match buf[i] {
                    // ESC - check for arrow keys or quit
                    0x1B => {
                        if i + 2 < n as usize && buf[i + 1] == b'[' {
                            match buf[i + 2] {
                                b'A' => self.forward = true,  // Up arrow
                                b'B' => self.backward = true, // Down arrow
                                b'C' => self.right = true,    // Right arrow
                                b'D' => self.left = true,     // Left arrow
                                _ => {}
                            }
                            i += 3;
                        } else {
                            self.quit = true; // ESC pressed alone
                            i += 1;
                        }
                    }
                    // WASD movement
                    b'w' | b'W' => self.forward = true,
                    b's' | b'S' => self.backward = true,
                    b'a' | b'A' => self.strafe_left = true,
                    b'd' | b'D' => self.strafe_right = true,
                    // Actions
                    b' ' => self.use_key = true, // Space - use/open
                    0x11 => self.fire = true,    // Ctrl+Q - fire
                    // Quit
                    b'q' | b'Q' => self.quit = true,
                    0x03 => self.quit = true, // Ctrl+C
                    _ => {}
                }
                i += 1;
            }
        }
    }

    // Input state queries
    pub fn is_forward(&self) -> bool {
        self.forward
    }
    pub fn is_backward(&self) -> bool {
        self.backward
    }
    pub fn is_left(&self) -> bool {
        self.left
    }
    pub fn is_right(&self) -> bool {
        self.right
    }
    pub fn is_strafe_left(&self) -> bool {
        self.strafe_left
    }
    pub fn is_strafe_right(&self) -> bool {
        self.strafe_right
    }
    pub fn is_fire(&self) -> bool {
        self.fire
    }
    pub fn is_use(&self) -> bool {
        self.use_key
    }
    pub fn is_quit(&self) -> bool {
        self.quit
    }
}
