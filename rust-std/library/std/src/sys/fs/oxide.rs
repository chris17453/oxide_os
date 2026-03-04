//! — TorqueJax: Filesystem operations for std::fs — open, stat, readdir, etc.
//! OXIDE uses (ptr, len) paths and POSIX-style syscalls.

use crate::ffi::OsString;
use crate::hash::Hash;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut, SeekFrom};
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, RawFd};
use crate::path::{Path, PathBuf};
use crate::sys::fd::FileDesc;
pub use crate::sys::fs::common::{Dir, exists};
use crate::sys::time::SystemTime;
use crate::sys::{AsInner, AsInnerMut, FromInner, IntoInner};
use crate::sys::pal::{cvt, unsupported};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileType {
    mode: u32,
}

impl FileType {
    pub fn is_dir(&self) -> bool { (self.mode & 0o170000) == 0o040000 }
    pub fn is_file(&self) -> bool { (self.mode & 0o170000) == 0o100000 }
    pub fn is_symlink(&self) -> bool { (self.mode & 0o170000) == 0o120000 }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FilePermissions {
    mode: u32,
}

impl FilePermissions {
    pub fn readonly(&self) -> bool { (self.mode & 0o222) == 0 }
    pub fn set_readonly(&mut self, readonly: bool) {
        if readonly { self.mode &= !0o222; } else { self.mode |= 0o222; }
    }
    pub fn mode(&self) -> u32 { self.mode }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FileTimes {}

impl FileTimes {
    pub fn set_accessed(&mut self, _t: SystemTime) {}
    pub fn set_modified(&mut self, _t: SystemTime) {}
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct FileAttr {
    stat: oxide_rt::types::Stat,
}

impl FileAttr {
    pub fn size(&self) -> u64 { self.stat.size }
    pub fn perm(&self) -> FilePermissions { FilePermissions { mode: self.stat.mode & 0o777 } }
    pub fn file_type(&self) -> FileType { FileType { mode: self.stat.mode } }
    pub fn modified(&self) -> io::Result<SystemTime> {
        Ok(SystemTime { secs: self.stat.mtime, nanos: 0 })
    }
    pub fn accessed(&self) -> io::Result<SystemTime> {
        Ok(SystemTime { secs: self.stat.atime, nanos: 0 })
    }
    pub fn created(&self) -> io::Result<SystemTime> {
        Ok(SystemTime { secs: self.stat.ctime, nanos: 0 })
    }
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    mode: u32,
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions { read: false, write: false, append: false, truncate: false, create: false, create_new: false, mode: 0o666 }
    }
    pub fn read(&mut self, read: bool) { self.read = read; }
    pub fn write(&mut self, write: bool) { self.write = write; }
    pub fn append(&mut self, append: bool) { self.append = append; }
    pub fn truncate(&mut self, truncate: bool) { self.truncate = truncate; }
    pub fn create(&mut self, create: bool) { self.create = create; }
    pub fn create_new(&mut self, create_new: bool) { self.create_new = create_new; }
    pub fn custom_flags(&mut self, _flags: i32) {}
    pub fn mode(&mut self, mode: u32) { self.mode = mode; }

    fn to_flags(&self) -> i32 {
        let mut flags = if self.read && self.write { 2 } // O_RDWR
            else if self.write { 1 } // O_WRONLY
            else { 0 }; // O_RDONLY
        if self.create { flags |= 0o100; }    // O_CREAT
        if self.create_new { flags |= 0o300; } // O_CREAT | O_EXCL
        if self.truncate { flags |= 0o1000; }  // O_TRUNC
        if self.append { flags |= 0o2000; }    // O_APPEND
        flags
    }
}

#[derive(Debug)]
pub struct File(FileDesc);

impl File {
    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
        let path_bytes = path.as_os_str().as_encoded_bytes();
        let fd = oxide_rt::fs::open(path_bytes, opts.to_flags(), opts.mode);
        if fd < 0 { Err(io::Error::from_raw_os_error(-fd)) }
        else { Ok(File(unsafe { FileDesc::from_raw_fd(fd) })) }
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        let mut stat = oxide_rt::types::Stat::zeroed();
        let ret = oxide_rt::fs::fstat(self.0.as_raw_fd(), &mut stat);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
        else { Ok(FileAttr { stat }) }
    }

    pub fn fsync(&self) -> io::Result<()> {
        let ret = oxide_rt::io::fsync(self.0.as_raw_fd());
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
        else { Ok(()) }
    }

    pub fn datasync(&self) -> io::Result<()> {
        let ret = oxide_rt::io::fdatasync(self.0.as_raw_fd());
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
        else { Ok(()) }
    }

    pub fn truncate(&self, size: u64) -> io::Result<()> {
        let ret = oxide_rt::fs::ftruncate(self.0.as_raw_fd(), size as i64);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
        else { Ok(()) }
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> { self.0.read(buf) }
    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> { self.0.read_vectored(bufs) }
    pub fn is_read_vectored(&self) -> bool { false }
    pub fn read_buf(&self, cursor: BorrowedCursor<'_>) -> io::Result<()> { self.0.read_buf(cursor) }
    pub fn write(&self, buf: &[u8]) -> io::Result<usize> { self.0.write(buf) }
    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> { self.0.write_vectored(bufs) }
    pub fn is_write_vectored(&self) -> bool { false }
    pub fn flush(&self) -> io::Result<()> { Ok(()) }

    pub fn seek(&self, pos: SeekFrom) -> io::Result<u64> {
        let (offset, whence) = match pos {
            SeekFrom::Start(off) => (off as i64, 0),
            SeekFrom::Current(off) => (off, 1),
            SeekFrom::End(off) => (off, 2),
        };
        let ret = oxide_rt::fs::lseek(self.0.as_raw_fd(), offset, whence);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret as i32)) }
        else { Ok(ret as u64) }
    }

    pub fn tell(&self) -> io::Result<u64> { self.seek(SeekFrom::Current(0)) }
    pub fn duplicate(&self) -> io::Result<File> { Ok(File(self.0.duplicate()?)) }
    pub fn set_permissions(&self, perm: FilePermissions) -> io::Result<()> {
        let ret = oxide_rt::fs::fchmod(self.0.as_raw_fd(), perm.mode);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
        else { Ok(()) }
    }
    pub fn set_times(&self, _times: FileTimes) -> io::Result<()> { unsupported() }
    pub fn lock(&self) -> io::Result<()> { unsupported() }
    pub fn lock_shared(&self) -> io::Result<()> { unsupported() }
    pub fn try_lock(&self) -> Result<(), crate::fs::TryLockError> { Err(crate::fs::TryLockError::Error(io::Error::UNSUPPORTED_PLATFORM)) }
    pub fn try_lock_shared(&self) -> Result<(), crate::fs::TryLockError> { Err(crate::fs::TryLockError::Error(io::Error::UNSUPPORTED_PLATFORM)) }
    pub fn unlock(&self) -> io::Result<()> { unsupported() }
    pub fn size(&self) -> Option<io::Result<u64>> { None }
}

impl AsInner<FileDesc> for File {
    fn as_inner(&self) -> &FileDesc { &self.0 }
}
impl AsInnerMut<FileDesc> for File {
    fn as_inner_mut(&mut self) -> &mut FileDesc { &mut self.0 }
}
impl IntoInner<FileDesc> for File {
    fn into_inner(self) -> FileDesc { self.0 }
}
impl FromInner<FileDesc> for File {
    fn from_inner(fd: FileDesc) -> Self { File(fd) }
}
impl AsFd for File {
    fn as_fd(&self) -> BorrowedFd<'_> { self.0.as_fd() }
}
impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd { self.0.as_raw_fd() }
}
impl IntoRawFd for File {
    fn into_raw_fd(self) -> RawFd { self.0.into_raw_fd() }
}
impl FromRawFd for File {
    unsafe fn from_raw_fd(fd: RawFd) -> Self { Self(unsafe { FileDesc::from_raw_fd(fd) }) }
}

#[derive(Debug)]
pub struct DirBuilder {}

impl DirBuilder {
    pub fn new() -> DirBuilder { DirBuilder {} }
    pub fn mkdir(&self, path: &Path) -> io::Result<()> {
        let path_bytes = path.as_os_str().as_encoded_bytes();
        let ret = oxide_rt::fs::mkdir(path_bytes, 0o755);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
        else { Ok(()) }
    }
}

pub fn unlink(path: &Path) -> io::Result<()> {
    let p = path.as_os_str().as_encoded_bytes();
    let ret = oxide_rt::fs::unlink(p);
    if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
    else { Ok(()) }
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    let o = old.as_os_str().as_encoded_bytes();
    let n = new.as_os_str().as_encoded_bytes();
    let ret = oxide_rt::fs::rename(o, n);
    if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
    else { Ok(()) }
}

pub fn rmdir(path: &Path) -> io::Result<()> {
    let p = path.as_os_str().as_encoded_bytes();
    let ret = oxide_rt::fs::rmdir(p);
    if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
    else { Ok(()) }
}

pub fn remove_dir_all(path: &Path) -> io::Result<()> {
    for entry in readdir(path)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            remove_dir_all(&entry.path())?;
        } else {
            unlink(&entry.path())?;
        }
    }
    rmdir(path)
}

pub fn set_perm(path: &Path, perm: FilePermissions) -> io::Result<()> {
    let p = path.as_os_str().as_encoded_bytes();
    let ret = oxide_rt::fs::chmod(p, perm.mode);
    if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
    else { Ok(()) }
}

pub fn set_times(_p: &Path, _times: FileTimes) -> io::Result<()> { unsupported() }
pub fn set_times_nofollow(_p: &Path, _times: FileTimes) -> io::Result<()> { unsupported() }

pub fn readlink(path: &Path) -> io::Result<PathBuf> {
    let p = path.as_os_str().as_encoded_bytes();
    let mut buf = [0u8; 4096];
    let ret = oxide_rt::fs::readlink(p, &mut buf);
    if ret < 0 { Err(io::Error::from_raw_os_error(-ret as i32)) }
    else {
        let s = core::str::from_utf8(&buf[..ret as usize])
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        Ok(PathBuf::from(s))
    }
}

pub fn symlink(original: &Path, link: &Path) -> io::Result<()> {
    let o = original.as_os_str().as_encoded_bytes();
    let l = link.as_os_str().as_encoded_bytes();
    let ret = oxide_rt::fs::symlink(o, l);
    if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
    else { Ok(()) }
}

pub fn link(src: &Path, dst: &Path) -> io::Result<()> {
    let s = src.as_os_str().as_encoded_bytes();
    let d = dst.as_os_str().as_encoded_bytes();
    let ret = oxide_rt::fs::link(s, d);
    if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
    else { Ok(()) }
}

pub fn stat(path: &Path) -> io::Result<FileAttr> {
    let p = path.as_os_str().as_encoded_bytes();
    let mut s = oxide_rt::types::Stat::zeroed();
    let ret = oxide_rt::fs::stat(p, &mut s);
    if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
    else { Ok(FileAttr { stat: s }) }
}

pub fn lstat(path: &Path) -> io::Result<FileAttr> {
    let p = path.as_os_str().as_encoded_bytes();
    let mut s = oxide_rt::types::Stat::zeroed();
    let ret = oxide_rt::fs::lstat(p, &mut s);
    if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
    else { Ok(FileAttr { stat: s }) }
}

pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    // — TorqueJax: Minimal canonicalization — just resolve to absolute path
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let mut cwd = crate::sys::os::getcwd()?;
        cwd.push(path);
        Ok(cwd)
    }
}

pub fn copy(from: &Path, to: &Path) -> io::Result<u64> {
    let mut read_opts = OpenOptions::new();
    read_opts.read(true);
    let mut reader = File::open(from, &read_opts)?;
    let mut write_opts = OpenOptions::new();
    write_opts.write(true);
    write_opts.create(true);
    write_opts.truncate(true);
    let mut writer = File::open(to, &write_opts)?;
    let mut buf = [0u8; 8192];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        writer.write(&buf[..n])?;
        total += n as u64;
    }
    Ok(total)
}

#[derive(Debug)]
pub struct ReadDir {
    fd: i32,
    path: PathBuf,
    buf: [u8; 4096],
    pos: usize,
    len: usize,
    done: bool,
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.done { return None; }

            if self.pos >= self.len {
                let ret = oxide_rt::fs::getdents(self.fd, &mut self.buf);
                if ret <= 0 {
                    self.done = true;
                    if ret < 0 { return Some(Err(io::Error::from_raw_os_error(-ret))); }
                    return None;
                }
                self.len = ret as usize;
                self.pos = 0;
            }

            // Parse dirent from buffer
            if self.pos + 19 > self.len {
                self.done = true;
                return None;
            }

            let buf = &self.buf[self.pos..self.len];
            let d_reclen = u16::from_le_bytes([buf[16], buf[17]]) as usize;
            if d_reclen == 0 || self.pos + d_reclen > self.len {
                self.done = true;
                return None;
            }

            let d_type = buf[18];
            let name_bytes = &buf[19..d_reclen];
            let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
            let name = &name_bytes[..name_end];

            self.pos += d_reclen;

            // Skip . and ..
            if name == b"." || name == b".." { continue; }

            let name_str = String::from_utf8_lossy(name).into_owned();
            let mut entry_path = self.path.clone();
            entry_path.push(&name_str);

            return Some(Ok(DirEntry {
                path: entry_path,
                name: OsString::from(name_str),
                file_type: d_type,
            }));
        }
    }
}

pub fn readdir(path: &Path) -> io::Result<ReadDir> {
    let path_bytes = path.as_os_str().as_encoded_bytes();
    let fd = oxide_rt::fs::open(path_bytes, 0o200000, 0); // O_DIRECTORY | O_RDONLY
    if fd < 0 {
        return Err(io::Error::from_raw_os_error(-fd));
    }
    Ok(ReadDir {
        fd,
        path: path.to_path_buf(),
        buf: [0u8; 4096],
        pos: 0,
        len: 0,
        done: false,
    })
}

impl Drop for ReadDir {
    fn drop(&mut self) {
        oxide_rt::io::close(self.fd);
    }
}

#[derive(Debug)]
pub struct DirEntry {
    path: PathBuf,
    name: OsString,
    file_type: u8,
}

impl DirEntry {
    pub fn path(&self) -> PathBuf { self.path.clone() }
    pub fn file_name(&self) -> OsString { self.name.clone() }
    pub fn metadata(&self) -> io::Result<FileAttr> { stat(&self.path) }
    pub fn file_type(&self) -> io::Result<FileType> {
        let mode = match self.file_type {
            4 => 0o040000,   // DT_DIR
            8 => 0o100000,   // DT_REG
            10 => 0o120000,  // DT_LNK
            2 => 0o020000,   // DT_CHR
            6 => 0o060000,   // DT_BLK
            1 => 0o010000,   // DT_FIFO
            12 => 0o140000,  // DT_SOCK
            _ => 0,
        };
        Ok(FileType { mode })
    }
}
