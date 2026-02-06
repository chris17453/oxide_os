//! OXIDE Logging — Kernel logging infrastructure
//!
//! # Overview
//!
//! Two output paths cover every context a kernel subsystem runs in:
//!
//! | Path       | Macro family                       | Lock | Context          |
//! |------------|------------------------------------|------|------------------|
//! | **Normal** | `print!` `println!` `info!` …      | Yes  | Process / thread |
//! | **Unsafe** | `print_unsafe!` `println_unsafe!`  | No   | ISR / exception  |
//!
//! Subsystem crates depend on `os_log` instead of reaching into arch.
//! The kernel main crate registers both writer paths during early init.
//!
//! # Quick start
//!
//! ```ignore
//! // Process context — uses Mutex-protected writer
//! os_log::println!("[TTY] read {} bytes", n);
//! os_log::warn!("allocation failed for size={}", sz);
//!
//! // Interrupt / exception context — lock-free, direct hardware writes
//! os_log::println_unsafe!("[SCHED] ctx switch pid={}", pid);
//!
//! // Lowest-level byte writes (custom formatting in ISR)
//! unsafe { os_log::write_byte_raw(b'!'); }
//! unsafe { os_log::write_str_raw("[DONE]\n"); }
//! ```
//!
//! — GraveShift, OXIDE kernel logging infrastructure

#![no_std]

use core::fmt::{self, Write};
use spin::Mutex;

// ═══════════════════════════════════════════════════════════════════
//  NORMAL PATH — Mutex-protected, safe from process context
// ═══════════════════════════════════════════════════════════════════

/// Trait for serial output devices (normal locking path).
pub trait SerialWriter: Send {
    /// Write a single byte to the output device.
    fn write_byte(&mut self, byte: u8);

    /// Write a string slice. Default impl calls `write_byte` per byte.
    fn write_str(&mut self, s: &str) {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
    }
}

/// Global serial writer (Mutex-protected).
static WRITER: Mutex<Option<&'static mut dyn SerialWriter>> = Mutex::new(None);

/// — GraveShift: Console write callback — sends println! output to the terminal too.
/// Set once during init, read from process context. Without this, kernel boot messages
/// only go to serial and the user stares at a black screen for 3 seconds.
type ConsoleWriteFn = fn(&[u8]);
static CONSOLE_WRITER: Mutex<Option<ConsoleWriteFn>> = Mutex::new(None);

/// Register the normal (locking) serial writer.
///
/// # Safety
/// The writer must remain valid for the lifetime of the kernel.
/// Must be called during single-threaded init before SMP bringup.
pub unsafe fn register_writer(writer: &'static mut dyn SerialWriter) {
    *WRITER.lock() = Some(writer);
}

/// Register a console write callback for dual-output println!.
///
/// After calling this, `println!` writes to both serial AND the terminal.
/// — GraveShift: The user deserves to see boot messages, not a black void.
pub fn register_console_writer(writer: ConsoleWriteFn) {
    *CONSOLE_WRITER.lock() = Some(writer);
}

/// Internal writer wrapper for `fmt::Write`.
struct LogWriter;

impl fmt::Write for LogWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.write_str(s);
        }
        // — GraveShift: Also echo to terminal so boot messages appear on screen.
        if let Some(console_write) = *CONSOLE_WRITER.lock() {
            console_write(s.as_bytes());
        }
        Ok(())
    }
}

/// Print formatted output through the normal (locking) writer.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _ = LogWriter.write_fmt(args);
}

// ═══════════════════════════════════════════════════════════════════
//  ISR-SAFE PATH — lock-free, direct hardware writes
// ═══════════════════════════════════════════════════════════════════

/// Function pointer: write a single byte without locking.
type UnsafeWriteByteFn = unsafe fn(u8);
/// Function pointer: write a string slice without locking.
type UnsafeWriteStrFn = unsafe fn(&str);

/// Lock-free byte writer — set once during init, read from any context.
static mut UNSAFE_WRITE_BYTE: Option<UnsafeWriteByteFn> = None;
/// Lock-free string writer — set once during init, read from any context.
static mut UNSAFE_WRITE_STR: Option<UnsafeWriteStrFn> = None;

/// Register lock-free write functions for ISR-safe output.
///
/// The provided functions must write directly to hardware (e.g. UART
/// port I/O) without acquiring any locks. Output from multiple CPUs
/// may interleave — this is acceptable for debug output where the
/// alternative is deadlock.
///
/// # Safety
/// - Must be called during single-threaded init before any interrupts.
/// - The functions must be callable from *any* context (ISR, NMI,
///   exception handlers) without acquiring locks or allocating.
pub unsafe fn register_unsafe_writer(write_byte: UnsafeWriteByteFn, write_str: UnsafeWriteStrFn) {
    unsafe {
        UNSAFE_WRITE_BYTE = Some(write_byte);
        UNSAFE_WRITE_STR = Some(write_str);
    }
}

/// ISR-safe writer wrapper for `fmt::Write`. Bypasses all locks.
struct UnsafeLogWriter;

impl fmt::Write for UnsafeLogWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            if let Some(write_fn) = UNSAFE_WRITE_STR {
                write_fn(s);
            }
        }
        Ok(())
    }
}

/// Print formatted output through the lock-free (ISR-safe) writer.
///
/// # Safety
/// Caller must ensure this is appropriate for the current context.
/// Output may interleave with other CPUs. No locks are taken.
#[doc(hidden)]
pub unsafe fn _print_unsafe(args: fmt::Arguments) {
    let _ = UnsafeLogWriter.write_fmt(args);
}

/// Write a single byte through the lock-free path.
///
/// Use for custom formatting in ISR context where `format_args!`
/// is too heavy or when building output byte-by-byte.
///
/// # Safety
/// Same constraints as `_print_unsafe`.
#[inline]
pub unsafe fn write_byte_raw(byte: u8) {
    unsafe {
        if let Some(write_fn) = UNSAFE_WRITE_BYTE {
            write_fn(byte);
        }
    }
}

/// Write a string slice through the lock-free path.
///
/// # Safety
/// Same constraints as `_print_unsafe`.
#[inline]
pub unsafe fn write_str_raw(s: &str) {
    unsafe {
        if let Some(write_fn) = UNSAFE_WRITE_STR {
            write_fn(s);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  MACROS — normal (locking) path
// ═══════════════════════════════════════════════════════════════════

/// Print to serial without newline (normal locking path).
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(format_args!($($arg)*))
    };
}

/// Print to serial with newline (normal locking path).
#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*))
    };
}

/// Log at trace level — most verbose, for hot-path debug output.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::println!("[TRACE] {}", format_args!($($arg)*))
    };
}

/// Log at debug level.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::println!("[DEBUG] {}", format_args!($($arg)*))
    };
}

/// Log at info level.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::println!("[INFO] {}", format_args!($($arg)*))
    };
}

/// Log at warning level.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::println!("[WARN] {}", format_args!($($arg)*))
    };
}

/// Log at error level.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::println!("[ERROR] {}", format_args!($($arg)*))
    };
}

// ═══════════════════════════════════════════════════════════════════
//  MACROS — ISR-safe (lock-free) path
// ═══════════════════════════════════════════════════════════════════

/// Print to serial without newline — ISR-safe, no locks.
///
/// Use in interrupt handlers, exception handlers, and any context
/// where acquiring a Mutex would risk deadlock.
#[macro_export]
macro_rules! print_unsafe {
    ($($arg:tt)*) => {
        unsafe { $crate::_print_unsafe(format_args!($($arg)*)) }
    };
}

/// Print to serial with newline — ISR-safe, no locks.
#[macro_export]
macro_rules! println_unsafe {
    () => {
        $crate::print_unsafe!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print_unsafe!("{}\n", format_args!($($arg)*))
    };
}
