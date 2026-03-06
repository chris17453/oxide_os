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

// — WireSaint: monotonic tick source — arch code registers the real thing at
// boot, everyone else just calls now_ns() and pretends clocks aren't hard.

/// Tick source provider: returns hardware ticks since boot
type TickSourceFn = fn() -> u64;

/// Fallback: returns 0 when no tick source registered
fn no_ticks() -> u64 {
    0
}

/// Global tick source function pointer
static TICK_SOURCE: AtomicPtr<()> = AtomicPtr::new(no_ticks as *mut ());

/// Nanoseconds per tick — set by register_tick_source
static NS_PER_TICK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(10_000_000);

/// Register the monotonic tick source (called once at boot from arch init)
///
/// `f` — function returning hardware ticks since boot
/// `ns_per_tick` — nanoseconds per tick (e.g. 10_000_000 for 100Hz PIT)
pub fn register_tick_source(f: TickSourceFn, ns_per_tick: u64) {
    TICK_SOURCE.store(f as *mut (), Ordering::Release);
    NS_PER_TICK.store(ns_per_tick, Ordering::Release);
}

/// Get current monotonic time in nanoseconds since boot.
///
/// Returns 0 if no tick source has been registered yet.
/// — WireSaint: the one true time function for arch-independent code.
pub fn now_ns() -> u64 {
    let ptr = TICK_SOURCE.load(Ordering::Acquire);
    let f: TickSourceFn = unsafe { core::mem::transmute(ptr) };
    let ticks = f();
    let ns = NS_PER_TICK.load(Ordering::Acquire);
    ticks.saturating_mul(ns)
}

/// Get raw hardware tick count from the registered tick source.
///
/// — WireSaint: for subsystems that need tick-level granularity (poll/select
/// deadline comparison) without the ns multiplication overhead. Same source
/// as now_ns(), just without the unit conversion. Returns 0 before registration.
pub fn ticks() -> u64 {
    let ptr = TICK_SOURCE.load(Ordering::Acquire);
    let f: TickSourceFn = unsafe { core::mem::transmute(ptr) };
    f()
}

/// Get the configured nanoseconds-per-tick value.
///
/// — WireSaint: needed by subsystems that convert between tick and ns domains
/// (e.g. poll timeout calculation). Returns 10_000_000 (100Hz default) before
/// registration.
pub fn ns_per_tick() -> u64 {
    NS_PER_TICK.load(Ordering::Acquire)
}

// ============================================================================
// High-resolution monotonic time
//
// — WireSaint: TSC-backed nanosecond-precision time for clock_gettime and
// friends. The 100Hz tick source gives 10ms granularity — fine for timeouts,
// terrible for benchmarks. The hires provider gives cycle-accurate resolution
// without arch dependencies leaking into every syscall.
// ============================================================================

/// High-res time provider: returns (seconds_since_boot, nanoseconds_remainder)
type HiresTimeFn = fn() -> (u64, u64);

/// Fallback: derives from tick source (10ms granularity)
fn no_hires() -> (u64, u64) {
    let ns = now_ns();
    (ns / 1_000_000_000, ns % 1_000_000_000)
}

/// Global high-res monotonic time function pointer
static HIRES_MONOTONIC: AtomicPtr<()> = AtomicPtr::new(no_hires as *mut ());

/// Register the high-resolution monotonic time provider (called once at boot).
///
/// `f` must return (seconds_since_boot, nanoseconds_remainder) with sub-tick
/// precision (e.g. TSC-derived).
pub fn register_hires_monotonic(f: HiresTimeFn) {
    HIRES_MONOTONIC.store(f as *mut (), Ordering::Release);
}

/// Get monotonic time with nanosecond precision as (seconds, nanoseconds).
///
/// — WireSaint: sub-tick precision when a TSC-backed provider is registered.
/// Falls back to tick-derived time (10ms granularity) during early boot.
pub fn monotonic_secs_ns() -> (u64, u64) {
    let ptr = HIRES_MONOTONIC.load(Ordering::Acquire);
    let f: HiresTimeFn = unsafe { core::mem::transmute(ptr) };
    f()
}

/// High-res wall-clock provider: returns (epoch_seconds, nanoseconds_remainder)
type HiresWallClockFn = fn() -> (u64, u64);

/// Fallback: derives from wall_clock_secs (1s granularity, no sub-second)
fn no_hires_wall() -> (u64, u64) {
    (wall_clock_secs(), 0)
}

/// Global high-res wall-clock function pointer
static HIRES_WALL_CLOCK: AtomicPtr<()> = AtomicPtr::new(no_hires_wall as *mut ());

/// Register the high-resolution wall-clock provider (called once at boot).
///
/// `f` must return (seconds_since_epoch, nanoseconds_remainder).
pub fn register_hires_wall_clock(f: HiresWallClockFn) {
    HIRES_WALL_CLOCK.store(f as *mut (), Ordering::Release);
}

/// Get wall-clock time with nanosecond precision as (epoch_seconds, nanoseconds).
///
/// — WireSaint: for clock_gettime(CLOCK_REALTIME) without arch coupling.
pub fn realtime_secs_ns() -> (u64, u64) {
    let ptr = HIRES_WALL_CLOCK.load(Ordering::Acquire);
    let f: HiresWallClockFn = unsafe { core::mem::transmute(ptr) };
    f()
}
