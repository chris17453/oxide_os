//! TTY subsystem for EFFLUX OS
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
//! use efflux_tty::{Tty, CallbackDriver};
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

pub mod termios;
pub mod winsize;
pub mod ldisc;
pub mod tty;

pub use termios::{Termios, InputFlags, OutputFlags, ControlFlags, LocalFlags};
pub use termios::{TCGETS, TCSETS, TCSETSW, TCSETSF, TIOCGWINSZ, TIOCSWINSZ, TIOCGPGRP, TIOCSPGRP};
pub use winsize::Winsize;
pub use ldisc::{LineDiscipline, Signal};
pub use tty::{Tty, TtyDriver, CallbackDriver};
