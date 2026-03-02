//! — SableWire: File descriptor wrapper for OXIDE OS.
#![unstable(reason = "not public", issue = "none", feature = "fd")]

use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut, Read};
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::sys::{AsInner, FromInner, IntoInner};
use crate::sys::pal::cvt;

#[derive(Debug)]
pub struct FileDesc(OwnedFd);

impl FileDesc {
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let ret = oxide_rt::io::read(self.as_raw_fd(), buf);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret as i32)) }
        else { Ok(ret as usize) }
    }

    pub fn read_buf(&self, cursor: BorrowedCursor<'_>) -> io::Result<()> {
        crate::io::default_read_buf(|buf| self.read(buf), cursor)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        io::default_read_vectored(|b| self.read(b), bufs)
    }

    pub fn read_to_end(&self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let mut me = self;
        (&mut me).read_to_end(buf)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let ret = oxide_rt::io::write(self.as_raw_fd(), buf);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret as i32)) }
        else { Ok(ret as usize) }
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        crate::io::default_write_vectored(|b| self.write(b), bufs)
    }

    pub fn is_write_vectored(&self) -> bool { false }

    #[inline]
    pub fn is_read_vectored(&self) -> bool { false }

    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        // — SableWire: TODO implement via fcntl
        Ok(())
    }

    #[inline]
    pub fn duplicate(&self) -> io::Result<FileDesc> {
        let new_fd = oxide_rt::io::dup(self.as_raw_fd());
        if new_fd < 0 {
            Err(io::Error::from_raw_os_error(-new_fd))
        } else {
            unsafe { Ok(Self::from_raw_fd(new_fd)) }
        }
    }

    #[inline]
    pub fn try_clone(&self) -> io::Result<Self> {
        self.duplicate()
    }
}

impl<'a> Read for &'a FileDesc {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> { (**self).read(buf) }
    fn read_buf(&mut self, cursor: BorrowedCursor<'_>) -> io::Result<()> { (**self).read_buf(cursor) }
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> { (**self).read_vectored(bufs) }
    #[inline]
    fn is_read_vectored(&self) -> bool { (**self).is_read_vectored() }
}

impl AsInner<OwnedFd> for FileDesc {
    #[inline]
    fn as_inner(&self) -> &OwnedFd { &self.0 }
}

impl IntoInner<OwnedFd> for FileDesc {
    fn into_inner(self) -> OwnedFd { self.0 }
}

impl FromInner<OwnedFd> for FileDesc {
    fn from_inner(owned_fd: OwnedFd) -> Self { Self(owned_fd) }
}

impl AsFd for FileDesc {
    fn as_fd(&self) -> BorrowedFd<'_> { self.0.as_fd() }
}

impl AsRawFd for FileDesc {
    #[inline]
    fn as_raw_fd(&self) -> RawFd { self.0.as_raw_fd() }
}

impl IntoRawFd for FileDesc {
    fn into_raw_fd(self) -> RawFd { self.0.into_raw_fd() }
}

impl FromRawFd for FileDesc {
    unsafe fn from_raw_fd(raw_fd: RawFd) -> Self {
        unsafe { Self(FromRawFd::from_raw_fd(raw_fd)) }
    }
}
