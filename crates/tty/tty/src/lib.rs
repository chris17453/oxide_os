//! TTY subsystem for OXIDE OS
//!
//! Provides terminal device abstraction with line discipline support.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐
//! │  User Input │     │   Process   │
//! │  (keyboard) │     │  (shell)    │
//! └──────┬──────┘     └──────┬──────┘
//!        │                   │
//!        ▼                   ▼
//! ┌─────────────────────────────────┐
//! │         Line Discipline         │
//! │  - Echo                         │
//! │  - Line editing (^H, ^U, ^W)    │
//! │  - Signal generation (^C, ^Z)   │
//! └──────────────┬──────────────────┘
//!                │
//!                ▼
//! ┌─────────────────────────────────┐
//! │           TTY Driver            │
//! │  (serial, console, pty)         │
//! └─────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use tty::{Tty, CallbackDriver};
//!
//! // Create a driver that writes to serial
//! let driver = CallbackDriver::new(serial_write);
//!
//! // Create a TTY
//! let tty = Tty::new(driver, inode_num, device_num);
//!
//! // Process input from keyboard
//! if let Some(signal) = tty.input(b"hello\n") {
//!     // Handle signal (e.g., ^C -> SIGINT)
//! }
//!
//! // Read from TTY
//! let mut buf = [0u8; 256];
//! let n = tty.read(0, &mut buf)?;
//! ```

#![no_std]

extern crate alloc;

pub mod ldisc;
pub mod termios;
pub mod tty;
pub mod winsize;

pub use ldisc::{LineDiscipline, Signal};
pub use termios::{ControlFlags, InputFlags, LocalFlags, OutputFlags, Termios};
pub use termios::{TCGETS, TCSETS, TCSETSF, TCSETSW, TIOCGPGRP, TIOCGPTN, TIOCGWINSZ, TIOCSPGRP, TIOCSPTLCK, TIOCSWINSZ};
pub use tty::{CallbackDriver, Tty, TtyDriver};
pub use winsize::Winsize;
