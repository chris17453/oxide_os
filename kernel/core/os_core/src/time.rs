//! Kernel wall-clock time bridge
//!
//! Provides a decoupled time source for subsystems that need wall-clock
//! seconds (e.g. filesystem timestamps) without pulling in arch or syscall
//! crate dependencies. The kernel registers a concrete provider at boot;
//! callers just invoke `wall_clock_secs()`.
//!
//! — WireSaint: time bridge between silicon and storage

use core::sync::atomic::{AtomicPtr, Ordering};

/// Wall-clock provider: returns seconds since Unix epoch
type WallClockFn = fn() -> u64;

/// Fallback: returns 0 when no provider is registered
fn no_clock() -> u64 {
    0
}

/// Global wall-clock function pointer
static WALL_CLOCK: AtomicPtr<()> = AtomicPtr::new(no_clock as *mut ());

/// Register the wall-clock provider (called once at boot from init.rs)
///
/// # Safety
/// `f` must be a valid function pointer that remains valid for 'static lifetime.
pub fn register_wall_clock(f: WallClockFn) {
    WALL_CLOCK.store(f as *mut (), Ordering::Release);
}

/// Get current wall-clock time in seconds since Unix epoch.
///
/// Returns 0 if no provider has been registered yet.
pub fn wall_clock_secs() -> u64 {
    let ptr = WALL_CLOCK.load(Ordering::Acquire);
    // Safety: we only store valid fn pointers (no_clock or a registered provider)
    let f: WallClockFn = unsafe { core::mem::transmute(ptr) };
    f()
}
