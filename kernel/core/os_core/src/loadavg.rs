//! Load average tracking — Linux-compatible exponential moving average.
//!
//! — StackTrace: three averages (1min, 5min, 15min) updated every tick.
//! Uses fixed-point arithmetic (×2048) to avoid floating point in kernel.
//! The decay constants match Linux's `calc_load()` exactly.

use core::sync::atomic::{AtomicU64, Ordering};

/// Fixed-point scale factor (11 bits of fraction)
const FSHIFT: u32 = 11;
const FIXED_1: u64 = 1 << FSHIFT; // 2048

/// Exponential decay factors for 1/5/15 minute averages.
/// These are `e^(-tick_interval/period)` scaled by FIXED_1.
/// With 100Hz ticks (10ms each):
///   1 min: e^(-1/6000)  ≈ 0.99983 → 1884/2048 per 5-second sample (Linux uses 5s sampling)
///   5 min: e^(-1/30000) ≈ 0.99997 → 2014/2048
///  15 min: e^(-1/90000) ≈ 0.99999 → 2037/2048
///
/// — StackTrace: we sample at every tick (10ms) not every 5s like Linux.
/// Recalculated: e^(-0.01/60) ≈ 0.999833 → 2047.66/2048
/// But that converges too slowly. Use Linux's 5-second constants and
/// sample every 500 ticks (5 seconds) instead — proven to work.
const EXP_1: u64 = 1884;
const EXP_5: u64 = 2014;
const EXP_15: u64 = 2037;

/// Tick interval for load average sampling (every 500 ticks = 5 seconds at 100Hz)
const LOAD_FREQ: u64 = 500;

/// Load averages stored as fixed-point × FIXED_1
static AVENRUN: [AtomicU64; 3] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// Tick counter for sampling interval
static LOAD_TICK: AtomicU64 = AtomicU64::new(0);

/// Calculate exponential moving average:
/// `load = load * exp + active * (FIXED_1 - exp)`
fn calc_load(load: u64, exp: u64, active: u64) -> u64 {
    let active_fixed = active * FIXED_1;
    if active_fixed > load {
        load + (active_fixed - load) * (FIXED_1 - exp) / FIXED_1
    } else {
        load - (load - active_fixed) * (FIXED_1 - exp) / FIXED_1
    }
}

/// Called from scheduler tick with the count of runnable tasks.
/// — StackTrace: only updates every LOAD_FREQ ticks (5 seconds).
pub fn update(nr_active: u64) {
    let tick = LOAD_TICK.fetch_add(1, Ordering::Relaxed);
    if tick % LOAD_FREQ != 0 {
        return;
    }

    for (i, exp) in [EXP_1, EXP_5, EXP_15].iter().enumerate() {
        let old = AVENRUN[i].load(Ordering::Relaxed);
        let new = calc_load(old, *exp, nr_active);
        AVENRUN[i].store(new, Ordering::Relaxed);
    }
}

/// Get load averages as (load1, load5, load15) in fixed-point × FIXED_1.
pub fn get_raw() -> (u64, u64, u64) {
    (
        AVENRUN[0].load(Ordering::Relaxed),
        AVENRUN[1].load(Ordering::Relaxed),
        AVENRUN[2].load(Ordering::Relaxed),
    )
}

/// Get load averages as (integer_part, frac_hundredths) for each.
/// Returns [(int, frac); 3] suitable for formatting as "X.XX".
pub fn get_formatted() -> [(u64, u64); 3] {
    let mut result = [(0u64, 0u64); 3];
    for i in 0..3 {
        let val = AVENRUN[i].load(Ordering::Relaxed);
        let integer = val >> FSHIFT;
        let frac = (val & (FIXED_1 - 1)) * 100 / FIXED_1;
        result[i] = (integer, frac);
    }
    result
}
