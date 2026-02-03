//! # Termcap Library for OXIDE OS
//!
//! Full-featured terminal capability library implementing both termcap and terminfo APIs.
//! This provides low-level terminal control for ncurses and other TUI applications.
//!
//! ## Features
//! - Complete termcap database with built-in terminal definitions
//! - Terminfo binary format support
//! - Terminal capability string parsing and expansion
//! - Parameter substitution (tgoto/tparm)
//! - Delay padding support (tputs)
//! - C-compatible API for linking with external programs
//!
//! ## Architecture
//! ```text
//! ┌─────────────────────────────────────┐
//! │  Application (ncurses, vim, etc.)   │
//! └──────────────┬──────────────────────┘
//!                │
//!      ┌─────────┴─────────┐
//!      │  Termcap API      │
//!      │  (tgetent, etc.)  │
//!      └─────────┬─────────┘
//!                │
//!      ┌─────────┴─────────┐
//!      │  Capability DB    │
//!      │  (xterm, vt100)   │
//!      └─────────┬─────────┘
//!                │
//!      ┌─────────┴─────────┐
//!      │  TTY/Terminal     │
//!      └───────────────────┘
//! ```
//!
//! -- GraveShift: Core terminal abstraction layer, the foundation of all TUI ops

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use core::prelude::v1::*;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub mod capabilities;
pub mod database;
pub mod parser;
pub mod expand;
pub mod c_api;

#[cfg(feature = "terminfo")]
pub mod terminfo;

/// Terminal capability entry
#[derive(Debug, Clone)]
pub struct TerminalEntry {
    /// Terminal name
    pub name: String,
    /// Terminal aliases
    pub aliases: Vec<String>,
    /// String capabilities (escape sequences)
    pub strings: BTreeMap<String, String>,
    /// Numeric capabilities
    pub numbers: BTreeMap<String, i32>,
    /// Boolean capabilities
    pub bools: BTreeMap<String, bool>,
}

impl TerminalEntry {
    /// Create a new empty terminal entry
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            aliases: Vec::new(),
            strings: BTreeMap::new(),
            numbers: BTreeMap::new(),
            bools: BTreeMap::new(),
        }
    }

    /// Get a string capability
    pub fn get_string(&self, cap: &str) -> Option<&str> {
        self.strings.get(cap).map(|s| s.as_str())
    }

    /// Get a numeric capability
    pub fn get_number(&self, cap: &str) -> Option<i32> {
        self.numbers.get(cap).copied()
    }

    /// Get a boolean capability
    pub fn get_flag(&self, cap: &str) -> bool {
        self.bools.get(cap).copied().unwrap_or(false)
    }

    /// Set a string capability
    pub fn set_string(&mut self, cap: &str, value: &str) {
        self.strings.insert(cap.to_string(), value.to_string());
    }

    /// Set a numeric capability
    pub fn set_number(&mut self, cap: &str, value: i32) {
        self.numbers.insert(cap.to_string(), value);
    }

    /// Set a boolean capability
    pub fn set_flag(&mut self, cap: &str, value: bool) {
        self.bools.insert(cap.to_string(), value);
    }
}

/// Global terminal state
static mut CURRENT_TERMINAL: Option<TerminalEntry> = None;

/// Load a terminal entry by name
pub fn load_terminal(name: &str) -> Result<TerminalEntry, &'static str> {
    database::get_terminal(name).ok_or("Terminal not found")
}

/// Get the current terminal entry
pub fn current_terminal() -> Option<&'static TerminalEntry> {
    unsafe { CURRENT_TERMINAL.as_ref() }
}

/// Set the current terminal
pub fn set_current_terminal(entry: TerminalEntry) {
    unsafe {
        CURRENT_TERMINAL = Some(entry);
    }
}

/// Error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Terminal not found in database
    TerminalNotFound,
    /// Invalid capability name
    InvalidCapability,
    /// Parse error
    ParseError,
    /// Parameter expansion error
    ExpansionError,
}

/// Result type for termcap operations
pub type Result<T> = core::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_entry_creation() {
        let entry = TerminalEntry::new("xterm");
        assert_eq!(entry.name, "xterm");
        assert!(entry.strings.is_empty());
        assert!(entry.numbers.is_empty());
        assert!(entry.bools.is_empty());
    }

    #[test]
    fn test_capability_storage() {
        let mut entry = TerminalEntry::new("test");
        
        entry.set_string("clear", "\x1b[H\x1b[2J");
        entry.set_number("cols", 80);
        entry.set_flag("am", true);
        
        assert_eq!(entry.get_string("clear"), Some("\x1b[H\x1b[2J"));
        assert_eq!(entry.get_number("cols"), Some(80));
        assert!(entry.get_flag("am"));
        assert!(!entry.get_flag("nonexistent"));
    }
}
