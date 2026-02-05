//! Time-related system calls
//!
//! Provides CLOCK_GETTIME, GETTIMEOFDAY, NANOSLEEP, etc.

use crate::errno;
use crate::with_current_meta;
use arch_x86_64 as arch;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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
    pid: AtomicU32,       // PID of sleeping task (0 = empty slot)
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
///
/// Store order matters: write wake_tick BEFORE pid. The timer interrupt's
/// check_sleepers() uses pid != 0 as the "slot occupied" signal and then
/// reads wake_tick. If we stored pid first, check_sleepers() could see a
/// valid pid with stale wake_tick=0, causing a spurious immediate wakeup.
fn sleep_queue_add(pid: u32, wake_tick: u64) -> bool {
    for slot in &SLEEP_QUEUE {
        // Check if slot is free without modifying it
        if slot.pid.load(Ordering::Relaxed) != 0 {
            continue;
        }
        // Write wake_tick first so it's valid when check_sleepers reads it
        slot.wake_tick.store(wake_tick, Ordering::Release);
        // Now atomically claim the slot — only succeeds if still empty
        if slot
            .pid
            .compare_exchange(0, pid, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return true;
        }
        // Another task claimed this slot between our load and CAS — keep looking
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
///
/// — GraveShift: This runs in ISR context. MUST NOT use blocking locks.
/// Uses sched::try_wake_up (non-blocking) instead of sched::wake_up.
/// If the RQ lock is contended (interrupted code holds it on this CPU),
/// the entry stays in the queue and we retry on the next tick. This
/// prevents the classic "ISR spins on lock held by interrupted code" deadlock.
pub fn check_sleepers() {
    let now = get_ticks();
    for slot in &SLEEP_QUEUE {
        let pid = slot.pid.load(Ordering::Acquire);
        if pid != 0 {
            let wake_tick = slot.wake_tick.load(Ordering::Acquire);
            if now >= wake_tick {
                // — GraveShift: Try to wake FIRST (non-blocking). Only clear
                // the slot if the wake succeeds. If the RQ lock is contended,
                // leave the entry intact — next tick will retry. This avoids
                // the deadlock where the timer ISR spins on a lock held by
                // the very code it interrupted.
                if sched::try_wake_up(pid) {
                    // Wake succeeded — clear the slot
                    slot.pid
                        .compare_exchange(pid, 0, Ordering::AcqRel, Ordering::Relaxed)
                        .ok();

                    #[cfg(feature = "debug-sched")]
                    unsafe {
                        os_log::write_str_raw("[SLEEP-WAKE] pid=");
                        let mut buf = [0u8; 10];
                        let mut n = pid as u64;
                        let mut pos = 0;
                        if n == 0 {
                            os_log::write_byte_raw(b'0');
                        } else {
                            while n > 0 {
                                buf[pos] = b'0' + (n % 10) as u8;
                                n /= 10;
                                pos += 1;
                            }
                            for i in (0..pos).rev() {
                                os_log::write_byte_raw(buf[i]);
                            }
                        }
                        os_log::write_str_raw(" tick=");
                        n = now;
                        pos = 0;
                        if n == 0 {
                            os_log::write_byte_raw(b'0');
                        } else {
                            while n > 0 {
                                buf[pos] = b'0' + (n % 10) as u8;
                                n /= 10;
                                pos += 1;
                            }
                            for i in (0..pos).rev() {
                                os_log::write_byte_raw(buf[i]);
                            }
                        }
                        os_log::write_str_raw("\n");
                    }
                }
                // If try_wake_up failed (lock contended), entry stays — retry next tick
            }
        }
    }
}

/// Block the current task for a specified number of deciseconds (1/10 second).
///
/// 🔥 VTIME SUPPORT (Priority #7 Fix) 🔥
/// Before: VTIME ignored → raw mode timeout reads didn't work
/// After: Proper timed blocking with sleep queue
///
/// Used by TTY read() for VTIME timeouts. Returns true if timeout expired,
/// false if woken early (by signal or data arrival).
///
/// At 100 Hz timer, 1 decisecond = 10 ticks.
pub fn block_deciseconds(deciseconds: u8) -> bool {
    if deciseconds == 0 {
        return true; // No timeout - caller should use regular blocking
    }

    let timeout_ticks = (deciseconds as u64) * 10; // 1 decisecond = 10 ticks at 100Hz
    let start_ticks = get_ticks();
    let wake_ticks = start_ticks + timeout_ticks;

    let current_pid = sched::current_pid().unwrap_or(0);
    if current_pid == 0 {
        return true; // No current task?
    }

    // Register in sleep queue
    let queued = sleep_queue_add(current_pid, wake_ticks);

    // Block until timeout or wakeup
    while get_ticks() < wake_ticks {
        // Check for early wakeup (signal or data arrival)
        // The TTY will wake us via sched_wake_up() when data arrives
        let has_signals = with_current_meta(|meta| meta.has_pending_signals()).unwrap_or(false);
        if has_signals {
            if queued {
                sleep_queue_remove(current_pid);
            }
            return false; // Woken early by signal
        }

        if queued {
            sched::block_current(TaskState::TASK_INTERRUPTIBLE);
            sched::set_need_resched();
        }

        arch::allow_kernel_preempt();

        unsafe {
            core::arch::asm!("sti", "hlt", "cli", options(nomem, nostack));
        }

        // If we get here and time hasn't expired, we were woken early (by data or signal)
        if get_ticks() < wake_ticks {
            if queued {
                sleep_queue_remove(current_pid);
            }
            return false; // Woken early
        }
    }

    true // Timeout expired
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

/// Get current wall-clock time as seconds since Unix epoch.
///
/// Suitable for registration with `os_core::register_wall_clock()` so
/// subsystems like ext4 can stamp inodes without depending on arch crates.
///
/// — WireSaint: the clock face for filesystem timestamps
pub fn wall_clock_secs() -> u64 {
    let ticks = arch::timer_ticks();
    let uptime_secs = (ticks * NS_PER_TICK) / 1_000_000_000;
    let boot_secs = BOOT_TIME_SECS.load(Ordering::Relaxed);
    boot_secs + uptime_secs
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

    // ⚡ GraveShift: Avoid overflow by computing seconds and nanoseconds separately
    let uptime_secs = uptime_ns / 1_000_000_000;
    let uptime_nsec_remainder = uptime_ns % 1_000_000_000;

    Timespec {
        tv_sec: (boot_secs + uptime_secs) as i64,
        tv_nsec: uptime_nsec_remainder as i64,
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
            // ⚡ GraveShift: Return actual per-process CPU time from ProcessMeta
            crate::with_current_meta(|meta| {
                let cpu_ns = meta.cpu_time_ns;
                Timespec {
                    tv_sec: (cpu_ns / 1_000_000_000) as i64,
                    tv_nsec: (cpu_ns % 1_000_000_000) as i64,
                }
            })
            .unwrap_or_else(|| get_monotonic_time())
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
        // NOTE: sti + hlt MUST be in the same asm block.  If separated, a timer
        // interrupt can fire between them; the ISR handles it, returns, and then
        // the CPU hits HLT and waits another full tick unnecessarily.
        unsafe {
            core::arch::asm!("sti", "hlt", options(nomem, nostack));
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

/// sys_clock_nanosleep - sleep with specified clock
///
/// # Arguments
/// * `clock_id` - Clock to use (CLOCK_REALTIME, CLOCK_MONOTONIC, etc.)
/// * `flags` - 0 = relative, 1 = TIMER_ABSTIME (absolute time)
/// * `req_ptr` - Pointer to requested sleep time
/// * `rem_ptr` - Pointer to store remaining time if interrupted
pub fn sys_clock_nanosleep(clock_id: i32, flags: i32, req_ptr: usize, rem_ptr: usize) -> i64 {
    // Validate clock ID
    match clock_id {
        clock::REALTIME
        | clock::MONOTONIC
        | clock::MONOTONIC_RAW
        | clock::REALTIME_COARSE
        | clock::MONOTONIC_COARSE
        | clock::BOOTTIME => {}
        _ => return errno::EINVAL,
    }

    const TIMER_ABSTIME: i32 = 1;

    if flags & TIMER_ABSTIME != 0 {
        // Absolute time: compute relative sleep from current time
        if req_ptr == 0 {
            return errno::EFAULT;
        }

        let req: Timespec = unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            let rp = req_ptr as *const Timespec;
            let val = core::ptr::read_volatile(rp);
            core::arch::asm!("clac", options(nomem, nostack));
            val
        };

        // Get current time for this clock
        let now = match clock_id {
            clock::REALTIME | clock::REALTIME_COARSE => get_realtime(),
            _ => get_monotonic_time(),
        };

        // If target time is already in the past, return immediately
        if req.tv_sec < now.tv_sec || (req.tv_sec == now.tv_sec && req.tv_nsec <= now.tv_nsec) {
            return 0;
        }

        // Compute relative duration
        let mut rel_sec = req.tv_sec - now.tv_sec;
        let mut rel_nsec = req.tv_nsec - now.tv_nsec;
        if rel_nsec < 0 {
            rel_sec -= 1;
            rel_nsec += 1_000_000_000;
        }

        // Create a temporary relative timespec on the stack and call nanosleep
        let rel_ts = Timespec {
            tv_sec: rel_sec,
            tv_nsec: rel_nsec,
        };

        // Write to a stack-local and pass its address
        // (we can't easily pass a kernel pointer to the nanosleep path,
        // so we'll duplicate the sleep logic inline)
        let sleep_ns = (rel_ts.tv_sec as u64 * 1_000_000_000) + (rel_ts.tv_nsec as u64);
        let sleep_ticks = (sleep_ns + NS_PER_TICK - 1) / NS_PER_TICK;
        let wake_ticks = get_ticks() + sleep_ticks;
        let current_pid = sched::current_pid().unwrap_or(0);
        let queued = sleep_queue_add(current_pid, wake_ticks);

        while get_ticks() < wake_ticks {
            if queued {
                sched::block_current(TaskState::TASK_INTERRUPTIBLE);
                sched::set_need_resched();
            }
            arch::allow_kernel_preempt();
            unsafe {
                core::arch::asm!("sti", "hlt", options(nomem, nostack));
            }
            arch::disallow_kernel_preempt();

            // ⚡ GraveShift: POSIX requires checking for signals in TIMER_ABSTIME mode
            // If interrupted by signal, return EINTR (absolute time doesn't use rem_ptr)
            let has_signals = crate::with_current_meta(|meta| meta.has_pending_signals())
                .unwrap_or(false);
            if has_signals {
                return errno::EINTR;
            }
        }

        0
    } else {
        // Relative time: just delegate to nanosleep
        sys_nanosleep(req_ptr, rem_ptr)
    }
}
