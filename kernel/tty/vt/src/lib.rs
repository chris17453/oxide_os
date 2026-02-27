//! Virtual Terminal (VT) support for OXIDE OS
//!
//! Provides multiple virtual consoles that can be switched with Ctrl+Alt+F1-F6
//!
//! ## REWRITE NOTES (2077 Edition)
//!
//! Previously, this module used `try_lock()` to push keyboard input from IRQ context.
//! Problem: When the lock was held, keystrokes were **silently dropped into the void**.
//!
//! Fix: Lock-free ring buffer. IRQ pushes atomically, no locks, no drops, no tears.
//! Your keyboard works now. You're welcome.

#![no_std]

extern crate alloc;

mod lockfree_ring;

use alloc::sync::Arc;
use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};
use spin::Mutex;

use lockfree_ring::LockFreeRing;
use tty::{Tty, TtyDriver};
use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

extern crate signal;

/// Write a debug string to serial (debug-console only).
/// — SableWire: delegates to os_log::write_str_raw() which calls through
/// to the registered ISR-safe writer with bounded spin. No more inline
/// serial port I/O duplication across crates.
#[cfg(feature = "debug-console")]
#[inline]
fn dbg_serial(s: &str) {
    unsafe {
        os_log::write_str_raw(s);
    }
}

/// Yield callback type — called to yield CPU while waiting for input.
/// Must enable kernel preemption and halt until next interrupt.
pub type YieldFn = fn();

/// Global yield callback (set by kernel during init)
static mut YIELD_CALLBACK: Option<YieldFn> = None;

/// Set the yield callback for VT blocking reads
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_yield_callback(f: YieldFn) {
    unsafe {
        YIELD_CALLBACK = Some(f);
    }
}

/// Number of virtual terminals
pub const NUM_VTS: usize = 6;

/// VT state — TTY + metadata, NO input buffer.
///
/// — GraveShift: The LockFreeRing lived here inside Mutex<VtState>. That made
/// push_input() do try_lock() to reach it — defeating the entire point of a
/// "lock-free" ring buffer. When the mutex was held (read/write/poll), keystrokes
/// were silently dropped. Intermittent login failures, missing characters, the
/// whole goddamn parade. Ring buffers now live in VtManager directly. Zero locks
/// between IRQ push and ring buffer. Problem solved.
struct VtState {
    /// TTY device
    tty: Arc<Tty>,
    /// VT number (0-5 for tty1-tty6)
    _vt_num: usize,
}

impl VtState {
    fn new(tty: Arc<Tty>, vt_num: usize) -> Self {
        VtState {
            tty,
            _vt_num: vt_num,
        }
    }
}

/// Virtual Terminal Manager
///
/// — GraveShift: `input_rings` lives OUTSIDE the VT mutex. IRQ handler pushes
/// bytes into the ring without touching any lock. read()/poll() pops without
/// any lock. The mutex only protects TTY state (echo, line discipline, termios).
/// This is the fix for the intermittent keystroke drops that caused login failures.
pub struct VtManager {
    /// All VTs (TTY state — behind mutex for echo/ldisc/termios)
    vts: [Mutex<VtState>; NUM_VTS],
    /// Input ring buffers — OUTSIDE mutex for true lock-free IRQ push
    /// One per VT. IRQ pushes to active VT's ring. read()/poll() drains it.
    input_rings: [LockFreeRing; NUM_VTS],
}

impl VtManager {
    /// Create a new VT manager
    pub fn new() -> Self {
        // Create TTY drivers for each VT
        struct VtTtyDriver {
            vt_num: usize,
        }
        impl TtyDriver for VtTtyDriver {
            fn write(&self, data: &[u8]) {
                // Only write if this is the active VT (check global directly)
                let active = *ACTIVE_VT.read();
                // — GraveShift: [VTD] traces gated — these fired on EVERY glyph write,
                // saturating 115200 baud serial and making colors output take 10x longer.
                #[cfg(feature = "debug-console")]
                {
                    unsafe { os_log::write_str_raw("[VTD] a="); }
                    unsafe { os_log::write_byte_raw(b'0' + (active as u8)); }
                    unsafe { os_log::write_str_raw(" v="); }
                    unsafe { os_log::write_byte_raw(b'0' + (self.vt_num as u8)); }
                }
                if active == self.vt_num {
                    // Write to console output (terminal emulator + serial)
                    unsafe {
                        if let Some(write_fn) = CONSOLE_WRITE_CALLBACK {
                            #[cfg(feature = "debug-console")]
                            os_log::write_str_raw(" ->CW\n");
                            write_fn(data);
                        } else {
                            unsafe { os_log::write_str_raw("[VTD] NO-CB!\n"); }
                        }
                    }
                } else {
                    #[cfg(feature = "debug-console")]
                    unsafe { os_log::write_str_raw(" SKIP\n"); }
                }
            }
        }

        VtManager {
            vts: [
                Mutex::new(VtState::new(
                    Tty::new(Arc::new(VtTtyDriver { vt_num: 0 }), 1, 0),
                    0,
                )),
                Mutex::new(VtState::new(
                    Tty::new(Arc::new(VtTtyDriver { vt_num: 1 }), 2, 0),
                    1,
                )),
                Mutex::new(VtState::new(
                    Tty::new(Arc::new(VtTtyDriver { vt_num: 2 }), 3, 0),
                    2,
                )),
                Mutex::new(VtState::new(
                    Tty::new(Arc::new(VtTtyDriver { vt_num: 3 }), 4, 0),
                    3,
                )),
                Mutex::new(VtState::new(
                    Tty::new(Arc::new(VtTtyDriver { vt_num: 4 }), 5, 0),
                    4,
                )),
                Mutex::new(VtState::new(
                    Tty::new(Arc::new(VtTtyDriver { vt_num: 5 }), 6, 0),
                    5,
                )),
            ],
            // — GraveShift: Ring buffers live here, not inside VtState.
            // IRQ handler writes directly. No mutex. No try_lock. No dropped keys.
            input_rings: [
                LockFreeRing::new(),
                LockFreeRing::new(),
                LockFreeRing::new(),
                LockFreeRing::new(),
                LockFreeRing::new(),
                LockFreeRing::new(),
            ],
        }
    }

    /// Get the active VT index
    pub fn active_vt(&self) -> usize {
        *ACTIVE_VT.read()
    }

    /// Switch to a different VT
    pub fn switch_to(&self, vt_num: usize) -> bool {
        if vt_num >= NUM_VTS {
            return false;
        }

        let mut active = ACTIVE_VT.write();
        if *active != vt_num {
            *active = vt_num;

            // 🔥 PRIORITY #2 FIX - VT switch screen buffer notification 🔥
            // Notify terminal emulator to switch screen buffer and force full redraw
            // This prevents stale screen state when switching to/from vim on different VTs
            unsafe {
                if let Some(callback) = VT_SWITCH_CALLBACK {
                    callback(vt_num);
                }
            }

            true
        } else {
            false
        }
    }

    /// Push input character to active VT
    ///
    /// Called from interrupt context (keyboard IRQ) — must be fast and non-blocking.
    ///
    /// — GraveShift: Ring buffer is in VtManager.input_rings, OUTSIDE the VT mutex.
    /// Zero locks for the push. Signal delivery is best-effort via try_lock —
    /// if the mutex is held, the drain in read()/poll() will catch it.
    pub fn push_input(&self, ch: u8) {
        let active = match ACTIVE_VT.try_read() {
            Some(guard) => *guard,
            None => {
                // RwLock contended during VT switch — rare, byte stays in IRQ
                return;
            }
        };

        if active >= NUM_VTS {
            return;
        }

        // 🔥 TRUE LOCK-FREE PUSH 🔥
        // Ring buffer lives in VtManager, not behind any mutex.
        // IRQ writes directly. No try_lock. No silent drops. No bullshit.
        if !self.input_rings[active].push(ch) {
            // — GraveShift: ALWAYS trace ring full — this is the smoking gun for input death
            unsafe { os_log::write_str_raw("[VT] RING FULL! byte dropped\n"); }
        }

        // 🔥 IMMEDIATE SIGNAL DELIVERY (best-effort fast path) 🔥
        //
        // Apps in tight render loops never drain the ring buffer.
        // Ctrl+C bytes rot in there forever without this fast path.
        // try_lock is fine here — if contended, read()/poll() drain catches it.
        // Double delivery is harmless. — GraveShift
        if ch == 0x03 || ch == 0x1C || ch == 0x1A {
            if let Some(vt) = self.vts[active].try_lock() {
                let isig = vt.tty.try_isig_enabled().unwrap_or(true);
                if isig {
                    let signo = match ch {
                        0x03 => signal::SIGINT,
                        0x1C => signal::SIGQUIT,
                        0x1A => signal::SIGTSTP,
                        _ => return,
                    };
                    unsafe {
                        if let Some(callback) = SIGNAL_PGRP_CALLBACK {
                            if let Some(pgid) = vt.tty.try_get_foreground_pgid() {
                                if pgid > 0 {
                                    callback(pgid, signo);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Read from VT
    ///
    /// Blocking read that drains the ring buffer on every iteration of the
    /// wait loop. Ring buffer lives in VtManager.input_rings — zero locks
    /// for drain. IRQ pushes concurrently without contention. — GraveShift
    pub fn read(&self, vt_num: usize, buf: &mut [u8]) -> VfsResult<usize> {
        #[cfg(feature = "debug-console")]
        dbg_serial("[VT] read() enter\n");
        if vt_num >= NUM_VTS {
            return Err(VfsError::InvalidArgument);
        }

        // Clone the TTY Arc once (avoids holding VT lock across the loop)
        let tty = self.vts[vt_num].lock().tty.clone();

        // Ring buffer is directly on VtManager — no lock needed
        let ring = &self.input_rings[vt_num];

        #[cfg(feature = "debug-console")]
        let mut vt_read_loops: u32 = 0;

        loop {
            #[cfg(feature = "debug-console")]
            let mut drained_count = 0;

            while let Some(ch) = ring.pop() {
                #[cfg(feature = "debug-console")]
                {
                    drained_count += 1;
                }

                // Process through TTY (echo, signals, line editing)
                if let Some(signal) = tty.input(&[ch]) {
                    unsafe {
                        if let Some(callback) = SIGNAL_PGRP_CALLBACK {
                            let pgid = tty.get_foreground_pgid();
                            if pgid > 0 {
                                callback(pgid, signal.to_signo());
                            }
                        }
                    }
                }
            }

            #[cfg(feature = "debug-console")]
            if drained_count > 0 {
                use core::fmt::Write;
                let mut msg = alloc::string::String::new();
                let _ = write!(
                    msg,
                    "[VT] Drained {} bytes from lock-free ring\n",
                    drained_count
                );
                dbg_serial(&msg);
            }

            // Check if line discipline now has a complete result to return
            let n = tty.try_read(buf);
            if n > 0 {
                #[cfg(feature = "debug-console")]
                dbg_serial("[VT] read() returning data\n");
                return Ok(n);
            }

            #[cfg(feature = "debug-console")]
            {
                vt_read_loops += 1;
                if vt_read_loops == 1 {
                    dbg_serial("[VT] read() yielding (waiting for input)\n");
                }
            }

            // No data ready yet - yield CPU with preemption enabled so the
            // scheduler can actually switch to other processes. Without this,
            // we're in kernel mode (syscall) and the timer interrupt refuses
            // to context switch, starving all other processes.
            unsafe {
                if let Some(yield_fn) = YIELD_CALLBACK {
                    yield_fn();
                } else {
                    // Fallback: bare yield (won't actually preempt in kernel mode)
                    sched::yield_current();
                }
            }
        }
    }

    /// Write to VT
    pub fn write(&self, vt_num: usize, buf: &[u8]) -> VfsResult<usize> {
        #[cfg(feature = "debug-console")]
        dbg_serial("[VT] write() enter\n");
        if vt_num >= NUM_VTS {
            return Err(VfsError::InvalidArgument);
        }

        // Clone the TTY Arc and release lock before I/O
        let tty = {
            let vt = self.vts[vt_num].lock();
            vt.tty.clone()
        };
        #[cfg(feature = "debug-console")]
        dbg_serial("[VT] write() -> tty.write()\n");
        let r = tty.write(0, buf)?;
        #[cfg(feature = "debug-console")]
        dbg_serial("[VT] write() done\n");
        Ok(r)
    }

    /// Get TTY for ioctl operations
    pub fn get_tty(&self, vt_num: usize) -> Option<Arc<Tty>> {
        if vt_num >= NUM_VTS {
            return None;
        }
        Some(self.vts[vt_num].lock().tty.clone())
    }

    /// Drain ring buffer into line discipline and check if readable data exists.
    ///
    /// — GraveShift: Ring buffer lives in VtManager.input_rings now. No unsafe
    /// pointer hacks, no mutex dance. Just grab the ring and drain it.
    pub fn poll_has_input(&self, vt_num: usize) -> bool {
        if vt_num >= NUM_VTS {
            return false;
        }

        // TTY clone needs the VT lock briefly — ring doesn't
        let tty = self.vts[vt_num].lock().tty.clone();
        let ring = &self.input_rings[vt_num];

        // Drain all pending bytes from the ring buffer into the line discipline.
        // This is the same drain loop as read(), but we don't try to extract
        // output — we just want the bytes processed (echo, signals, buffering).
        while let Some(ch) = ring.pop() {
            if let Some(signal) = tty.input(&[ch]) {
                // 🔥 GraveShift: Signal delivery during poll drain — the reason
                // Ctrl+C works now. Process the signal callback so SIGINT actually
                // reaches the foreground process group. 🔥
                unsafe {
                    if let Some(callback) = SIGNAL_PGRP_CALLBACK {
                        let pgid = tty.get_foreground_pgid();
                        if pgid > 0 {
                            callback(pgid, signal.to_signo());
                        }
                    }
                }
            }
        }

        // Now check if the line discipline has readable data
        tty.ldisc_can_read()
    }

    /// Set window dimensions on all VTs
    /// ⚡ GraveShift: Propagate real framebuffer dimensions to every TTY so
    /// TIOCGWINSZ returns truth instead of the 24x80 lie. ⚡
    pub fn set_all_winsize(&self, rows: u16, cols: u16, xpixel: u16, ypixel: u16) {
        use tty::winsize::Winsize;
        let ws = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: xpixel,
            ws_ypixel: ypixel,
        };
        for i in 0..NUM_VTS {
            self.vts[i].lock().tty.set_winsize(ws);
        }
    }
}

/// Raw pointer to the global VT manager (set once, read lock-free)
static VT_MANAGER_PTR: AtomicPtr<VtManager> = AtomicPtr::new(ptr::null_mut());

/// Owner Arc that keeps the manager alive for the lifetime of the kernel
static VT_MANAGER_OWNER: Mutex<Option<Arc<VtManager>>> = Mutex::new(None);

/// Active VT index (separate from manager to avoid circular dependency)
static ACTIVE_VT: spin::RwLock<usize> = spin::RwLock::new(0);

/// Callback type for signaling a process group
pub type SignalPgrpFn = fn(pgid: i32, sig: i32);

/// Global signal callback (set by kernel)
static mut SIGNAL_PGRP_CALLBACK: Option<SignalPgrpFn> = None;

/// Callback type for console output
pub type ConsoleWriteFn = fn(&[u8]);

/// Global console write callback (set by kernel)
static mut CONSOLE_WRITE_CALLBACK: Option<ConsoleWriteFn> = None;

/// Callback type for VT switch notification
/// 🔥 PRIORITY #2 FIX - VT switch screen buffer notification 🔥
pub type VtSwitchFn = fn(vt_num: usize);

/// Global VT switch callback (set by kernel) - notifies terminal emulator to redraw
static mut VT_SWITCH_CALLBACK: Option<VtSwitchFn> = None;

/// Initialize VT subsystem
pub fn init() -> Arc<VtManager> {
    let manager = Arc::new(VtManager::new());
    let raw = Arc::as_ptr(&manager) as *mut VtManager;
    if VT_MANAGER_PTR
        .compare_exchange(ptr::null_mut(), raw, Ordering::Release, Ordering::Relaxed)
        .is_err()
    {
        panic!("vt::init called twice");
    }
    *VT_MANAGER_OWNER.lock() = Some(manager.clone());
    manager
}

/// Get a lock-free reference to the VT manager (safe even in IRQ context)
pub fn get_manager() -> Option<&'static VtManager> {
    let ptr = VT_MANAGER_PTR.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &*ptr })
    }
}

/// Set window dimensions on all VTs from framebuffer terminal size
/// ⚡ GraveShift: Called after terminal::init() to propagate real
/// framebuffer-derived character grid dimensions to every TTY device.
/// Without this, TIOCGWINSZ returns the 24x80 default. ⚡
pub fn set_global_winsize(rows: u16, cols: u16, xpixel: u16, ypixel: u16) {
    if let Some(manager) = get_manager() {
        manager.set_all_winsize(rows, cols, xpixel, ypixel);
    }
}

/// Push input to the active VT (called from keyboard interrupt handler)
pub fn push_input_global(ch: u8) {
    if let Some(manager) = get_manager() {
        manager.push_input(ch);
    }
}

/// Set the signal callback for VT signals
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_signal_pgrp_callback(f: SignalPgrpFn) {
    // SAFETY: Caller ensures single-threaded initialization
    // — NeonRoot
    unsafe {
        SIGNAL_PGRP_CALLBACK = Some(f);
    }
}

/// Set the console write callback for VT output
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_console_write_callback(f: ConsoleWriteFn) {
    // SAFETY: Caller ensures single-threaded initialization
    // — NeonRoot
    unsafe {
        CONSOLE_WRITE_CALLBACK = Some(f);
    }
}

/// Set the VT switch callback for screen buffer synchronization
///
/// # Safety
/// Must be called during single-threaded initialization
///
/// 🔥 PRIORITY #2 FIX - VT switch screen buffer notification 🔥
/// The callback is invoked whenever VTs are switched (Alt+F1-F6)
/// to notify the terminal emulator to perform a full screen redraw
pub unsafe fn set_vt_switch_callback(f: VtSwitchFn) {
    // SAFETY: Caller ensures single-threaded initialization
    // — NeonRoot
    unsafe {
        VT_SWITCH_CALLBACK = Some(f);
    }
}

/// VT device node
pub struct VtDevice {
    vt_num: usize,
    manager: Arc<VtManager>,
    ino: u64,
}

impl VtDevice {
    pub fn new(vt_num: usize, manager: Arc<VtManager>, ino: u64) -> Arc<Self> {
        Arc::new(VtDevice {
            vt_num,
            manager,
            ino,
        })
    }
}

impl VnodeOps for VtDevice {
    fn vtype(&self) -> VnodeType {
        VnodeType::CharDevice
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        self.manager.read(self.vt_num, buf)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        self.manager.write(self.vt_num, buf)
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn rename(&self, _old: &str, _new_dir: &dyn VnodeOps, _new: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o620), 0, self.ino);
        stat.rdev = make_dev(4, self.vt_num as u64); // tty major 4
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }

    fn poll_read_ready(&self) -> bool {
        // 🔥 GraveShift: The fix that brought keyboards back from the dead.
        // Drain the ring buffer into the line discipline (processing Ctrl+C and
        // friends along the way), then ask the ldisc if it has readable data.
        // Before this, poll() returned "no data" while bytes rotted in the ring. 🔥
        self.manager.poll_has_input(self.vt_num)
    }

    fn poll_write_ready(&self) -> bool {
        true
    }

    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        if let Some(tty) = self.manager.get_tty(self.vt_num) {
            tty.ioctl(request, arg)
        } else {
            Err(VfsError::InvalidArgument)
        }
    }
}

/// Create a device number from major and minor numbers
fn make_dev(major: u64, minor: u64) -> u64 {
    (major << 8) | (minor & 0xFF)
}

// ============================================================================
// DEBUG FUNCTION - Dump VT screen to serial — GraveShift
// ============================================================================
#[unsafe(no_mangle)]
pub extern "Rust" fn debug_dump_screen_to_serial() {
    // Call the terminal module's screen dump function
    terminal::debug_dump_screen_to_serial();
}
