//! File control operations
//!
//! Open flags and file descriptor operations.

/// Open flags
pub const O_RDONLY: u32 = 0;
pub const O_WRONLY: u32 = 1;
pub const O_RDWR: u32 = 2;
pub const O_CREAT: u32 = 0o100;
pub const O_EXCL: u32 = 0o200;
pub const O_TRUNC: u32 = 0o1000;
pub const O_APPEND: u32 = 0o2000;
pub const O_NONBLOCK: u32 = 0o4000;
pub const O_NOCTTY: u32 = 0o400;
pub const O_DIRECTORY: u32 = 0o200000;

/// Seek whence values
pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

/// File modes
pub const S_IRWXU: u32 = 0o700;
pub const S_IRUSR: u32 = 0o400;
pub const S_IWUSR: u32 = 0o200;
pub const S_IXUSR: u32 = 0o100;
pub const S_IRWXG: u32 = 0o070;
pub const S_IRGRP: u32 = 0o040;
pub const S_IWGRP: u32 = 0o020;
pub const S_IXGRP: u32 = 0o010;
pub const S_IRWXO: u32 = 0o007;
pub const S_IROTH: u32 = 0o004;
pub const S_IWOTH: u32 = 0o002;
pub const S_IXOTH: u32 = 0o001;

/// Standard file descriptors
pub const STDIN_FILENO: i32 = 0;
pub const STDOUT_FILENO: i32 = 1;
pub const STDERR_FILENO: i32 = 2;

/// fcntl commands
pub const F_DUPFD: i32 = 0;
pub const F_GETFD: i32 = 1;
pub const F_SETFD: i32 = 2;
pub const F_GETFL: i32 = 3;
pub const F_SETFL: i32 = 4;

/// File descriptor flags (for F_GETFD/F_SETFD)
pub const FD_CLOEXEC: i32 = 1;

/// fcntl - File control operations
/// — GraveShift: must go through errno conversion — raw cast was swallowing errors
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fcntl(fd: i32, cmd: i32, arg: u64) -> i32 {
    let raw = crate::syscall::syscall3(42, fd as usize, cmd as usize, arg as usize);
    // — IronGhost: errno dance — negative kernel returns become -1 + ERRNO_VAR
    if raw < 0 && raw >= -4096 {
        crate::c_exports::set_errno_raw((-raw) as i32);
        -1
    } else {
        raw as i32
    }
}
