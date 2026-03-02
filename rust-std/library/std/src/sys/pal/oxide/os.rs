//! — NeonRoot: OS-level operations for std — getcwd, chdir, getpid, exit.
//! Existential questions answered through syscalls.

use crate::error::Error as StdError;
use crate::ffi::{OsStr, OsString};
use crate::marker::PhantomData;
use crate::os::oxide::ffi::OsStrExt;
use crate::path::{self, PathBuf};
use crate::{fmt, io};

pub fn getcwd() -> io::Result<PathBuf> {
    let mut buf = [0u8; 4096];
    let ret = oxide_rt::os::getcwd(&mut buf);
    if ret < 0 {
        Err(io::Error::from_raw_os_error(-ret as i32))
    } else {
        let path = core::str::from_utf8(&buf[..ret as usize])
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        Ok(PathBuf::from(path))
    }
}

pub fn chdir(path: &path::Path) -> io::Result<()> {
    let path_bytes = path.as_os_str().as_bytes();
    let ret = oxide_rt::os::chdir(path_bytes);
    if ret < 0 {
        Err(io::Error::from_raw_os_error(-ret))
    } else {
        Ok(())
    }
}

pub struct SplitPaths<'a>(!, PhantomData<&'a ()>);

pub fn split_paths(_unparsed: &OsStr) -> SplitPaths<'_> {
    panic!("unsupported")
}

impl<'a> Iterator for SplitPaths<'a> {
    type Item = PathBuf;
    fn next(&mut self) -> Option<PathBuf> {
        self.0
    }
}

#[derive(Debug)]
pub struct JoinPathsError;

pub fn join_paths<I, T>(_paths: I) -> Result<OsString, JoinPathsError>
where
    I: Iterator<Item = T>,
    T: AsRef<OsStr>,
{
    Err(JoinPathsError)
}

impl fmt::Display for JoinPathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "not supported on this platform yet".fmt(f)
    }
}

impl StdError for JoinPathsError {
    #[allow(deprecated)]
    fn description(&self) -> &str {
        "not supported on this platform yet"
    }
}

pub fn current_exe() -> io::Result<PathBuf> {
    // — NeonRoot: Not yet implemented, return a placeholder
    Err(io::Error::UNSUPPORTED_PLATFORM)
}

pub fn temp_dir() -> PathBuf {
    PathBuf::from("/tmp")
}

pub fn home_dir() -> Option<PathBuf> {
    oxide_rt::env::getenv_bytes(b"HOME").map(|h| {
        let s = core::str::from_utf8(h).unwrap_or("/root");
        PathBuf::from(s)
    })
}

pub fn exit(code: i32) -> ! {
    oxide_rt::os::exit(code)
}

pub fn getpid() -> u32 {
    oxide_rt::os::getpid() as u32
}
