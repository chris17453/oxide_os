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
        })
    }

    /// Process input from hardware
    ///
    /// Called by the driver when input is received.
    /// Returns a signal if one should be delivered.
    pub fn input(&self, data: &[u8]) -> Option<Signal> {
        let mut ldisc = self.ldisc.lock();
        let mut signal = None;

        for &c in data {
            // Process character with echo callback
            let sig = ldisc.input_char(c, |echo_data| {
                self.driver.write(echo_data);
            });

            if sig.is_some() {
                signal = sig;
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
                // Should send SIGWINCH to foreground process group
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
        // Spinloop waiting for input - yield CPU while waiting
        loop {
            {
                let mut ldisc = self.ldisc.lock();

                // Check if data is available
                if ldisc.can_read() {
                    let count = ldisc.read(buf);

                    #[cfg(feature = "debug-tty-read")]
                    {
                        use arch_x86_64::serial;
                        use core::fmt::Write;
                        let _ = write!(serial::SerialWriter, "[TTY-READ] Tty::read returning {} bytes (buf.len()={})\n",
                            count, buf.len());
                    }

                    return Ok(count);
                }
            }

            // No data available - yield to other processes and try again
            sched::yield_current();
        }
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        let ldisc = self.ldisc.lock();

        // Pre-allocate output buffer (worst case: 2x size for \n -> \r\n expansion)
        let mut output = Vec::with_capacity(buf.len() * 2);

        // Process output through line discipline (OPTIMIZED: single pass, single allocation)
        ldisc.process_output_bulk(buf, &mut output);

        // Write to hardware
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
