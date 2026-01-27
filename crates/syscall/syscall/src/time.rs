//! Time-related system calls
//!
//! Provides CLOCK_GETTIME, GETTIMEOFDAY, NANOSLEEP, etc.

use crate::errno;
use crate::with_current_meta;
use arch_x86_64 as arch;
use core::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use sched::TaskState;

/// Timer frequency in Hz (ticks per second)
const TIMER_HZ: u64 = 100;

/// Nanoseconds per tick (at 100 Hz, each tick is 10ms = 10,000,000 ns)
const NS_PER_TICK: u64 = 1_000_000_000 / TIMER_HZ;

/// Boot time in seconds since Unix epoch (can be set during init)
/// Default to Jan 1, 2024 00:00:00 UTC if not set
static BOOT_TIME_SECS: AtomicU64 = AtomicU64::new(1704067200);

// ============================================================================
// Sleep Queue - Timer-based wakeup for nanosleep
// ============================================================================

/// Maximum number of concurrent sleepers
const MAX_SLEEPERS: usize = 64;

/// A sleeping task entry
struct Sleeper {
    pid: AtomicU32,      // PID of sleeping task (0 = empty slot)
    wake_tick: AtomicU64, // Tick count at which to wake
}

impl Sleeper {
    const fn empty() -> Self {
        Self {
            pid: AtomicU32::new(0),
            wake_tick: AtomicU64::new(0),
        }
    }
}

/// Global sleep queue - checked by timer interrupt each tick
static SLEEP_QUEUE: [Sleeper; MAX_SLEEPERS] = [const { Sleeper::empty() }; MAX_SLEEPERS];

/// Register a task in the sleep queue
fn sleep_queue_add(pid: u32, wake_tick: u64) -> bool {
    for slot in &SLEEP_QUEUE {
        // Try to claim an empty slot (pid == 0)
        if slot.pid.compare_exchange(0, pid, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
            slot.wake_tick.store(wake_tick, Ordering::Release);
            return true;
        }
    }
    false // Queue full
}

/// Remove a task from the sleep queue (e.g., on signal interrupt)
fn sleep_queue_remove(pid: u32) {
    for slot in &SLEEP_QUEUE {
        if slot.pid.load(Ordering::Acquire) == pid {
            slot.pid.store(0, Ordering::Release);
            return;
        }
    }
}

/// Check sleep queue and wake expired sleepers.
/// Called from timer interrupt at 100Hz.
pub fn check_sleepers() {
    let now = get_ticks();
    for slot in &SLEEP_QUEUE {
        let pid = slot.pid.load(Ordering::Acquire);
        if pid != 0 {
            let wake_tick = slot.wake_tick.load(Ordering::Acquire);
            if now >= wake_tick {
                // Clear the slot first, then wake
                if slot.pid.compare_exchange(pid, 0, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                    sched::wake_up(pid);
                }
            }
        }
    }
}

/// Clock IDs (POSIX)
pub mod clock {
    pub const REALTIME: i32 = 0;
    pub const MONOTONIC: i32 = 1;
    pub const PROCESS_CPUTIME_ID: i32 = 2;
    pub const THREAD_CPUTIME_ID: i32 = 3;
    pub const MONOTONIC_RAW: i32 = 4;
    pub const REALTIME_COARSE: i32 = 5;
    pub const MONOTONIC_COARSE: i32 = 6;
    pub const BOOTTIME: i32 = 7;
}

/// Timespec structure (matches Linux)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// Timeval structure (matches Linux)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

/// Timezone structure (mostly unused, but needed for compatibility)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timezone {
    pub tz_minuteswest: i32,
    pub tz_dsttime: i32,
}

/// Set the boot time (call during kernel init if RTC is available)
pub fn set_boot_time(secs_since_epoch: u64) {
    BOOT_TIME_SECS.store(secs_since_epoch, Ordering::SeqCst);
}

/// Get current timer ticks
fn get_ticks() -> u64 {
    arch::timer_ticks()
}

/// Convert ticks to timespec (monotonic time since boot)
fn ticks_to_timespec(ticks: u64) -> Timespec {
    let total_ns = ticks * NS_PER_TICK;
    Timespec {
        tv_sec: (total_ns / 1_000_000_000) as i64,
        tv_nsec: (total_ns % 1_000_000_000) as i64,
    }
}

/// Get monotonic time (time since boot)
fn get_monotonic_time() -> Timespec {
    ticks_to_timespec(get_ticks())
}

/// Get real (wall clock) time
fn get_realtime() -> Timespec {
    let ticks = get_ticks();
    let uptime_ns = ticks * NS_PER_TICK;
    let boot_secs = BOOT_TIME_SECS.load(Ordering::SeqCst);

    let total_ns = (boot_secs * 1_000_000_000) + uptime_ns;
    Timespec {
        tv_sec: (total_ns / 1_000_000_000) as i64,
        tv_nsec: (total_ns % 1_000_000_000) as i64,
    }
}

/// sys_clock_gettime - get time from specified clock
///
/// # Arguments
/// * `clock_id` - Which clock to query
/// * `tp_ptr` - Pointer to userspace timespec to fill
pub fn sys_clock_gettime(clock_id: i32, tp_ptr: usize) -> i64 {
    if tp_ptr == 0 {
        return errno::EFAULT;
    }

    let ts = match clock_id {
        clock::REALTIME | clock::REALTIME_COARSE => get_realtime(),
        clock::MONOTONIC | clock::MONOTONIC_RAW | clock::MONOTONIC_COARSE | clock::BOOTTIME => {
            get_monotonic_time()
        }
        clock::PROCESS_CPUTIME_ID | clock::THREAD_CPUTIME_ID => {
            // TODO: Track per-process/thread CPU time
            // For now, return monotonic time
            get_monotonic_time()
        }
        _ => return errno::EINVAL,
    };

    // Write to userspace
    unsafe {
        // Enable SMAP access
        core::arch::asm!("stac", options(nomem, nostack));

        let tp = tp_ptr as *mut Timespec;
        core::ptr::write_volatile(tp, ts);

        // Disable SMAP access
        core::arch::asm!("clac", options(nomem, nostack));
    }

    0
}

/// sys_clock_getres - get resolution of specified clock
///
/// # Arguments
/// * `clock_id` - Which clock to query
/// * `res_ptr` - Pointer to userspace timespec to fill with resolution
pub fn sys_clock_getres(clock_id: i32, res_ptr: usize) -> i64 {
    // Validate clock ID
    match clock_id {
        clock::REALTIME
        | clock::MONOTONIC
        | clock::MONOTONIC_RAW
        | clock::REALTIME_COARSE
        | clock::MONOTONIC_COARSE
        | clock::BOOTTIME
        | clock::PROCESS_CPUTIME_ID
        | clock::THREAD_CPUTIME_ID => {}
        _ => return errno::EINVAL,
    }

    // If res_ptr is NULL, just validate the clock_id
    if res_ptr == 0 {
        return 0;
    }

    // Our resolution is 10ms (100 Hz timer)
    let res = Timespec {
        tv_sec: 0,
        tv_nsec: NS_PER_TICK as i64,
    };

    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let rp = res_ptr as *mut Timespec;
        core::ptr::write_volatile(rp, res);
        core::arch::asm!("clac", options(nomem, nostack));
    }

    0
}

/// sys_gettimeofday - get current time (legacy interface)
///
/// # Arguments
/// * `tv_ptr` - Pointer to userspace timeval to fill
/// * `tz_ptr` - Pointer to timezone (usually NULL, deprecated)
pub fn sys_gettimeofday(tv_ptr: usize, tz_ptr: usize) -> i64 {
    if tv_ptr != 0 {
        let ts = get_realtime();
        let tv = Timeval {
            tv_sec: ts.tv_sec,
            tv_usec: ts.tv_nsec / 1000, // Convert ns to us
        };

        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            let tvp = tv_ptr as *mut Timeval;
            core::ptr::write_volatile(tvp, tv);
            core::arch::asm!("clac", options(nomem, nostack));
        }
    }

    if tz_ptr != 0 {
        // Return UTC (no timezone offset)
        let tz = Timezone {
            tz_minuteswest: 0,
            tz_dsttime: 0,
        };

        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            let tzp = tz_ptr as *mut Timezone;
            core::ptr::write_volatile(tzp, tz);
            core::arch::asm!("clac", options(nomem, nostack));
        }
    }

    0
}

/// sys_nanosleep - sleep for specified time
///
/// # Arguments
/// * `req_ptr` - Pointer to requested sleep time
/// * `rem_ptr` - Pointer to store remaining time if interrupted (can be NULL)
pub fn sys_nanosleep(req_ptr: usize, rem_ptr: usize) -> i64 {
    if req_ptr == 0 {
        return errno::EFAULT;
    }

    // Read requested time from userspace
    let req: Timespec = unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let rp = req_ptr as *const Timespec;
        let val = core::ptr::read_volatile(rp);
        core::arch::asm!("clac", options(nomem, nostack));
        val
    };

    // Validate
    if req.tv_sec < 0 || req.tv_nsec < 0 || req.tv_nsec >= 1_000_000_000 {
        return errno::EINVAL;
    }

    // Calculate wake time in ticks
    let sleep_ns = (req.tv_sec as u64 * 1_000_000_000) + (req.tv_nsec as u64);
    let sleep_ticks = (sleep_ns + NS_PER_TICK - 1) / NS_PER_TICK; // Round up

    let start_ticks = get_ticks();
    let wake_ticks = start_ticks + sleep_ticks;

    // Get current PID for sleep queue management
    let current_pid = sched::current_pid().unwrap_or(0);

    // Register in sleep queue so timer interrupt will wake us
    let queued = sleep_queue_add(current_pid, wake_ticks);

    // Sleep loop: block ourselves, let timer interrupt wake us when time expires
    while get_ticks() < wake_ticks {
        // Check for pending signals
        let has_signals = with_current_meta(|meta| meta.has_pending_signals()).unwrap_or(false);
        if has_signals {
            // Remove from sleep queue on signal
            if queued {
                sleep_queue_remove(current_pid);
            }

            let elapsed_ticks = get_ticks() - start_ticks;
            let remaining_ticks = sleep_ticks.saturating_sub(elapsed_ticks);

            if rem_ptr != 0 && remaining_ticks > 0 {
                let remaining_ns = remaining_ticks * NS_PER_TICK;
                let rem = Timespec {
                    tv_sec: (remaining_ns / 1_000_000_000) as i64,
                    tv_nsec: (remaining_ns % 1_000_000_000) as i64,
                };

                unsafe {
                    core::arch::asm!("stac", options(nomem, nostack));
                    let rp = rem_ptr as *mut Timespec;
                    core::ptr::write_volatile(rp, rem);
                    core::arch::asm!("clac", options(nomem, nostack));
                }
            }

            return errno::EINTR;
        }

        if queued {
            // Block ourselves - removes from run queue so scheduler skips us.
            // The timer interrupt calls check_sleepers() which will wake_up()
            // us when wake_ticks is reached, re-enqueueing us.
            sched::block_current(TaskState::TASK_INTERRUPTIBLE);
            sched::set_need_resched();
        }

        // Allow scheduler to preempt us while we wait
        arch::allow_kernel_preempt();

        // HLT yields CPU until next interrupt.
        // If queued: scheduler will switch away (we're blocked), timer will wake us.
        // If not queued (queue full): fallback to polling with preempt.
        unsafe {
            core::arch::asm!("sti"); // Ensure interrupts enabled
            core::arch::asm!("hlt", options(nomem, nostack));
        }

        // Clear preempt flag
        arch::disallow_kernel_preempt();
    }

    // Sleep complete, set remaining to zero if requested
    if rem_ptr != 0 {
        let rem = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };

        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            let rp = rem_ptr as *mut Timespec;
            core::ptr::write_volatile(rp, rem);
            core::arch::asm!("clac", options(nomem, nostack));
        }
    }

    0
}
