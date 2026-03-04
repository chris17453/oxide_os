//! — IronGhost: libc compatibility shim for std's fd/io modules.
//! std's os/fd code uses `libc::STDIN_FILENO`, `libc::close()` etc.
//! On oxide, we provide these from libc_compat and our own syscall wrappers.

pub use crate::libc_compat::*;

/// close — wraps our io::close syscall
pub fn close(fd: i32) -> i32 {
    crate::io::close(fd)
}
