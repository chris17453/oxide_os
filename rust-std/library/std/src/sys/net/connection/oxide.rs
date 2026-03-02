//! — ShadePacket: TCP/UDP socket implementation for std::net.
//! Stubbed for initial bring-up. Full implementation coming later.

use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::net::{Shutdown, SocketAddr, ToSocketAddrs};
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, RawFd};
use crate::sys::fd::FileDesc;
use crate::sys::{AsInner, FromInner, IntoInner};
use crate::time::Duration;

#[derive(Debug)]
pub struct Socket(FileDesc);

#[derive(Debug)]
pub struct TcpStream { inner: Socket }

impl TcpStream {
    pub fn socket(&self) -> &Socket { &self.inner }
    pub fn into_socket(self) -> Socket { self.inner }
    pub fn connect<A: ToSocketAddrs>(_addr: A) -> io::Result<TcpStream> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn connect_timeout(_addr: &SocketAddr, _timeout: Duration) -> io::Result<TcpStream> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn set_read_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> { Ok(()) }
    pub fn set_write_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> { Ok(()) }
    pub fn read_timeout(&self) -> io::Result<Option<Duration>> { Ok(None) }
    pub fn write_timeout(&self) -> io::Result<Option<Duration>> { Ok(None) }
    pub fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> { self.inner.0.read(buf) }
    pub fn read_buf(&self, cursor: BorrowedCursor<'_>) -> io::Result<()> { self.inner.0.read_buf(cursor) }
    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> { self.inner.0.read_vectored(bufs) }
    pub fn is_read_vectored(&self) -> bool { false }
    pub fn write(&self, buf: &[u8]) -> io::Result<usize> { self.inner.0.write(buf) }
    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> { self.inner.0.write_vectored(bufs) }
    pub fn is_write_vectored(&self) -> bool { false }
    pub fn peer_addr(&self) -> io::Result<SocketAddr> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn socket_addr(&self) -> io::Result<SocketAddr> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn shutdown(&self, _shutdown: Shutdown) -> io::Result<()> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn duplicate(&self) -> io::Result<TcpStream> { Ok(TcpStream { inner: Socket(self.inner.0.duplicate()?) }) }
    pub fn set_linger(&self, _timeout: Option<Duration>) -> io::Result<()> { Ok(()) }
    pub fn linger(&self) -> io::Result<Option<Duration>> { Ok(None) }
    pub fn set_nodelay(&self, _nodelay: bool) -> io::Result<()> { Ok(()) }
    pub fn nodelay(&self) -> io::Result<bool> { Ok(false) }
    pub fn set_ttl(&self, _ttl: u32) -> io::Result<()> { Ok(()) }
    pub fn ttl(&self) -> io::Result<u32> { Ok(64) }
    pub fn take_error(&self) -> io::Result<Option<io::Error>> { Ok(None) }
    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> { Ok(()) }
}

#[derive(Debug)]
pub struct TcpListener { inner: Socket }

impl TcpListener {
    pub fn socket(&self) -> &Socket { &self.inner }
    pub fn into_socket(self) -> Socket { self.inner }
    pub fn bind<A: ToSocketAddrs>(_addr: A) -> io::Result<TcpListener> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn socket_addr(&self) -> io::Result<SocketAddr> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn duplicate(&self) -> io::Result<TcpListener> { Ok(TcpListener { inner: Socket(self.inner.0.duplicate()?) }) }
    pub fn set_ttl(&self, _ttl: u32) -> io::Result<()> { Ok(()) }
    pub fn ttl(&self) -> io::Result<u32> { Ok(64) }
    pub fn set_only_v6(&self, _only_v6: bool) -> io::Result<()> { Ok(()) }
    pub fn only_v6(&self) -> io::Result<bool> { Ok(false) }
    pub fn take_error(&self) -> io::Result<Option<io::Error>> { Ok(None) }
    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> { Ok(()) }
}

#[derive(Debug)]
pub struct UdpSocket { inner: Socket }

impl UdpSocket {
    pub fn socket(&self) -> &Socket { &self.inner }
    pub fn into_socket(self) -> Socket { self.inner }
    pub fn bind<A: ToSocketAddrs>(_addr: A) -> io::Result<UdpSocket> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn peer_addr(&self) -> io::Result<SocketAddr> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn socket_addr(&self) -> io::Result<SocketAddr> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn peek_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn send_to<A: ToSocketAddrs>(&self, _buf: &[u8], _addr: A) -> io::Result<usize> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn duplicate(&self) -> io::Result<UdpSocket> { Ok(UdpSocket { inner: Socket(self.inner.0.duplicate()?) }) }
    pub fn set_read_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> { Ok(()) }
    pub fn set_write_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> { Ok(()) }
    pub fn read_timeout(&self) -> io::Result<Option<Duration>> { Ok(None) }
    pub fn write_timeout(&self) -> io::Result<Option<Duration>> { Ok(None) }
    pub fn set_broadcast(&self, _broadcast: bool) -> io::Result<()> { Ok(()) }
    pub fn broadcast(&self) -> io::Result<bool> { Ok(false) }
    pub fn set_multicast_loop_v4(&self, _multicast_loop_v4: bool) -> io::Result<()> { Ok(()) }
    pub fn multicast_loop_v4(&self) -> io::Result<bool> { Ok(false) }
    pub fn set_multicast_ttl_v4(&self, _multicast_ttl_v4: u32) -> io::Result<()> { Ok(()) }
    pub fn multicast_ttl_v4(&self) -> io::Result<u32> { Ok(1) }
    pub fn set_multicast_loop_v6(&self, _multicast_loop_v6: bool) -> io::Result<()> { Ok(()) }
    pub fn multicast_loop_v6(&self) -> io::Result<bool> { Ok(false) }
    pub fn join_multicast_v4(&self, _multiaddr: &crate::net::Ipv4Addr, _interface: &crate::net::Ipv4Addr) -> io::Result<()> { Ok(()) }
    pub fn join_multicast_v6(&self, _multiaddr: &crate::net::Ipv6Addr, _interface: u32) -> io::Result<()> { Ok(()) }
    pub fn leave_multicast_v4(&self, _multiaddr: &crate::net::Ipv4Addr, _interface: &crate::net::Ipv4Addr) -> io::Result<()> { Ok(()) }
    pub fn leave_multicast_v6(&self, _multiaddr: &crate::net::Ipv6Addr, _interface: u32) -> io::Result<()> { Ok(()) }
    pub fn set_ttl(&self, _ttl: u32) -> io::Result<()> { Ok(()) }
    pub fn ttl(&self) -> io::Result<u32> { Ok(64) }
    pub fn take_error(&self) -> io::Result<Option<io::Error>> { Ok(None) }
    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> { Ok(()) }
    pub fn recv(&self, _buf: &mut [u8]) -> io::Result<usize> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn send(&self, _buf: &[u8]) -> io::Result<usize> { Err(io::Error::UNSUPPORTED_PLATFORM) }
    pub fn connect<A: ToSocketAddrs>(&self, _addr: A) -> io::Result<()> { Err(io::Error::UNSUPPORTED_PLATFORM) }
}

pub struct LookupHost { addresses: alloc::vec::Vec<SocketAddr> }

pub fn lookup_host(_host: &str, _port: u16) -> io::Result<LookupHost> {
    Err(io::Error::UNSUPPORTED_PLATFORM)
}

impl Iterator for LookupHost {
    type Item = SocketAddr;
    fn next(&mut self) -> Option<SocketAddr> { self.addresses.pop() }
}

impl TryFrom<&str> for LookupHost {
    type Error = io::Error;
    fn try_from(_v: &str) -> io::Result<LookupHost> { Err(io::Error::UNSUPPORTED_PLATFORM) }
}

impl<'a> TryFrom<(&'a str, u16)> for LookupHost {
    type Error = io::Error;
    fn try_from(_v: (&'a str, u16)) -> io::Result<LookupHost> { Err(io::Error::UNSUPPORTED_PLATFORM) }
}

impl AsInner<FileDesc> for Socket { fn as_inner(&self) -> &FileDesc { &self.0 } }
impl IntoInner<FileDesc> for Socket { fn into_inner(self) -> FileDesc { self.0 } }
impl FromInner<FileDesc> for Socket { fn from_inner(fd: FileDesc) -> Self { Socket(fd) } }
impl AsFd for Socket { fn as_fd(&self) -> BorrowedFd<'_> { self.0.as_fd() } }
impl AsRawFd for Socket { fn as_raw_fd(&self) -> RawFd { self.0.as_raw_fd() } }
impl IntoRawFd for Socket { fn into_raw_fd(self) -> RawFd { self.0.into_raw_fd() } }
impl FromRawFd for Socket { unsafe fn from_raw_fd(fd: RawFd) -> Self { Socket(unsafe { FileDesc::from_raw_fd(fd) }) } }

impl AsInner<Socket> for TcpStream { fn as_inner(&self) -> &Socket { &self.inner } }
impl FromInner<Socket> for TcpStream { fn from_inner(s: Socket) -> Self { TcpStream { inner: s } } }
impl AsInner<Socket> for TcpListener { fn as_inner(&self) -> &Socket { &self.inner } }
impl FromInner<Socket> for TcpListener { fn from_inner(s: Socket) -> Self { TcpListener { inner: s } } }
impl AsInner<Socket> for UdpSocket { fn as_inner(&self) -> &Socket { &self.inner } }
impl FromInner<Socket> for UdpSocket { fn from_inner(s: Socket) -> Self { UdpSocket { inner: s } } }
