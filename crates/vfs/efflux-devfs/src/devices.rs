//! Device implementations for devfs
//!
//! Provides /dev/null, /dev/zero, /dev/console, etc.

use alloc::sync::Arc;

use efflux_vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

/// /dev/null - discards all writes, reads return EOF
pub struct NullDevice {
    ino: u64,
}

impl NullDevice {
    pub fn new(ino: u64) -> Self {
        NullDevice { ino }
    }
}

impl VnodeOps for NullDevice {
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
        // Reading from /dev/null always returns EOF (0 bytes)
        Ok(0)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Writing to /dev/null always succeeds and discards the data
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
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o666), 0, self.ino);
        stat.rdev = make_dev(1, 3); // Major 1, Minor 3 (standard /dev/null)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        // Truncating /dev/null is a no-op
        Ok(())
    }
}

/// /dev/zero - reads return zeros, writes are discarded
pub struct ZeroDevice {
    ino: u64,
}

impl ZeroDevice {
    pub fn new(ino: u64) -> Self {
        ZeroDevice { ino }
    }
}

impl VnodeOps for ZeroDevice {
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
        // Reading from /dev/zero returns zeros
        buf.fill(0);
        Ok(buf.len())
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Writing to /dev/zero discards the data
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
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o666), 0, self.ino);
        stat.rdev = make_dev(1, 5); // Major 1, Minor 5 (standard /dev/zero)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

/// Console write function type
pub type ConsoleFn = fn(&[u8]);

/// /dev/console - writes go to system console
pub struct ConsoleDevice {
    ino: u64,
}

impl ConsoleDevice {
    pub fn new(ino: u64) -> Self {
        ConsoleDevice { ino }
    }
}

/// Global console write function (set by kernel)
static mut CONSOLE_WRITE: Option<ConsoleFn> = None;

/// Set the console write function
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_console_write(f: ConsoleFn) {
    unsafe {
        CONSOLE_WRITE = Some(f);
    }
}

impl VnodeOps for ConsoleDevice {
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
        // Console read would require keyboard input - return EOF for now
        Ok(0)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Write to the system console
        // Safety: We only read CONSOLE_WRITE
        unsafe {
            if let Some(write_fn) = CONSOLE_WRITE {
                write_fn(buf);
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
        stat.rdev = make_dev(5, 1); // Major 5, Minor 1 (console)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

/// Create a device number from major and minor numbers
fn make_dev(major: u64, minor: u64) -> u64 {
    (major << 8) | (minor & 0xFF)
}
