//! epoll Event Notification for OXIDE OS
//!
//! Scalable I/O event notification mechanism.

#![no_std]

extern crate alloc;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use spin::Mutex;

/// File descriptor type
pub type Fd = i32;

/// epoll event flags
pub mod events {
    /// Available for read
    pub const EPOLLIN: u32 = 0x001;
    /// Available for write
    pub const EPOLLOUT: u32 = 0x004;
    /// Error condition
    pub const EPOLLERR: u32 = 0x008;
    /// Hang up
    pub const EPOLLHUP: u32 = 0x010;
    /// Priority data available
    pub const EPOLLPRI: u32 = 0x002;
    /// Remote peer closed connection
    pub const EPOLLRDHUP: u32 = 0x2000;
    /// Edge-triggered mode
    pub const EPOLLET: u32 = 0x80000000;
    /// One-shot mode
    pub const EPOLLONESHOT: u32 = 0x40000000;
    /// Wake up only once
    pub const EPOLLWAKEUP: u32 = 0x20000000;
    /// Exclusive wake up
    pub const EPOLLEXCLUSIVE: u32 = 0x10000000;
}

/// epoll control operations
pub mod ctl {
    /// Add a file descriptor
    pub const EPOLL_CTL_ADD: i32 = 1;
    /// Remove a file descriptor
    pub const EPOLL_CTL_DEL: i32 = 2;
    /// Modify a file descriptor
    pub const EPOLL_CTL_MOD: i32 = 3;
}

/// epoll event
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct EpollEvent {
    /// Event flags
    pub events: u32,
    /// User data
    pub data: u64,
}

impl EpollEvent {
    /// Create new event
    pub fn new(events: u32, data: u64) -> Self {
        EpollEvent { events, data }
    }

    /// Check if readable
    pub fn is_readable(&self) -> bool {
        self.events & events::EPOLLIN != 0
    }

    /// Check if writable
    pub fn is_writable(&self) -> bool {
        self.events & events::EPOLLOUT != 0
    }

    /// Check if error
    pub fn is_error(&self) -> bool {
        self.events & events::EPOLLERR != 0
    }

    /// Check if hung up
    pub fn is_hangup(&self) -> bool {
        self.events & events::EPOLLHUP != 0
    }
}

/// Interest entry for a file descriptor
#[derive(Debug, Clone)]
pub struct EpollInterest {
    /// File descriptor
    pub fd: Fd,
    /// Registered events
    pub events: u32,
    /// User data
    pub data: u64,
    /// Edge triggered
    pub edge_triggered: bool,
    /// One shot
    pub one_shot: bool,
    /// Current state (for edge triggered)
    pub last_events: u32,
}

impl EpollInterest {
    /// Create new interest
    pub fn new(fd: Fd, events: u32, data: u64) -> Self {
        EpollInterest {
            fd,
            events: events & !events::EPOLLET & !events::EPOLLONESHOT,
            data,
            edge_triggered: events & events::EPOLLET != 0,
            one_shot: events & events::EPOLLONESHOT != 0,
            last_events: 0,
        }
    }

    /// Check if interested in event
    pub fn matches(&self, event: u32) -> bool {
        self.events & event != 0
    }
}

/// epoll instance
pub struct EpollInstance {
    /// Monitored file descriptors
    interest_list: Mutex<BTreeMap<Fd, EpollInterest>>,
    /// Ready list (fds with pending events)
    ready_list: Mutex<VecDeque<(Fd, u32)>>,
    /// ID for debugging
    id: u64,
}

impl EpollInstance {
    /// Create new epoll instance
    pub fn new(id: u64) -> Self {
        EpollInstance {
            interest_list: Mutex::new(BTreeMap::new()),
            ready_list: Mutex::new(VecDeque::new()),
            id,
        }
    }

    /// Add file descriptor to interest list
    pub fn add(&self, fd: Fd, event: &EpollEvent) -> Result<(), EpollError> {
        let mut interests = self.interest_list.lock();

        if interests.contains_key(&fd) {
            return Err(EpollError::AlreadyExists);
        }

        let interest = EpollInterest::new(fd, event.events, event.data);
        interests.insert(fd, interest);

        Ok(())
    }

    /// Modify file descriptor in interest list
    pub fn modify(&self, fd: Fd, event: &EpollEvent) -> Result<(), EpollError> {
        let mut interests = self.interest_list.lock();

        let interest = interests.get_mut(&fd).ok_or(EpollError::NotFound)?;

        interest.events = event.events & !events::EPOLLET & !events::EPOLLONESHOT;
        interest.data = event.data;
        interest.edge_triggered = event.events & events::EPOLLET != 0;
        interest.one_shot = event.events & events::EPOLLONESHOT != 0;

        Ok(())
    }

    /// Remove file descriptor from interest list
    pub fn remove(&self, fd: Fd) -> Result<(), EpollError> {
        let mut interests = self.interest_list.lock();

        if interests.remove(&fd).is_none() {
            return Err(EpollError::NotFound);
        }

        // Also remove from ready list
        let mut ready = self.ready_list.lock();
        ready.retain(|(f, _)| *f != fd);

        Ok(())
    }

    /// Report event for a file descriptor
    pub fn report_event(&self, fd: Fd, events: u32) {
        let interests = self.interest_list.lock();

        if let Some(interest) = interests.get(&fd) {
            let matching = interest.events & events;
            if matching != 0 {
                if interest.edge_triggered {
                    // Edge triggered: only report if state changed
                    if matching != interest.last_events {
                        let mut ready = self.ready_list.lock();
                        ready.push_back((fd, matching));
                    }
                } else {
                    // Level triggered: always report
                    let mut ready = self.ready_list.lock();
                    ready.push_back((fd, matching));
                }
            }
        }
    }

    /// Wait for events
    pub fn wait(&self, max_events: usize) -> Vec<EpollEvent> {
        let mut results = Vec::with_capacity(max_events);
        let mut ready = self.ready_list.lock();
        let interests = self.interest_list.lock();

        while results.len() < max_events {
            if let Some((fd, events)) = ready.pop_front() {
                if let Some(interest) = interests.get(&fd) {
                    results.push(EpollEvent {
                        events,
                        data: interest.data,
                    });
                }
            } else {
                break;
            }
        }

        results
    }

    /// Check if there are pending events
    pub fn has_pending(&self) -> bool {
        !self.ready_list.lock().is_empty()
    }

    /// Get number of monitored fds
    pub fn count(&self) -> usize {
        self.interest_list.lock().len()
    }

    /// Get instance ID
    pub fn id(&self) -> u64 {
        self.id
    }
}

/// epoll error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpollError {
    /// File descriptor already exists
    AlreadyExists,
    /// File descriptor not found
    NotFound,
    /// Invalid operation
    InvalidOperation,
    /// Bad file descriptor
    BadFd,
    /// Would block
    WouldBlock,
}

impl core::fmt::Display for EpollError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AlreadyExists => write!(f, "fd already exists"),
            Self::NotFound => write!(f, "fd not found"),
            Self::InvalidOperation => write!(f, "invalid operation"),
            Self::BadFd => write!(f, "bad file descriptor"),
            Self::WouldBlock => write!(f, "would block"),
        }
    }
}

/// epoll create flags
pub mod flags {
    /// Close on exec
    pub const EPOLL_CLOEXEC: i32 = 0x80000;
}

/// Global epoll instances
static INSTANCES: Mutex<BTreeMap<Fd, EpollInstance>> = Mutex::new(BTreeMap::new());
static NEXT_ID: Mutex<u64> = Mutex::new(1);

/// Create new epoll instance
pub fn epoll_create(flags: i32) -> Result<Fd, EpollError> {
    let _ = flags; // Flags are handled at syscall level

    let mut id = NEXT_ID.lock();
    let instance_id = *id;
    *id += 1;

    let fd = instance_id as Fd;
    let instance = EpollInstance::new(instance_id);

    INSTANCES.lock().insert(fd, instance);

    Ok(fd)
}

/// Control epoll instance
pub fn epoll_ctl(epfd: Fd, op: i32, fd: Fd, event: Option<&EpollEvent>) -> Result<(), EpollError> {
    let instances = INSTANCES.lock();
    let instance = instances.get(&epfd).ok_or(EpollError::BadFd)?;

    match op {
        ctl::EPOLL_CTL_ADD => {
            let event = event.ok_or(EpollError::InvalidOperation)?;
            instance.add(fd, event)
        }
        ctl::EPOLL_CTL_MOD => {
            let event = event.ok_or(EpollError::InvalidOperation)?;
            instance.modify(fd, event)
        }
        ctl::EPOLL_CTL_DEL => instance.remove(fd),
        _ => Err(EpollError::InvalidOperation),
    }
}

/// Wait for events
pub fn epoll_wait(epfd: Fd, events: &mut [EpollEvent], _timeout: i32) -> Result<usize, EpollError> {
    let instances = INSTANCES.lock();
    let instance = instances.get(&epfd).ok_or(EpollError::BadFd)?;

    let ready = instance.wait(events.len());
    let count = ready.len();

    for (i, event) in ready.into_iter().enumerate() {
        events[i] = event;
    }

    Ok(count)
}
