//! io_uring Async I/O for OXIDE OS
//!
//! High-performance async I/O using submission/completion queues.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use spin::Mutex;

/// io_uring operation codes
pub mod opcode {
    pub const IORING_OP_NOP: u8 = 0;
    pub const IORING_OP_READV: u8 = 1;
    pub const IORING_OP_WRITEV: u8 = 2;
    pub const IORING_OP_FSYNC: u8 = 3;
    pub const IORING_OP_READ_FIXED: u8 = 4;
    pub const IORING_OP_WRITE_FIXED: u8 = 5;
    pub const IORING_OP_POLL_ADD: u8 = 6;
    pub const IORING_OP_POLL_REMOVE: u8 = 7;
    pub const IORING_OP_SYNC_FILE_RANGE: u8 = 8;
    pub const IORING_OP_SENDMSG: u8 = 9;
    pub const IORING_OP_RECVMSG: u8 = 10;
    pub const IORING_OP_TIMEOUT: u8 = 11;
    pub const IORING_OP_TIMEOUT_REMOVE: u8 = 12;
    pub const IORING_OP_ACCEPT: u8 = 13;
    pub const IORING_OP_ASYNC_CANCEL: u8 = 14;
    pub const IORING_OP_LINK_TIMEOUT: u8 = 15;
    pub const IORING_OP_CONNECT: u8 = 16;
    pub const IORING_OP_FALLOCATE: u8 = 17;
    pub const IORING_OP_OPENAT: u8 = 18;
    pub const IORING_OP_CLOSE: u8 = 19;
    pub const IORING_OP_FILES_UPDATE: u8 = 20;
    pub const IORING_OP_STATX: u8 = 21;
    pub const IORING_OP_READ: u8 = 22;
    pub const IORING_OP_WRITE: u8 = 23;
    pub const IORING_OP_FADVISE: u8 = 24;
    pub const IORING_OP_MADVISE: u8 = 25;
    pub const IORING_OP_SEND: u8 = 26;
    pub const IORING_OP_RECV: u8 = 27;
    pub const IORING_OP_OPENAT2: u8 = 28;
    pub const IORING_OP_EPOLL_CTL: u8 = 29;
    pub const IORING_OP_SPLICE: u8 = 30;
    pub const IORING_OP_PROVIDE_BUFFERS: u8 = 31;
    pub const IORING_OP_REMOVE_BUFFERS: u8 = 32;
}

/// io_uring SQE flags
pub mod sqe_flags {
    /// Use fixed file
    pub const IOSQE_FIXED_FILE: u8 = 1 << 0;
    /// Issue after inflight operations complete
    pub const IOSQE_IO_DRAIN: u8 = 1 << 1;
    /// Links next SQE
    pub const IOSQE_IO_LINK: u8 = 1 << 2;
    /// Hard links next SQE
    pub const IOSQE_IO_HARDLINK: u8 = 1 << 3;
    /// Always async
    pub const IOSQE_ASYNC: u8 = 1 << 4;
    /// Select buffer from buffer pool
    pub const IOSQE_BUFFER_SELECT: u8 = 1 << 5;
}

/// io_uring enter flags
pub mod enter_flags {
    pub const IORING_ENTER_GETEVENTS: u32 = 1 << 0;
    pub const IORING_ENTER_SQ_WAKEUP: u32 = 1 << 1;
    pub const IORING_ENTER_SQ_WAIT: u32 = 1 << 2;
    pub const IORING_ENTER_EXT_ARG: u32 = 1 << 3;
}

/// io_uring setup flags
pub mod setup_flags {
    pub const IORING_SETUP_IOPOLL: u32 = 1 << 0;
    pub const IORING_SETUP_SQPOLL: u32 = 1 << 1;
    pub const IORING_SETUP_SQ_AFF: u32 = 1 << 2;
    pub const IORING_SETUP_CQSIZE: u32 = 1 << 3;
    pub const IORING_SETUP_CLAMP: u32 = 1 << 4;
    pub const IORING_SETUP_ATTACH_WQ: u32 = 1 << 5;
}

/// Submission Queue Entry
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct IoUringSqe {
    /// Operation code
    pub opcode: u8,
    /// Flags
    pub flags: u8,
    /// I/O priority
    pub ioprio: u16,
    /// File descriptor
    pub fd: i32,
    /// Offset for read/write
    pub off: u64,
    /// Buffer address
    pub addr: u64,
    /// Length
    pub len: u32,
    /// Operation-specific flags
    pub op_flags: u32,
    /// User data (returned in CQE)
    pub user_data: u64,
    /// Buffer index for fixed buffers
    pub buf_index: u16,
    /// Personality
    pub personality: u16,
    /// Splice fd
    pub splice_fd_in: i32,
    /// Padding
    pub __pad2: [u64; 2],
}

impl IoUringSqe {
    /// Create new SQE
    pub fn new() -> Self {
        Self::default()
    }

    /// Set opcode
    pub fn opcode(mut self, opcode: u8) -> Self {
        self.opcode = opcode;
        self
    }

    /// Set file descriptor
    pub fn fd(mut self, fd: i32) -> Self {
        self.fd = fd;
        self
    }

    /// Set buffer address
    pub fn addr(mut self, addr: u64) -> Self {
        self.addr = addr;
        self
    }

    /// Set length
    pub fn len(mut self, len: u32) -> Self {
        self.len = len;
        self
    }

    /// Set offset
    pub fn offset(mut self, off: u64) -> Self {
        self.off = off;
        self
    }

    /// Set user data
    pub fn user_data(mut self, data: u64) -> Self {
        self.user_data = data;
        self
    }

    /// Set flags
    pub fn flags(mut self, flags: u8) -> Self {
        self.flags = flags;
        self
    }

    /// Create read operation
    pub fn prep_read(fd: i32, buf: u64, len: u32, offset: u64) -> Self {
        Self::new()
            .opcode(opcode::IORING_OP_READ)
            .fd(fd)
            .addr(buf)
            .len(len)
            .offset(offset)
    }

    /// Create write operation
    pub fn prep_write(fd: i32, buf: u64, len: u32, offset: u64) -> Self {
        Self::new()
            .opcode(opcode::IORING_OP_WRITE)
            .fd(fd)
            .addr(buf)
            .len(len)
            .offset(offset)
    }

    /// Create accept operation
    pub fn prep_accept(fd: i32, addr: u64, addrlen: u64) -> Self {
        Self::new()
            .opcode(opcode::IORING_OP_ACCEPT)
            .fd(fd)
            .addr(addr)
            .offset(addrlen)
    }

    /// Create connect operation
    pub fn prep_connect(fd: i32, addr: u64, addrlen: u32) -> Self {
        Self::new()
            .opcode(opcode::IORING_OP_CONNECT)
            .fd(fd)
            .addr(addr)
            .offset(addrlen as u64)
    }

    /// Create send operation
    pub fn prep_send(fd: i32, buf: u64, len: u32) -> Self {
        Self::new()
            .opcode(opcode::IORING_OP_SEND)
            .fd(fd)
            .addr(buf)
            .len(len)
    }

    /// Create recv operation
    pub fn prep_recv(fd: i32, buf: u64, len: u32) -> Self {
        Self::new()
            .opcode(opcode::IORING_OP_RECV)
            .fd(fd)
            .addr(buf)
            .len(len)
    }

    /// Create timeout operation
    pub fn prep_timeout(ts: u64, count: u32) -> Self {
        Self::new()
            .opcode(opcode::IORING_OP_TIMEOUT)
            .addr(ts)
            .len(count)
    }

    /// Create close operation
    pub fn prep_close(fd: i32) -> Self {
        Self::new().opcode(opcode::IORING_OP_CLOSE).fd(fd)
    }
}

/// Completion Queue Entry
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct IoUringCqe {
    /// User data from SQE
    pub user_data: u64,
    /// Result (bytes transferred or -errno)
    pub res: i32,
    /// Flags
    pub flags: u32,
}

impl IoUringCqe {
    /// Check if error
    pub fn is_error(&self) -> bool {
        self.res < 0
    }

    /// Get error code (positive errno)
    pub fn error(&self) -> Option<i32> {
        if self.res < 0 {
            Some(-self.res)
        } else {
            None
        }
    }

    /// Get result as usize
    pub fn result(&self) -> Result<usize, i32> {
        if self.res < 0 {
            Err(-self.res)
        } else {
            Ok(self.res as usize)
        }
    }
}

/// io_uring setup parameters
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct IoUringParams {
    /// Number of SQ entries
    pub sq_entries: u32,
    /// Number of CQ entries
    pub cq_entries: u32,
    /// Setup flags
    pub flags: u32,
    /// SQ thread CPU
    pub sq_thread_cpu: u32,
    /// SQ thread idle timeout
    pub sq_thread_idle: u32,
    /// Features supported
    pub features: u32,
    /// WQ file descriptor
    pub wq_fd: u32,
    /// Reserved
    pub resv: [u32; 3],
    /// SQ offset
    pub sq_off: SqRingOffsets,
    /// CQ offset
    pub cq_off: CqRingOffsets,
}

/// SQ ring offsets
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SqRingOffsets {
    pub head: u32,
    pub tail: u32,
    pub ring_mask: u32,
    pub ring_entries: u32,
    pub flags: u32,
    pub dropped: u32,
    pub array: u32,
    pub resv1: u32,
    pub resv2: u64,
}

/// CQ ring offsets
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CqRingOffsets {
    pub head: u32,
    pub tail: u32,
    pub ring_mask: u32,
    pub ring_entries: u32,
    pub overflow: u32,
    pub cqes: u32,
    pub flags: u32,
    pub resv1: u32,
    pub resv2: u64,
}

/// io_uring instance
pub struct IoUringInstance {
    /// Submission queue entries
    sq: Mutex<SubmissionQueue>,
    /// Completion queue entries
    cq: Mutex<CompletionQueue>,
    /// Pending operations
    pending: Mutex<VecDeque<PendingOp>>,
    /// Instance ID
    id: u64,
    /// Parameters
    params: IoUringParams,
}

struct SubmissionQueue {
    entries: Vec<IoUringSqe>,
    head: u32,
    tail: u32,
    mask: u32,
}

struct CompletionQueue {
    entries: Vec<IoUringCqe>,
    head: u32,
    tail: u32,
    mask: u32,
}

struct PendingOp {
    sqe: IoUringSqe,
    submitted_at: u64,
}

impl IoUringInstance {
    /// Create new io_uring instance
    pub fn new(entries: u32, id: u64) -> Self {
        let entries = entries.next_power_of_two();
        let mask = entries - 1;

        IoUringInstance {
            sq: Mutex::new(SubmissionQueue {
                entries: alloc::vec![IoUringSqe::default(); entries as usize],
                head: 0,
                tail: 0,
                mask,
            }),
            cq: Mutex::new(CompletionQueue {
                entries: alloc::vec![IoUringCqe::default(); entries as usize * 2],
                head: 0,
                tail: 0,
                mask: entries * 2 - 1,
            }),
            pending: Mutex::new(VecDeque::new()),
            id,
            params: IoUringParams {
                sq_entries: entries,
                cq_entries: entries * 2,
                ..Default::default()
            },
        }
    }

    /// Submit an SQE
    pub fn submit(&self, sqe: IoUringSqe) -> Result<(), IoUringError> {
        let mut sq = self.sq.lock();

        // Check if full
        if sq.tail.wrapping_sub(sq.head) >= sq.entries.len() as u32 {
            return Err(IoUringError::QueueFull);
        }

        let idx = (sq.tail & sq.mask) as usize;
        sq.entries[idx] = sqe;
        sq.tail = sq.tail.wrapping_add(1);

        // Add to pending
        self.pending.lock().push_back(PendingOp {
            sqe,
            submitted_at: 0,
        });

        Ok(())
    }

    /// Submit and wait for completion
    pub fn submit_and_wait(&self, min_complete: u32) -> Result<usize, IoUringError> {
        let sq = self.sq.lock();
        let submissions = sq.tail.wrapping_sub(sq.head);
        drop(sq);

        // Process pending operations
        // In real implementation, this would trigger kernel processing
        self.process_pending();

        // Wait for completions
        let cq = self.cq.lock();
        let completions = cq.tail.wrapping_sub(cq.head);

        if completions >= min_complete {
            Ok(submissions as usize)
        } else {
            Err(IoUringError::WouldBlock)
        }
    }

    /// Get next completion
    pub fn peek_cqe(&self) -> Option<IoUringCqe> {
        let cq = self.cq.lock();

        if cq.head == cq.tail {
            None
        } else {
            let idx = (cq.head & cq.mask) as usize;
            Some(cq.entries[idx])
        }
    }

    /// Consume completion
    pub fn consume_cqe(&self) {
        let mut cq = self.cq.lock();
        if cq.head != cq.tail {
            cq.head = cq.head.wrapping_add(1);
        }
    }

    /// Add completion
    pub fn complete(&self, user_data: u64, result: i32, flags: u32) {
        let mut cq = self.cq.lock();

        let idx = (cq.tail & cq.mask) as usize;
        cq.entries[idx] = IoUringCqe {
            user_data,
            res: result,
            flags,
        };
        cq.tail = cq.tail.wrapping_add(1);
    }

    /// Process pending operations (stub)
    fn process_pending(&self) {
        let mut pending = self.pending.lock();

        // In real implementation, this would process async I/O
        // For now, just complete with success
        while let Some(op) = pending.pop_front() {
            self.complete(op.sqe.user_data, 0, 0);
        }
    }

    /// Get parameters
    pub fn params(&self) -> &IoUringParams {
        &self.params
    }
}

/// io_uring error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoUringError {
    /// Queue is full
    QueueFull,
    /// Would block
    WouldBlock,
    /// Invalid operation
    InvalidOp,
    /// Bad file descriptor
    BadFd,
    /// Busy
    Busy,
}

impl core::fmt::Display for IoUringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::QueueFull => write!(f, "queue full"),
            Self::WouldBlock => write!(f, "would block"),
            Self::InvalidOp => write!(f, "invalid operation"),
            Self::BadFd => write!(f, "bad file descriptor"),
            Self::Busy => write!(f, "busy"),
        }
    }
}

/// Global io_uring instances
static INSTANCES: Mutex<BTreeMap<i32, IoUringInstance>> = Mutex::new(BTreeMap::new());
static NEXT_FD: Mutex<i32> = Mutex::new(1000);

/// Setup io_uring
pub fn io_uring_setup(entries: u32, params: &mut IoUringParams) -> Result<i32, IoUringError> {
    let mut next_fd = NEXT_FD.lock();
    let fd = *next_fd;
    *next_fd += 1;

    let instance = IoUringInstance::new(entries, fd as u64);
    *params = instance.params.clone();

    INSTANCES.lock().insert(fd, instance);

    Ok(fd)
}

/// Enter io_uring
pub fn io_uring_enter(
    fd: i32,
    to_submit: u32,
    min_complete: u32,
    _flags: u32,
) -> Result<i32, IoUringError> {
    let instances = INSTANCES.lock();
    let instance = instances.get(&fd).ok_or(IoUringError::BadFd)?;

    let _ = to_submit; // All pending SQEs are submitted

    instance.submit_and_wait(min_complete)?;

    Ok(0)
}
