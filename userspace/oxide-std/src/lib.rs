//! OXIDE Standard Library Compatibility Layer
//!
//! Provides `std`-like APIs for OXIDE OS userspace applications.
//! This allows porting applications that use Rust's standard library
//! with minimal changes.
//!
//! # Usage
//!
//! Instead of `use std::io::Write;` use `use oxide_std::io::Write;`
//!
//! # Modules
//!
//! - `io` - Input/output traits and types (Read, Write, stdin, stdout, etc.)
//! - `fs` - Filesystem operations (File, read_to_string, etc.)
//! - `env` - Environment and command line arguments
//! - `collections` - HashMap and other collections
//! - `string` - String type (re-exported from alloc)
//! - `vec` - Vec type (re-exported from alloc)

#![no_std]

extern crate alloc;

pub mod collections;
pub mod env;
pub mod fs;
pub mod io;
pub mod process;
pub mod sync;
pub mod thread;

// Re-export common types from alloc
pub mod string {
    pub use alloc::string::{String, ToString};
}

pub mod vec {
    pub use alloc::vec::Vec;
}

pub mod boxed {
    pub use alloc::boxed::Box;
}

pub mod fmt {
    pub use core::fmt::*;
}

// Prelude - commonly used items
pub mod prelude {
    pub use crate::boxed::Box;
    pub use crate::io::{BufRead, Read, Write};
    pub use crate::string::{String, ToString};
    pub use crate::vec::Vec;
    pub use alloc::format;
}

// Re-export alloc's vec! macro at crate level
#[macro_export]
macro_rules! vec {
    () => { alloc::vec::Vec::new() };
    ($elem:expr; $n:expr) => { alloc::vec![$elem; $n] };
    ($($x:expr),+ $(,)?) => { alloc::vec![$($x),+] };
}

/// Print to stdout
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut stdout = $crate::io::stdout();
        let _ = write!(stdout, $($arg)*);
    }};
}

/// Print to stdout with newline
#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut stdout = $crate::io::stdout();
        let _ = writeln!(stdout, $($arg)*);
    }};
}

/// Print to stderr
#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut stderr = $crate::io::stderr();
        let _ = write!(stderr, $($arg)*);
    }};
}

/// Print to stderr with newline
#[macro_export]
macro_rules! eprintln {
    () => { $crate::eprint!("\n") };
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut stderr = $crate::io::stderr();
        let _ = writeln!(stderr, $($arg)*);
    }};
}
