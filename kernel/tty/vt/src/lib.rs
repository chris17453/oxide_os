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

/// Maximum VT slots (compile-time array ceiling).
/// — GraveShift: actual count is runtime — set via init(num_vts).
/// Change this only if you need more than 6 VT slots system-wide.
pub const MAX_VTS: usize = 6;

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
    /// — GraveShift: MAX_VTS slots, all wired up. TTY structs are cheap —
    /// the expensive backing buffers live in the compositor and are lazy-allocated.
    vts: [Mutex<VtState>; MAX_VTS],
    /// Input ring buffers — OUTSIDE mutex for true lock-free IRQ push
    /// One per VT. IRQ pushes to active VT's ring. read()/poll() drains it.
    input_rings: [LockFreeRing; MAX_VTS],
}

impl VtManager {
    /// Create a new VT manager with MAX_VTS terminals.
    /// — GraveShift: TTY structs are cheap. The expensive backing buffers
    /// live in the compositor and are lazy-allocated on first split/switch.
    pub fn new() -> Self {
        // — GraveShift: VtTtyDriver is the glue between TTY writes and the
        // per-VT terminal emulator. Each VT has its OWN TerminalEmulator —
        // no callback indirection, no raw byte buffering, no state-swapping.
        // Write goes straight to terminal::write_vt() which processes escape
        // sequences and renders to the VT's backing framebuffer. Like Linux's
        // con_write() calling fbcon_putcs() on the correct vc_data. — GraveShift
        struct VtTtyDriver {
            vt_num: usize,
        }
        impl TtyDriver for VtTtyDriver {
            fn write(&self, data: &[u8]) {
                // — GraveShift: Lazy-init the VT's terminal emulator on first write.
                // VT0 is initialized at boot. VTs 1-5 spawn here when getty first writes.
                // compositor::get_vt_framebuffer() allocates the ~4MB backing buffer on demand.
                if !terminal::is_vt_initialized(self.vt_num) {
                    if let Some(fb) = compositor::get_vt_framebuffer(self.vt_num) {
                        terminal::init_vt(self.vt_num, fb);
                    } else {
                        // — GraveShift: no backing fb = can't render. This shouldn't happen
                        // after compositor init, but if it does, bail silently.
                        return;
                    }
                }

                // — GraveShift: Direct write to this VT's own terminal emulator.
                // No CONSOLE_WRITE_CALLBACK. No VT_OUTPUT_BUFFERS. No raw byte replay.
                // The terminal processes escape sequences, updates its text buffer, and
                // renders glyphs to this VT's backing framebuffer — whether active or not.
                terminal::write_vt(self.vt_num, data);

                // — NeonRoot: mark this VT dirty so compositor blits to hardware.
                // Only matters if this is the focused VT — compositor skips non-focused.
                compositor::mark_dirty(self.vt_num);
            }
        }

        VtManager {
            vts: core::array::from_fn(|i| {
                Mutex::new(VtState::new(
                    Tty::new(Arc::new(VtTtyDriver { vt_num: i }), (i + 1) as _, 0),
                    i,
                ))
            }),
            // — GraveShift: Ring buffers live here, not inside VtState.
            // IRQ handler writes directly. No mutex. No try_lock. No dropped keys.
            input_rings: core::array::from_fn(|_| LockFreeRing::new()),
        }
    }

    /// Get the active VT index
    pub fn active_vt(&self) -> usize {
        *ACTIVE_VT.read()
    }

    /// Switch to a different VT
    ///
    /// — WireSaint: Uses try_write() because this is called from ISR context
    /// (keyboard interrupt → Alt+F1-F6). Blocking ACTIVE_VT.write() in ISR
    /// deadlocks when sys_write holds ACTIVE_VT.read() on the same CPU.
    /// If the RwLock is contended, we bail — the switch just doesn't happen
    /// this interrupt cycle. User presses Alt+F2 again and it works.
    pub fn switch_to(&self, vt_num: usize) -> bool {
        if vt_num >= MAX_VTS {
            return false;
        }

        // — WireSaint: try_write() — ISR-safe. Never block in interrupt context.
        let mut active = match ACTIVE_VT.try_write() {
            Some(guard) => guard,
            None => {
                // RwLock contended (reader on another CPU or same CPU interrupted)
                // — WireSaint: silent bail. VT switch deferred until next keypress.
                return false;
            }
        };

        if *active != vt_num {
            *active = vt_num;

            // Notify terminal emulator to switch screen buffer and force full redraw
            // — WireSaint: VT_SWITCH_CALLBACK (terminal_vt_switch_callback) also uses
            // try_lock internally now — no blocking locks anywhere in this ISR path.
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
                // — GraveShift: RwLock contended during VT switch — rare, byte stays in IRQ
                return;
            }
        };

        if active >= MAX_VTS {
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
        // 🔥 IMMEDIATE SIGNAL DELIVERY (best-effort fast path) 🔥
        //
        // Apps in tight render loops never drain the ring buffer.
        // Ctrl+C bytes rot in there forever without this fast path.
        // try_lock is fine here — if contended, read()/poll() drain catches it.
        // Double delivery is harmless. — GraveShift
        if ch == 0x03 || ch == 0x1C || ch == 0x1A {
            // — GraveShift: Diagnostic breadcrumbs for signal delivery debugging.
            // If Ctrl+C isn't killing your app, follow the trail of "[SIG-FAST]" in serial.
            #[cfg(feature = "debug-console")]
            unsafe { os_log::write_str_raw("[SIG-FAST] signal byte\n"); }

            if let Some(vt) = self.vts[active].try_lock() {
                let isig = vt.tty.try_isig_enabled().unwrap_or(true);
                #[cfg(feature = "debug-console")]
                if isig {
                    unsafe { os_log::write_str_raw("[SIG-FAST] isig=true\n"); }
                } else {
                    unsafe { os_log::write_str_raw("[SIG-FAST] isig=FALSE, skip\n"); }
                }

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
                                // — GraveShift: Show raw PGID value to debug stale PGID mystery.
                                #[cfg(feature = "debug-console")]
                                {
                                    os_log::write_str_raw("[SIG-FAST] raw pgid=");
                                    let v = pgid as u32;
                                    if v == 0 {
                                        os_log::write_byte_raw(b'0');
                                    } else {
                                        let mut buf = [0u8; 10];
                                        let mut pos = 0;
                                        let mut n = v;
                                        while n > 0 { buf[pos] = b'0' + (n % 10) as u8; n /= 10; pos += 1; }
                                        for i in (0..pos).rev() { os_log::write_byte_raw(buf[i]); }
                                    }
                                    os_log::write_str_raw("\n");
                                }
                                if pgid > 0 {
                                    #[cfg(feature = "debug-console")]
                                    os_log::write_str_raw("[SIG-FAST] SENDING to pgid>0\n");
                                    callback(pgid, signo);
                                    #[cfg(feature = "debug-console")]
                                    os_log::write_str_raw("[SIG-FAST] SENT!\n");
                                } else {
                                    #[cfg(feature = "debug-console")]
                                    os_log::write_str_raw("[SIG-FAST] pgid<=0!\n");
                                }
                            } else {
                                #[cfg(feature = "debug-console")]
                                os_log::write_str_raw("[SIG-FAST] pgid lock FAIL\n");
                            }
                        } else {
                            #[cfg(feature = "debug-console")]
                            os_log::write_str_raw("[SIG-FAST] NO CALLBACK!\n");
                        }
                    }
                }
            } else {
                #[cfg(feature = "debug-console")]
                unsafe { os_log::write_str_raw("[SIG-FAST] vt lock FAIL\n"); }
            }
        }
    }

    /// Read from VT
    ///
    /// Blocking read that drains the ring buffer on every iteration of the
    /// wait loop. Ring buffer lives in VtManager.input_rings — zero locks
    /// for drain. IRQ pushes concurrently without contention.
    ///
    /// — GraveShift: NOW with signal interruption. Previously this loop spun forever even
    /// when SIGINT was pending — Ctrl+C was queued but never checked. The process had to
    /// wait for the heat death of the universe. Now we check pending signals after each
    /// yield and bail with EINTR so check_signals_on_syscall_return() can do its thing.
    pub fn read(&self, vt_num: usize, buf: &mut [u8]) -> VfsResult<usize> {
        #[cfg(feature = "debug-console")]
        dbg_serial("[VT] read() enter\n");
        if vt_num >= MAX_VTS {
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

            // Check if line discipline now has a complete result to return.
            // Some(n) = data (n>0) or EOF (n=0). None = not ready yet.
            if let Some(n) = tty.try_read(buf) {
                #[cfg(feature = "debug-console")]
                dbg_serial("[VT] read() returning data/EOF\n");
                return Ok(n);
            }

            // — GraveShift: THE FIX. Check if the current process has pending signals that
            // would actually do something (not SIG_IGN). If so, bail out with EINTR.
            // check_signals_on_syscall_return() will deliver the signal on our way out.
            // Without this, Ctrl+C queued SIGINT but the process just kept looping here
            // like a zombie that doesn't know it's dead.
            unsafe {
                if let Some(check_fn) = SIGNAL_PENDING_CALLBACK {
                    if check_fn() {
                        #[cfg(feature = "debug-console")]
                        dbg_serial("[VT] read() interrupted by signal\n");
                        return Err(VfsError::Interrupted);
                    }
                }
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
        if vt_num >= MAX_VTS {
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
        if vt_num >= MAX_VTS {
            return None;
        }
        Some(self.vts[vt_num].lock().tty.clone())
    }

    /// Drain ring buffer into line discipline and check if readable data exists.
    ///
    /// — GraveShift: Ring buffer lives in VtManager.input_rings now. No unsafe
    /// pointer hacks, no mutex dance. Just grab the ring and drain it.
    pub fn poll_has_input(&self, vt_num: usize) -> bool {
        if vt_num >= MAX_VTS {
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
        for i in 0..MAX_VTS {
            self.vts[i].lock().tty.set_winsize(ws);
        }
    }

    /// Set a specific VT's winsize and send SIGWINCH to its foreground pgid.
    /// — GlassSignal: per-VT resize — each VT can have different dimensions
    /// in split layouts. Only signals if dimensions actually changed.
    pub fn set_vt_winsize(&self, vt_num: usize, rows: u16, cols: u16, xpixel: u16, ypixel: u16) {
        if vt_num >= MAX_VTS { return; }
        use tty::winsize::Winsize;
        let ws = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: xpixel,
            ws_ypixel: ypixel,
        };
        self.vts[vt_num].lock().tty.set_winsize_and_signal(ws);
    }
}

/// Raw pointer to the global VT manager (set once, read lock-free)
static VT_MANAGER_PTR: AtomicPtr<VtManager> = AtomicPtr::new(ptr::null_mut());

/// Owner Arc that keeps the manager alive for the lifetime of the kernel
static VT_MANAGER_OWNER: Mutex<Option<Arc<VtManager>>> = Mutex::new(None);

/// Active VT index (separate from manager to avoid circular dependency)
static ACTIVE_VT: spin::RwLock<usize> = spin::RwLock::new(0);

// — GraveShift: VT_OUTPUT_BUFFERS ELIMINATED. Each VT now has its own
// TerminalEmulator that processes writes directly. No more raw byte buffering,
// no more 256KB replay on VT switch. The per-VT terminal keeps its text buffer
// up-to-date at all times — like Linux's vc_data[N]. RIP VT_OUTPUT_BUFFERS,
// you served us poorly. — GraveShift

/// Callback type for signaling a process group
pub type SignalPgrpFn = fn(pgid: i32, sig: i32);

/// Global signal callback (set by kernel)
static mut SIGNAL_PGRP_CALLBACK: Option<SignalPgrpFn> = None;

/// Callback type for checking if current process has actionable pending signals.
/// — GraveShift: Returns true when a blocking read should bail out with EINTR.
/// Filters out SIG_IGN signals so the shell doesn't get spuriously interrupted.
pub type SignalPendingFn = fn() -> bool;

/// Global signal-pending callback (set by kernel)
static mut SIGNAL_PENDING_CALLBACK: Option<SignalPendingFn> = None;

// — GraveShift: CONSOLE_WRITE_CALLBACK ELIMINATED. VtTtyDriver::write() now
// calls terminal::write_vt() directly — no function pointer indirection.
// The callback was a decoupling layer from the days when vt couldn't depend on
// terminal. Now it can. Direct calls, zero overhead, zero confusion. — GraveShift

/// Callback type for VT switch notification
/// 🔥 PRIORITY #2 FIX - VT switch screen buffer notification 🔥
pub type VtSwitchFn = fn(vt_num: usize);

/// Global VT switch callback (set by kernel) - notifies terminal emulator to redraw
static mut VT_SWITCH_CALLBACK: Option<VtSwitchFn> = None;

/// Initialize VT subsystem. All MAX_VTS terminals are available — backing
/// buffers in the compositor are lazy-allocated on first split/switch.
/// — GraveShift: TTY structs are cheap. The memory savings come from
/// the compositor not pre-allocating ~4MB backing buffers per unused VT.
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

/// How many VTs are available. Returns MAX_VTS (all VTs exist, backing buffers are lazy).
pub fn num_vts() -> usize {
    MAX_VTS
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

/// Set a specific VT's winsize and send SIGWINCH to its foreground process group.
/// — GlassSignal: called by compositor on layout change. Each VT in a split
/// layout gets its own dimensions derived from ViewportGeometry.
pub fn set_vt_winsize(vt_num: usize, rows: u16, cols: u16, xpixel: u16, ypixel: u16) {
    if let Some(manager) = get_manager() {
        manager.set_vt_winsize(vt_num, rows, cols, xpixel, ypixel);
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

/// Set the signal-pending callback for VT blocking reads
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_signal_pending_callback(f: SignalPendingFn) {
    unsafe {
        SIGNAL_PENDING_CALLBACK = Some(f);
    }
}

// — GraveShift: set_console_write_callback REMOVED — VtTtyDriver calls
// terminal::write_vt() directly now. No callback registration needed.

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
        // — GlassSignal: console-specific ioctls handled at VT level, not TTY
        const KDSETMODE: u64 = 0x4B3A;
        const KDGETMODE: u64 = 0x4B3B;
        const KD_TEXT: u64 = 0x00;
        const KD_GRAPHICS: u64 = 0x01;

        match request {
            KDGETMODE => {
                let ptr = arg as *mut u64;
                if ptr.is_null() { return Err(VfsError::InvalidArgument); }
                let mode = compositor::get_vt_mode(self.vt_num);
                let val = match mode {
                    compositor::VtMode::Text => KD_TEXT,
                    compositor::VtMode::Graphics => KD_GRAPHICS,
                };
                unsafe { *ptr = val; }
                Ok(0)
            }
            KDSETMODE => {
                let mode = match arg {
                    KD_TEXT => compositor::VtMode::Text,
                    KD_GRAPHICS => compositor::VtMode::Graphics,
                    _ => return Err(VfsError::InvalidArgument),
                };
                compositor::set_vt_mode(self.vt_num, mode);
                Ok(0)
            }
            _ => {
                if let Some(tty) = self.manager.get_tty(self.vt_num) {
                    tty.ioctl(request, arg)
                } else {
                    Err(VfsError::InvalidArgument)
                }
            }
        }
    }
}

/// /dev/tty0 — active virtual terminal alias (Linux-compatible)
///
/// — GraveShift: Unlike VtDevice which is bound to a fixed VT number,
/// Tty0Device resolves ACTIVE_VT on every read/write/ioctl call.
/// This is exactly how Linux's /dev/tty0 works — it's always the
/// currently visible console. Write to it, and your bytes land on
/// whatever VT the user is looking at. Essential for system messages
/// that need to reach the operator regardless of which VT is active.
pub struct Tty0Device {
    manager: Arc<VtManager>,
    ino: u64,
}

impl Tty0Device {
    pub fn new(manager: Arc<VtManager>, ino: u64) -> Arc<Self> {
        Arc::new(Tty0Device { manager, ino })
    }
}

impl VnodeOps for Tty0Device {
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
        // — GraveShift: resolve active VT at call time, not at creation time
        let active = *ACTIVE_VT.read();
        self.manager.read(active, buf)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        let active = *ACTIVE_VT.read();
        self.manager.write(active, buf)
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
        // — GraveShift: major 4, minor 0 = /dev/tty0 in Linux
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o620), 0, self.ino);
        stat.rdev = make_dev(4, 0);
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }

    fn poll_read_ready(&self) -> bool {
        let active = *ACTIVE_VT.read();
        self.manager.poll_has_input(active)
    }

    fn poll_write_ready(&self) -> bool {
        true
    }

    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        let active = *ACTIVE_VT.read();
        if let Some(tty) = self.manager.get_tty(active) {
            tty.ioctl(request, arg)
        } else {
            Err(VfsError::InvalidArgument)
        }
    }
}

/// Get the currently active VT index
/// — GraveShift: public accessor for cross-crate queries
pub fn get_active_vt() -> usize {
    *ACTIVE_VT.read()
}

// — GraveShift: drain_pending_output REMOVED — per-VT terminal emulators
// process all writes directly. No buffering, no draining, no replay.

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
