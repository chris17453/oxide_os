//! — SableWire: Pipe implementation for std.
use crate::io;
use crate::sys::fd::FileDesc;
use crate::os::fd::FromRawFd;

pub type Pipe = FileDesc;

#[inline]
pub fn pipe() -> io::Result<(Pipe, Pipe)> {
    let mut fds = [0i32; 2];
    let ret = oxide_rt::pipe::pipe(&mut fds);
    if ret < 0 {
        Err(io::Error::from_raw_os_error(-ret))
    } else {
        unsafe {
            Ok((Pipe::from_raw_fd(fds[0]), Pipe::from_raw_fd(fds[1])))
        }
    }
}
