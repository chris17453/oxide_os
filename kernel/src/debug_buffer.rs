//! Non-reentrant debug output system
//!
//! BlackLatch: Prevent debug feedback loops with recursion guard
//!
//! Problem: Debug output with locks causes deadlock/slowdown when called from:
//! - Timer interrupts
//! - Nested contexts
//! - While already printing
//!
//! Solution: Atomic recursion guard - if already printing, skip the message

use core::sync::atomic::{AtomicUsize, Ordering};

/// Recursion guard - if we're already printing, don't recurse
static PRINTING: AtomicUsize = AtomicUsize::new(0);

/// Per-CPU recursion depth counter (simple approach: global for now)
static RECURSION_DEPTH: AtomicUsize = AtomicUsize::new(0);

/// Write bytes to debug output (with recursion guard)
///
/// If already printing (recursive call), the message is silently dropped.
/// This prevents the debug feedback loop while still allowing debug output.
pub fn write_debug(data: &[u8]) {
    // Increment recursion depth
    let depth = RECURSION_DEPTH.fetch_add(1, Ordering::Relaxed);

    // If we're nested more than 1 level deep, drop the message
    if depth > 0 {
        RECURSION_DEPTH.fetch_sub(1, Ordering::Relaxed);
        return;
    }

    // Try to acquire print lock (non-blocking)
    if PRINTING.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        // Someone else is printing - drop this message
        RECURSION_DEPTH.fetch_sub(1, Ordering::Relaxed);
        return;
    }

    // Write directly to serial (unsafe but we hold the logical lock)
    for &byte in data {
        unsafe {
            arch_x86_64::serial::write_byte_unsafe(byte);
        }
    }

    // Release print lock
    PRINTING.store(0, Ordering::Release);

    // Decrement recursion depth
    RECURSION_DEPTH.fetch_sub(1, Ordering::Relaxed);
}

/// Flush is a no-op now (we write directly)
pub fn flush_debug() {
    // No-op - we don't buffer anymore
}

/// Try flush is a no-op now (we write directly)
pub fn try_flush_debug() {
    // No-op - we don't buffer anymore
}
