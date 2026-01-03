# EFFLUX Timer and Clock Specification

**Version:** 1.0
**Status:** Draft
**License:** MIT

---

## 0) Overview

The timer subsystem provides:

- High-resolution timekeeping
- Periodic tick for scheduler
- One-shot timers for sleep/timeout
- Monotonic and wall-clock time sources
- Per-CPU timer events

All architecture-specific implementations are in `docs/arch/<arch>/TIMER.md`.

---

## 1) Architecture Abstraction

### 1.1 Clock Source Trait

```rust
/// Generic clock source - architecture implements this
pub trait ClockSource: Send + Sync {
    /// Human-readable name
    fn name(&self) -> &'static str;

    /// Read current counter value
    fn read(&self) -> u64;

    /// Counter frequency in Hz
    fn frequency(&self) -> u64;

    /// Resolution in nanoseconds
    fn resolution_ns(&self) -> u64 {
        1_000_000_000 / self.frequency()
    }

    /// Rating (higher = better, used for selection)
    fn rating(&self) -> u32;

    /// Is this clock source stable? (not affected by CPU frequency scaling)
    fn is_stable(&self) -> bool;

    /// Convert counter ticks to nanoseconds
    fn ticks_to_ns(&self, ticks: u64) -> u64 {
        ticks * 1_000_000_000 / self.frequency()
    }

    /// Convert nanoseconds to ticks
    fn ns_to_ticks(&self, ns: u64) -> u64 {
        ns * self.frequency() / 1_000_000_000
    }
}
```

### 1.2 Timer Device Trait

```rust
/// Hardware timer device - architecture implements this
pub trait TimerDevice: Send + Sync {
    /// Human-readable name
    fn name(&self) -> &'static str;

    /// Set up periodic interrupt at given frequency
    fn set_periodic(&mut self, hz: u32) -> Result<()>;

    /// Set up one-shot interrupt after given nanoseconds
    fn set_oneshot(&mut self, ns: u64) -> Result<()>;

    /// Stop the timer
    fn stop(&mut self);

    /// Acknowledge interrupt (call from handler)
    fn ack_interrupt(&mut self);

    /// Is this timer per-CPU or global?
    fn is_per_cpu(&self) -> bool;

    /// Minimum programmable delay in nanoseconds
    fn min_delay_ns(&self) -> u64;

    /// Maximum programmable delay in nanoseconds
    fn max_delay_ns(&self) -> u64;
}
```

### 1.3 Architecture Registration

```rust
/// Each architecture registers its clock sources and timers at boot
pub trait ArchTimer {
    /// Detect and register available clock sources
    fn init_clock_sources() -> Vec<Box<dyn ClockSource>>;

    /// Detect and register available timer devices
    fn init_timer_devices() -> Vec<Box<dyn TimerDevice>>;

    /// Get early boot clock source (before full init)
    fn early_clock_source() -> Box<dyn ClockSource>;

    /// Calibrate timers against reference (e.g., PIT against TSC)
    fn calibrate() -> Result<()>;
}

// Architecture implementations
#[cfg(target_arch = "x86_64")]
impl ArchTimer for crate::arch::x86_64::X86_64 { /* ... */ }

#[cfg(target_arch = "aarch64")]
impl ArchTimer for crate::arch::aarch64::Aarch64 { /* ... */ }

#[cfg(target_arch = "mips64")]
impl ArchTimer for crate::arch::mips64::Mips64 { /* ... */ }

#[cfg(target_arch = "riscv64")]
impl ArchTimer for crate::arch::riscv64::Riscv64 { /* ... */ }
```

---

## 2) Clock Sources by Architecture

| Architecture | Primary | Secondary | Fallback |
|--------------|---------|-----------|----------|
| x86_64 | TSC (if invariant) | HPET | PIT |
| i686 | TSC (if stable) | HPET | PIT |
| AArch64 | Generic Timer (CNTPCT) | - | - |
| ARM32 | Generic Timer | SP804 | - |
| MIPS64 | CP0 Count | - | - |
| MIPS32 | CP0 Count | - | - |
| RISC-V 64 | mtime CSR | - | - |
| RISC-V 32 | mtime CSR | - | - |

See `docs/arch/<arch>/TIMER.md` for implementation details.

---

## 3) Timer Devices by Architecture

| Architecture | Per-CPU Timer | Global Timer |
|--------------|---------------|--------------|
| x86_64 | LAPIC Timer | HPET, PIT |
| i686 | LAPIC Timer | HPET, PIT |
| AArch64 | Generic Timer | SP804 (if present) |
| ARM32 | Generic Timer | SP804 |
| MIPS64 | CP0 Compare | - |
| MIPS32 | CP0 Compare | - |
| RISC-V 64 | SBI timer / CLINT | - |
| RISC-V 32 | SBI timer / CLINT | - |

---

## 4) Kernel Time Subsystem

### 4.1 Time Types

```rust
/// Absolute time since epoch (wall clock)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WallTime {
    pub secs: i64,
    pub nsecs: u32,
}

/// Monotonic time since boot (never goes backwards)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Monotonic {
    pub secs: u64,
    pub nsecs: u32,
}

/// Boot time (monotonic but paused during suspend)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BootTime {
    pub secs: u64,
    pub nsecs: u32,
}

/// Raw monotonic (not adjusted by NTP)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RawMonotonic {
    pub secs: u64,
    pub nsecs: u32,
}

/// Duration
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Duration {
    pub secs: u64,
    pub nsecs: u32,
}

impl Duration {
    pub const ZERO: Self = Self { secs: 0, nsecs: 0 };

    pub fn from_secs(secs: u64) -> Self {
        Self { secs, nsecs: 0 }
    }

    pub fn from_millis(ms: u64) -> Self {
        Self {
            secs: ms / 1000,
            nsecs: ((ms % 1000) * 1_000_000) as u32,
        }
    }

    pub fn from_micros(us: u64) -> Self {
        Self {
            secs: us / 1_000_000,
            nsecs: ((us % 1_000_000) * 1000) as u32,
        }
    }

    pub fn from_nanos(ns: u64) -> Self {
        Self {
            secs: ns / 1_000_000_000,
            nsecs: (ns % 1_000_000_000) as u32,
        }
    }

    pub fn as_nanos(&self) -> u128 {
        self.secs as u128 * 1_000_000_000 + self.nsecs as u128
    }
}
```

### 4.2 Timekeeper

```rust
/// Central timekeeping structure
pub struct Timekeeper {
    /// Current best clock source
    clock: RwLock<Box<dyn ClockSource>>,

    /// Base monotonic time at last update
    base_mono: AtomicU64,

    /// Clock source reading at last update
    base_cycles: AtomicU64,

    /// Wall clock offset from monotonic
    wall_offset: RwLock<WallTime>,

    /// NTP adjustment (parts per billion)
    ntp_adjust: AtomicI64,

    /// Sequence lock for consistent reads
    seq: SeqLock,
}

impl Timekeeper {
    /// Get current monotonic time
    pub fn monotonic(&self) -> Monotonic {
        loop {
            let seq = self.seq.read_begin();

            let base_mono = self.base_mono.load(Ordering::Relaxed);
            let base_cycles = self.base_cycles.load(Ordering::Relaxed);
            let clock = self.clock.read();

            let current_cycles = clock.read();
            let delta_cycles = current_cycles.wrapping_sub(base_cycles);
            let delta_ns = clock.ticks_to_ns(delta_cycles);

            let total_ns = base_mono + delta_ns;

            if self.seq.read_check(seq) {
                return Monotonic {
                    secs: total_ns / 1_000_000_000,
                    nsecs: (total_ns % 1_000_000_000) as u32,
                };
            }
        }
    }

    /// Get current wall clock time
    pub fn wall_time(&self) -> WallTime {
        let mono = self.monotonic();
        let offset = self.wall_offset.read();

        WallTime {
            secs: offset.secs + mono.secs as i64,
            nsecs: offset.nsecs + mono.nsecs,
        }
    }

    /// Set wall clock time (requires CAP_SYS_TIME)
    pub fn set_wall_time(&self, time: WallTime) {
        let mono = self.monotonic();
        let mut offset = self.wall_offset.write();

        offset.secs = time.secs - mono.secs as i64;
        offset.nsecs = time.nsecs.saturating_sub(mono.nsecs);
    }

    /// Update from clock source (called periodically)
    pub fn update(&self) {
        self.seq.write_begin();

        let clock = self.clock.read();
        let cycles = clock.read();
        let mono_ns = self.monotonic().as_nanos();

        self.base_mono.store(mono_ns as u64, Ordering::Relaxed);
        self.base_cycles.store(cycles, Ordering::Relaxed);

        self.seq.write_end();
    }
}

/// Global timekeeper instance
pub static TIMEKEEPER: Timekeeper = Timekeeper::new();
```

---

## 5) Timer Wheel

Efficient management of many timers using a hierarchical wheel:

```rust
/// Timer wheel for O(1) timer insertion and near-O(1) expiration
pub struct TimerWheel {
    /// Wheel levels (each level has 256 buckets)
    levels: [TimerWheelLevel; 8],

    /// Current position in each level
    positions: [u8; 8],

    /// Base time (jiffies)
    base_jiffies: u64,

    /// Lock
    lock: SpinLock<()>,
}

struct TimerWheelLevel {
    buckets: [TimerList; 256],
}

struct TimerList {
    head: Option<Box<Timer>>,
}

pub struct Timer {
    /// Expiration time (jiffies)
    expires: u64,

    /// Callback function
    callback: Box<dyn FnOnce() + Send>,

    /// Link to next timer in list
    next: Option<Box<Timer>>,

    /// Flags
    flags: TimerFlags,
}

bitflags! {
    pub struct TimerFlags: u32 {
        const DEFERRABLE = 1 << 0;   // Can be deferred for power saving
        const PINNED     = 1 << 1;   // Must run on specific CPU
        const IRQSAFE    = 1 << 2;   // Callback is IRQ-safe
    }
}

impl TimerWheel {
    /// Add a timer
    pub fn add(&mut self, timer: Timer) {
        let delta = timer.expires.saturating_sub(self.base_jiffies);
        let (level, bucket) = self.calculate_bucket(delta);

        self.levels[level].buckets[bucket].insert(timer);
    }

    /// Process expired timers (called on tick)
    pub fn tick(&mut self) {
        let _lock = self.lock.lock();

        // Process level 0 current bucket
        let bucket = self.positions[0] as usize;
        while let Some(timer) = self.levels[0].buckets[bucket].pop() {
            (timer.callback)();
        }

        // Advance positions, cascade if needed
        self.advance();
    }

    fn calculate_bucket(&self, delta: u64) -> (usize, usize) {
        // Level 0: 0-255 jiffies (each bucket = 1 jiffy)
        // Level 1: 256-65535 jiffies (each bucket = 256 jiffies)
        // Level 2: 65536-16M jiffies (each bucket = 65536 jiffies)
        // etc.

        if delta < 256 {
            (0, delta as usize)
        } else if delta < 256 * 256 {
            (1, (delta >> 8) as usize)
        } else if delta < 256 * 256 * 256 {
            (2, (delta >> 16) as usize)
        } else {
            // Higher levels...
            (7, 255)  // Cap at max
        }
    }
}
```

---

## 6) High-Resolution Timers (hrtimers)

For sub-jiffy precision (nanosecond timers):

```rust
/// High-resolution timer using hardware one-shot
pub struct HrTimer {
    /// Expiration time (absolute monotonic nanoseconds)
    expires: u64,

    /// Callback
    callback: Box<dyn FnOnce() + Send>,

    /// Softirq vs hardirq context
    mode: HrTimerMode,
}

pub enum HrTimerMode {
    /// Run in hard IRQ context (minimal latency)
    HardIrq,

    /// Run in softirq context (can sleep)
    SoftIrq,
}

/// Per-CPU high-resolution timer queue
pub struct HrTimerQueue {
    /// Red-black tree of pending timers
    timers: RBTree<u64, HrTimer>,

    /// Currently programmed hardware timer expiration
    programmed: AtomicU64,

    /// Hardware timer device
    device: Box<dyn TimerDevice>,
}

impl HrTimerQueue {
    pub fn add(&mut self, timer: HrTimer) {
        let expires = timer.expires;
        self.timers.insert(expires, timer);

        // Reprogram hardware if new timer is earlier
        let current = self.programmed.load(Ordering::Relaxed);
        if expires < current {
            self.reprogram(expires);
        }
    }

    pub fn handle_interrupt(&mut self) {
        let now = TIMEKEEPER.monotonic().as_nanos() as u64;

        // Process all expired timers
        while let Some((expires, _)) = self.timers.first() {
            if *expires > now {
                break;
            }

            let (_, timer) = self.timers.pop_first().unwrap();
            (timer.callback)();
        }

        // Reprogram for next timer
        if let Some((expires, _)) = self.timers.first() {
            self.reprogram(*expires);
        }
    }

    fn reprogram(&mut self, expires: u64) {
        let now = TIMEKEEPER.monotonic().as_nanos() as u64;
        let delta = expires.saturating_sub(now);

        self.device.set_oneshot(delta).ok();
        self.programmed.store(expires, Ordering::Relaxed);
    }
}
```

---

## 7) Scheduler Tick

```rust
/// Called by timer interrupt at HZ frequency
pub fn scheduler_tick() {
    let cpu = current_cpu();

    // Update jiffies
    JIFFIES.fetch_add(1, Ordering::Relaxed);

    // Process timer wheel
    TIMER_WHEEL.lock().tick();

    // Update process accounting
    let current = current_thread();
    current.time_slice -= 1;
    current.runtime_ns += TICK_NS;

    // Check for preemption
    if current.time_slice == 0 {
        current.time_slice = DEFAULT_TIMESLICE;
        set_need_resched();
    }

    // Balance load periodically
    if JIFFIES.load(Ordering::Relaxed) % LOAD_BALANCE_PERIOD == 0 {
        trigger_load_balance(cpu);
    }
}

/// Jiffies counter (increments at HZ rate)
pub static JIFFIES: AtomicU64 = AtomicU64::new(0);

/// Kernel tick rate
pub const HZ: u64 = 1000;  // 1ms tick (configurable)

/// Nanoseconds per tick
pub const TICK_NS: u64 = 1_000_000_000 / HZ;
```

---

## 8) Sleep and Delay Functions

```rust
/// Sleep for at least `duration` (may wake early on signal)
pub fn sleep(duration: Duration) -> Result<Duration> {
    let expires = TIMEKEEPER.monotonic() + duration;

    let timer = HrTimer {
        expires: expires.as_nanos() as u64,
        callback: Box::new(|| wake_current_thread()),
        mode: HrTimerMode::SoftIrq,
    };

    PER_CPU_HRTIMER.add(timer);
    schedule();  // Sleep until woken

    // Calculate remaining time
    let now = TIMEKEEPER.monotonic();
    if now >= expires {
        Ok(Duration::ZERO)
    } else {
        Ok(expires - now)
    }
}

/// Busy-wait delay (for short delays, no sleeping)
pub fn udelay(us: u64) {
    let start = TIMEKEEPER.monotonic();
    let target = start + Duration::from_micros(us);

    while TIMEKEEPER.monotonic() < target {
        core::hint::spin_loop();
    }
}

/// Busy-wait delay in nanoseconds
pub fn ndelay(ns: u64) {
    let start = TIMEKEEPER.monotonic();
    let target = start + Duration::from_nanos(ns);

    while TIMEKEEPER.monotonic() < target {
        core::hint::spin_loop();
    }
}

/// Delay for driver initialization (may sleep if safe)
pub fn mdelay(ms: u64) {
    if can_sleep() {
        sleep(Duration::from_millis(ms)).ok();
    } else {
        udelay(ms * 1000);
    }
}
```

---

## 9) Syscalls

```rust
// Time retrieval
pub fn sys_clock_gettime(clock_id: i32, tp: *mut Timespec) -> Result<()>;
pub fn sys_clock_settime(clock_id: i32, tp: *const Timespec) -> Result<()>;
pub fn sys_clock_getres(clock_id: i32, res: *mut Timespec) -> Result<()>;
pub fn sys_gettimeofday(tv: *mut Timeval, tz: *mut Timezone) -> Result<()>;
pub fn sys_settimeofday(tv: *const Timeval, tz: *const Timezone) -> Result<()>;
pub fn sys_time(tloc: *mut i64) -> Result<i64>;

// Sleeping
pub fn sys_nanosleep(req: *const Timespec, rem: *mut Timespec) -> Result<()>;
pub fn sys_clock_nanosleep(clock_id: i32, flags: i32, req: *const Timespec,
                           rem: *mut Timespec) -> Result<()>;

// Timers
pub fn sys_timer_create(clock_id: i32, evp: *mut SigEvent,
                        timerid: *mut i32) -> Result<()>;
pub fn sys_timer_settime(timerid: i32, flags: i32, new: *const ItimerSpec,
                         old: *mut ItimerSpec) -> Result<()>;
pub fn sys_timer_gettime(timerid: i32, curr: *mut ItimerSpec) -> Result<()>;
pub fn sys_timer_delete(timerid: i32) -> Result<()>;
pub fn sys_timer_getoverrun(timerid: i32) -> Result<i32>;

// Interval timers (legacy)
pub fn sys_getitimer(which: i32, curr: *mut Itimerval) -> Result<()>;
pub fn sys_setitimer(which: i32, new: *const Itimerval,
                     old: *mut Itimerval) -> Result<()>;
pub fn sys_alarm(seconds: u32) -> Result<u32>;

// Clock IDs
pub const CLOCK_REALTIME: i32           = 0;
pub const CLOCK_MONOTONIC: i32          = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;
pub const CLOCK_THREAD_CPUTIME_ID: i32  = 3;
pub const CLOCK_MONOTONIC_RAW: i32      = 4;
pub const CLOCK_REALTIME_COARSE: i32    = 5;
pub const CLOCK_MONOTONIC_COARSE: i32   = 6;
pub const CLOCK_BOOTTIME: i32           = 7;
```

---

## 10) vDSO (Virtual Dynamic Shared Object)

Fast userspace time access without syscall:

```rust
/// vDSO data page (mapped read-only into every process)
#[repr(C)]
pub struct VdsoData {
    /// Sequence counter for consistent reads
    pub seq: u32,
    pub _pad: u32,

    /// Clock source parameters
    pub clock_mode: u32,          // Which clock source
    pub clock_mult: u32,          // Multiplier for conversion
    pub clock_shift: u32,         // Shift for conversion

    /// Base values
    pub base_mono_ns: u64,
    pub base_cycles: u64,
    pub base_realtime_sec: i64,
    pub base_realtime_nsec: u32,

    /// Wall-to-monotonic offset
    pub wall_to_mono_sec: i64,
    pub wall_to_mono_nsec: i32,
}

/// vDSO functions (userspace implementations)
/// These are mapped into every process and called without syscall

// __vdso_clock_gettime
// __vdso_gettimeofday
// __vdso_time
// __vdso_getcpu
```

---

## 11) Architecture Implementation Files

Each architecture must implement the timer subsystem in:

```
docs/arch/<arch>/TIMER.md
```

These files contain:
- Hardware timer details
- Clock source implementation
- Timer device implementation
- Calibration procedures
- Errata and workarounds

---

## 12) Exit Criteria

- [ ] All arch clock sources implemented and registered
- [ ] Timer wheel handles millions of timers
- [ ] High-resolution timers achieve microsecond precision
- [ ] vDSO provides sub-100ns time reads
- [ ] NTP adjustment works
- [ ] Works on all 8 architectures

---

*End of EFFLUX Timer and Clock Specification*

*See `docs/arch/<arch>/TIMER.md` for architecture-specific details.*
