//! OXIDE Logging - Kernel logging infrastructure
//!
//! Provides macros for logging at different levels.
//! Output is directed to the registered serial writer.

#![no_std]

use core::fmt::{self, Write};
use spin::Mutex;

/// Global serial writer
static WRITER: Mutex<Option<&'static mut dyn SerialWriter>> = Mutex::new(None);

/// Trait for serial output devices
pub trait SerialWriter: Send {
    fn write_byte(&mut self, byte: u8);

    fn write_str(&mut self, s: &str) {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
    }
}

/// Register a serial writer for log output
///
/// # Safety
/// The writer must remain valid for the lifetime of the kernel.
pub unsafe fn register_writer(writer: &'static mut dyn SerialWriter) {
    *WRITER.lock() = Some(writer);
}

/// Internal writer wrapper for fmt::Write
struct LogWriter;

impl fmt::Write for LogWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.write_str(s);
        }
        Ok(())
    }
}

/// Print to the serial console
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _ = LogWriter.write_fmt(args);
}

/// Print to serial without newline
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(format_args!($($arg)*))
    };
}

/// Print to serial with newline
#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*))
    };
}

/// Log at info level
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::println!("[INFO] {}", format_args!($($arg)*))
    };
}

/// Log at debug level
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::println!("[DEBUG] {}", format_args!($($arg)*))
    };
}

/// Log at warning level
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::println!("[WARN] {}", format_args!($($arg)*))
    };
}

/// Log at error level
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::println!("[ERROR] {}", format_args!($($arg)*))
    };
}
