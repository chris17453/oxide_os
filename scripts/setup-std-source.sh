#!/usr/bin/env bash
# — PulseForge: Copies Rust std source, creates sysroot symlinks, and applies OXIDE patches.
# Idempotent — safe to re-run after rustup update nightly.
# Usage: ./scripts/setup-std-source.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STD_DST="$REPO_ROOT/rust-std/library"
SYSROOT_DIR="$REPO_ROOT/target/oxide-sysroot"
NIGHTLY_SYSROOT="$(rustc +nightly --print sysroot)"
STD_SRC="$NIGHTLY_SYSROOT/lib/rustlib/src/rust/library"

echo "=== OXIDE std Source Setup ==="
echo "Nightly sysroot: $NIGHTLY_SYSROOT"
echo "std source:      $STD_SRC"
echo "Destination:     $STD_DST"
echo ""

# ── Step 1: Verify rust-src is installed ──
if [ ! -d "$STD_SRC" ]; then
    echo "ERROR: rust-src not installed. Run: rustup +nightly component add rust-src"
    exit 1
fi

# ── Step 2: Copy std library source ──
echo "Copying std library source..."
rm -rf "$STD_DST"
mkdir -p "$STD_DST"
cp -a "$STD_SRC"/* "$STD_DST"/
echo "  Copied $(du -sh "$STD_DST" | cut -f1) of library source"

# ── Step 3: Create custom sysroot ──
echo "Creating custom sysroot..."
rm -rf "$SYSROOT_DIR"
mkdir -p "$SYSROOT_DIR/lib/rustlib/src/rust"

# Symlink the toolchain binaries and host target libraries
ln -sf "$NIGHTLY_SYSROOT/bin" "$SYSROOT_DIR/bin"
for dir in "$NIGHTLY_SYSROOT/lib/rustlib"/*; do
    base="$(basename "$dir")"
    if [ "$base" != "src" ]; then
        ln -sf "$dir" "$SYSROOT_DIR/lib/rustlib/$base"
    fi
done

# Point source to our patched copy
ln -sf "$STD_DST" "$SYSROOT_DIR/lib/rustlib/src/rust/library"
echo "  Sysroot created at $SYSROOT_DIR"

# ── Step 4: Add oxide-rt dependency to std's Cargo.toml ──
# — PulseForge: target-gated dependency, same pattern motor uses.
# This means oxide-rt is automatically linked when target_os = "oxide" — no feature flag needed.
echo "Patching std/Cargo.toml..."
STD_CARGO="$STD_DST/std/Cargo.toml"
if ! grep -q 'oxide-rt' "$STD_CARGO"; then
    # Append oxide-rt target-gated dependency section to Cargo.toml
    cat >> "$STD_CARGO" <<TOML

[target.'cfg(target_os = "oxide")'.dependencies]
oxide-rt = { path = "$REPO_ROOT/userspace/libs/oxide-rt", features = ["rustc-dep-of-std"], public = true }
TOML
    echo "  Added oxide-rt target-gated dependency"
else
    echo "  oxide-rt already present"
fi

# ── Step 5: Add std/build.rs platform support ──
echo "Patching std/build.rs..."
STD_BUILD="$STD_DST/std/build.rs"
if ! grep -q '"oxide"' "$STD_BUILD"; then
    # Add oxide to the list of supported platforms (find the motor line and add after)
    sed -i '/"motor"/a\        || target_os == "oxide"' "$STD_BUILD"
    echo "  Added oxide to build.rs"
else
    echo "  oxide already in build.rs"
fi

# ── Step 6: Create OXIDE PAL directory ──
echo "Creating OXIDE PAL files..."
PAL_DIR="$STD_DST/std/src/sys/pal/oxide"
mkdir -p "$PAL_DIR"

# ── PAL mod.rs ──
cat > "$PAL_DIR/mod.rs" << 'OXIDE_PAL_MOD'
//! — IronGhost: OXIDE OS Platform Abstraction Layer.
//! Where Rust's std meets our syscalls. No moto_rt, no libc — just oxide_rt.

#![allow(unsafe_op_in_unsafe_fn)]

pub mod os;
pub mod time;

pub use oxide_rt::futex;

use crate::io;

/// Map a raw errno (positive i32) to an io::Error
pub(crate) fn map_oxide_error(errno: i32) -> io::Error {
    io::Error::from_raw_os_error(errno)
}

/// Check a syscall return value and convert to io::Result
pub(crate) fn cvt(ret: i64) -> io::Result<usize> {
    if ret < 0 {
        Err(map_oxide_error((-ret) as i32))
    } else {
        Ok(ret as usize)
    }
}

/// Check a syscall return value (i32 version)
pub(crate) fn cvt_i32(ret: i32) -> io::Result<i32> {
    if ret < 0 {
        Err(map_oxide_error(-ret))
    } else {
        Ok(ret)
    }
}

// SAFETY: must be called only once during runtime initialization.
pub unsafe fn init(_argc: isize, _argv: *const *const u8, _sigpipe: u8) {
    // oxide_rt::start::_start handles init — nothing needed here
}

// SAFETY: must be called only once during runtime cleanup.
pub unsafe fn cleanup() {}

pub fn unsupported<T>() -> io::Result<T> {
    Err(unsupported_err())
}

pub fn unsupported_err() -> io::Error {
    io::Error::UNSUPPORTED_PLATFORM
}

pub fn abort_internal() -> ! {
    core::intrinsics::abort();
}
OXIDE_PAL_MOD

# ── PAL os.rs ──
cat > "$PAL_DIR/os.rs" << 'OXIDE_PAL_OS'
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
OXIDE_PAL_OS

# ── PAL time.rs ──
cat > "$PAL_DIR/time.rs" << 'OXIDE_PAL_TIME'
//! — WireSaint: Time primitives for std::time — Instant and SystemTime.
//! Built on CLOCK_MONOTONIC and CLOCK_REALTIME respectively.

use crate::time::Duration;
use crate::fmt;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    secs: u64,
    nanos: u32,
}

impl Instant {
    pub fn now() -> Self {
        let mut ts = oxide_rt::types::Timespec::zero();
        oxide_rt::time::clock_gettime(oxide_rt::time::CLOCK_MONOTONIC, &mut ts);
        Self {
            secs: ts.tv_sec as u64,
            nanos: ts.tv_nsec as u32,
        }
    }

    pub fn checked_sub_instant(&self, other: &Instant) -> Option<Duration> {
        let (secs, nanos) = if self.nanos >= other.nanos {
            (self.secs.checked_sub(other.secs)?, self.nanos - other.nanos)
        } else {
            (self.secs.checked_sub(other.secs)?.checked_sub(1)?, self.nanos + 1_000_000_000 - other.nanos)
        };
        Some(Duration::new(secs, nanos))
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<Instant> {
        let mut secs = self.secs.checked_add(other.as_secs())?;
        let mut nanos = self.nanos + other.subsec_nanos();
        if nanos >= 1_000_000_000 {
            nanos -= 1_000_000_000;
            secs = secs.checked_add(1)?;
        }
        Some(Instant { secs, nanos })
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Instant> {
        let mut secs = self.secs.checked_sub(other.as_secs())?;
        let nanos = if self.nanos >= other.subsec_nanos() {
            self.nanos - other.subsec_nanos()
        } else {
            secs = secs.checked_sub(1)?;
            self.nanos + 1_000_000_000 - other.subsec_nanos()
        };
        Some(Instant { secs, nanos })
    }
}

impl fmt::Debug for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Instant({}.{:09})", self.secs, self.nanos)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemTime {
    pub(crate) secs: u64,
    pub(crate) nanos: u32,
}

pub const UNIX_EPOCH: SystemTime = SystemTime { secs: 0, nanos: 0 };

impl SystemTime {
    /// — WireSaint: Minimum representable time (epoch)
    pub const MIN: SystemTime = SystemTime { secs: 0, nanos: 0 };
    /// — WireSaint: Maximum representable time (u64::MAX seconds)
    pub const MAX: SystemTime = SystemTime { secs: u64::MAX, nanos: 999_999_999 };

    pub fn now() -> Self {
        let mut ts = oxide_rt::types::Timespec::zero();
        oxide_rt::time::clock_gettime(oxide_rt::time::CLOCK_REALTIME, &mut ts);
        Self {
            secs: ts.tv_sec as u64,
            nanos: ts.tv_nsec as u32,
        }
    }

    pub fn sub_time(&self, other: &SystemTime) -> Result<Duration, Duration> {
        if self >= other {
            let (secs, nanos) = if self.nanos >= other.nanos {
                (self.secs - other.secs, self.nanos - other.nanos)
            } else {
                (self.secs - other.secs - 1, self.nanos + 1_000_000_000 - other.nanos)
            };
            Ok(Duration::new(secs, nanos))
        } else {
            Err(other.sub_time(self).unwrap())
        }
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<SystemTime> {
        let mut secs = self.secs.checked_add(other.as_secs())?;
        let mut nanos = self.nanos + other.subsec_nanos();
        if nanos >= 1_000_000_000 {
            nanos -= 1_000_000_000;
            secs = secs.checked_add(1)?;
        }
        Some(SystemTime { secs, nanos })
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<SystemTime> {
        let mut secs = self.secs.checked_sub(other.as_secs())?;
        let nanos = if self.nanos >= other.subsec_nanos() {
            self.nanos - other.subsec_nanos()
        } else {
            secs = secs.checked_sub(1)?;
            self.nanos + 1_000_000_000 - other.subsec_nanos()
        };
        Some(SystemTime { secs, nanos })
    }
}

impl fmt::Debug for SystemTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SystemTime({}.{:09})", self.secs, self.nanos)
    }
}
OXIDE_PAL_TIME

echo "  Created PAL core files (mod.rs, os.rs, time.rs)"

# ── Step 7: Create other sys module files ──
echo "Creating sys module implementations..."

# sys/alloc/oxide.rs
cat > "$STD_DST/std/src/sys/alloc/oxide.rs" << 'OXIDE_ALLOC'
//! — ByteRiot: GlobalAlloc for System using oxide_rt's mmap-backed allocator.
use crate::alloc::{GlobalAlloc, Layout, System};

#[stable(feature = "alloc_system_type", since = "1.28.0")]
unsafe impl GlobalAlloc for System {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Use oxide_rt's mmap-backed allocator
        unsafe { oxide_rt::alloc::mmap(0, layout.size().max(layout.align()), 0x3, 0x22, -1, 0) }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { oxide_rt::alloc::munmap(ptr, layout.size()); }
    }
}
OXIDE_ALLOC
echo "  Created sys/alloc/oxide.rs"

# sys/args/oxide.rs
cat > "$STD_DST/std/src/sys/args/oxide.rs" << 'OXIDE_ARGS'
//! — ThreadRogue: Command-line argument retrieval for std::env::args().
pub use super::common::Args;
use crate::ffi::OsString;

pub fn args() -> Args {
    let argc = oxide_rt::args::argc() as usize;
    let mut rust_args = Vec::new();
    for i in 0..argc {
        if let Some(bytes) = oxide_rt::args::arg(i) {
            let s = String::from_utf8_lossy(bytes).into_owned();
            rust_args.push(OsString::from(s));
        }
    }
    Args::new(rust_args)
}
OXIDE_ARGS
echo "  Created sys/args/oxide.rs"

# sys/env/oxide.rs
cat > "$STD_DST/std/src/sys/env/oxide.rs" << 'OXIDE_ENV'
//! — NeonRoot: Environment variable access for std::env.
pub use super::common::Env;
use crate::ffi::{OsStr, OsString};
use crate::io;

pub fn env() -> Env {
    let mut rust_env = vec![];
    oxide_rt::env::env_iter(|k, v| {
        let key = String::from_utf8_lossy(k).into_owned();
        let val = String::from_utf8_lossy(v).into_owned();
        rust_env.push((OsString::from(key), OsString::from(val)));
    });
    Env::new(rust_env)
}

pub fn getenv(key: &OsStr) -> Option<OsString> {
    let key_bytes = key.as_encoded_bytes();
    oxide_rt::env::getenv_bytes(key_bytes).map(|v| {
        OsString::from(String::from_utf8_lossy(v).into_owned())
    })
}

pub unsafe fn setenv(key: &OsStr, val: &OsStr) -> io::Result<()> {
    let k = key.as_encoded_bytes();
    let v = val.as_encoded_bytes();
    oxide_rt::env::setenv_bytes(k, v);
    Ok(())
}

pub unsafe fn unsetenv(key: &OsStr) -> io::Result<()> {
    let k = key.as_encoded_bytes();
    oxide_rt::env::unsetenv_bytes(k);
    Ok(())
}
OXIDE_ENV
echo "  Created sys/env/oxide.rs"

# sys/stdio/oxide.rs
cat > "$STD_DST/std/src/sys/stdio/oxide.rs" << 'OXIDE_STDIO'
//! — SableWire: stdin/stdout/stderr for std — fd 0/1/2, simple as it gets.
use crate::{io, process, sys};
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::sys::{AsInner, FromInner, IntoInner};

pub const STDIN_BUF_SIZE: usize = crate::sys::io::DEFAULT_BUF_SIZE;

pub struct Stdin {}
pub struct Stdout {}
pub struct Stderr {}

impl Stdin {
    pub const fn new() -> Self { Self {} }
}

impl Stdout {
    pub const fn new() -> Self { Self {} }
}

impl Stderr {
    pub const fn new() -> Self { Self {} }
}

impl crate::sealed::Sealed for Stdin {}

impl crate::io::IsTerminal for Stdin {
    fn is_terminal(&self) -> bool {
        // ioctl(0, TIOCGWINSZ, ...) — if it succeeds, it's a terminal
        let mut ws = oxide_rt::types::Winsize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };
        oxide_rt::io::ioctl(0, oxide_rt::types::ioctl_nr::TIOCGWINSZ, &mut ws as *mut _ as usize) == 0
    }
}

impl io::Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let ret = oxide_rt::io::read(0, buf);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret as i32)) }
        else { Ok(ret as usize) }
    }
}

impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let ret = oxide_rt::io::write(1, buf);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret as i32)) }
        else { Ok(ret as usize) }
    }

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

impl io::Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let ret = oxide_rt::io::write(2, buf);
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret as i32)) }
        else { Ok(ret as usize) }
    }

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

pub fn panic_output() -> Option<impl io::Write> {
    Some(Stderr::new())
}

pub fn is_ebadf(_err: &io::Error) -> bool {
    true
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl FromRawFd for process::Stdio {
    #[inline]
    unsafe fn from_raw_fd(fd: RawFd) -> process::Stdio {
        let fd = unsafe { sys::fd::FileDesc::from_raw_fd(fd) };
        let io = sys::process::Stdio::Fd(fd);
        process::Stdio::from_inner(io)
    }
}

#[stable(feature = "io_safety", since = "1.63.0")]
impl From<OwnedFd> for process::Stdio {
    #[inline]
    fn from(fd: OwnedFd) -> process::Stdio {
        let fd = sys::fd::FileDesc::from_inner(fd);
        let io = sys::process::Stdio::Fd(fd);
        process::Stdio::from_inner(io)
    }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl AsRawFd for process::ChildStdin {
    #[inline]
    fn as_raw_fd(&self) -> RawFd { self.as_inner().as_raw_fd() }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl AsRawFd for process::ChildStdout {
    #[inline]
    fn as_raw_fd(&self) -> RawFd { self.as_inner().as_raw_fd() }
}

#[stable(feature = "process_extensions", since = "1.2.0")]
impl AsRawFd for process::ChildStderr {
    #[inline]
    fn as_raw_fd(&self) -> RawFd { self.as_inner().as_raw_fd() }
}

#[stable(feature = "into_raw_os", since = "1.4.0")]
impl IntoRawFd for process::ChildStdin {
    #[inline]
    fn into_raw_fd(self) -> RawFd { self.into_inner().into_raw_fd() }
}

#[stable(feature = "into_raw_os", since = "1.4.0")]
impl IntoRawFd for process::ChildStdout {
    #[inline]
    fn into_raw_fd(self) -> RawFd { self.into_inner().into_raw_fd() }
}

#[stable(feature = "into_raw_os", since = "1.4.0")]
impl IntoRawFd for process::ChildStderr {
    #[inline]
    fn into_raw_fd(self) -> RawFd { self.into_inner().into_raw_fd() }
}

#[stable(feature = "io_safety", since = "1.63.0")]
impl AsFd for crate::process::ChildStdin {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> { self.as_inner().as_fd() }
}

#[stable(feature = "io_safety", since = "1.63.0")]
impl From<crate::process::ChildStdin> for OwnedFd {
    #[inline]
    fn from(child_stdin: crate::process::ChildStdin) -> OwnedFd {
        child_stdin.into_inner().into_inner()
    }
}

#[stable(feature = "child_stream_from_fd", since = "1.74.0")]
impl From<OwnedFd> for process::ChildStdin {
    #[inline]
    fn from(fd: OwnedFd) -> process::ChildStdin {
        let pipe = sys::process::ChildPipe::from_inner(fd);
        process::ChildStdin::from_inner(pipe)
    }
}

#[stable(feature = "io_safety", since = "1.63.0")]
impl AsFd for crate::process::ChildStdout {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> { self.as_inner().as_fd() }
}

#[stable(feature = "io_safety", since = "1.63.0")]
impl From<crate::process::ChildStdout> for OwnedFd {
    #[inline]
    fn from(child_stdout: crate::process::ChildStdout) -> OwnedFd {
        child_stdout.into_inner().into_inner()
    }
}

#[stable(feature = "child_stream_from_fd", since = "1.74.0")]
impl From<OwnedFd> for process::ChildStdout {
    #[inline]
    fn from(fd: OwnedFd) -> process::ChildStdout {
        let pipe = sys::process::ChildPipe::from_inner(fd);
        process::ChildStdout::from_inner(pipe)
    }
}

#[stable(feature = "io_safety", since = "1.63.0")]
impl AsFd for crate::process::ChildStderr {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> { self.as_inner().as_fd() }
}

#[stable(feature = "io_safety", since = "1.63.0")]
impl From<crate::process::ChildStderr> for OwnedFd {
    #[inline]
    fn from(child_stderr: crate::process::ChildStderr) -> OwnedFd {
        child_stderr.into_inner().into_inner()
    }
}

#[stable(feature = "child_stream_from_fd", since = "1.74.0")]
impl From<OwnedFd> for process::ChildStderr {
    #[inline]
    fn from(fd: OwnedFd) -> process::ChildStderr {
        let pipe = sys::process::ChildPipe::from_inner(fd);
        process::ChildStderr::from_inner(pipe)
    }
}
OXIDE_STDIO
echo "  Created sys/stdio/oxide.rs"

# sys/random/oxide.rs
cat > "$STD_DST/std/src/sys/random/oxide.rs" << 'OXIDE_RANDOM'
//! — ColdCipher: Random bytes from the kernel's entropy pool.
pub fn fill_bytes(bytes: &mut [u8]) {
    oxide_rt::random::fill_random(bytes);
}
OXIDE_RANDOM
echo "  Created sys/random/oxide.rs"

# sys/io/error/oxide.rs — OXIDE error mapping (Linux errno ABI)
cat > "$STD_DST/std/src/sys/io/error/oxide.rs" << 'OXIDE_IO_ERROR'
//! — ByteRiot: OXIDE error mapping. Linux errno ABI — syscalls return -errno directly.
use crate::io;
use crate::sys::io::RawOsError;

pub fn errno() -> RawOsError {
    0 // OXIDE propagates errors via return values, not thread-local errno
}

pub fn is_interrupted(code: io::RawOsError) -> bool {
    code == 4 // EINTR
}

pub fn decode_error_kind(code: io::RawOsError) -> io::ErrorKind {
    match code {
        1 => io::ErrorKind::PermissionDenied,       // EPERM
        2 => io::ErrorKind::NotFound,               // ENOENT
        4 => io::ErrorKind::Interrupted,             // EINTR
        9 => io::ErrorKind::InvalidInput,            // EBADF
        11 => io::ErrorKind::WouldBlock,             // EAGAIN
        12 => io::ErrorKind::OutOfMemory,            // ENOMEM
        13 => io::ErrorKind::PermissionDenied,       // EACCES
        17 => io::ErrorKind::AlreadyExists,          // EEXIST
        20 => io::ErrorKind::NotADirectory,          // ENOTDIR
        21 => io::ErrorKind::IsADirectory,           // EISDIR
        22 => io::ErrorKind::InvalidInput,           // EINVAL
        28 => io::ErrorKind::StorageFull,            // ENOSPC
        32 => io::ErrorKind::BrokenPipe,             // EPIPE
        36 => io::ErrorKind::InvalidFilename,        // ENAMETOOLONG
        38 => io::ErrorKind::Unsupported,            // ENOSYS
        39 => io::ErrorKind::DirectoryNotEmpty,      // ENOTEMPTY
        110 => io::ErrorKind::TimedOut,              // ETIMEDOUT
        111 => io::ErrorKind::ConnectionRefused,     // ECONNREFUSED
        _ => io::ErrorKind::Uncategorized,
    }
}

pub fn error_string(errno: RawOsError) -> String {
    match errno {
        1 => "Operation not permitted".to_string(),
        2 => "No such file or directory".to_string(),
        4 => "Interrupted system call".to_string(),
        9 => "Bad file descriptor".to_string(),
        11 => "Resource temporarily unavailable".to_string(),
        12 => "Out of memory".to_string(),
        13 => "Permission denied".to_string(),
        17 => "File exists".to_string(),
        20 => "Not a directory".to_string(),
        21 => "Is a directory".to_string(),
        22 => "Invalid argument".to_string(),
        28 => "No space left on device".to_string(),
        32 => "Broken pipe".to_string(),
        36 => "File name too long".to_string(),
        38 => "Function not implemented".to_string(),
        39 => "Directory not empty".to_string(),
        110 => "Connection timed out".to_string(),
        111 => "Connection refused".to_string(),
        _ => format!("Unknown error {}", errno),
    }
}
OXIDE_IO_ERROR
echo "  Created sys/io/error/oxide.rs"

# sys/fd/oxide.rs
cat > "$STD_DST/std/src/sys/fd/oxide.rs" << 'OXIDE_FD'
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
OXIDE_FD
echo "  Created sys/fd/oxide.rs"

# sys/fs/oxide.rs
cat > "$STD_DST/std/src/sys/fs/oxide.rs" << 'OXIDE_FS'
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
OXIDE_FS
echo "  Created sys/fs/oxide.rs"

# sys/thread/oxide.rs
cat > "$STD_DST/std/src/sys/thread/oxide.rs" << 'OXIDE_THREAD'
//! — ThreadRogue: Threading for std::thread — spawn, join, sleep, yield.
use crate::ffi::CStr;
use crate::io;
use crate::num::NonZeroUsize;
use crate::thread::ThreadInit;
use crate::time::Duration;

pub const DEFAULT_MIN_STACK_SIZE: usize = 256 * 1024;

pub struct Thread {
    // — ThreadRogue: OXIDE doesn't support thread joining yet,
    // so we just track the thread ID for now
    tid: i64,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
    pub unsafe fn new(_stack: usize, _init: Box<ThreadInit>) -> io::Result<Thread> {
        // — ThreadRogue: Thread creation via clone() not yet wired up in std path
        // For now, return unsupported
        Err(io::Error::UNSUPPORTED_PLATFORM)
    }

    pub fn join(self) {
        // — ThreadRogue: No join support yet
    }
}

pub fn set_name(_name: &CStr) {}

pub fn current_os_id() -> Option<u64> {
    Some(oxide_rt::os::gettid() as u64)
}

pub fn available_parallelism() -> io::Result<NonZeroUsize> {
    // — ThreadRogue: Report 1 CPU for now (safe default)
    Ok(unsafe { NonZeroUsize::new_unchecked(1) })
}

pub fn yield_now() {
    oxide_rt::thread::sched_yield();
}

pub fn sleep(dur: Duration) {
    let ts = oxide_rt::types::Timespec {
        tv_sec: dur.as_secs() as i64,
        tv_nsec: dur.subsec_nanos() as i64,
    };
    oxide_rt::thread::nanosleep(&ts, None);
}
OXIDE_THREAD
echo "  Created sys/thread/oxide.rs"

# sys/process/oxide.rs (as a directory with mod.rs)
PROC_DIR="$STD_DST/std/src/sys/process/oxide"
mkdir -p "$PROC_DIR"
cat > "$PROC_DIR/mod.rs" << 'OXIDE_PROCESS'
//! — BlackLatch: Process management for std::process — Command, spawn, wait.

use super::CommandEnvs;
use super::env::CommandEnv;
use crate::ffi::OsStr;
pub use crate::ffi::OsString as EnvKey;
use crate::num::NonZeroI32;
use crate::path::Path;
use crate::process::StdioPipes;
use crate::sys::fs::File;
use crate::sys::{AsInner, FromInner};
use crate::{fmt, io};

pub enum Stdio {
    Inherit,
    Null,
    MakePipe,
    Fd(crate::sys::fd::FileDesc),
}

impl Stdio {
    fn try_clone(&self) -> io::Result<Self> {
        match self {
            Stdio::Inherit => Ok(Stdio::Inherit),
            Stdio::Null => Ok(Stdio::Null),
            Stdio::MakePipe => Ok(Stdio::MakePipe),
            Stdio::Fd(fd) => Ok(Stdio::Fd(fd.try_clone()?)),
        }
    }
}

#[derive(Default)]
pub struct Command {
    program: String,
    args: Vec<String>,
    cwd: Option<String>,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
    env: CommandEnv,
}

impl Command {
    pub fn new(program: &OsStr) -> Command {
        Command {
            program: program.to_string_lossy().into_owned(),
            args: vec![program.to_string_lossy().into_owned()],
            cwd: None,
            stdin: None,
            stdout: None,
            stderr: None,
            env: Default::default(),
        }
    }

    pub fn arg(&mut self, arg: &OsStr) {
        self.args.push(arg.to_string_lossy().into_owned());
    }

    pub fn env_mut(&mut self) -> &mut CommandEnv { &mut self.env }
    pub fn cwd(&mut self, dir: &OsStr) { self.cwd = Some(dir.to_string_lossy().into_owned()); }
    pub fn stdin(&mut self, stdin: Stdio) { self.stdin = Some(stdin); }
    pub fn stdout(&mut self, stdout: Stdio) { self.stdout = Some(stdout); }
    pub fn stderr(&mut self, stderr: Stdio) { self.stderr = Some(stderr); }
    pub fn get_program(&self) -> &OsStr { OsStr::new(&self.program) }
    pub fn get_args(&self) -> CommandArgs<'_> { CommandArgs { iter: self.args[1..].iter() } }
    pub fn get_envs(&self) -> CommandEnvs<'_> { self.env.iter() }
    pub fn get_env_clear(&self) -> bool { self.env.does_clear() }
    pub fn get_current_dir(&self) -> Option<&Path> { self.cwd.as_ref().map(|s| Path::new(s.as_str())) }

    pub fn spawn(&mut self, _default: Stdio, _needs_stdin: bool)
        -> io::Result<(Process, StdioPipes)>
    {
        let pid = oxide_rt::process::fork();
        if pid < 0 {
            return Err(io::Error::from_raw_os_error(-pid));
        }

        if pid == 0 {
            // Child process
            // Build null-terminated argv
            let c_args: Vec<Vec<u8>> = self.args.iter()
                .map(|a| { let mut v = a.as_bytes().to_vec(); v.push(0); v })
                .collect();
            let argv_ptrs: Vec<*const u8> = c_args.iter().map(|a| a.as_ptr()).collect();
            let mut argv_with_null = argv_ptrs.clone();
            argv_with_null.push(core::ptr::null());

            let path_bytes = self.program.as_bytes();
            oxide_rt::process::execve(
                path_bytes,
                argv_with_null.as_ptr(),
                core::ptr::null(),
            );
            // If execve returns, it failed
            oxide_rt::os::exit(127);
        }

        // Parent process
        Ok((
            Process { pid, status: None },
            StdioPipes { stdin: None, stdout: None, stderr: None },
        ))
    }
}

impl From<crate::sys::fd::FileDesc> for Stdio {
    fn from(fd: crate::sys::fd::FileDesc) -> Self { Stdio::Fd(fd) }
}

impl From<File> for Stdio {
    fn from(file: File) -> Self { Stdio::Fd(file.into_inner()) }
}

impl From<io::Stdout> for Stdio {
    fn from(_: io::Stdout) -> Self { Stdio::Inherit }
}

impl From<io::Stderr> for Stdio {
    fn from(_: io::Stderr) -> Self { Stdio::Inherit }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Command {{ program: {:?}, args: {:?} }}", self.program, self.args)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ExitStatus(i32);

impl ExitStatus {
    pub fn exit_ok(&self) -> Result<(), ExitStatusError> {
        if self.0 == 0 { Ok(()) } else { Err(ExitStatusError(*self)) }
    }
    pub fn code(&self) -> Option<i32> { Some(self.0) }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit status: {}", self.0)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitStatusError(ExitStatus);

impl Into<ExitStatus> for ExitStatusError {
    fn into(self) -> ExitStatus { self.0 }
}

impl ExitStatusError {
    pub fn code(self) -> Option<NonZeroI32> { NonZeroI32::new(self.0.0) }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitCode(i32);

impl ExitCode {
    pub const SUCCESS: ExitCode = ExitCode(0);
    pub const FAILURE: ExitCode = ExitCode(1);
    pub fn as_i32(&self) -> i32 { self.0 }
}

impl From<u8> for ExitCode {
    fn from(code: u8) -> Self { ExitCode(code as i32) }
}

pub struct Process {
    pid: i32,
    status: Option<ExitStatus>,
}

impl Drop for Process {
    fn drop(&mut self) {
        // — BlackLatch: Don't leave zombies. If not waited on, wait now.
    }
}

impl Process {
    pub fn id(&self) -> u32 { self.pid as u32 }

    pub fn kill(&mut self) -> io::Result<()> {
        let ret = oxide_rt::process::kill(self.pid, 9); // SIGKILL
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
        else { Ok(()) }
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.status { return Ok(status); }
        let mut raw_status: i32 = 0;
        let ret = oxide_rt::process::waitpid(self.pid, &mut raw_status, 0);
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else {
            let status = ExitStatus(oxide_rt::process::wexitstatus(raw_status));
            self.status = Some(status);
            Ok(status)
        }
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status { return Ok(Some(status)); }
        let mut raw_status: i32 = 0;
        let ret = oxide_rt::process::waitpid(self.pid, &mut raw_status, 1); // WNOHANG
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else if ret == 0 {
            Ok(None)
        } else {
            let status = ExitStatus(oxide_rt::process::wexitstatus(raw_status));
            self.status = Some(status);
            Ok(Some(status))
        }
    }

    pub fn handle(&self) -> u64 { self.pid as u64 }
}

pub struct CommandArgs<'a> {
    iter: core::slice::Iter<'a, String>,
}

impl<'a> Iterator for CommandArgs<'a> {
    type Item = &'a OsStr;
    fn next(&mut self) -> Option<&'a OsStr> {
        self.iter.next().map(|s| OsStr::new(s.as_str()))
    }
}

impl<'a> ExactSizeIterator for CommandArgs<'a> {
    fn len(&self) -> usize { self.iter.len() }
}

impl<'a> fmt::Debug for CommandArgs<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter.clone()).finish()
    }
}

pub type ChildPipe = crate::sys::pipe::Pipe;

pub fn read_output(
    _out: ChildPipe, _stdout: &mut Vec<u8>,
    _err: ChildPipe, _stderr: &mut Vec<u8>,
) -> io::Result<()> {
    // — BlackLatch: TODO implement output capture
    Ok(())
}
OXIDE_PROCESS
echo "  Created sys/process/oxide/mod.rs"

# sys/pipe/oxide.rs
cat > "$STD_DST/std/src/sys/pipe/oxide.rs" << 'OXIDE_PIPE'
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
OXIDE_PIPE
echo "  Created sys/pipe/oxide.rs"

# sys/net/connection/oxide.rs
cat > "$STD_DST/std/src/sys/net/connection/oxide.rs" << 'OXIDE_NET'
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
OXIDE_NET
echo "  Created sys/net/connection/oxide.rs"

# sys/io/is_terminal/oxide.rs
IS_TERM_DIR="$STD_DST/std/src/sys/io/is_terminal"
mkdir -p "$IS_TERM_DIR"
cat > "$IS_TERM_DIR/oxide.rs" << 'OXIDE_IS_TERM'
//! — NeonVale: Terminal detection via ioctl TIOCGWINSZ.
use crate::os::fd::{AsFd, AsRawFd};

pub fn is_terminal(fd: &impl AsFd) -> bool {
    let fd = fd.as_fd();
    let mut ws = oxide_rt::types::Winsize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };
    oxide_rt::io::ioctl(fd.as_raw_fd(), oxide_rt::types::ioctl_nr::TIOCGWINSZ, &mut ws as *mut _ as usize) == 0
}
OXIDE_IS_TERM
echo "  Created sys/io/is_terminal/oxide.rs"

# os/oxide/mod.rs
OS_OXIDE_DIR="$STD_DST/std/src/os/oxide"
mkdir -p "$OS_OXIDE_DIR"
cat > "$OS_OXIDE_DIR/mod.rs" << 'OXIDE_OS_MOD'
//! — NeonRoot: OXIDE OS-specific extensions.
#![stable(feature = "rust1", since = "1.0.0")]

pub mod ffi;
OXIDE_OS_MOD

cat > "$OS_OXIDE_DIR/ffi.rs" << 'OXIDE_OS_FFI'
//! OXIDE OS-specific extensions to primitives in the [`std::ffi`] module.
//!
//! OXIDE uses byte-oriented paths (like Unix), so OsStr is just bytes.
#![stable(feature = "rust1", since = "1.0.0")]

use crate::ffi::{OsStr, OsString};
use crate::sealed::Sealed;

/// OXIDE OS-specific extensions to [`OsString`].
#[stable(feature = "rust1", since = "1.0.0")]
pub trait OsStringExt: Sealed {
    /// Creates an `OsString` from a byte vector.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn from_vec(vec: Vec<u8>) -> Self;

    /// Yields the underlying byte vector of this `OsString`.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn into_vec(self) -> Vec<u8>;
}

#[stable(feature = "rust1", since = "1.0.0")]
impl OsStringExt for OsString {
    #[inline]
    fn from_vec(vec: Vec<u8>) -> OsString {
        unsafe { OsString::from_encoded_bytes_unchecked(vec) }
    }

    #[inline]
    fn into_vec(self) -> Vec<u8> {
        self.into_encoded_bytes()
    }
}

/// OXIDE OS-specific extensions to [`OsStr`].
#[stable(feature = "rust1", since = "1.0.0")]
pub trait OsStrExt: Sealed {
    /// Creates an `OsStr` from a byte slice.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn from_bytes(slice: &[u8]) -> &Self;

    /// Gets the underlying byte view of the `OsStr` slice.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn as_bytes(&self) -> &[u8];
}

#[stable(feature = "rust1", since = "1.0.0")]
impl OsStrExt for OsStr {
    #[inline]
    fn from_bytes(slice: &[u8]) -> &OsStr {
        unsafe { OsStr::from_encoded_bytes_unchecked(slice) }
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self.as_encoded_bytes()
    }
}
OXIDE_OS_FFI
echo "  Created os/oxide/ (mod.rs, ffi.rs)"

# ── Step 8: Apply dispatch patches ──
echo ""
echo "Applying dispatch patches..."

SYS="$STD_DST/std/src/sys"
OS="$STD_DST/std/src/os"

# ── Helper: add oxide as a SEPARATE cfg_select! arm (before motor) ──
# These modules have their own oxide/ implementation files.
add_oxide_cfg_select_arm() {
    local file="$1"
    local oxide_arm="$2"
    if [ -f "$file" ] && ! grep -q 'target_os = "oxide"' "$file"; then
        sed -i "/target_os = \"motor\" =>/i\\    $oxide_arm" "$file" 2>/dev/null
        echo "  Patched: $file"
    fi
}

# ── Helper: add oxide to an any() list inside cfg_select! or #[cfg(any(...))] ──
# These are places where oxide shares the SAME implementation path as motor.
add_oxide_to_any_list() {
    local file="$1"
    if [ -f "$file" ]; then
        # Only add after motor entries that are inside any() — i.e., where motor
        # appears with a trailing comma (list item) rather than with => (arm condition).
        # Pattern: target_os = "motor",  →  target_os = "motor", target_os = "oxide",
        sed -i 's/target_os = "motor",$/target_os = "motor", target_os = "oxide",/' "$file" 2>/dev/null
        echo "  Patched: $file (any-list)"
    fi
}

# ═══ sys/pal/mod.rs — separate oxide PAL ═══
add_oxide_cfg_select_arm "$SYS/pal/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use self::oxide::*; }'

# ═══ sys/alloc/mod.rs — separate oxide allocator ═══
add_oxide_cfg_select_arm "$SYS/alloc/mod.rs" \
    'target_os = "oxide" => { mod oxide; }'

# ═══ sys/args/mod.rs — separate oxide args + shared #[cfg(any(...))] ═══
add_oxide_cfg_select_arm "$SYS/args/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use oxide::*; }'
# Also add oxide to the #[cfg(any(...))] that gates `mod common`
add_oxide_to_any_list "$SYS/args/mod.rs"

# ═══ sys/env/mod.rs — separate oxide env + shared #[cfg(any(...))] ═══
add_oxide_cfg_select_arm "$SYS/env/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use oxide::*; }'
add_oxide_to_any_list "$SYS/env/mod.rs"

# ═══ sys/stdio/mod.rs ═══
add_oxide_cfg_select_arm "$SYS/stdio/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use oxide::*; }'

# ═══ sys/random/mod.rs ═══
add_oxide_cfg_select_arm "$SYS/random/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use oxide::fill_bytes; }'

# ═══ sys/fd/mod.rs ═══
add_oxide_cfg_select_arm "$SYS/fd/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use oxide::*; }'

# ═══ sys/fs/mod.rs ═══
add_oxide_cfg_select_arm "$SYS/fs/mod.rs" \
    'target_os = "oxide" => { mod oxide; use oxide as imp; }'

# ═══ sys/thread/mod.rs ═══
add_oxide_cfg_select_arm "$SYS/thread/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use oxide::*; }'

# ═══ sys/process/mod.rs ═══
add_oxide_cfg_select_arm "$SYS/process/mod.rs" \
    'target_os = "oxide" => { mod oxide; use oxide as imp; }'
# — BlackLatch: oxide supports process spawning, so use the generic output() function.
# Add oxide to both the positive and negative #[cfg(any(...))] gates around pub fn output.
if [ -f "$SYS/process/mod.rs" ]; then
    python3 -c "
import sys
with open(sys.argv[1], 'r') as f:
    content = f.read()
# Add oxide after motor in both cfg blocks around the output() function
# Pattern: target_os = \"motor\"\n))] before pub fn output
content = content.replace(
    '    target_os = \"motor\"\n))]\npub fn output',
    '    target_os = \"motor\",\n    target_os = \"oxide\"\n))]\npub fn output'
)
# Pattern: target_os = \"motor\"\n)))]\npub use imp::output
content = content.replace(
    '    target_os = \"motor\"\n)))]\npub use imp::output',
    '    target_os = \"motor\",\n    target_os = \"oxide\"\n)))]\npub use imp::output'
)
with open(sys.argv[1], 'w') as f:
    f.write(content)
" "$SYS/process/mod.rs"
    echo "  Patched: process/mod.rs (output cfg gates)"
fi
# Fix IntoInner import in oxide process module
if [ -f "$SYS/process/oxide/mod.rs" ]; then
    sed -i 's/use crate::sys::{AsInner, FromInner};/use crate::sys::{AsInner, FromInner, IntoInner};/' \
        "$SYS/process/oxide/mod.rs" 2>/dev/null || true
fi

# ═══ sys/pipe/mod.rs ═══
add_oxide_cfg_select_arm "$SYS/pipe/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use oxide::{Pipe, pipe}; }'

# ═══ sys/io/mod.rs — oxide io dispatch ═══
if [ -f "$SYS/io/mod.rs" ]; then
    # Add oxide arm to the is_terminal cfg_select if present
    add_oxide_cfg_select_arm "$SYS/io/mod.rs" \
        'target_os = "oxide" => { mod oxide; pub use oxide::*; }'
fi
if [ -f "$SYS/io/is_terminal/mod.rs" ]; then
    add_oxide_cfg_select_arm "$SYS/io/is_terminal/mod.rs" \
        'target_os = "oxide" => { mod oxide; pub use oxide::*; }'
fi

# ═══ sys/io/error/mod.rs — oxide error mapping ═══
add_oxide_cfg_select_arm "$SYS/io/error/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use oxide::*; }'

# ═══ sys/net/connection/mod.rs ═══
add_oxide_cfg_select_arm "$SYS/net/connection/mod.rs" \
    'target_os = "oxide" => { mod oxide; pub use oxide::*; }'

# ═══ sys/sync — oxide shares futex impl with motor (inside any() lists) ═══
# The sync modules use cfg_select! with any() lists that already include motor.
# We add oxide alongside motor INSIDE the any() predicate.
for syncmod in mutex condvar rwlock once thread_parking; do
    if [ -f "$SYS/sync/$syncmod/mod.rs" ]; then
        add_oxide_to_any_list "$SYS/sync/$syncmod/mod.rs"
        echo "  Patched: sync/$syncmod/mod.rs (futex shared)"
    fi
done

# ═══ sys/personality/mod.rs — oxide shares aborting stub (inside any()) ═══
if [ -f "$SYS/personality/mod.rs" ]; then
    add_oxide_to_any_list "$SYS/personality/mod.rs"
    echo "  Patched: personality/mod.rs"
fi

# ═══ sys/thread_local/mod.rs — oxide thread-local dispatch ═══
if [ -f "$SYS/thread_local/mod.rs" ]; then
    # Add oxide as separate arm before motor's arm (includes racy lazy-init wrapper)
    if ! grep -q 'target_os = "oxide"' "$SYS/thread_local/mod.rs"; then
        sed -i '/target_os = "motor" =>/i\
    target_os = "oxide" => {\
        mod racy;\
        pub(super) use racy::LazyKey;\
        pub(super) use oxide_rt::tls::{Key, get, set};\
        use oxide_rt::tls::{create, destroy};\
    }' "$SYS/thread_local/mod.rs" 2>/dev/null
        echo "  Patched: $SYS/thread_local/mod.rs"
    fi
    # skip the generic add_oxide_cfg_select_arm since we did it manually above
    true
    # Also add to any() lists (for #[cfg(any(...))] guards)
    add_oxide_to_any_list "$SYS/thread_local/mod.rs"
fi

# ═══ os/mod.rs — add oxide module declaration ═══
if [ -f "$OS/mod.rs" ] && ! grep -q 'target_os = "oxide"' "$OS/mod.rs"; then
    # Add oxide module AFTER the motor module declaration (pub mod motor;)
    sed -i '/^pub mod motor;/a\
#[cfg(target_os = "oxide")]\
pub mod oxide;' "$OS/mod.rs" 2>/dev/null && echo "  Patched: os/mod.rs (module decl)"
fi
# Add oxide to the fd module's #[cfg(any(...))] gate in os/mod.rs
add_oxide_to_any_list "$OS/mod.rs"

# ═══ os/fd/raw.rs — oxide fd types ═══
# — PulseForge: oxide uses i32 for RawFd (like motor), imports OwnedFd from super,
#   provides oxide_rt::libc for STDIN/STDOUT/STDERR_FILENO constants.
#   IMPORTANT: oxide has SEPARATE code paths from motor — never share moto_rt references.
if [ -f "$OS/fd/raw.rs" ] && ! grep -q 'target_os = "oxide"' "$OS/fd/raw.rs"; then
    python3 -c "
import re, sys
with open(sys.argv[1], 'r') as f:
    content = f.read()

# 1. Add oxide libc import AFTER motor's moto_rt::libc line (separate, not shared)
content = content.replace(
    '#[cfg(target_os = \"motor\")]\nuse moto_rt::libc;',
    '#[cfg(target_os = \"motor\")]\nuse moto_rt::libc;\n#[cfg(target_os = \"oxide\")]\nuse oxide_rt::libc;'
)

# 2. Add oxide OwnedFd import — the line '#[cfg(target_os = \"motor\")]\nuse super::owned::OwnedFd;'
content = content.replace(
    '#[cfg(target_os = \"motor\")]\nuse super::owned::OwnedFd;',
    '#[cfg(any(target_os = \"motor\", target_os = \"oxide\"))]\nuse super::owned::OwnedFd;'
)

# 3. Add oxide to RawFd = i32 gate
content = content.replace(
    'any(target_os = \"hermit\", target_os = \"motor\")',
    'any(target_os = \"hermit\", target_os = \"motor\", target_os = \"oxide\")'
)

# 4. Exclude oxide from raw::c_int RawFd path (the not(target_os = \"motor\") guards)
content = content.replace(
    'not(target_os = \"motor\")',
    'not(any(target_os = \"motor\", target_os = \"oxide\"))'
)

with open(sys.argv[1], 'w') as f:
    f.write(content)
" "$OS/fd/raw.rs"
    echo "  Patched: os/fd/raw.rs"
fi

# ═══ os/fd/owned.rs — oxide fd ownership (uses oxide_rt::io) ═══
# — PulseForge: oxide needs its own try_clone_to_owned (via oxide_rt::io::dup) and
#   its own close in Drop (via oxide_rt::io::close). Completely separate from motor.
if [ -f "$OS/fd/owned.rs" ] && ! grep -q 'target_os = "oxide"' "$OS/fd/owned.rs"; then
    python3 -c "
import sys
with open(sys.argv[1], 'r') as f:
    content = f.read()

# 1a. Exclude oxide from the cvt import (oxide doesn't use libc::fcntl)
#     Pattern: target_os = \"motor\"\n)))]\nuse crate::sys::cvt;
content = content.replace(
    '    target_os = \"motor\"\n)))]\nuse crate::sys::cvt;',
    '    target_os = \"motor\",\n    target_os = \"oxide\"\n)))]\nuse crate::sys::cvt;'
)

# 1b. Exclude oxide from the cvt/libc-based try_clone_to_owned cfg gate
#     Pattern: target_os = \"motor\"\n    )))]\n    #[stable...
content = content.replace(
    '        target_os = \"motor\"\n    )))]\n    #[stable(feature = \"io_safety\", since = \"1.63.0\")]\n    pub fn try_clone_to_owned(&self) -> io::Result<OwnedFd> {\n        // We want to atomically',
    '        target_os = \"motor\",\n        target_os = \"oxide\"\n    )))]\n    #[stable(feature = \"io_safety\", since = \"1.63.0\")]\n    pub fn try_clone_to_owned(&self) -> io::Result<OwnedFd> {\n        // We want to atomically'
)

# 2. Add oxide try_clone_to_owned AFTER motor's closing brace
#    Motor's block ends with: Ok(unsafe { OwnedFd::from_raw_fd(fd) })\n    }
#    followed by the closing '}' of impl BorrowedFd
# Find the motor try_clone block end and insert oxide's after it
# Match: motor's closing brace + the impl closing brace
motor_clone_end = '        Ok(unsafe { OwnedFd::from_raw_fd(fd) })\n    }\n}'
oxide_clone = '        Ok(unsafe { OwnedFd::from_raw_fd(fd) })\n    }\n\n    /// \xe2\x80\x94 SableWire: OXIDE clone via dup syscall\n    #[cfg(target_os = \"oxide\")]\n    #[stable(feature = \"io_safety\", since = \"1.63.0\")]\n    pub fn try_clone_to_owned(&self) -> io::Result<OwnedFd> {\n        let fd = oxide_rt::io::dup(self.as_raw_fd());\n        if fd < 0 { Err(io::Error::from_raw_os_error(-fd)) }\n        else { Ok(unsafe { OwnedFd::from_raw_fd(fd) }) }\n    }\n}'
content = content.replace(motor_clone_end, oxide_clone, 1)

# 3. Fix Drop impl — add oxide close and exclude oxide from libc::close block
#    Original Drop has:
#      #[cfg(not(target_os = \"hermit\"))]
#      {
#          #[cfg(unix)]
#          crate::sys::fs::debug_assert_fd_is_open(self.fd.as_inner());
#
#          let _ = libc::close(self.fd.as_inner());
#      }
content = content.replace(
    '            #[cfg(not(target_os = \"hermit\"))]\n            {\n                #[cfg(unix)]\n                crate::sys::fs::debug_assert_fd_is_open(self.fd.as_inner());\n\n                let _ = libc::close(self.fd.as_inner());\n            }',
    '            #[cfg(target_os = \"oxide\")]\n            {\n                let _ = oxide_rt::io::close(self.fd.as_inner());\n            }\n            #[cfg(not(any(target_os = \"hermit\", target_os = \"oxide\")))]\n            {\n                #[cfg(unix)]\n                crate::sys::fs::debug_assert_fd_is_open(self.fd.as_inner());\n\n                let _ = libc::close(self.fd.as_inner());\n            }'
)

with open(sys.argv[1], 'w') as f:
    f.write(content)
" "$OS/fd/owned.rs"
    echo "  Patched: os/fd/owned.rs"
fi

# ═══ os/fd/mod.rs — add oxide to fd module cfg gate ═══
if [ -f "$OS/fd/mod.rs" ]; then
    add_oxide_to_any_list "$OS/fd/mod.rs"
    echo "  Patched: os/fd/mod.rs"
fi

echo ""
echo "=== OXIDE std Source Setup Complete ==="
echo ""
echo "Next steps:"
echo "  1. Build oxide-rt: cargo build --package oxide-rt --target x86_64-unknown-none"
echo "  2. Build std userspace: make userspace-std-pkg PKG=hello-std"
echo "  3. Or build all: make build && make create-rootfs"
