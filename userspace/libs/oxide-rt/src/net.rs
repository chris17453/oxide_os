//! Network syscall wrappers — sockets, connections, the whole stack.
//!
//! — ShadePacket: TCP, UDP, Unix sockets — all the ways bytes can
//! travel between processes. Each syscall is a thin wrapper because
//! the kernel does the heavy lifting. We just pass the args through.

use crate::syscall::*;
use crate::nr;

/// socket — create a network socket
pub fn socket(domain: i32, sock_type: i32, protocol: i32) -> i32 {
    syscall3(nr::SOCKET, domain as usize, sock_type as usize, protocol as usize) as i32
}

/// bind — bind a socket to an address
pub fn bind(fd: i32, addr: *const u8, addrlen: u32) -> i32 {
    syscall3(nr::BIND, fd as usize, addr as usize, addrlen as usize) as i32
}

/// listen — mark socket as passive (accepting connections)
pub fn listen(fd: i32, backlog: i32) -> i32 {
    syscall2(nr::LISTEN, fd as usize, backlog as usize) as i32
}

/// accept — accept a connection on a socket
pub fn accept(fd: i32, addr: *mut u8, addrlen: *mut u32) -> i32 {
    syscall3(nr::ACCEPT, fd as usize, addr as usize, addrlen as usize) as i32
}

/// accept4 — accept with flags (SOCK_NONBLOCK, SOCK_CLOEXEC)
pub fn accept4(fd: i32, addr: *mut u8, addrlen: *mut u32, flags: i32) -> i32 {
    syscall4(nr::ACCEPT4, fd as usize, addr as usize, addrlen as usize, flags as usize) as i32
}

/// connect — connect to a remote address
pub fn connect(fd: i32, addr: *const u8, addrlen: u32) -> i32 {
    syscall3(nr::CONNECT, fd as usize, addr as usize, addrlen as usize) as i32
}

/// send — send data on a connected socket
pub fn send(fd: i32, buf: &[u8], flags: i32) -> isize {
    syscall4(nr::SEND, fd as usize, buf.as_ptr() as usize, buf.len(), flags as usize) as isize
}

/// recv — receive data from a connected socket
pub fn recv(fd: i32, buf: &mut [u8], flags: i32) -> isize {
    syscall4(nr::RECV, fd as usize, buf.as_mut_ptr() as usize, buf.len(), flags as usize) as isize
}

/// sendto — send data to a specific address
pub fn sendto(fd: i32, buf: &[u8], flags: i32, addr: *const u8, addrlen: u32) -> isize {
    syscall6(
        nr::SENDTO,
        fd as usize,
        buf.as_ptr() as usize,
        buf.len(),
        flags as usize,
        addr as usize,
        addrlen as usize,
    ) as isize
}

/// recvfrom — receive data with source address
pub fn recvfrom(
    fd: i32,
    buf: &mut [u8],
    flags: i32,
    addr: *mut u8,
    addrlen: *mut u32,
) -> isize {
    syscall6(
        nr::RECVFROM,
        fd as usize,
        buf.as_mut_ptr() as usize,
        buf.len(),
        flags as usize,
        addr as usize,
        addrlen as usize,
    ) as isize
}

/// setsockopt — set socket option
pub fn setsockopt(fd: i32, level: i32, optname: i32, optval: *const u8, optlen: u32) -> i32 {
    syscall5(
        nr::SETSOCKOPT,
        fd as usize,
        level as usize,
        optname as usize,
        optval as usize,
        optlen as usize,
    ) as i32
}

/// getsockopt — get socket option
pub fn getsockopt(fd: i32, level: i32, optname: i32, optval: *mut u8, optlen: *mut u32) -> i32 {
    syscall5(
        nr::GETSOCKOPT,
        fd as usize,
        level as usize,
        optname as usize,
        optval as usize,
        optlen as usize,
    ) as i32
}

/// getsockname — get local socket address
pub fn getsockname(fd: i32, addr: *mut u8, addrlen: *mut u32) -> i32 {
    syscall3(nr::GETSOCKNAME, fd as usize, addr as usize, addrlen as usize) as i32
}

/// getpeername — get remote socket address
pub fn getpeername(fd: i32, addr: *mut u8, addrlen: *mut u32) -> i32 {
    syscall3(nr::GETPEERNAME, fd as usize, addr as usize, addrlen as usize) as i32
}

/// shutdown — shut down part of a full-duplex connection
pub fn shutdown(fd: i32, how: i32) -> i32 {
    syscall2(nr::SHUTDOWN, fd as usize, how as usize) as i32
}
