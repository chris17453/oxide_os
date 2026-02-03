//! VT100/ANSI Virtual Terminal Engine
//!
//! Portable VT100 parser and terminal state management.
//! No kernel dependencies - only `alloc` + `bitflags`.
//!
//! # Architecture
//!
//! ```text
//! Input bytes ──▶ Parser ──▶ Action ──▶ Handler ──▶ ScreenBuffer
//!                                         │
//!                                    Cell + CellAttrs
//! ```
//!
//! -- GraveShift: Extracted VTE core - the beating heart of terminal emulation

#![no_std]
#![allow(unused)]

extern crate alloc;

pub mod parser;
pub mod handler;
pub mod buffer;
pub mod cell;
pub mod color;
pub mod wcwidth;

// Re-export primary types for ergonomic access
pub use parser::{Parser, Action, State};
pub use handler::{Handler, TerminalModes, MouseMode, MouseEncoding, Charset, SavedCursor};
pub use buffer::{ScreenBuffer, ScrollbackBuffer};
pub use cell::{Cell, CellAttrs, CellFlags, Cursor, CursorShape};
pub use color::TermColor;
