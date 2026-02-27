//! TTY device abstraction
//!
//! Provides the core TTY device that combines line discipline with a hardware driver.

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

use crate::ldisc::{LineDiscipline, Signal};
use crate::termios::{
    FIONREAD, TCGETS, TCSETS, TCSETSF, TCSETSW, TIOCGPGRP, TIOCGWINSZ, TIOCSPGRP, TIOCSWINSZ,
    Termios,
};
use crate::winsize::Winsize;

/// Callback for signaling process groups (set by kernel)
/// 🔥 GraveShift: Signal delivery for SIGWINCH on window resize 🔥
pub type SignalPgrpFn = fn(pgid: i32, sig: i32);
static mut SIGNAL_PGRP_CALLBACK: Option<SignalPgrpFn> = None;

/// Set the signal callback (called by kernel during init)
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

/// TTY driver operations - implemented by the actual hardware driver
pub trait TtyDriver: Send + Sync {
    /// Write data to the hardware
    fn write(&self, data: &[u8]);

    /// Check if output is possible (not blocked)
    fn can_write(&self) -> bool {
        true
    }

    /// Flush output
    fn flush(&self) {}
}

// External scheduler functions (same as in pipe.rs)
// Can't depend on sched crate directly due to circular dependencies
unsafe extern "Rust" {
    fn sched_block_interruptible();
    fn sched_block_deciseconds(deciseconds: u8) -> bool;
    fn sched_wake_up(pid: u32);
    fn sched_current_pid() -> Option<u32>;
}

/// TTY device
pub struct Tty {
    /// Line discipline
    ldisc: Mutex<LineDiscipline>,
    /// Window size
    winsize: Mutex<Winsize>,
    /// Foreground process group
    foreground_pgid: Mutex<i32>,
    /// Session ID
    session: Mutex<i32>,
    /// Hardware driver
    driver: Arc<dyn TtyDriver>,
    /// Inode number
    ino: u64,
    /// Device number
    dev: u64,
    /// PIDs of processes waiting to read (no data available)
    /// 🔥 NO MORE SPINLOOPS - Proper blocking like a real OS 🔥
    read_waiters: Mutex<Vec<u32>>,
    /// Non-blocking mode flag (set via fcntl F_SETFL O_NONBLOCK)
    /// 🔥 PRIORITY #1 FIX - O_NONBLOCK support 🔥
    nonblocking: core::sync::atomic::AtomicBool,
}

impl Tty {
    /// Create a new TTY with the given driver
    pub fn new(driver: Arc<dyn TtyDriver>, ino: u64, dev: u64) -> Arc<Self> {
        Arc::new(Tty {
            ldisc: Mutex::new(LineDiscipline::new()),
            winsize: Mutex::new(Winsize::new()),
            foreground_pgid: Mutex::new(0),
            session: Mutex::new(0),
            driver,
            ino,
            dev,
            read_waiters: Mutex::new(Vec::new()),
            nonblocking: core::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Process input from hardware
    ///
    /// Called by the driver when input is received.
    /// Returns a signal if one should be delivered.
    pub fn input(&self, data: &[u8]) -> Option<Signal> {
        let mut signal = None;
        let mut echo_buf = Vec::new();

        // Process input while holding ldisc lock, but defer driver writes until
        // after the lock is released to avoid LDISC -> TERMINAL lock nesting.
        {
            let mut ldisc = self.ldisc.lock();

            for &c in data {
                // Process character with echo callback
                let sig = ldisc.input_char(c, |echo_data| {
                    echo_buf.extend_from_slice(echo_data);
                });

                if sig.is_some() {
                    signal = sig;
                }
            }
        } // Release ldisc lock

        if !echo_buf.is_empty() {
            self.driver.write(&echo_buf);
        }

        // Wake all processes waiting to read
        // 🔥 NO MORE SPINLOOPS - Wake sleeping readers when data arrives 🔥
        let waiters = {
            let mut w = self.read_waiters.lock();
            let pids = w.clone();
            w.clear();
            pids
        };

        for pid in waiters {
            unsafe {
                sched_wake_up(pid);
            }
        }

        signal
    }

    /// Get termios settings
    pub fn get_termios(&self) -> Termios {
        *self.ldisc.lock().termios()
    }

    /// Set termios settings
    pub fn set_termios(&self, termios: Termios) {
        self.ldisc.lock().set_termios(termios);
    }

    /// Get window size
    pub fn get_winsize(&self) -> Winsize {
        *self.winsize.lock()
    }

    /// Set window size
    pub fn set_winsize(&self, winsize: Winsize) {
        *self.winsize.lock() = winsize;
    }

    /// Get foreground process group
    pub fn get_foreground_pgid(&self) -> i32 {
        *self.foreground_pgid.lock()
    }

    /// Set foreground process group
    pub fn set_foreground_pgid(&self, pgid: i32) {
        *self.foreground_pgid.lock() = pgid;
    }

    /// Get session ID
    pub fn get_session(&self) -> i32 {
        *self.session.lock()
    }

    /// Set session (set controlling terminal)
    pub fn set_session(&self, sid: i32) {
        *self.session.lock() = sid;
    }

    /// Check if line discipline has readable data — for poll() without VnodeOps
    /// — GraveShift: Because sometimes you need the answer without the ceremony
    pub fn ldisc_can_read(&self) -> bool {
        self.ldisc.lock().can_read()
    }

    /// Non-blocking foreground pgid read — ISR-safe via try_lock
    /// — GraveShift: For when you need the pgid but can't afford to spin forever
    pub fn try_get_foreground_pgid(&self) -> Option<i32> {
        self.foreground_pgid.try_lock().map(|g| *g)
    }

    /// Check if ISIG is enabled in the line discipline (signals on Ctrl+C etc.)
    /// — GraveShift: Non-blocking check for ISR signal delivery fast-path
    pub fn try_isig_enabled(&self) -> Option<bool> {
        self.ldisc.try_lock().map(|l| l.isig_enabled())
    }

    /// Flush input buffer
    pub fn flush_input(&self) {
        self.ldisc.lock().flush_input();
    }

    /// Non-blocking read: returns data if available, 0 bytes if not.
    ///
    /// Unlike the VnodeOps::read() which spinloops, this returns immediately.
    /// Used by VtManager to own the blocking loop so it can drain input_buffer
    /// on every iteration.
    pub fn try_read(&self, buf: &mut [u8]) -> usize {
        let mut ldisc = self.ldisc.lock();
        if ldisc.can_read() { ldisc.read(buf) } else { 0 }
    }

    /// Non-blocking check if a byte is a signal character
    ///
    /// Called from interrupt context (push_input) to detect signal characters
    /// like Ctrl+C (0x03) without going through the full line discipline.
    /// Uses try_lock to avoid blocking. Returns the signal and foreground pgid
    /// if this byte should generate a signal, None otherwise.
    pub fn try_check_signal(&self, ch: u8) -> Option<(Signal, i32)> {
        // Try to lock ldisc without blocking (interrupt context)
        let ldisc = self.ldisc.try_lock()?;
        let termios = ldisc.termios();

        // Check if ISIG is enabled
        if !termios.c_lflag.contains(crate::termios::LocalFlags::ISIG) {
            return None;
        }

        // Check against configured signal characters
        let signal = if ch == termios.c_cc[crate::termios::VINTR] {
            Signal::Int
        } else if ch == termios.c_cc[crate::termios::VQUIT] {
            Signal::Quit
        } else if ch == termios.c_cc[crate::termios::VSUSP] {
            Signal::Tstp
        } else {
            return None;
        };

        drop(ldisc);

        // Get foreground pgid (also non-blocking)
        let pgid = self.foreground_pgid.try_lock().map(|g| *g).unwrap_or(0);

        Some((signal, pgid))
    }

    /// Flush output
    pub fn flush_output(&self) {
        self.driver.flush();
    }

    /// Set non-blocking mode
    ///
    /// 🔥 PRIORITY #1 FIX - O_NONBLOCK support 🔥
    /// Called by fcntl when F_SETFL sets/clears O_NONBLOCK flag
    pub fn set_nonblocking(&self, nonblocking: bool) {
        self.nonblocking
            .store(nonblocking, core::sync::atomic::Ordering::Relaxed);
    }

    /// Check if in non-blocking mode
    pub fn is_nonblocking(&self) -> bool {
        self.nonblocking.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Handle ioctl
    pub fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        match request {
            TCGETS => {
                // Get termios - arg is pointer to Termios
                let termios = self.get_termios();
                let ptr = arg as *mut Termios;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                unsafe {
                    *ptr = termios;
                }
                Ok(0)
            }
            TCSETS | TCSETSW | TCSETSF => {
                // Set termios - arg is pointer to Termios
                let ptr = arg as *const Termios;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                let termios = unsafe { *ptr };

                // TCSETSW: wait for output to drain first
                if request == TCSETSW || request == TCSETSF {
                    self.flush_output();
                }

                // TCSETSF: also flush input
                if request == TCSETSF {
                    self.flush_input();
                }

                self.set_termios(termios);
                Ok(0)
            }
            TIOCGWINSZ => {
                // Get window size
                let winsize = self.get_winsize();
                let ptr = arg as *mut Winsize;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                unsafe {
                    *ptr = winsize;
                }
                Ok(0)
            }
            TIOCSWINSZ => {
                // Set window size
                let ptr = arg as *const Winsize;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                let winsize = unsafe { *ptr };
                self.set_winsize(winsize);

                // 🔥 GraveShift: Send SIGWINCH (28) to foreground process group 🔥
                // Vim and other full-screen apps need this to redraw on resize
                unsafe {
                    if let Some(callback) = SIGNAL_PGRP_CALLBACK {
                        let pgid = self.get_foreground_pgid();
                        if pgid > 0 {
                            const SIGWINCH: i32 = 28;
                            callback(pgid, SIGWINCH);
                        }
                    }
                }
                Ok(0)
            }
            TIOCGPGRP => {
                // Get foreground process group
                let ptr = arg as *mut i32;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                unsafe {
                    *ptr = self.get_foreground_pgid();
                }
                Ok(0)
            }
            TIOCSPGRP => {
                // Set foreground process group
                let ptr = arg as *const i32;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                let pgid = unsafe { *ptr };
                self.set_foreground_pgid(pgid);
                Ok(0)
            }
            FIONREAD => {
                // Get number of bytes available to read
                let ptr = arg as *mut i32;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                let available = self.ldisc.lock().input_available();
                unsafe {
                    *ptr = available as i32;
                }
                Ok(0)
            }
            0x5490 => {
                // TIOC_SET_NONBLOCK - Custom ioctl for fcntl O_NONBLOCK support
                // 🔥 GraveShift: fcntl F_SETFL needs to notify TTY when O_NONBLOCK changes 🔥
                // arg: 0 = blocking, 1 = non-blocking
                self.set_nonblocking(arg != 0);
                Ok(0)
            }
            _ => Err(VfsError::NotSupported),
        }
    }
}

impl VnodeOps for Tty {
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
        // 🔥 NO MORE SPINLOOPS - Proper blocking with wait queues 🔥
        // 🔥 NOW WITH VTIME SUPPORT (Priority #7 Fix) 🔥
        // 🔥 NOW WITH O_NONBLOCK SUPPORT (Priority #1 Fix) 🔥
        //
        // Before: Spinloop waking up every 10ms, checking, yielding. VTIME ignored.
        // After:  Block in TASK_INTERRUPTIBLE, wake only when data arrives.
        //         VTIME implemented with timed blocking.
        //         O_NONBLOCK returns EAGAIN immediately if no data.
        //
        // CPU usage drops from ~100% spinning to 0% while waiting.
        // Welcome to having a real OS.

        // 🔥 PRIORITY #1 FIX: Check O_NONBLOCK flag first 🔥
        // If set via fcntl(fd, F_SETFL, O_NONBLOCK), return EAGAIN immediately when no data
        let is_nonblocking = self.is_nonblocking();

        // Get VMIN and VTIME from termios (need to check once before loop)
        use crate::termios::{VMIN, VTIME};
        let (vmin, vtime) = {
            let ldisc = self.ldisc.lock();
            let termios = ldisc.termios();
            (termios.c_cc[VMIN], termios.c_cc[VTIME])
        };

        loop {
            // Try to read with lock held (keep critical section small)
            let (has_data, available) = {
                let mut ldisc = self.ldisc.lock();

                // Check if data is available
                if ldisc.can_read() {
                    let count = ldisc.read(buf);

                    #[cfg(feature = "debug-tty-read")]
                    os_log::println!(
                        "[TTY-READ] Tty::read returning {} bytes (buf.len()={})",
                        count,
                        buf.len()
                    );

                    return Ok(count);
                }

                (false, ldisc.input_available())
            }; // Release lock!

            // No data available - check VTIME behavior
            if !has_data {
                // 🔥 PRIORITY #1 FIX: O_NONBLOCK takes precedence 🔥
                // If O_NONBLOCK is set, return EAGAIN immediately
                if is_nonblocking {
                    return Err(VfsError::WouldBlock);
                }

                // VMIN=0, VTIME=0: Non-blocking, return immediately
                if vmin == 0 && vtime == 0 {
                    return Ok(0);
                }

                // VMIN=0, VTIME>0: Timeout read - wait VTIME deciseconds, return available data
                // VMIN>0, VTIME=0: Block indefinitely until VMIN bytes available
                // VMIN>0, VTIME>0: Interbyte timeout 🔥 PRIORITY #3 FIX 🔥
                //   - Timer starts when first character received
                //   - Timer resets on each new character
                //   - Return when VMIN reached OR timer expires since last character

                // Get current PID and add to wait queue
                let pid = unsafe { sched_current_pid() };
                if let Some(pid) = pid {
                    {
                        let mut waiters = self.read_waiters.lock();
                        if !waiters.contains(&pid) {
                            waiters.push(pid);
                        }
                    }

                    // Block with timeout if VTIME > 0
                    if vtime > 0 && vmin == 0 {
                        // Timeout read: block for VTIME deciseconds, return available data
                        let _timeout_expired = unsafe { sched_block_deciseconds(vtime) };

                        // Remove ourselves from wait queue
                        let mut waiters = self.read_waiters.lock();
                        waiters.retain(|&p| p != pid);

                        // Return whatever data is available (may be 0 if timeout expired)
                        let mut ldisc = self.ldisc.lock();
                        let available = ldisc.input_available();
                        if available > 0 {
                            let count = ldisc.read(buf);
                            return Ok(count);
                        } else {
                            return Ok(0); // Timeout with no data
                        }
                    } else if vtime > 0 && vmin > 0 {
                        // 🔥 PRIORITY #3 FIX - Interbyte timeout 🔥
                        // VMIN>0, VTIME>0: Block with interbyte timeout
                        // Wait for VMIN characters OR timeout between characters
                        //
                        // Implementation: Use decisecond blocking for interbyte timeout.
                        // If we already have some data (available > 0), start interbyte timer.
                        // Otherwise, block indefinitely waiting for first character.
                        if available > 0 {
                            // Have some data, start interbyte timeout
                            let _timeout_expired = unsafe { sched_block_deciseconds(vtime) };

                            // Remove ourselves from wait queue
                            let mut waiters = self.read_waiters.lock();
                            waiters.retain(|&p| p != pid);

                            // Return whatever we have (timeout or more data arrived)
                            let mut ldisc = self.ldisc.lock();
                            let count = ldisc.read(buf);
                            return Ok(count);
                        } else {
                            // No data yet, block indefinitely for first character
                            unsafe {
                                sched_block_interruptible();
                            }

                            // When we wake up, remove ourselves and retry
                            let mut waiters = self.read_waiters.lock();
                            waiters.retain(|&p| p != pid);
                        }
                    } else {
                        // Block indefinitely (VMIN>0, VTIME=0 or default behavior)
                        // Will wake when: keyboard input arrives OR signal delivered (Ctrl+C)
                        unsafe {
                            sched_block_interruptible();
                        }

                        // When we wake up, remove ourselves and retry
                        let mut waiters = self.read_waiters.lock();
                        waiters.retain(|&p| p != pid);
                    }
                }
                // Loop back and retry the read
            }
        }
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // 🔥 GraveShift: Process output with lock held, then release before hardware write
        // Holding lock during driver.write() caused deadlock with timer interrupts
        let output = {
            let ldisc = self.ldisc.lock();

            // Pre-allocate output buffer (worst case: 2x size for \n -> \r\n expansion)
            let mut output = Vec::with_capacity(buf.len() * 2);

            // Process output through line discipline (OPTIMIZED: single pass, single allocation)
            ldisc.process_output_bulk(buf, &mut output);

            output
        }; // Release lock!

        // Write to hardware without holding lock
        self.driver.write(&output);

        Ok(buf.len())
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

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o620), 0, self.ino);
        stat.rdev = self.dev;
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }

    fn poll_read_ready(&self) -> bool {
        self.ldisc.lock().can_read()
    }

    fn poll_write_ready(&self) -> bool {
        // TTYs are always ready for writing (output buffer is unbounded)
        true
    }
}

/// Simple driver that writes to a callback function
pub struct CallbackDriver {
    write_fn: fn(&[u8]),
}

impl CallbackDriver {
    pub fn new(write_fn: fn(&[u8])) -> Arc<Self> {
        Arc::new(CallbackDriver { write_fn })
    }
}

impl TtyDriver for CallbackDriver {
    fn write(&self, data: &[u8]) {
        (self.write_fn)(data);
    }
}
