//! VT100/ANSI escape sequence parser
//!
//! Implements a state machine for parsing escape sequences.
//! Zero kernel dependencies - pure alloc.
//!
//! -- GraveShift: State machine parser - decodes the wire protocol of terminals

extern crate alloc;

use alloc::vec::Vec;

/// Maximum number of CSI parameters
const MAX_PARAMS: usize = 16;

/// Parser state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Ground state - normal character processing
    Ground,
    /// After ESC (0x1B)
    Escape,
    /// After ESC [
    CsiEntry,
    /// Collecting CSI parameters
    CsiParam,
    /// Intermediate bytes in CSI
    CsiIntermediate,
    /// Ignoring rest of CSI sequence (invalid)
    CsiIgnore,
    /// After ESC ] (OSC string)
    OscString,
    /// DCS entry (device control string)
    DcsEntry,
    /// DCS parameter collection
    DcsParam,
    /// DCS intermediate bytes
    DcsIntermediate,
    /// DCS passthrough (collecting data)
    DcsPassthrough,
    /// DCS ignore
    DcsIgnore,
    /// After ESC (
    DesignateG0,
    /// After ESC )
    DesignateG1,
}

/// Parsed action from the parser
#[derive(Debug, Clone)]
pub enum Action {
    /// Print a character
    Print(char),
    /// Execute a control character (C0/C1)
    Execute(u8),
    /// CSI sequence with final char
    CsiDispatch {
        params: Vec<i32>,
        intermediates: Vec<u8>,
        final_char: u8,
    },
    /// ESC sequence with final char
    EscDispatch {
        intermediates: Vec<u8>,
        final_char: u8,
    },
    /// OSC (Operating System Command) string
    OscDispatch(Vec<u8>),
    /// DCS (Device Control String) sequence
    DcsDispatch {
        params: Vec<i32>,
        intermediates: Vec<u8>,
        final_char: u8,
        data: Vec<u8>,
    },
    /// No action
    None,
}

/// VT100/ANSI escape sequence parser
pub struct Parser {
    /// Current state
    state: State,
    /// CSI parameters being collected
    params: Vec<i32>,
    /// Current parameter being built
    current_param: i32,
    /// Whether we've seen any digit for current param
    param_started: bool,
    /// Intermediate bytes
    intermediates: Vec<u8>,
    /// OSC string being collected
    osc_string: Vec<u8>,
    /// DCS data being collected
    dcs_data: Vec<u8>,
    /// DCS final character
    dcs_final: u8,
    /// UTF-8 decoding buffer (up to 4 bytes)
    utf8_buffer: [u8; 4],
    /// Number of UTF-8 bytes collected
    utf8_count: u8,
    /// Expected number of UTF-8 bytes
    utf8_expected: u8,
}

impl Parser {
    /// Create a new parser
    pub fn new() -> Self {
        Parser {
            state: State::Ground,
            params: Vec::with_capacity(MAX_PARAMS),
            current_param: 0,
            param_started: false,
            intermediates: Vec::with_capacity(4),
            osc_string: Vec::with_capacity(256),
            dcs_data: Vec::with_capacity(256),
            dcs_final: 0,
            utf8_buffer: [0; 4],
            utf8_count: 0,
            utf8_expected: 0,
        }
    }

    /// Reset parser to ground state
    pub fn reset(&mut self) {
        self.state = State::Ground;
        self.params.clear();
        self.current_param = 0;
        self.param_started = false;
        self.intermediates.clear();
        self.osc_string.clear();
        self.dcs_data.clear();
        self.dcs_final = 0;
    }

    /// Process a single byte and return any action
    pub fn advance(&mut self, byte: u8) -> Action {
        // Handle C0 controls in any state (except OSC)
        if byte < 0x20 && self.state != State::OscString {
            match byte {
                // These are always processed
                0x1B => {
                    // ESC - start escape sequence
                    self.enter_escape();
                    return Action::None;
                }
                0x18 | 0x1A => {
                    // CAN/SUB - cancel sequence
                    self.reset();
                    return Action::None;
                }
                _ => {
                    // Other C0 controls are executed
                    return Action::Execute(byte);
                }
            }
        }

        match self.state {
            State::Ground => self.handle_ground(byte),
            State::Escape => self.handle_escape(byte),
            State::CsiEntry => self.handle_csi_entry(byte),
            State::CsiParam => self.handle_csi_param(byte),
            State::CsiIntermediate => self.handle_csi_intermediate(byte),
            State::CsiIgnore => self.handle_csi_ignore(byte),
            State::OscString => self.handle_osc_string(byte),
            State::DcsEntry => self.handle_dcs_entry(byte),
            State::DcsParam => self.handle_dcs_param(byte),
            State::DcsIntermediate => self.handle_dcs_intermediate(byte),
            State::DcsPassthrough => self.handle_dcs_passthrough(byte),
            State::DcsIgnore => self.handle_dcs_ignore(byte),
            State::DesignateG0 | State::DesignateG1 => self.handle_designate(byte),
        }
    }

    /// Enter escape state
    fn enter_escape(&mut self) {
        self.state = State::Escape;
        self.params.clear();
        self.current_param = 0;
        self.param_started = false;
        self.intermediates.clear();
    }

    /// Handle ground state
    fn handle_ground(&mut self, byte: u8) -> Action {
        if byte >= 0x20 && byte < 0x7F {
            // Printable ASCII (fast path)
            Action::Print(byte as char)
        } else if byte >= 0x80 {
            // UTF-8 multi-byte sequence or C1 control
            self.handle_utf8(byte)
        } else {
            Action::None
        }
    }

    /// Handle UTF-8 multi-byte sequences
    ///
    /// UTF-8 encoding:
    /// - 1 byte:  0xxxxxxx  (0x00-0x7F)  ASCII (handled in fast path)
    /// - 2 bytes: 110xxxxx 10xxxxxx  (0xC0-0xDF + 0x80-0xBF)
    /// - 3 bytes: 1110xxxx 10xxxxxx 10xxxxxx  (0xE0-0xEF + 2x continuation)
    /// - 4 bytes: 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx  (0xF0-0xF7 + 3x continuation)
    fn handle_utf8(&mut self, byte: u8) -> Action {
        // Check if this is a UTF-8 start byte
        if byte >= 0xC0 && byte <= 0xF7 {
            // Start of new UTF-8 sequence
            self.utf8_buffer[0] = byte;
            self.utf8_count = 1;

            // Determine expected length
            self.utf8_expected = if byte < 0xE0 {
                2 // 110xxxxx
            } else if byte < 0xF0 {
                3 // 1110xxxx
            } else {
                4 // 11110xxx
            };

            Action::None // Wait for continuation bytes
        } else if byte >= 0x80 && byte < 0xC0 {
            // UTF-8 continuation byte (10xxxxxx)
            if self.utf8_expected > 0 && self.utf8_count < self.utf8_expected {
                self.utf8_buffer[self.utf8_count as usize] = byte;
                self.utf8_count += 1;

                // Check if sequence is complete
                if self.utf8_count == self.utf8_expected {
                    let result = self.decode_utf8();
                    // Reset for next sequence
                    self.utf8_count = 0;
                    self.utf8_expected = 0;
                    result
                } else {
                    Action::None // Wait for more bytes
                }
            } else {
                // Unexpected continuation byte - reset and ignore
                self.utf8_count = 0;
                self.utf8_expected = 0;
                Action::None
            }
        } else {
            // Invalid UTF-8 start byte (0xF8-0xFF) - ignore
            self.utf8_count = 0;
            self.utf8_expected = 0;
            Action::None
        }
    }

    /// Decode accumulated UTF-8 bytes to char
    fn decode_utf8(&self) -> Action {
        if let Ok(s) = core::str::from_utf8(&self.utf8_buffer[..self.utf8_count as usize]) {
            if let Some(ch) = s.chars().next() {
                return Action::Print(ch);
            }
        }
        // Invalid UTF-8 sequence - ignore
        Action::None
    }

    /// Handle escape state (after ESC)
    fn handle_escape(&mut self, byte: u8) -> Action {
        match byte {
            b'[' => {
                // CSI sequence
                self.state = State::CsiEntry;
                Action::None
            }
            b']' => {
                // OSC sequence
                self.state = State::OscString;
                self.osc_string.clear();
                Action::None
            }
            b'P' => {
                // DCS sequence
                self.state = State::DcsEntry;
                Action::None
            }
            b'(' => {
                // Designate G0 character set
                self.state = State::DesignateG0;
                Action::None
            }
            b')' => {
                // Designate G1 character set
                self.state = State::DesignateG1;
                Action::None
            }
            0x20..=0x2F => {
                // Intermediate byte
                self.intermediates.push(byte);
                Action::None
            }
            0x30..=0x7E => {
                // Final character
                let action = Action::EscDispatch {
                    intermediates: self.intermediates.clone(),
                    final_char: byte,
                };
                self.reset();
                action
            }
            _ => {
                self.reset();
                Action::None
            }
        }
    }

    /// Handle CSI entry state
    fn handle_csi_entry(&mut self, byte: u8) -> Action {
        match byte {
            b'0'..=b'9' => {
                self.current_param = (byte - b'0') as i32;
                self.param_started = true;
                self.state = State::CsiParam;
                Action::None
            }
            b';' => {
                // Empty parameter (use default)
                self.params.push(-1); // -1 indicates default
                self.state = State::CsiParam;
                Action::None
            }
            b':' => {
                // Sub-parameter separator (for SGR extended colors)
                self.params.push(-1);
                self.state = State::CsiParam;
                Action::None
            }
            b'<' | b'=' | b'>' | b'?' => {
                // Private parameter prefix
                self.intermediates.push(byte);
                self.state = State::CsiParam;
                Action::None
            }
            0x20..=0x2F => {
                // Intermediate byte
                self.intermediates.push(byte);
                self.state = State::CsiIntermediate;
                Action::None
            }
            0x40..=0x7E => {
                // Final character with no parameters
                let action = Action::CsiDispatch {
                    params: Vec::new(),
                    intermediates: self.intermediates.clone(),
                    final_char: byte,
                };
                self.reset();
                action
            }
            _ => {
                self.state = State::CsiIgnore;
                Action::None
            }
        }
    }

    /// Handle CSI parameter state
    fn handle_csi_param(&mut self, byte: u8) -> Action {
        match byte {
            b'0'..=b'9' => {
                self.current_param = self.current_param * 10 + (byte - b'0') as i32;
                self.param_started = true;
                Action::None
            }
            b';' | b':' => {
                // Parameter separator
                if self.param_started {
                    self.params.push(self.current_param);
                } else {
                    self.params.push(-1); // Default
                }
                self.current_param = 0;
                self.param_started = false;
                Action::None
            }
            0x20..=0x2F => {
                // Intermediate byte
                if self.param_started {
                    self.params.push(self.current_param);
                }
                self.intermediates.push(byte);
                self.state = State::CsiIntermediate;
                Action::None
            }
            0x40..=0x7E => {
                // Final character
                if self.param_started {
                    self.params.push(self.current_param);
                }
                let action = Action::CsiDispatch {
                    params: self.params.clone(),
                    intermediates: self.intermediates.clone(),
                    final_char: byte,
                };
                self.reset();
                action
            }
            _ => {
                self.state = State::CsiIgnore;
                Action::None
            }
        }
    }

    /// Handle CSI intermediate state
    fn handle_csi_intermediate(&mut self, byte: u8) -> Action {
        match byte {
            0x20..=0x2F => {
                self.intermediates.push(byte);
                Action::None
            }
            0x40..=0x7E => {
                // Final character
                let action = Action::CsiDispatch {
                    params: self.params.clone(),
                    intermediates: self.intermediates.clone(),
                    final_char: byte,
                };
                self.reset();
                action
            }
            _ => {
                self.state = State::CsiIgnore;
                Action::None
            }
        }
    }

    /// Handle CSI ignore state
    fn handle_csi_ignore(&mut self, byte: u8) -> Action {
        if byte >= 0x40 && byte <= 0x7E {
            // Final character - end ignore
            self.reset();
        }
        Action::None
    }

    /// Handle OSC string state
    fn handle_osc_string(&mut self, byte: u8) -> Action {
        match byte {
            0x07 => {
                // BEL terminates OSC
                let action = Action::OscDispatch(self.osc_string.clone());
                self.reset();
                action
            }
            0x1B => {
                // Might be ST (ESC \)
                let action = Action::OscDispatch(self.osc_string.clone());
                self.reset();
                action
            }
            0x9C => {
                // ST (String Terminator)
                let action = Action::OscDispatch(self.osc_string.clone());
                self.reset();
                action
            }
            _ => {
                // Add to OSC string (with limit)
                if self.osc_string.len() < 4096 {
                    self.osc_string.push(byte);
                }
                Action::None
            }
        }
    }

    /// Handle DCS entry state
    fn handle_dcs_entry(&mut self, byte: u8) -> Action {
        match byte {
            b'0'..=b'9' => {
                self.current_param = (byte - b'0') as i32;
                self.param_started = true;
                self.state = State::DcsParam;
                Action::None
            }
            b';' => {
                self.params.push(-1);
                self.state = State::DcsParam;
                Action::None
            }
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::DcsIntermediate;
                Action::None
            }
            0x40..=0x7E => {
                // Final character - start collecting data
                self.dcs_final = byte;
                self.state = State::DcsPassthrough;
                Action::None
            }
            _ => {
                self.state = State::DcsIgnore;
                Action::None
            }
        }
    }

    /// Handle DCS parameter state
    fn handle_dcs_param(&mut self, byte: u8) -> Action {
        match byte {
            b'0'..=b'9' => {
                self.current_param = self.current_param * 10 + (byte - b'0') as i32;
                self.param_started = true;
                Action::None
            }
            b';' => {
                if self.param_started {
                    self.params.push(self.current_param);
                } else {
                    self.params.push(-1);
                }
                self.current_param = 0;
                self.param_started = false;
                Action::None
            }
            0x20..=0x2F => {
                if self.param_started {
                    self.params.push(self.current_param);
                }
                self.intermediates.push(byte);
                self.state = State::DcsIntermediate;
                Action::None
            }
            0x40..=0x7E => {
                if self.param_started {
                    self.params.push(self.current_param);
                }
                self.dcs_final = byte;
                self.state = State::DcsPassthrough;
                Action::None
            }
            _ => {
                self.state = State::DcsIgnore;
                Action::None
            }
        }
    }

    /// Handle DCS intermediate state
    fn handle_dcs_intermediate(&mut self, byte: u8) -> Action {
        match byte {
            0x20..=0x2F => {
                self.intermediates.push(byte);
                Action::None
            }
            0x40..=0x7E => {
                self.dcs_final = byte;
                self.state = State::DcsPassthrough;
                Action::None
            }
            _ => {
                self.state = State::DcsIgnore;
                Action::None
            }
        }
    }

    /// Handle DCS passthrough state (collect data until ST)
    fn handle_dcs_passthrough(&mut self, byte: u8) -> Action {
        match byte {
            0x1B => {
                // Might be start of ST (ESC \)
                self.dcs_data.push(byte);
                Action::None
            }
            0x9C => {
                // ST (C1 form) - dispatch DCS
                let action = Action::DcsDispatch {
                    params: self.params.clone(),
                    intermediates: self.intermediates.clone(),
                    final_char: self.dcs_final,
                    data: self.dcs_data.clone(),
                };
                self.reset();
                action
            }
            _ => {
                // Check if previous byte was ESC and this is backslash (ST)
                if self.dcs_data.last() == Some(&0x1B) && byte == b'\\' {
                    self.dcs_data.pop(); // Remove the ESC
                    let action = Action::DcsDispatch {
                        params: self.params.clone(),
                        intermediates: self.intermediates.clone(),
                        final_char: self.dcs_final,
                        data: self.dcs_data.clone(),
                    };
                    self.reset();
                    action
                } else {
                    // Regular data byte - add to buffer (with limit)
                    if self.dcs_data.len() < 8192 {
                        self.dcs_data.push(byte);
                    }
                    Action::None
                }
            }
        }
    }

    /// Handle DCS ignore state
    fn handle_dcs_ignore(&mut self, byte: u8) -> Action {
        // Wait for ST or ESC
        if byte == 0x9C {
            self.reset();
        } else if byte == 0x1B {
            // Might be ST, wait for next byte
        }
        Action::None
    }

    /// Handle character set designation
    fn handle_designate(&mut self, byte: u8) -> Action {
        // Just consume the character set indicator and return to ground
        let _ = byte; // B = ASCII, 0 = DEC Graphics, etc.
        self.reset();
        Action::None
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}
