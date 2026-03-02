//! Command-line argument storage — static argc/argv set by _start.
//!
//! — ThreadRogue: std::env::args() needs to find these somewhere.
//! _start shoves them in here before calling lang_start.
//! Thread-safe? It's set once at program start. That's safe enough.

use core::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

static ARGC: AtomicIsize = AtomicIsize::new(0);
static ARGV: AtomicUsize = AtomicUsize::new(0);

/// Called by _start to store argc/argv before std initialization
pub fn set_args(argc: isize, argv: *const *const u8) {
    ARGC.store(argc, Ordering::Release);
    ARGV.store(argv as usize, Ordering::Release);
}

/// Get the argument count
pub fn argc() -> i32 {
    ARGC.load(Ordering::Acquire) as i32
}

/// Get the argument vector pointer
pub fn argv() -> *const *const u8 {
    ARGV.load(Ordering::Acquire) as *const *const u8
}

/// Get argument at index as a byte slice (returns None if out of bounds or null)
pub fn arg(index: usize) -> Option<&'static [u8]> {
    let argc = argc() as usize;
    let argv = argv();
    if index >= argc || argv.is_null() {
        return None;
    }
    unsafe {
        let ptr = *argv.add(index);
        if ptr.is_null() {
            return None;
        }
        // — ThreadRogue: OXIDE passes (ptr, len) pairs on the stack for argv,
        // but the ELF ABI stores null-terminated C strings. We handle both
        // by scanning for null terminator.
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        Some(core::slice::from_raw_parts(ptr, len))
    }
}
