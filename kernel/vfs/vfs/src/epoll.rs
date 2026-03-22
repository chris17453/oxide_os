//! epoll implementation
//!
//! Provides an I/O event notification facility. An epoll instance maintains
//! a set of file descriptors being monitored for readiness events.
//!
//! epoll uses a callback-free model: epoll_wait checks all registered fds
//! for readiness using their poll_read_ready/poll_write_ready methods.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use spin::Mutex;

use crate::error::{VfsError, VfsResult};
use crate::file::File;
use crate::vnode::{DirEntry, Mode, Stat, VnodeOps, VnodeType};

/// epoll event flags
pub const EPOLLIN: u32 = 0x001;
pub const EPOLLOUT: u32 = 0x004;
pub const EPOLLERR: u32 = 0x008;
pub const EPOLLHUP: u32 = 0x010;
pub const EPOLLRDHUP: u32 = 0x2000;
pub const EPOLLET: u32 = 1 << 31;
pub const EPOLLONESHOT: u32 = 1 << 30;

/// epoll_ctl operations
pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;

/// Maximum fds per epoll instance
const MAX_EPOLL_ENTRIES: usize = 256;

/// An entry in the epoll interest list
/// — ShadePacket: tracks last-reported ready state for edge-triggered mode.
/// EPOLLET entries only fire when transitioning from not-ready to ready.
#[derive(Clone)]
pub struct EpollEntry {
    /// The monitored file descriptor number
    pub fd: i32,
    /// The file handle for polling
    pub file: Arc<File>,
    /// Events of interest (EPOLLIN, EPOLLOUT, EPOLLET, etc.)
    pub events: u32,
    /// User data (passed through untouched)
    pub data: u64,
    /// Last reported ready events (for edge-triggered mode)
    /// — ShadePacket: 0 = never reported. When EPOLLET is set, an event is only
    /// returned if it wasn't in `last_ready` (state transition detection).
    pub last_ready: u32,
    /// Whether this entry is disabled (EPOLLONESHOT after first trigger)
    pub disabled: bool,
}

/// epoll event structure (matches Linux struct epoll_event)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct EpollEvent {
    pub events: u32,
    pub data: u64,
}

/// Internal state for an epoll instance
pub struct EpollInstance {
    /// Registered file descriptors
    entries: Vec<EpollEntry>,
}

impl EpollInstance {
    fn new() -> Self {
        EpollInstance {
            entries: Vec::new(),
        }
    }

    /// Add a file descriptor to the interest list
    pub fn add(&mut self, fd: i32, file: Arc<File>, events: u32, data: u64) -> VfsResult<()> {
        // Check if already registered
        if self.entries.iter().any(|e| e.fd == fd) {
            return Err(VfsError::AlreadyExists);
        }
        if self.entries.len() >= MAX_EPOLL_ENTRIES {
            return Err(VfsError::NoSpace);
        }
        self.entries.push(EpollEntry {
            fd,
            file,
            events,
            data,
            last_ready: 0,
            disabled: false,
        });
        Ok(())
    }

    /// Remove a file descriptor from the interest list
    pub fn del(&mut self, fd: i32) -> VfsResult<()> {
        let pos = self.entries.iter().position(|e| e.fd == fd);
        match pos {
            Some(i) => {
                self.entries.swap_remove(i);
                Ok(())
            }
            None => Err(VfsError::NotFound),
        }
    }

    /// Modify events for a registered file descriptor
    /// — ShadePacket: also re-enables EPOLLONESHOT entries
    pub fn modify(&mut self, fd: i32, events: u32, data: u64) -> VfsResult<()> {
        let entry = self.entries.iter_mut().find(|e| e.fd == fd);
        match entry {
            Some(e) => {
                e.events = events;
                e.data = data;
                e.disabled = false; // re-enable for EPOLLONESHOT
                e.last_ready = 0;   // reset edge state
                Ok(())
            }
            None => Err(VfsError::NotFound),
        }
    }

    /// Poll all registered fds and return ready events
    /// — ShadePacket: handles EPOLLET (edge-triggered) and EPOLLONESHOT.
    /// Edge-triggered: only report events that are NEW since last wait.
    /// One-shot: disable the entry after first trigger (re-enable via EPOLL_CTL_MOD).
    pub fn wait(&mut self, max_events: usize) -> Vec<EpollEvent> {
        let mut ready = Vec::new();
        let limit = max_events.min(self.entries.len());

        for entry in self.entries.iter_mut() {
            if ready.len() >= limit { break; }
            if entry.disabled { continue; }

            let mut revents = 0u32;

            if entry.events & EPOLLIN != 0 && entry.file.can_read() {
                revents |= EPOLLIN;
            }
            if entry.events & EPOLLOUT != 0 && entry.file.can_write() {
                revents |= EPOLLOUT;
            }

            if revents != 0 {
                // — ShadePacket: edge-triggered mode — only report if this is a
                // NEW event (wasn't in last_ready). Level-triggered reports every time.
                let is_edge = entry.events & EPOLLET != 0;
                let new_events = revents & !entry.last_ready;

                if is_edge && new_events == 0 {
                    // — ShadePacket: no new edges — skip this entry
                    continue;
                }

                let report_events = if is_edge { new_events } else { revents };

                ready.push(EpollEvent {
                    events: report_events,
                    data: entry.data,
                });

                entry.last_ready = revents;

                // — ShadePacket: one-shot mode — disable after first trigger
                if entry.events & EPOLLONESHOT != 0 {
                    entry.disabled = true;
                }
            } else {
                // — ShadePacket: fd is no longer ready — reset last_ready so the
                // next time it becomes ready, edge-triggered will fire again.
                entry.last_ready = 0;
            }
        }

        ready
    }

    /// Check if any registered fd is ready (for poll_read_ready)
    pub fn has_ready(&self) -> bool {
        self.entries.iter().any(|entry| {
            (entry.events & EPOLLIN != 0 && entry.file.can_read())
                || (entry.events & EPOLLOUT != 0 && entry.file.can_write())
        })
    }
}

/// Epoll vnode — represents an epoll instance as a file descriptor
pub struct EpollNode {
    pub instance: Mutex<EpollInstance>,
}

impl EpollNode {
    pub fn new() -> Self {
        EpollNode {
            instance: Mutex::new(EpollInstance::new()),
        }
    }
}

impl VnodeOps for EpollNode {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::InvalidOperation)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::InvalidOperation)
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
        self.instance.lock().has_ready()
    }

    fn poll_write_ready(&self) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Create a new epoll instance vnode
pub fn create_epoll() -> Arc<EpollNode> {
    Arc::new(EpollNode::new())
}
