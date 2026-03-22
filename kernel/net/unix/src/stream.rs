//! Unix Stream Channel — Bidirectional byte stream for SOCK_STREAM
//!
//! — ByteRiot: Two ring buffers welded back-to-back. End A writes to a_to_b,
//! reads from b_to_a. End B is the mirror. Each direction has its own WaitQueue
//! pair (read/write) so blocking is per-direction, not per-socket.
//!
//! Modeled after pipe.rs but bidirectional. The Mutex protects only the ring
//! buffer data — WaitQueues live outside for ISR-safe wake without lock contention.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use waitqueue::WaitQueue;

use crate::ancillary::CmsgData;
use crate::{ChannelEnd, UnixError};

// — ByteRiot: External scheduler hooks. Same pattern as pipe.rs.
// Can't depend on sched directly (circular dep), so we use extern "Rust".
unsafe extern "Rust" {
    fn sched_block_interruptible();
    fn sched_current_pid() -> Option<u32>;
}

/// Ring buffer for one direction of a Unix stream channel.
/// — ByteRiot: Fixed capacity, no reallocation. Ring wraps at capacity.
struct RingBuffer {
    data: Vec<u8>,
    read_pos: usize,
    write_pos: usize,
    count: usize,
    capacity: usize,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        let mut data = Vec::with_capacity(capacity);
        data.resize(capacity, 0);
        RingBuffer {
            data,
            read_pos: 0,
            write_pos: 0,
            count: 0,
            capacity,
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> usize {
        if self.count == 0 {
            return 0;
        }
        let to_read = buf.len().min(self.count);
        for i in 0..to_read {
            buf[i] = self.data[self.read_pos];
            self.read_pos = (self.read_pos + 1) % self.capacity;
        }
        self.count -= to_read;
        to_read
    }

    fn write(&mut self, buf: &[u8]) -> usize {
        let available = self.capacity - self.count;
        if available == 0 {
            return 0;
        }
        let to_write = buf.len().min(available);
        for i in 0..to_write {
            self.data[self.write_pos] = buf[i];
            self.write_pos = (self.write_pos + 1) % self.capacity;
        }
        self.count += to_write;
        to_write
    }

    fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn is_full(&self) -> bool {
        self.count == self.capacity
    }

    fn has_space(&self) -> bool {
        self.count < self.capacity
    }
}

/// One direction of a Unix stream channel.
/// — ByteRiot: Data ring + ancillary data queue. The cmsg queue is separate
/// because control messages have message boundaries even in a byte stream.
struct DirectionState {
    ring: RingBuffer,
    /// Ancillary data queue — each entry corresponds to the NEXT read boundary
    cmsg_queue: VecDeque<(usize, Vec<CmsgData>)>,
    /// Bytes read so far in current cmsg boundary tracking
    bytes_read_since_cmsg: usize,
}

impl DirectionState {
    fn new(capacity: usize) -> Self {
        DirectionState {
            ring: RingBuffer::new(capacity),
            cmsg_queue: VecDeque::new(),
            bytes_read_since_cmsg: 0,
        }
    }
}

/// Bidirectional Unix stream channel.
/// — ByteRiot: Shared between two connected sockets via Arc. Each socket holds
/// one end (A or B). Dropping one end doesn't destroy the channel — the other
/// end sees EOF on next read (peer_closed flags).
pub struct UnixStreamChannel {
    /// A→B direction: A writes, B reads
    a_to_b: Mutex<DirectionState>,
    /// B→A direction: B writes, A reads
    b_to_a: Mutex<DirectionState>,

    /// — ByteRiot: WaitQueues OUTSIDE the Mutex. Lock-free register/wake.
    /// Read WQ for end A (woken when B writes to b_to_a, or B closes)
    pub a_read_wq: WaitQueue,
    /// Write WQ for end A (woken when B reads from a_to_b)
    pub a_write_wq: WaitQueue,
    /// Read WQ for end B (woken when A writes to a_to_b, or A closes)
    pub b_read_wq: WaitQueue,
    /// Write WQ for end B (woken when A reads from b_to_a)
    pub b_write_wq: WaitQueue,

    /// Shutdown flags — per-end, per-direction
    a_read_shutdown: AtomicBool,
    a_write_shutdown: AtomicBool,
    b_read_shutdown: AtomicBool,
    b_write_shutdown: AtomicBool,

    /// Closed flags — set when socket is dropped
    a_closed: AtomicBool,
    b_closed: AtomicBool,
}

impl UnixStreamChannel {
    pub fn new(buf_size: usize) -> Self {
        UnixStreamChannel {
            a_to_b: Mutex::new(DirectionState::new(buf_size)),
            b_to_a: Mutex::new(DirectionState::new(buf_size)),
            a_read_wq: WaitQueue::new(),
            a_write_wq: WaitQueue::new(),
            b_read_wq: WaitQueue::new(),
            b_write_wq: WaitQueue::new(),
            a_read_shutdown: AtomicBool::new(false),
            a_write_shutdown: AtomicBool::new(false),
            b_read_shutdown: AtomicBool::new(false),
            b_write_shutdown: AtomicBool::new(false),
            a_closed: AtomicBool::new(false),
            b_closed: AtomicBool::new(false),
        }
    }

    /// Read data from the channel for the given end.
    /// — ByteRiot: End A reads from b_to_a, End B reads from a_to_b.
    /// Returns 0 on EOF (peer closed + buffer empty). Blocks if no data
    /// and peer is alive (handled by vnode wrapper, not here).
    pub fn read(&self, end: ChannelEnd, buf: &mut [u8]) -> Result<usize, UnixError> {
        let dir = match end {
            ChannelEnd::A => &self.b_to_a,
            ChannelEnd::B => &self.a_to_b,
        };

        let mut state = dir.lock();
        let n = state.ring.read(buf);

        if n > 0 {
            // Wake the writer — we freed space
            match end {
                ChannelEnd::A => self.b_write_wq.wake_all(),
                ChannelEnd::B => self.a_write_wq.wake_all(),
            }
            return Ok(n);
        }

        // Buffer empty — check if peer is gone
        if self.peer_closed(end) {
            return Ok(0); // EOF
        }

        Err(UnixError::WouldBlock)
    }

    /// Read data + any pending ancillary data.
    pub fn read_with_cmsg(
        &self,
        end: ChannelEnd,
        buf: &mut [u8],
    ) -> Result<(usize, Vec<CmsgData>), UnixError> {
        let dir = match end {
            ChannelEnd::A => &self.b_to_a,
            ChannelEnd::B => &self.a_to_b,
        };

        let mut state = dir.lock();
        let n = state.ring.read(buf);

        let cmsg = if n > 0 {
            // Check if there's ancillary data at this read boundary
            let boundary_val = state.cmsg_queue.front().map(|(b, _)| *b);
            if let Some(boundary) = boundary_val {
                state.bytes_read_since_cmsg += n;
                if state.bytes_read_since_cmsg >= boundary {
                    state.bytes_read_since_cmsg = 0;
                    state.cmsg_queue.pop_front().map(|(_, c)| c).unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else if !state.cmsg_queue.is_empty() {
                // Cmsg with zero-byte boundary (pure ancillary, no data)
                state.cmsg_queue.pop_front().map(|(_, c)| c).unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            // No data — but there might be pure ancillary messages
            let is_zero_boundary = state.cmsg_queue.front().map(|(b, _)| *b == 0).unwrap_or(false);
            if is_zero_boundary {
                state.cmsg_queue.pop_front().map(|(_, c)| c).unwrap_or_default()
            } else {
                Vec::new()
            }
        };

        if n > 0 {
            match end {
                ChannelEnd::A => self.b_write_wq.wake_all(),
                ChannelEnd::B => self.a_write_wq.wake_all(),
            }
            return Ok((n, cmsg));
        }

        if self.peer_closed(end) {
            return Ok((0, cmsg));
        }

        if !cmsg.is_empty() {
            return Ok((0, cmsg));
        }

        Err(UnixError::WouldBlock)
    }

    /// Write data to the channel for the given end.
    /// — ByteRiot: End A writes to a_to_b, End B writes to b_to_a.
    pub fn write(&self, end: ChannelEnd, buf: &[u8]) -> Result<usize, UnixError> {
        if self.peer_closed(end) {
            return Err(UnixError::BrokenPipe);
        }

        let dir = match end {
            ChannelEnd::A => &self.a_to_b,
            ChannelEnd::B => &self.b_to_a,
        };

        let mut state = dir.lock();
        let n = state.ring.write(buf);

        if n > 0 {
            // Wake the reader
            match end {
                ChannelEnd::A => self.b_read_wq.wake_all(),
                ChannelEnd::B => self.a_read_wq.wake_all(),
            }
            return Ok(n);
        }

        Err(UnixError::WouldBlock)
    }

    /// Write data + ancillary data.
    pub fn write_with_cmsg(
        &self,
        end: ChannelEnd,
        buf: &[u8],
        cmsg: Vec<CmsgData>,
    ) -> Result<usize, UnixError> {
        if self.peer_closed(end) {
            return Err(UnixError::BrokenPipe);
        }

        let dir = match end {
            ChannelEnd::A => &self.a_to_b,
            ChannelEnd::B => &self.b_to_a,
        };

        let mut state = dir.lock();
        let n = state.ring.write(buf);

        if n > 0 || !cmsg.is_empty() {
            if !cmsg.is_empty() {
                state.cmsg_queue.push_back((n, cmsg));
            }
            // Wake the reader
            match end {
                ChannelEnd::A => self.b_read_wq.wake_all(),
                ChannelEnd::B => self.a_read_wq.wake_all(),
            }
            if n > 0 {
                return Ok(n);
            }
            // Pure ancillary with no data — still "sent" something
            return Ok(0);
        }

        Err(UnixError::WouldBlock)
    }

    /// Check if the peer end is closed.
    pub fn peer_closed(&self, our_end: ChannelEnd) -> bool {
        match our_end {
            ChannelEnd::A => self.b_closed.load(Ordering::Acquire),
            ChannelEnd::B => self.a_closed.load(Ordering::Acquire),
        }
    }

    /// Check if our read buffer has data.
    pub fn has_data(&self, our_end: ChannelEnd) -> bool {
        let dir = match our_end {
            ChannelEnd::A => &self.b_to_a,
            ChannelEnd::B => &self.a_to_b,
        };
        !dir.lock().ring.is_empty()
    }

    /// Check if our write buffer has space.
    pub fn has_space(&self, our_end: ChannelEnd) -> bool {
        let dir = match our_end {
            ChannelEnd::A => &self.a_to_b,
            ChannelEnd::B => &self.b_to_a,
        };
        dir.lock().ring.has_space()
    }

    /// Mark our end as closed. Called when socket is dropped.
    /// — ByteRiot: Wakes the peer so they get EOF on next read.
    pub fn close_end(&self, end: ChannelEnd) {
        match end {
            ChannelEnd::A => {
                self.a_closed.store(true, Ordering::Release);
                // Wake B's readers (they'll see EOF) and writers (they'll see EPIPE)
                self.b_read_wq.wake_all();
                self.b_write_wq.wake_all();
            }
            ChannelEnd::B => {
                self.b_closed.store(true, Ordering::Release);
                self.a_read_wq.wake_all();
                self.a_write_wq.wake_all();
            }
        }
    }

    /// Shutdown read direction for given end.
    pub fn shutdown_read(&self, end: ChannelEnd) {
        match end {
            ChannelEnd::A => self.a_read_shutdown.store(true, Ordering::Release),
            ChannelEnd::B => self.b_read_shutdown.store(true, Ordering::Release),
        }
    }

    /// Shutdown write direction for given end.
    pub fn shutdown_write(&self, end: ChannelEnd) {
        match end {
            ChannelEnd::A => {
                self.a_write_shutdown.store(true, Ordering::Release);
                // Wake B's readers — no more data coming from A
                self.b_read_wq.wake_all();
            }
            ChannelEnd::B => {
                self.b_write_shutdown.store(true, Ordering::Release);
                self.a_read_wq.wake_all();
            }
        }
    }

    /// Get the read wait queue for the given end.
    pub fn read_wq(&self, end: ChannelEnd) -> &WaitQueue {
        match end {
            ChannelEnd::A => &self.a_read_wq,
            ChannelEnd::B => &self.b_read_wq,
        }
    }

    /// Get the write wait queue for the given end.
    pub fn write_wq(&self, end: ChannelEnd) -> &WaitQueue {
        match end {
            ChannelEnd::A => &self.a_write_wq,
            ChannelEnd::B => &self.b_write_wq,
        }
    }
}
