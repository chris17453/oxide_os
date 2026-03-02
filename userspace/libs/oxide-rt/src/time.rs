//! Time syscall wrappers — because even chaos needs a clock.
//!
//! — WireSaint: CLOCK_MONOTONIC for durations, CLOCK_REALTIME for wall time.
//! Don't mix them up or your benchmarks will travel through time zones.

use crate::syscall::*;
use crate::nr;
use crate::types::{Timespec, Timeval};

/// Clock IDs
pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;

/// clock_gettime — get the time of a specified clock
pub fn clock_gettime(clock_id: i32, tp: &mut Timespec) -> i32 {
    syscall2(nr::CLOCK_GETTIME, clock_id as usize, tp as *mut Timespec as usize) as i32
}

/// gettimeofday — get time of day (wall clock)
pub fn gettimeofday(tv: &mut Timeval) -> i32 {
    syscall2(nr::GETTIMEOFDAY, tv as *mut Timeval as usize, 0) as i32
}

/// Convenience: get monotonic time as (seconds, nanoseconds)
pub fn monotonic_now() -> (i64, i64) {
    let mut ts = Timespec::zero();
    clock_gettime(CLOCK_MONOTONIC, &mut ts);
    (ts.tv_sec, ts.tv_nsec)
}

/// Convenience: get realtime as (seconds, nanoseconds)
pub fn realtime_now() -> (i64, i64) {
    let mut ts = Timespec::zero();
    clock_gettime(CLOCK_REALTIME, &mut ts);
    (ts.tv_sec, ts.tv_nsec)
}
