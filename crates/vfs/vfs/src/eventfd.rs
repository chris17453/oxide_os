//! Event file descriptor (eventfd)
//!
//! Provides a file descriptor for event wait/notify mechanisms.
//! The kernel maintains a 64-bit unsigned integer counter.
//! - write: adds value to counter (blocks/EAGAIN if would overflow)
//! - read: returns counter value and resets to 0 (or decrements by 1 for semaphore mode)

use alloc::sync::Arc;
use spin::Mutex;

use crate::error::{VfsError, VfsResult};
use crate::vnode::{DirEntry, Mode, Stat, VnodeOps, VnodeType};

/// eventfd flags
pub const EFD_SEMAPHORE: u32 = 0x00000001;
pub const EFD_CLOEXEC: u32 = 0x00080000;
pub const EFD_NONBLOCK: u32 = 0x00000800;

/// Internal state for an eventfd
struct EventFdState {
    counter: u64,
    flags: u32,
}

/// Event file descriptor vnode
pub struct EventFdNode {
    state: Mutex<EventFdState>,
}

impl EventFdNode {
    fn new(initval: u32, flags: u32) -> Self {
        EventFdNode {
            state: Mutex::new(EventFdState {
                counter: initval as u64,
                flags,
            }),
        }
    }
}

impl VnodeOps for EventFdNode {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        // eventfd read must be exactly 8 bytes
        if buf.len() < 8 {
            return Err(VfsError::InvalidArgument);
        }

        let mut state = self.state.lock();

        if state.counter == 0 {
            // Would block
            return Err(VfsError::WouldBlock);
        }

        let val = if state.flags & EFD_SEMAPHORE != 0 {
            // Semaphore mode: decrement by 1
            state.counter -= 1;
            1u64
        } else {
            // Normal mode: return counter, reset to 0
            let v = state.counter;
            state.counter = 0;
            v
        };

        buf[..8].copy_from_slice(&val.to_ne_bytes());
        Ok(8)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // eventfd write must be exactly 8 bytes
        if buf.len() < 8 {
            return Err(VfsError::InvalidArgument);
        }

        let val = u64::from_ne_bytes([
            buf[0], buf[1], buf[2], buf[3],
            buf[4], buf[5], buf[6], buf[7],
        ]);

        // u64::MAX (0xFFFFFFFFFFFFFFFF) is not a valid write value
        if val == u64::MAX {
            return Err(VfsError::InvalidArgument);
        }

        let mut state = self.state.lock();

        // Check for overflow
        if state.counter > u64::MAX - 1 - val {
            return Err(VfsError::WouldBlock);
        }

        state.counter += val;
        Ok(8)
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotSupported)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn rename(&self, _old: &str, _new_dir: &dyn VnodeOps, _new: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::File, Mode::new(0o600), 0, 0))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn poll_read_ready(&self) -> bool {
        let state = self.state.lock();
        state.counter > 0
    }

    fn poll_write_ready(&self) -> bool {
        let state = self.state.lock();
        state.counter < u64::MAX - 1
    }
}

/// Create a new eventfd vnode
pub fn create_eventfd(initval: u32, flags: u32) -> Arc<dyn VnodeOps> {
    Arc::new(EventFdNode::new(initval, flags))
}
