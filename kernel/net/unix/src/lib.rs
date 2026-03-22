//! AF_UNIX Domain Socket Implementation
//!
//! — ShadePacket: The neon underground of IPC. While IP sockets route through
//! the loopback wasteland, Unix sockets cut through the noise — direct process-
//! to-process channels with zero network overhead. Every Wayland frame, every
//! DBus method call, every X11 event flows through these pipes.
//!
//! Custom OXIDE implementation. Linux ABI compatible. Not a port.

#![no_std]

extern crate alloc;

pub mod stream;
pub mod dgram;
pub mod ancillary;
pub mod vnode;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;
use waitqueue::WaitQueue;

use stream::UnixStreamChannel;

// — ShadePacket: Linux sockaddr_un.sun_path is 108 bytes. Not 107, not 109.
// Every userspace library in existence hardcodes this. Don't get creative.
pub const UNIX_PATH_MAX: usize = 108;

/// — ShadePacket: Default buffer size per direction. Linux uses 212992 (208KB)
/// for AF_UNIX SO_SNDBUF. We match it because Wayland compositors assume they
/// can shove entire frame metadata through without blocking.
pub const UNIX_STREAM_BUF_SIZE: usize = 212992;

/// — ShadePacket: Datagram max message size. Linux caps at SO_SNDBUF.
pub const UNIX_DGRAM_MAX_MSG: usize = 65536;

/// — ShadePacket: Max pending connections for listen backlog.
pub const UNIX_MAX_BACKLOG: u32 = 128;

/// Global inode counter for socket filesystem entries
static UNIX_INODE_COUNTER: AtomicU64 = AtomicU64::new(0x5F_0000_0000);

pub fn next_unix_inode() -> u64 {
    UNIX_INODE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Unix socket address — the identity of a bound socket.
/// — ShadePacket: Three flavors, each with different lifetime rules:
/// - Path: lives in the filesystem, survives process death, must be unlink'd
/// - Abstract: lives in kernel memory, auto-cleaned on last close
/// - Unnamed: socketpair/unbound — no address at all
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnixAddr {
    /// Filesystem path (e.g., "/tmp/wayland-0", "/run/dbus/system_bus_socket")
    Path(String),
    /// Abstract namespace — NUL-prefixed, no filesystem entry.
    /// — ShadePacket: Linux-specific extension. sun_path[0] == '\0', rest is name.
    /// Wayland uses this when XDG_RUNTIME_DIR is unavailable.
    Abstract(Vec<u8>),
    /// Unnamed — socketpair or unconnected socket.
    Unnamed,
}

/// Socket state machine
/// — ShadePacket: Simpler than TCP's 11-state FSM. Unix sockets are local —
/// no SYN/FIN handshakes, no TIME_WAIT, no retransmits. Just connect or don't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixSocketState {
    /// Fresh socket, not bound or connected
    Unbound,
    /// Bound to an address (path or abstract)
    Bound,
    /// Listening for connections (SOCK_STREAM only)
    Listening,
    /// Connected to a peer (SOCK_STREAM only)
    Connected,
    /// Connection refused or peer gone
    Disconnected,
}

/// Socket type — only Stream and Dgram for AF_UNIX.
/// — ShadePacket: SeqPacket exists in Linux but literally nobody uses it.
/// We implement it anyway because glib checks for it at configure time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixSocketType {
    /// Reliable bidirectional byte stream (like TCP but local)
    Stream = 1,
    /// Unreliable datagram (like UDP but local — actually reliable since no network)
    Dgram = 2,
}

/// Peer credentials — captured at connect time for SCM_CREDENTIALS.
/// — ColdCipher: These are immutable once captured. The process that connected
/// is identified forever by these values, even if it later calls setuid().
#[derive(Debug, Clone, Copy, Default)]
pub struct UCred {
    pub pid: u32,
    pub uid: u32,
    pub gid: u32,
}

/// Which end of a bidirectional channel we are
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelEnd {
    A,
    B,
}

/// The Unix domain socket.
/// — ShadePacket: One struct to rule them all — stream and dgram, client and
/// server, bound and unbound. The state machine + sock_type determine which
/// fields are active at any given time.
pub struct UnixSocket {
    /// Stream or Datagram
    pub sock_type: UnixSocketType,
    /// Local address (Unnamed until bind())
    pub local_addr: Mutex<UnixAddr>,
    /// Current state
    pub state: Mutex<UnixSocketState>,
    /// For connected SOCK_STREAM: the shared bidirectional channel + our end
    pub channel: Mutex<Option<(Arc<UnixStreamChannel>, ChannelEnd)>>,
    /// For listening SOCK_STREAM: pending connection queue (connecting sockets)
    pub backlog: Mutex<Vec<Arc<UnixSocket>>>,
    /// Maximum backlog size
    pub backlog_max: AtomicU32,
    /// For SOCK_DGRAM: received messages with sender address and ancillary data
    pub dgram_queue: Mutex<Vec<(Vec<u8>, UnixAddr, Vec<ancillary::CmsgData>)>>,
    /// Wait queue for dgram receive
    pub dgram_recv_wq: WaitQueue,
    /// Wait queue for accept() — woken when new connection arrives
    pub accept_wq: WaitQueue,
    /// Non-blocking mode flag
    pub nonblocking: AtomicBool,
    /// Socket has been shut down / closed
    pub closed: AtomicBool,
    /// Peer credentials (set on connect/accept)
    pub peer_cred: Mutex<Option<UCred>>,
    /// Our own credentials (captured at socket creation)
    pub local_cred: UCred,
    /// Pass credentials flag (SO_PASSCRED)
    pub passcred: AtomicBool,
    /// Inode number for stat()
    pub ino: u64,
}

impl UnixSocket {
    /// Create a new unbound Unix socket.
    /// — ShadePacket: Born unnamed, unconnected, full of potential.
    pub fn new(sock_type: UnixSocketType, cred: UCred) -> Arc<Self> {
        Arc::new(UnixSocket {
            sock_type,
            local_addr: Mutex::new(UnixAddr::Unnamed),
            state: Mutex::new(UnixSocketState::Unbound),
            channel: Mutex::new(None),
            backlog: Mutex::new(Vec::new()),
            backlog_max: AtomicU32::new(UNIX_MAX_BACKLOG),
            dgram_queue: Mutex::new(Vec::new()),
            dgram_recv_wq: WaitQueue::new(),
            accept_wq: WaitQueue::new(),
            nonblocking: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            peer_cred: Mutex::new(None),
            local_cred: cred,
            passcred: AtomicBool::new(false),
            ino: next_unix_inode(),
        })
    }

    /// Create a connected pair of stream sockets (for socketpair).
    /// — ShadePacket: The fast path. No bind, no listen, no accept dance.
    /// Just two sockets welded together at birth. Wayland uses this internally.
    pub fn create_pair(cred: UCred) -> (Arc<Self>, Arc<Self>) {
        let channel = Arc::new(UnixStreamChannel::new(UNIX_STREAM_BUF_SIZE));

        let a = Arc::new(UnixSocket {
            sock_type: UnixSocketType::Stream,
            local_addr: Mutex::new(UnixAddr::Unnamed),
            state: Mutex::new(UnixSocketState::Connected),
            channel: Mutex::new(Some((Arc::clone(&channel), ChannelEnd::A))),
            backlog: Mutex::new(Vec::new()),
            backlog_max: AtomicU32::new(0),
            dgram_queue: Mutex::new(Vec::new()),
            dgram_recv_wq: WaitQueue::new(),
            accept_wq: WaitQueue::new(),
            nonblocking: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            peer_cred: Mutex::new(Some(cred)),
            local_cred: cred,
            passcred: AtomicBool::new(false),
            ino: next_unix_inode(),
        });

        let b = Arc::new(UnixSocket {
            sock_type: UnixSocketType::Stream,
            local_addr: Mutex::new(UnixAddr::Unnamed),
            state: Mutex::new(UnixSocketState::Connected),
            channel: Mutex::new(Some((channel, ChannelEnd::B))),
            backlog: Mutex::new(Vec::new()),
            backlog_max: AtomicU32::new(0),
            dgram_queue: Mutex::new(Vec::new()),
            dgram_recv_wq: WaitQueue::new(),
            accept_wq: WaitQueue::new(),
            nonblocking: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            peer_cred: Mutex::new(Some(cred)),
            local_cred: cred,
            passcred: AtomicBool::new(false),
            ino: next_unix_inode(),
        });

        (a, b)
    }

    /// Bind this socket to an address.
    /// — ShadePacket: Claims a name in the registry. Two sockets can't bind
    /// the same path — first one wins, everyone else gets EADDRINUSE.
    pub fn bind(&self, addr: UnixAddr) -> Result<(), UnixError> {
        let mut state = self.state.lock();
        if *state != UnixSocketState::Unbound {
            return Err(UnixError::AlreadyBound);
        }

        // Register in global namespace
        let registry = UNIX_SOCKET_REGISTRY.lock();
        if registry.contains_key(&addr) {
            return Err(UnixError::AddrInUse);
        }

        // We don't store Arc<Self> in registry from &self — caller must handle this
        *self.local_addr.lock() = addr.clone();
        *state = UnixSocketState::Bound;
        Ok(())
    }

    /// Start listening for connections (SOCK_STREAM only).
    /// — ShadePacket: Flips the switch from "I want to connect" to "come to me."
    pub fn listen(&self, backlog: u32) -> Result<(), UnixError> {
        if self.sock_type != UnixSocketType::Stream {
            return Err(UnixError::WrongType);
        }

        let mut state = self.state.lock();
        if *state != UnixSocketState::Bound {
            return Err(UnixError::NotBound);
        }

        self.backlog_max.store(backlog.min(UNIX_MAX_BACKLOG), Ordering::Release);
        *state = UnixSocketState::Listening;
        Ok(())
    }

    /// Connect to a listening socket (SOCK_STREAM).
    /// — ShadePacket: Creates the channel, queues ourselves on the listener's
    /// backlog, and wakes up anyone sleeping in accept(). If the backlog is full
    /// we fail with ECONNREFUSED — no SYN retries in the Unix domain.
    pub fn connect_to(
        self: &Arc<Self>,
        listener: &Arc<UnixSocket>,
    ) -> Result<(), UnixError> {
        if self.sock_type != UnixSocketType::Stream {
            return Err(UnixError::WrongType);
        }

        let listener_state = listener.state.lock();
        if *listener_state != UnixSocketState::Listening {
            return Err(UnixError::ConnectionRefused);
        }
        drop(listener_state);

        let mut listener_backlog = listener.backlog.lock();
        let max = listener.backlog_max.load(Ordering::Acquire) as usize;
        if listener_backlog.len() >= max {
            return Err(UnixError::ConnectionRefused);
        }

        // Create the bidirectional channel
        let channel = Arc::new(UnixStreamChannel::new(UNIX_STREAM_BUF_SIZE));

        // We are end A, the accepted socket will be end B
        *self.channel.lock() = Some((Arc::clone(&channel), ChannelEnd::A));
        *self.state.lock() = UnixSocketState::Connected;
        *self.peer_cred.lock() = Some(listener.local_cred);

        // Queue ourselves — accept() will create the server-side socket with end B
        listener_backlog.push(Arc::clone(self));
        drop(listener_backlog);

        // — ShadePacket: Wake the accept() sleepers. Time to serve.
        listener.accept_wq.wake_all();

        Ok(())
    }

    /// Accept a pending connection (SOCK_STREAM, listening socket only).
    /// — ShadePacket: Pulls the next connector off the backlog, creates the
    /// server-side socket with the other end of their channel. Returns None
    /// if no pending connections (caller should block or return EAGAIN).
    pub fn accept(&self, local_cred: UCred) -> Result<Option<Arc<UnixSocket>>, UnixError> {
        if self.sock_type != UnixSocketType::Stream {
            return Err(UnixError::WrongType);
        }

        let state = self.state.lock();
        if *state != UnixSocketState::Listening {
            return Err(UnixError::NotListening);
        }
        drop(state);

        let mut backlog = self.backlog.lock();
        if backlog.is_empty() {
            return Ok(None);
        }

        let connector = backlog.remove(0);

        // Get the channel from the connector (they're end A)
        let channel = {
            let ch = connector.channel.lock();
            match ch.as_ref() {
                Some((ch, ChannelEnd::A)) => Arc::clone(ch),
                _ => return Err(UnixError::Internal),
            }
        };

        // Create the server-side socket (end B)
        let server_sock = Arc::new(UnixSocket {
            sock_type: UnixSocketType::Stream,
            local_addr: Mutex::new(self.local_addr.lock().clone()),
            state: Mutex::new(UnixSocketState::Connected),
            channel: Mutex::new(Some((channel, ChannelEnd::B))),
            backlog: Mutex::new(Vec::new()),
            backlog_max: AtomicU32::new(0),
            dgram_queue: Mutex::new(Vec::new()),
            dgram_recv_wq: WaitQueue::new(),
            accept_wq: WaitQueue::new(),
            nonblocking: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            peer_cred: Mutex::new(Some(connector.local_cred)),
            local_cred,
            passcred: AtomicBool::new(false),
            ino: next_unix_inode(),
        });

        // Set the connector's peer cred to ours
        *connector.peer_cred.lock() = Some(local_cred);

        Ok(Some(server_sock))
    }

    /// Read data from a connected stream socket.
    /// — ShadePacket: Delegates to the channel. Returns 0 on EOF (peer closed).
    pub fn stream_read(&self, buf: &mut [u8]) -> Result<usize, UnixError> {
        let ch = self.channel.lock();
        let (channel, end) = ch.as_ref().ok_or(UnixError::NotConnected)?;
        let channel = Arc::clone(channel);
        let end = *end;
        drop(ch);

        channel.read(end, buf)
    }

    /// Write data to a connected stream socket.
    /// — ShadePacket: Delegates to the channel. Returns EPIPE if peer closed.
    pub fn stream_write(&self, buf: &[u8]) -> Result<usize, UnixError> {
        let ch = self.channel.lock();
        let (channel, end) = ch.as_ref().ok_or(UnixError::NotConnected)?;
        let channel = Arc::clone(channel);
        let end = *end;
        drop(ch);

        channel.write(end, buf)
    }

    /// Send a message with ancillary data (for sendmsg).
    /// — ShadePacket: The SCM_RIGHTS express lane. Data + fds in one atomic shot.
    pub fn stream_send_with_ancillary(
        &self,
        buf: &[u8],
        cmsg: Vec<ancillary::CmsgData>,
    ) -> Result<usize, UnixError> {
        let ch = self.channel.lock();
        let (channel, end) = ch.as_ref().ok_or(UnixError::NotConnected)?;
        let channel = Arc::clone(channel);
        let end = *end;
        drop(ch);

        channel.write_with_cmsg(end, buf, cmsg)
    }

    /// Receive a message with ancillary data (for recvmsg).
    pub fn stream_recv_with_ancillary(
        &self,
        buf: &mut [u8],
    ) -> Result<(usize, Vec<ancillary::CmsgData>), UnixError> {
        let ch = self.channel.lock();
        let (channel, end) = ch.as_ref().ok_or(UnixError::NotConnected)?;
        let channel = Arc::clone(channel);
        let end = *end;
        drop(ch);

        channel.read_with_cmsg(end, buf)
    }

    /// Check if data is available for reading without blocking.
    pub fn poll_read_ready(&self) -> bool {
        match self.sock_type {
            UnixSocketType::Stream => {
                let state = self.state.lock();
                match *state {
                    UnixSocketState::Listening => {
                        !self.backlog.lock().is_empty()
                    }
                    UnixSocketState::Connected => {
                        let ch = self.channel.lock();
                        if let Some((channel, end)) = ch.as_ref() {
                            channel.has_data(*end) || channel.peer_closed(*end)
                        } else {
                            true // disconnected = readable (returns 0/EOF)
                        }
                    }
                    UnixSocketState::Disconnected => true, // EOF
                    _ => false,
                }
            }
            UnixSocketType::Dgram => {
                !self.dgram_queue.lock().is_empty()
            }
        }
    }

    /// Check if the socket can accept writes without blocking.
    pub fn poll_write_ready(&self) -> bool {
        match self.sock_type {
            UnixSocketType::Stream => {
                let ch = self.channel.lock();
                if let Some((channel, end)) = ch.as_ref() {
                    channel.has_space(*end)
                } else {
                    false
                }
            }
            UnixSocketType::Dgram => true, // dgram sends don't block in our impl
        }
    }

    /// Get the appropriate WaitQueue for poll registration.
    pub fn get_read_wq(&self) -> Option<&WaitQueue> {
        match self.sock_type {
            UnixSocketType::Stream => {
                let state = self.state.lock();
                if *state == UnixSocketState::Listening {
                    Some(&self.accept_wq)
                } else {
                    // For connected sockets, the WaitQueue is inside the channel
                    // We can't return a reference to it from behind an Arc+Mutex
                    // Poll registration happens in the vnode wrapper instead
                    None
                }
            }
            UnixSocketType::Dgram => Some(&self.dgram_recv_wq),
        }
    }

    /// Shutdown read/write/both directions.
    pub fn shutdown(&self, how: u32) -> Result<(), UnixError> {
        let ch = self.channel.lock();
        if let Some((channel, end)) = ch.as_ref() {
            match how {
                0 => channel.shutdown_read(*end),
                1 => channel.shutdown_write(*end),
                2 => {
                    channel.shutdown_read(*end);
                    channel.shutdown_write(*end);
                }
                _ => return Err(UnixError::InvalidArg),
            }
            Ok(())
        } else if self.sock_type == UnixSocketType::Dgram {
            // Dgram shutdown just marks closed
            self.closed.store(true, Ordering::Release);
            Ok(())
        } else {
            Err(UnixError::NotConnected)
        }
    }
}

/// Error types for Unix socket operations.
/// — ShadePacket: Maps 1:1 to Linux errno values in the syscall layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixError {
    /// Socket already bound to an address (EINVAL)
    AlreadyBound,
    /// Address already in use (EADDRINUSE)
    AddrInUse,
    /// Wrong socket type for operation (EOPNOTSUPP)
    WrongType,
    /// Socket not bound (EINVAL)
    NotBound,
    /// Socket not listening (EINVAL)
    NotListening,
    /// Socket not connected (ENOTCONN)
    NotConnected,
    /// Connection refused (ECONNREFUSED)
    ConnectionRefused,
    /// Peer closed their end (EPIPE)
    BrokenPipe,
    /// Would block (EAGAIN/EWOULDBLOCK)
    WouldBlock,
    /// Invalid argument (EINVAL)
    InvalidArg,
    /// No buffer space (ENOBUFS)
    NoBufferSpace,
    /// Address not found (ECONNREFUSED)
    AddrNotFound,
    /// Message too long for dgram (EMSGSIZE)
    MsgTooLong,
    /// Internal error — should never happen
    Internal,
}

/// Global registry of bound Unix sockets.
/// — ShadePacket: The phone book. bind() adds entries, close()/unlink() removes them.
/// connect() looks up the target here. Path-based sockets also get a VFS entry,
/// but this registry is the source of truth for finding the actual socket object.
pub static UNIX_SOCKET_REGISTRY: Mutex<BTreeMap<UnixAddr, Arc<UnixSocket>>> =
    Mutex::new(BTreeMap::new());

/// Register a socket in the global namespace.
/// — ShadePacket: Must be called AFTER bind() succeeds on the socket itself.
pub fn register_socket(addr: UnixAddr, socket: Arc<UnixSocket>) -> Result<(), UnixError> {
    let mut registry = UNIX_SOCKET_REGISTRY.lock();
    if registry.contains_key(&addr) {
        return Err(UnixError::AddrInUse);
    }
    registry.insert(addr, socket);
    Ok(())
}

/// Look up a socket by address.
pub fn lookup_socket(addr: &UnixAddr) -> Option<Arc<UnixSocket>> {
    UNIX_SOCKET_REGISTRY.lock().get(addr).cloned()
}

/// Remove a socket from the global namespace.
pub fn unregister_socket(addr: &UnixAddr) -> Option<Arc<UnixSocket>> {
    UNIX_SOCKET_REGISTRY.lock().remove(addr)
}
