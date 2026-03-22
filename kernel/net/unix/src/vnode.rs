//! VnodeOps wrapper for Unix sockets
//!
//! — SableWire: The bridge between AF_UNIX sockets and the VFS world. This
//! wrapper lets Unix sockets live in the FdTable alongside regular files, pipes,
//! and device nodes. poll/epoll/select work natively because we implement
//! poll_read_ready/poll_write_ready/poll_register_wait.
//!
//! Without this wrapper, socket fds would be stuck in the SOCKET_TABLE (1000+)
//! ghetto, invisible to the VFS poll infrastructure. Wayland needs epoll on
//! its socket fd — this makes it happen.

use alloc::sync::Arc;
use core::any::Any;

use vfs::error::{VfsError, VfsResult};
use vfs::vnode::{DirEntry, Mode, Stat, VnodeOps, VnodeType};

use crate::{UnixError, UnixSocket, UnixSocketState, UnixSocketType};

// — SableWire: Scheduler hooks for blocking I/O. Same pattern as pipe.rs.
unsafe extern "Rust" {
    fn sched_block_interruptible();
    fn sched_current_pid() -> Option<u32>;
}

/// VnodeOps wrapper around a UnixSocket.
/// — SableWire: Installed in the FdTable when sys_socket(AF_UNIX) is called.
/// read() and write() delegate to stream_read/stream_write with blocking.
pub struct UnixSocketVnode {
    pub socket: Arc<UnixSocket>,
}

impl UnixSocketVnode {
    pub fn new(socket: Arc<UnixSocket>) -> Self {
        UnixSocketVnode { socket }
    }

    /// Get the underlying socket (for syscall layer to access bind/listen/connect/etc.)
    pub fn socket(&self) -> &Arc<UnixSocket> {
        &self.socket
    }
}

impl VnodeOps for UnixSocketVnode {
    fn vtype(&self) -> VnodeType {
        VnodeType::Socket
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        // — SableWire: Blocking read loop. Mirrors pipe.rs PipeRead pattern:
        // try read → if WouldBlock, register on WaitQueue → re-check → block.
        match self.socket.sock_type {
            UnixSocketType::Stream => {
                loop {
                    match self.socket.stream_read(buf) {
                        Ok(n) => return Ok(n),
                        Err(UnixError::WouldBlock) => {
                            if self.socket.nonblocking.load(core::sync::atomic::Ordering::Acquire) {
                                return Err(VfsError::WouldBlock);
                            }
                            // Block on the channel's read wait queue
                            let ch = self.socket.channel.lock();
                            if let Some((channel, end)) = ch.as_ref() {
                                let channel = Arc::clone(channel);
                                let end = *end;
                                drop(ch);

                                let pid = unsafe { sched_current_pid() };
                                if let Some(pid) = pid {
                                    let wq = channel.read_wq(end);
                                    if let Some(slot) = wq.register(pid) {
                                        // Re-check before blocking (lost wake window)
                                        if channel.has_data(end) || channel.peer_closed(end) {
                                            wq.unregister(slot);
                                            continue;
                                        }
                                        unsafe { sched_block_interruptible(); }
                                        wq.unregister(slot);
                                    }
                                }
                            } else {
                                return Err(VfsError::NotConnected);
                            }
                        }
                        Err(UnixError::NotConnected) => return Err(VfsError::NotConnected),
                        Err(UnixError::BrokenPipe) => return Ok(0),
                        Err(_) => return Err(VfsError::IoError),
                    }
                }
            }
            UnixSocketType::Dgram => {
                // For dgram, read returns the next message (truncated to buf size)
                loop {
                    match self.socket.dgram_recvfrom(buf) {
                        Ok((n, _addr, _cmsg)) => return Ok(n),
                        Err(UnixError::WouldBlock) => {
                            if self.socket.nonblocking.load(core::sync::atomic::Ordering::Acquire) {
                                return Err(VfsError::WouldBlock);
                            }
                            let pid = unsafe { sched_current_pid() };
                            if let Some(pid) = pid {
                                if let Some(slot) = self.socket.dgram_recv_wq.register(pid) {
                                    if !self.socket.dgram_queue.lock().is_empty() {
                                        self.socket.dgram_recv_wq.unregister(slot);
                                        continue;
                                    }
                                    unsafe { sched_block_interruptible(); }
                                    self.socket.dgram_recv_wq.unregister(slot);
                                }
                            }
                        }
                        Err(_) => return Err(VfsError::IoError),
                    }
                }
            }
        }
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // — SableWire: Blocking write loop for stream sockets.
        match self.socket.sock_type {
            UnixSocketType::Stream => {
                loop {
                    match self.socket.stream_write(buf) {
                        Ok(n) => return Ok(n),
                        Err(UnixError::WouldBlock) => {
                            if self.socket.nonblocking.load(core::sync::atomic::Ordering::Acquire) {
                                return Err(VfsError::WouldBlock);
                            }
                            let ch = self.socket.channel.lock();
                            if let Some((channel, end)) = ch.as_ref() {
                                let channel = Arc::clone(channel);
                                let end = *end;
                                drop(ch);

                                let pid = unsafe { sched_current_pid() };
                                if let Some(pid) = pid {
                                    let wq = channel.write_wq(end);
                                    if let Some(slot) = wq.register(pid) {
                                        if channel.has_space(end) || channel.peer_closed(end) {
                                            wq.unregister(slot);
                                            continue;
                                        }
                                        unsafe { sched_block_interruptible(); }
                                        wq.unregister(slot);
                                    }
                                }
                            } else {
                                return Err(VfsError::NotConnected);
                            }
                        }
                        Err(UnixError::BrokenPipe) => return Err(VfsError::BrokenPipe),
                        Err(UnixError::NotConnected) => return Err(VfsError::NotConnected),
                        Err(_) => return Err(VfsError::IoError),
                    }
                }
            }
            UnixSocketType::Dgram => {
                // Dgram write without destination doesn't make sense via VnodeOps
                // Userspace should use sendto/sendmsg instead
                Err(VfsError::NotSupported)
            }
        }
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
        Ok(Stat::new(VnodeType::Socket, Mode::new(0o777), 0, self.socket.ino))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn poll_read_ready(&self) -> bool {
        self.socket.poll_read_ready()
    }

    fn poll_write_ready(&self) -> bool {
        self.socket.poll_write_ready()
    }

    fn poll_register_wait(&self, table: &mut waitqueue::PollTable) {
        // — SableWire: Register on the appropriate wait queue based on socket state.
        // For connected stream sockets, register on the channel's read WQ.
        // For listening sockets, register on the accept WQ.
        // For dgram sockets, register on the dgram recv WQ.
        match self.socket.sock_type {
            UnixSocketType::Stream => {
                let state = self.socket.state.lock();
                match *state {
                    UnixSocketState::Listening => {
                        table.register(&self.socket.accept_wq);
                    }
                    UnixSocketState::Connected => {
                        drop(state);
                        let ch = self.socket.channel.lock();
                        if let Some((channel, end)) = ch.as_ref() {
                            table.register(channel.read_wq(*end));
                        }
                    }
                    _ => {}
                }
            }
            UnixSocketType::Dgram => {
                table.register(&self.socket.dgram_recv_wq);
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
