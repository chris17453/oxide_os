//! VT100/ANSI escape sequence parser
//!
//! Implements a state machine for parsing escape sequences.

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
            // Printable ASCII
            Action::Print(byte as char)
        } else if byte >= 0x80 {
            // Could be UTF-8 or C1 control
            // For now, try to print extended ASCII
            if byte >= 0xC0 {
                // Start of UTF-8 sequence - simplified handling
                Action::Print(byte as char)
            } else {
                Action::None
            }
        } else {
            Action::None
        }
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
                // For now, just end the OSC
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

    /// Handle DCS entry state (not fully implemented)
    fn handle_dcs_entry(&mut self, byte: u8) -> Action {
        // For now, just wait for ST
        if byte == 0x1B || byte == 0x9C {
            self.reset();
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
