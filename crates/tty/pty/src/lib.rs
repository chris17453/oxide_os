//! Pseudo-terminal (PTY) support for OXIDE OS
//!
//! PTYs provide a bidirectional communication channel between a terminal emulator
//! (master side) and a shell/application (slave side).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐         ┌─────────────┐
//! │  Terminal   │         │   Shell     │
//! │  Emulator   │         │  Process    │
//! │  (xterm)    │         │             │
//! └──────┬──────┘         └──────┬──────┘
//!        │                       │
//!        ▼                       ▼
//! ┌─────────────┐         ┌─────────────┐
//! │  PTY Master │◄───────►│  PTY Slave  │
//! │  /dev/ptmx  │         │ /dev/pts/N  │
//! └─────────────┘         └─────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use pty::PtyManager;
//!
//! // Create PTY manager
//! let manager = PtyManager::new();
//!
//! // Allocate a new PTY pair
//! let (master, slave_path) = manager.allocate()?;
//!
//! // Terminal emulator uses master, shell opens slave_path
//! ```

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

use tty::{LineDiscipline, Winsize};
use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

/// Maximum number of PTYs
const MAX_PTYS: u32 = 256;

/// PTY buffer size
const PTY_BUF_SIZE: usize = 4096;

/// Global PTY number allocator
static NEXT_PTY_NUM: AtomicU32 = AtomicU32::new(0);

/// Callback type for signaling a process group
pub type SignalPgrpFn = fn(pgid: i32, sig: i32);

/// Global signal callback (set by kernel)
static mut SIGNAL_PGRP_CALLBACK: Option<SignalPgrpFn> = None;

/// Set the signal callback for sending signals to process groups
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

/// Allocate a new PTY number
fn alloc_pty_num() -> Option<u32> {
    let num = NEXT_PTY_NUM.fetch_add(1, Ordering::Relaxed);
    if num < MAX_PTYS { Some(num) } else { None }
}

/// Shared state between master and slave
struct PtyPair {
    /// Line discipline for the slave side
    ldisc: LineDiscipline,
    /// Window size
    winsize: Winsize,
    /// Foreground process group
    foreground_pgid: i32,
    /// Session ID
    session: i32,
    /// Data from master to slave (input to slave)
    master_to_slave: Vec<u8>,
    /// Data from slave to master (output from slave)
    slave_to_master: Vec<u8>,
    /// Master is open
    master_open: bool,
    /// Slave is open
    slave_open: bool,
}

impl PtyPair {
    fn new() -> Self {
        PtyPair {
            ldisc: LineDiscipline::new(),
            winsize: Winsize::new(),
            foreground_pgid: 0,
            session: 0,
            master_to_slave: Vec::with_capacity(PTY_BUF_SIZE),
            slave_to_master: Vec::with_capacity(PTY_BUF_SIZE),
            master_open: true,
            slave_open: false,
        }
    }
}

/// PTY master device (/dev/ptmx opens this, or direct master handle)
pub struct PtyMaster {
    /// PTY number
    num: u32,
    /// Shared state with slave
    pair: Arc<Mutex<PtyPair>>,
    /// Inode number
    ino: u64,
}

impl PtyMaster {
    fn new(num: u32, pair: Arc<Mutex<PtyPair>>, ino: u64) -> Self {
        PtyMaster { num, pair, ino }
    }

    /// Get the PTY number
    pub fn pty_num(&self) -> u32 {
        self.num
    }

    /// Get the slave device path
    pub fn slave_path(&self) -> String {
        format!("/dev/pts/{}", self.num)
    }

    /// Write data to the PTY and check for signals
    ///
    /// Returns (bytes_written, optional_signal) where signal contains
    /// (signal_number, foreground_pgid) if a signal should be delivered.
    pub fn write_with_signal(&self, buf: &[u8]) -> VfsResult<(usize, Option<(i32, i32)>)> {
        let mut pair = self.pair.lock();
        let mut signal_info = None;

        if !pair.slave_open {
            return Err(VfsError::BrokenPipe);
        }

        // Process input through line discipline
        for &c in buf {
            // Use a temporary buffer to collect echo output
            let mut echo_buffer = alloc::vec::Vec::new();

            let sig = pair.ldisc.input_char(c, |echo_data| {
                echo_buffer.extend_from_slice(echo_data);
            });

            // Now write collected echo to slave_to_master
            if !echo_buffer.is_empty()
                && pair.slave_to_master.len() + echo_buffer.len() <= PTY_BUF_SIZE
            {
                pair.slave_to_master.extend(echo_buffer);
            }

            // Check for signals
            if let Some(sig) = sig {
                signal_info = Some((sig.to_signo(), pair.foreground_pgid));
            }
        }

        Ok((buf.len(), signal_info))
    }

    /// Get the foreground process group
    pub fn get_foreground_pgid(&self) -> i32 {
        self.pair.lock().foreground_pgid
    }
}

impl VnodeOps for PtyMaster {
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
        let mut pair = self.pair.lock();

        // Master reads output from slave (slave_to_master buffer)
        if pair.slave_to_master.is_empty() {
            if !pair.slave_open {
                return Ok(0); // EOF - slave closed
            }
            return Ok(0); // Would block
        }

        let count = buf.len().min(pair.slave_to_master.len());
        buf[..count].copy_from_slice(&pair.slave_to_master[..count]);
        pair.slave_to_master.drain(..count);

        Ok(count)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        let mut pair = self.pair.lock();

        if !pair.slave_open {
            return Err(VfsError::BrokenPipe);
        }

        // Master writes input to slave - process through line discipline
        for &c in buf {
            // Use a temporary buffer to collect echo output
            let mut echo_buffer = alloc::vec::Vec::new();

            let sig = pair.ldisc.input_char(c, |echo_data| {
                echo_buffer.extend_from_slice(echo_data);
            });

            // Now write collected echo to slave_to_master
            if !echo_buffer.is_empty()
                && pair.slave_to_master.len() + echo_buffer.len() <= PTY_BUF_SIZE
            {
                pair.slave_to_master.extend(echo_buffer);
            }

            // Check if a signal was generated (Ctrl+C, Ctrl+\, Ctrl+Z)
            if let Some(sig) = sig {
                let pgid = pair.foreground_pgid;
                if pgid > 0 {
                    // Call signal delivery callback if registered
                    unsafe {
                        if let Some(callback) = SIGNAL_PGRP_CALLBACK {
                            callback(pgid, sig.to_signo());
                        }
                    }
                }
            }
        }

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
        stat.rdev = make_dev(5, 2); // ptmx major/minor
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }

    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        use tty::termios::*;

        match request {
            TIOCGPTN => {
                // Return PTY number
                let ptr = arg as *mut u32;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                unsafe {
                    *ptr = self.num;
                }
                Ok(0)
            }
            TIOCSPTLCK => {
                // Lock/unlock slave — no-op (always unlocked)
                Ok(0)
            }
            TIOCGWINSZ => {
                let ptr = arg as *mut tty::Winsize;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                let pair = self.pair.lock();
                unsafe {
                    *ptr = pair.winsize;
                }
                Ok(0)
            }
            TIOCSWINSZ => {
                let ptr = arg as *const tty::Winsize;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                let mut pair = self.pair.lock();
                pair.winsize = unsafe { *ptr };
                Ok(0)
            }
            _ => Err(VfsError::NotSupported),
        }
    }

    fn poll_read_ready(&self) -> bool {
        let pair = self.pair.lock();
        // Master can read if there's output from slave or slave is closed
        !pair.slave_to_master.is_empty() || !pair.slave_open
    }

    fn poll_write_ready(&self) -> bool {
        let pair = self.pair.lock();
        // Master can write if slave is open
        pair.slave_open
    }
}

impl Drop for PtyMaster {
    fn drop(&mut self) {
        let mut pair = self.pair.lock();
        pair.master_open = false;
    }
}

/// PTY slave device (/dev/pts/N)
pub struct PtySlave {
    /// PTY number
    num: u32,
    /// Shared state with master
    pair: Arc<Mutex<PtyPair>>,
    /// Inode number
    ino: u64,
}

impl PtySlave {
    fn new(num: u32, pair: Arc<Mutex<PtyPair>>, ino: u64) -> Self {
        {
            let mut p = pair.lock();
            p.slave_open = true;
        }
        PtySlave { num, pair, ino }
    }

    /// Get the PTY number
    pub fn pty_num(&self) -> u32 {
        self.num
    }

    /// Handle ioctl on the slave
    pub fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        use tty::termios::*;

        let mut pair = self.pair.lock();

        match request {
            TCGETS => {
                let ptr = arg as *mut Termios;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                unsafe {
                    *ptr = *pair.ldisc.termios();
                }
                Ok(0)
            }
            TCSETS | TCSETSW | TCSETSF => {
                let ptr = arg as *const Termios;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                let termios = unsafe { *ptr };

                if request == TCSETSF {
                    pair.ldisc.flush_input();
                }

                pair.ldisc.set_termios(termios);
                Ok(0)
            }
            TIOCGWINSZ => {
                let ptr = arg as *mut Winsize;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                unsafe {
                    *ptr = pair.winsize;
                }
                Ok(0)
            }
            TIOCSWINSZ => {
                let ptr = arg as *const Winsize;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                pair.winsize = unsafe { *ptr };
                Ok(0)
            }
            TIOCGPGRP => {
                let ptr = arg as *mut i32;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                unsafe {
                    *ptr = pair.foreground_pgid;
                }
                Ok(0)
            }
            TIOCSPGRP => {
                let ptr = arg as *const i32;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                pair.foreground_pgid = unsafe { *ptr };
                Ok(0)
            }
            FIONREAD => {
                let ptr = arg as *mut i32;
                if ptr.is_null() {
                    return Err(VfsError::InvalidArgument);
                }
                let available = pair.ldisc.input_available();
                unsafe {
                    *ptr = available as i32;
                }
                Ok(0)
            }
            _ => Err(VfsError::NotSupported),
        }
    }
}

impl VnodeOps for PtySlave {
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
        let mut pair = self.pair.lock();

        if !pair.master_open {
            return Ok(0); // EOF - master closed
        }

        // Slave reads from line discipline
        if !pair.ldisc.can_read() {
            return Ok(0); // Would block
        }

        let count = pair.ldisc.read(buf);
        Ok(count)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        let mut pair = self.pair.lock();

        if !pair.master_open {
            return Err(VfsError::BrokenPipe);
        }

        // Slave writes go to master (terminal emulator)
        // Process through line discipline output processing
        for &c in buf {
            let output = pair.ldisc.process_output(c);
            if pair.slave_to_master.len() + output.len() <= PTY_BUF_SIZE {
                pair.slave_to_master.extend(output);
            }
        }

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
        stat.rdev = make_dev(136, self.num as u64); // pts major + pty number
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }

    fn poll_read_ready(&self) -> bool {
        let pair = self.pair.lock();
        // Slave can read if line discipline has data or master is closed
        pair.ldisc.can_read() || !pair.master_open
    }

    fn poll_write_ready(&self) -> bool {
        let pair = self.pair.lock();
        // Slave can write if there's buffer space and master is open
        pair.master_open && pair.slave_to_master.len() < PTY_BUF_SIZE
    }
}

impl Drop for PtySlave {
    fn drop(&mut self) {
        let mut pair = self.pair.lock();
        pair.slave_open = false;
    }
}

/// Create a device number from major and minor numbers
fn make_dev(major: u64, minor: u64) -> u64 {
    (major << 8) | (minor & 0xFF)
}

/// PTY manager - handles allocation of PTY pairs
pub struct PtyManager {
    /// Allocated PTY pairs, indexed by PTY number
    ptys: RwLock<BTreeMap<u32, Arc<Mutex<PtyPair>>>>,
}

impl PtyManager {
    /// Create a new PTY manager
    pub fn new() -> Self {
        PtyManager {
            ptys: RwLock::new(BTreeMap::new()),
        }
    }

    /// Allocate a new PTY pair
    ///
    /// Returns (master, slave_path)
    pub fn allocate(&self) -> VfsResult<(Arc<PtyMaster>, String)> {
        let num = alloc_pty_num().ok_or(VfsError::NoSpace)?;

        let pair = Arc::new(Mutex::new(PtyPair::new()));

        // Store in manager
        self.ptys.write().insert(num, pair.clone());

        let master = Arc::new(PtyMaster::new(num, pair, 1000 + num as u64));
        let slave_path = format!("/dev/pts/{}", num);

        Ok((master, slave_path))
    }

    /// Get the slave device for a PTY number
    pub fn get_slave(&self, num: u32) -> VfsResult<Arc<PtySlave>> {
        let ptys = self.ptys.read();
        let pair = ptys.get(&num).cloned().ok_or(VfsError::NotFound)?;

        Ok(Arc::new(PtySlave::new(num, pair, 2000 + num as u64)))
    }

    /// List all allocated PTY numbers
    pub fn list(&self) -> Vec<u32> {
        self.ptys.read().keys().copied().collect()
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// /dev/ptmx device - opening this allocates a new PTY
pub struct Ptmx {
    /// PTY manager
    manager: Arc<PtyManager>,
    /// Inode number
    ino: u64,
}

impl Ptmx {
    /// Create a new /dev/ptmx device
    pub fn new(manager: Arc<PtyManager>, ino: u64) -> Arc<Self> {
        Arc::new(Ptmx { manager, ino })
    }

    /// Allocate a new PTY pair
    pub fn allocate(&self) -> VfsResult<(Arc<PtyMaster>, String)> {
        self.manager.allocate()
    }
}

impl VnodeOps for Ptmx {
    fn vtype(&self) -> VnodeType {
        VnodeType::CharDevice
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        // Can't read from ptmx directly - must open to get master
        Err(VfsError::InvalidArgument)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::InvalidArgument)
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
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o666), 0, self.ino);
        stat.rdev = make_dev(5, 2); // ptmx
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

/// /dev/pts directory - contains slave PTY devices
pub struct PtsDir {
    /// PTY manager
    manager: Arc<PtyManager>,
    /// Inode number
    ino: u64,
}

impl PtsDir {
    /// Create a new /dev/pts directory
    pub fn new(manager: Arc<PtyManager>, ino: u64) -> Arc<Self> {
        Arc::new(PtsDir { manager, ino })
    }
}

impl VnodeOps for PtsDir {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        // Parse PTY number from name
        let num: u32 = name.parse().map_err(|_| VfsError::NotFound)?;

        // Get the slave device
        self.manager.get_slave(num).map(|s| s as Arc<dyn VnodeOps>)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::PermissionDenied)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let offset = offset as usize;

        // . entry
        if offset == 0 {
            return Ok(Some(DirEntry {
                name: ".".to_string(),
                ino: self.ino,
                file_type: VnodeType::Directory,
            }));
        }

        // .. entry
        if offset == 1 {
            return Ok(Some(DirEntry {
                name: "..".to_string(),
                ino: self.ino,
                file_type: VnodeType::Directory,
            }));
        }

        // PTY devices
        let ptys = self.manager.list();
        let idx = offset - 2;
        if idx < ptys.len() {
            let num = ptys[idx];
            return Ok(Some(DirEntry {
                name: format!("{}", num),
                ino: 2000 + num as u64,
                file_type: VnodeType::CharDevice,
            }));
        }

        Ok(None)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::PermissionDenied)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::PermissionDenied)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::PermissionDenied)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::PermissionDenied)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(
            VnodeType::Directory,
            Mode::new(0o755),
            0,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::IsDirectory)
    }
}
