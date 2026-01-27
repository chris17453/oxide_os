//! Virtual Terminal (VT) support for OXIDE OS
//!
//! Provides multiple virtual consoles that can be switched with Ctrl+Alt+F1-F6

#![no_std]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use tty::{Tty, TtyDriver};
use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

/// Number of virtual terminals
pub const NUM_VTS: usize = 6;

/// VT buffer size
const VT_BUF_SIZE: usize = 4096;

/// VT state
struct VtState {
    /// Input buffer
    input_buffer: Vec<u8>,
    /// TTY device
    tty: Arc<Tty>,
    /// VT number (0-5 for tty1-tty6)
    vt_num: usize,
}

impl VtState {
    fn new(tty: Arc<Tty>, vt_num: usize) -> Self {
        VtState {
            input_buffer: Vec::with_capacity(VT_BUF_SIZE),
            tty,
            vt_num,
        }
    }
}

/// Virtual Terminal Manager
pub struct VtManager {
    /// All VTs
    vts: [Mutex<VtState>; NUM_VTS],
    /// Active VT index (0-5)
    active_vt: Mutex<usize>,
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
                if *ACTIVE_VT.read() == self.vt_num {
                    // Write to console output (terminal emulator + serial)
                    unsafe {
                        if let Some(write_fn) = CONSOLE_WRITE_CALLBACK {
                            write_fn(data);
                        }
                    }
                }
            }
        }

        VtManager {
            vts: [
                Mutex::new(VtState::new(Tty::new(Arc::new(VtTtyDriver { vt_num: 0 }), 1, 0), 0)),
                Mutex::new(VtState::new(Tty::new(Arc::new(VtTtyDriver { vt_num: 1 }), 2, 0), 1)),
                Mutex::new(VtState::new(Tty::new(Arc::new(VtTtyDriver { vt_num: 2 }), 3, 0), 2)),
                Mutex::new(VtState::new(Tty::new(Arc::new(VtTtyDriver { vt_num: 3 }), 4, 0), 3)),
                Mutex::new(VtState::new(Tty::new(Arc::new(VtTtyDriver { vt_num: 4 }), 5, 0), 4)),
                Mutex::new(VtState::new(Tty::new(Arc::new(VtTtyDriver { vt_num: 5 }), 6, 0), 5)),
            ],
            active_vt: Mutex::new(0),
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
            // TODO: Notify terminal emulator to switch screen buffer
            true
        } else {
            false
        }
    }

    /// Push input character to active VT
    pub fn push_input(&self, ch: u8) {
        let active = *self.active_vt.lock();
        let mut vt = self.vts[active].lock();

        // Process through TTY line discipline
        if let Some(signal) = vt.tty.input(&[ch]) {
            // Signal was generated - deliver to foreground process group
            unsafe {
                if let Some(callback) = SIGNAL_PGRP_CALLBACK {
                    let pgid = vt.tty.get_foreground_pgid();
                    if pgid > 0 {
                        callback(pgid, signal.to_signo());
                    }
                }
            }
        }
    }

    /// Read from VT
    pub fn read(&self, vt_num: usize, buf: &mut [u8]) -> VfsResult<usize> {
        if vt_num >= NUM_VTS {
            return Err(VfsError::InvalidArgument);
        }

        let vt = self.vts[vt_num].lock();
        Ok(vt.tty.read(0, buf)?)
    }

    /// Write to VT
    pub fn write(&self, vt_num: usize, buf: &[u8]) -> VfsResult<usize> {
        if vt_num >= NUM_VTS {
            return Err(VfsError::InvalidArgument);
        }

        let vt = self.vts[vt_num].lock();
        Ok(vt.tty.write(0, buf)?)
    }

    /// Get TTY for ioctl operations
    pub fn get_tty(&self, vt_num: usize) -> Option<Arc<Tty>> {
        if vt_num >= NUM_VTS {
            return None;
        }
        Some(self.vts[vt_num].lock().tty.clone())
    }
}

/// Global VT manager
static VT_MANAGER: Mutex<Option<Arc<VtManager>>> = Mutex::new(None);

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

/// Initialize VT subsystem
pub fn init() -> Arc<VtManager> {
    let manager = Arc::new(VtManager::new());
    *VT_MANAGER.lock() = Some(manager.clone());
    manager
}

/// Get the global VT manager
pub fn get_manager() -> Option<Arc<VtManager>> {
    VT_MANAGER.lock().clone()
}

/// Set the signal callback for VT signals
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_signal_pgrp_callback(f: SignalPgrpFn) {
    SIGNAL_PGRP_CALLBACK = Some(f);
}

/// Set the console write callback for VT output
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_console_write_callback(f: ConsoleWriteFn) {
    CONSOLE_WRITE_CALLBACK = Some(f);
}

/// VT device node
pub struct VtDevice {
    vt_num: usize,
    manager: Arc<VtManager>,
    ino: u64,
}

impl VtDevice {
    pub fn new(vt_num: usize, manager: Arc<VtManager>, ino: u64) -> Arc<Self> {
        Arc::new(VtDevice { vt_num, manager, ino })
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
        stat.rdev = make_dev(4, self.vt_num as u32); // tty major 4
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }

    fn poll_read_ready(&self) -> bool {
        true
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

fn make_dev(major: u32, minor: u32) -> u64 {
    ((major as u64) << 32) | (minor as u64)
}
