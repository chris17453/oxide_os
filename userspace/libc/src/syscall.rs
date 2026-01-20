//! System call interface
//!
//! Provides syscall wrappers for OXIDE userspace.
//! Architecture-specific raw syscall implementations are in arch/.

// Re-export arch-specific raw syscall functions
pub use crate::arch::syscall::{syscall0, syscall1, syscall2, syscall3, syscall4, syscall5, syscall6};

/// Syscall numbers (must match kernel)
pub mod nr {
    pub const EXIT: u64 = 0;
    pub const WRITE: u64 = 1;
    pub const READ: u64 = 2;
    pub const FORK: u64 = 3;
    pub const EXEC: u64 = 4;
    pub const WAIT: u64 = 5;
    pub const WAITPID: u64 = 6;
    pub const GETPID: u64 = 7;
    pub const GETPPID: u64 = 8;
    pub const SETPGID: u64 = 9;
    pub const GETPGID: u64 = 10;
    pub const SETSID: u64 = 11;
    pub const GETSID: u64 = 12;
    pub const OPEN: u64 = 20;
    pub const CLOSE: u64 = 21;
    pub const LSEEK: u64 = 22;
    pub const FSTAT: u64 = 23;
    pub const STAT: u64 = 24;
    pub const DUP: u64 = 25;
    pub const DUP2: u64 = 26;
    pub const FTRUNCATE: u64 = 27;
    pub const MKDIR: u64 = 30;
    pub const RMDIR: u64 = 31;
    pub const UNLINK: u64 = 32;
    pub const RENAME: u64 = 33;
    pub const GETDENTS: u64 = 34;
    pub const CHDIR: u64 = 35;
    pub const GETCWD: u64 = 36;
    pub const PIPE: u64 = 37;
    pub const LINK: u64 = 38;
    pub const SYMLINK: u64 = 39;
    pub const IOCTL: u64 = 40;
    pub const READLINK: u64 = 41;
    pub const KILL: u64 = 50;
    pub const SIGACTION: u64 = 51;
    pub const SIGPROCMASK: u64 = 52;
    pub const SIGPENDING: u64 = 53;
    pub const SIGSUSPEND: u64 = 54;
    pub const PAUSE: u64 = 55;
    pub const SIGRETURN: u64 = 56;
    // Time syscalls
    pub const GETTIMEOFDAY: u64 = 60;
    pub const CLOCK_GETTIME: u64 = 61;
    pub const CLOCK_GETRES: u64 = 62;
    pub const NANOSLEEP: u64 = 63;
    // Socket syscalls (must match kernel)
    pub const SOCKET: u64 = 70;
    pub const BIND: u64 = 71;
    pub const LISTEN: u64 = 72;
    pub const ACCEPT: u64 = 73;
    pub const CONNECT: u64 = 74;
    pub const SEND: u64 = 75;
    pub const RECV: u64 = 76;
    pub const SENDTO: u64 = 77;
    pub const RECVFROM: u64 = 78;
    pub const SHUTDOWN: u64 = 79;
    pub const GETSOCKNAME: u64 = 80;
    pub const GETPEERNAME: u64 = 81;
    pub const SETSOCKOPT: u64 = 82;
    pub const GETSOCKOPT: u64 = 83;
    // Directory syscalls
    pub const GETDENTS64: u64 = 84;
    // Poll/select syscalls
    pub const POLL: u64 = 90;
    pub const PPOLL: u64 = 91;
    pub const SELECT: u64 = 92;
    pub const PSELECT6: u64 = 93;
    // User/group syscalls
    pub const GETUID: u64 = 100;
    pub const GETEUID: u64 = 101;
    pub const GETGID: u64 = 102;
    pub const GETEGID: u64 = 103;
    pub const SETUID: u64 = 104;
    pub const SETGID: u64 = 105;
    pub const SETEUID: u64 = 106;
    pub const SETEGID: u64 = 107;
    // Memory syscalls (must match kernel)
    pub const MMAP: u64 = 90;
    pub const MUNMAP: u64 = 91;
    pub const MPROTECT: u64 = 92;
    pub const MREMAP: u64 = 93;
    pub const BRK: u64 = 94;
    // Keyboard layout syscalls
    pub const SETKEYMAP: u64 = 120;
    pub const GETKEYMAP: u64 = 121;

    // Thread syscalls (Linux-compatible numbers)
    pub const CLONE: u64 = 56;
    pub const GETTID: u64 = 186;
    pub const FUTEX: u64 = 202;
    pub const SET_TID_ADDRESS: u64 = 218;
    pub const EXIT_GROUP: u64 = 231;
}

// Re-export syscall numbers at module level for convenience
pub use nr::OPEN as SYS_OPEN;
pub use nr::CLOSE as SYS_CLOSE;
pub use nr::READ as SYS_READ;
pub use nr::WRITE as SYS_WRITE;
pub use nr::LSEEK as SYS_LSEEK;
pub use nr::IOCTL as SYS_IOCTL;
pub use nr::GETDENTS as SYS_GETDENTS;
pub use nr::GETTIMEOFDAY as SYS_GETTIMEOFDAY;
pub use nr::CLOCK_GETTIME as SYS_CLOCK_GETTIME;
pub use nr::CLOCK_GETRES as SYS_CLOCK_GETRES;
pub use nr::NANOSLEEP as SYS_NANOSLEEP;
pub use nr::POLL as SYS_POLL;
pub use nr::PPOLL as SYS_PPOLL;
pub use nr::SELECT as SYS_SELECT;
pub use nr::PSELECT6 as SYS_PSELECT6;
pub use nr::GETDENTS64 as SYS_GETDENTS64;
pub use nr::GETUID as SYS_GETUID;
pub use nr::GETEUID as SYS_GETEUID;
pub use nr::GETGID as SYS_GETGID;
pub use nr::GETEGID as SYS_GETEGID;
pub use nr::SETUID as SYS_SETUID;
pub use nr::SETGID as SYS_SETGID;
pub use nr::SETEUID as SYS_SETEUID;
pub use nr::SETEGID as SYS_SETEGID;
pub use nr::MMAP as SYS_MMAP;
pub use nr::MUNMAP as SYS_MUNMAP;
pub use nr::MPROTECT as SYS_MPROTECT;
pub use nr::MREMAP as SYS_MREMAP;
pub use nr::BRK as SYS_BRK;
pub use nr::SETKEYMAP as SYS_SETKEYMAP;
pub use nr::GETKEYMAP as SYS_GETKEYMAP;
pub use nr::CLONE as SYS_CLONE;
pub use nr::GETTID as SYS_GETTID;
pub use nr::FUTEX as SYS_FUTEX;
pub use nr::SET_TID_ADDRESS as SYS_SET_TID_ADDRESS;
pub use nr::EXIT_GROUP as SYS_EXIT_GROUP;

// ============================================================================
// High-level syscall wrappers (architecture-independent)
// ============================================================================

/// sys_exit - Terminate process
pub fn sys_exit(status: i32) -> ! {
    syscall1(nr::EXIT, status as usize);
    loop {}
}

/// sys_write - Write to file descriptor
pub fn sys_write(fd: i32, buf: &[u8]) -> isize {
    syscall3(nr::WRITE, fd as usize, buf.as_ptr() as usize, buf.len()) as isize
}

/// sys_read - Read from file descriptor
pub fn sys_read(fd: i32, buf: &mut [u8]) -> isize {
    syscall3(nr::READ, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize
}

/// sys_open - Open file
pub fn sys_open(path: &str, flags: u32, mode: u32) -> i32 {
    syscall4(nr::OPEN, path.as_ptr() as usize, path.len(), flags as usize, mode as usize) as i32
}

/// sys_close - Close file descriptor
pub fn sys_close(fd: i32) -> i32 {
    syscall1(nr::CLOSE, fd as usize) as i32
}

/// sys_fork - Create child process
pub fn sys_fork() -> i32 {
    syscall0(nr::FORK) as i32
}

/// sys_exec - Execute new program
pub fn sys_exec(path: &str) -> i32 {
    syscall2(nr::EXEC, path.as_ptr() as usize, path.len()) as i32
}

/// sys_wait - Wait for any child
pub fn sys_wait(status: &mut i32) -> i32 {
    syscall1(nr::WAIT, status as *mut i32 as usize) as i32
}

/// sys_waitpid - Wait for specific child
pub fn sys_waitpid(pid: i32, status: &mut i32, options: i32) -> i32 {
    syscall3(nr::WAITPID, pid as usize, status as *mut i32 as usize, options as usize) as i32
}

/// sys_getpid - Get process ID
pub fn sys_getpid() -> i32 {
    syscall0(nr::GETPID) as i32
}

/// sys_getppid - Get parent process ID
pub fn sys_getppid() -> i32 {
    syscall0(nr::GETPPID) as i32
}

/// sys_kill - Send signal to process
pub fn sys_kill(pid: i32, sig: i32) -> i32 {
    syscall2(nr::KILL, pid as usize, sig as usize) as i32
}

/// sys_dup - Duplicate file descriptor
pub fn sys_dup(fd: i32) -> i32 {
    syscall1(nr::DUP, fd as usize) as i32
}

/// sys_dup2 - Duplicate file descriptor to specific fd
pub fn sys_dup2(oldfd: i32, newfd: i32) -> i32 {
    syscall2(nr::DUP2, oldfd as usize, newfd as usize) as i32
}

/// sys_mkdir - Create directory
pub fn sys_mkdir(path: &str, mode: u32) -> i32 {
    syscall3(nr::MKDIR, path.as_ptr() as usize, path.len(), mode as usize) as i32
}

/// sys_rmdir - Remove directory
pub fn sys_rmdir(path: &str) -> i32 {
    syscall2(nr::RMDIR, path.as_ptr() as usize, path.len()) as i32
}

/// sys_unlink - Remove file
pub fn sys_unlink(path: &str) -> i32 {
    syscall2(nr::UNLINK, path.as_ptr() as usize, path.len()) as i32
}

/// sys_rename - Rename/move file
pub fn sys_rename(old: &str, new: &str) -> i32 {
    syscall4(nr::RENAME, old.as_ptr() as usize, old.len(), new.as_ptr() as usize, new.len()) as i32
}

/// sys_link - Create hard link
pub fn sys_link(target: &str, link_name: &str) -> i32 {
    syscall4(nr::LINK, target.as_ptr() as usize, target.len(), link_name.as_ptr() as usize, link_name.len()) as i32
}

/// sys_symlink - Create symbolic link
pub fn sys_symlink(target: &str, link_name: &str) -> i32 {
    syscall4(nr::SYMLINK, target.as_ptr() as usize, target.len(), link_name.as_ptr() as usize, link_name.len()) as i32
}

/// sys_readlink - Read value of symbolic link
pub fn sys_readlink(path: &str, buf: &mut [u8]) -> i32 {
    syscall4(nr::READLINK, path.as_ptr() as usize, path.len(), buf.as_mut_ptr() as usize, buf.len()) as i32
}

/// sys_nanosleep - High resolution sleep
pub fn sys_nanosleep(seconds: u64, nanoseconds: u64) -> i32 {
    // Pack seconds and nanoseconds
    syscall2(nr::NANOSLEEP, seconds as usize, nanoseconds as usize) as i32
}

/// sys_gettimeofday - Get current time
pub fn sys_gettimeofday(tv_sec: &mut i64, tv_usec: &mut i64) -> i32 {
    syscall2(nr::GETTIMEOFDAY, tv_sec as *mut i64 as usize, tv_usec as *mut i64 as usize) as i32
}

/// sys_getdents - Read directory entries
pub fn sys_getdents(fd: i32, buf: &mut [u8]) -> i32 {
    syscall3(nr::GETDENTS, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as i32
}

/// sys_ioctl - Device control
pub fn sys_ioctl(fd: i32, request: u64, arg: u64) -> i32 {
    syscall3(nr::IOCTL, fd as usize, request as usize, arg as usize) as i32
}

/// sys_chdir - Change working directory
pub fn sys_chdir(path: &str) -> i32 {
    syscall2(nr::CHDIR, path.as_ptr() as usize, path.len()) as i32
}

/// sys_getcwd - Get current working directory
pub fn sys_getcwd(buf: &mut [u8]) -> i32 {
    syscall2(nr::GETCWD, buf.as_mut_ptr() as usize, buf.len()) as i32
}

/// sys_pipe - Create pipe
pub fn sys_pipe(pipefd: &mut [i32; 2]) -> i32 {
    syscall1(nr::PIPE, pipefd.as_mut_ptr() as usize) as i32
}

/// sys_lseek - Seek in file
pub fn sys_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    syscall3(nr::LSEEK, fd as usize, offset as usize, whence as usize)
}

/// sys_setsid - Create new session
pub fn sys_setsid() -> i32 {
    syscall0(nr::SETSID) as i32
}

/// sys_setpgid - Set process group
pub fn sys_setpgid(pid: i32, pgid: i32) -> i32 {
    syscall2(nr::SETPGID, pid as usize, pgid as usize) as i32
}

/// sys_getpgid - Get process group
pub fn sys_getpgid(pid: i32) -> i32 {
    syscall1(nr::GETPGID, pid as usize) as i32
}

// ============================================================================
// Thread syscall wrappers
// ============================================================================

/// Clone flags
pub mod clone_flags {
    pub const CLONE_VM: u32 = 0x0000_0100;
    pub const CLONE_FS: u32 = 0x0000_0200;
    pub const CLONE_FILES: u32 = 0x0000_0400;
    pub const CLONE_SIGHAND: u32 = 0x0000_0800;
    pub const CLONE_THREAD: u32 = 0x0001_0000;
    pub const CLONE_SETTLS: u32 = 0x0008_0000;
    pub const CLONE_CHILD_SETTID: u32 = 0x0100_0000;
    pub const CLONE_CHILD_CLEARTID: u32 = 0x0020_0000;
    pub const CLONE_PARENT_SETTID: u32 = 0x0010_0000;
}

/// Futex operations
pub mod futex_op {
    pub const FUTEX_WAIT: i32 = 0;
    pub const FUTEX_WAKE: i32 = 1;
    pub const FUTEX_PRIVATE_FLAG: i32 = 128;
    pub const FUTEX_WAIT_PRIVATE: i32 = FUTEX_WAIT | FUTEX_PRIVATE_FLAG;
    pub const FUTEX_WAKE_PRIVATE: i32 = FUTEX_WAKE | FUTEX_PRIVATE_FLAG;
}

/// sys_clone - Create a new process or thread
///
/// # Arguments
/// * `flags` - Clone flags (CLONE_VM, CLONE_THREAD, etc.)
/// * `stack` - New stack pointer (0 to inherit parent's)
/// * `parent_tid` - Location to store parent TID
/// * `child_tid` - Location to store child TID
/// * `tls` - Thread-local storage pointer
pub fn sys_clone(flags: u32, stack: *mut u8, parent_tid: *mut u32, child_tid: *mut u32, tls: u64) -> i32 {
    syscall5(
        nr::CLONE,
        flags as usize,
        stack as usize,
        parent_tid as usize,
        child_tid as usize,
        tls as usize,
    ) as i32
}

/// sys_gettid - Get thread ID
pub fn sys_gettid() -> i32 {
    syscall0(nr::GETTID) as i32
}

/// sys_futex - Fast userspace mutex operations
///
/// # Arguments
/// * `addr` - Address of the futex word
/// * `op` - Operation (FUTEX_WAIT, FUTEX_WAKE, etc.)
/// * `val` - Value (expected value for WAIT, count for WAKE)
/// * `timeout` - Timeout in nanoseconds (0 = infinite)
/// * `addr2` - Second address (for some operations)
/// * `val3` - Third value (for some operations)
pub fn sys_futex(addr: *mut u32, op: i32, val: u32, timeout: u64, addr2: *mut u32, val3: u32) -> i32 {
    syscall6(
        nr::FUTEX,
        addr as usize,
        op as usize,
        val as usize,
        timeout as usize,
        addr2 as usize,
        val3 as usize,
    ) as i32
}

/// sys_futex_wait - Wait on a futex
pub fn sys_futex_wait(addr: *mut u32, expected: u32, timeout_ns: u64) -> i32 {
    sys_futex(addr, futex_op::FUTEX_WAIT_PRIVATE, expected, timeout_ns, core::ptr::null_mut(), 0)
}

/// sys_futex_wake - Wake waiters on a futex
pub fn sys_futex_wake(addr: *mut u32, count: u32) -> i32 {
    sys_futex(addr, futex_op::FUTEX_WAKE_PRIVATE, count, 0, core::ptr::null_mut(), 0)
}

/// sys_set_tid_address - Set clear_child_tid pointer
pub fn sys_set_tid_address(tidptr: *mut u32) -> i32 {
    syscall1(nr::SET_TID_ADDRESS, tidptr as usize) as i32
}

/// sys_exit_group - Exit all threads in thread group
pub fn sys_exit_group(status: i32) -> ! {
    syscall1(nr::EXIT_GROUP, status as usize);
    loop {}
}

// ============================================================================
// Memory mapping syscall wrappers
// ============================================================================

/// Protection flags for mmap/mprotect
pub mod prot {
    pub const PROT_NONE: i32 = 0x0;
    pub const PROT_READ: i32 = 0x1;
    pub const PROT_WRITE: i32 = 0x2;
    pub const PROT_EXEC: i32 = 0x4;
}

/// Map flags for mmap
pub mod map_flags {
    pub const MAP_SHARED: i32 = 0x01;
    pub const MAP_PRIVATE: i32 = 0x02;
    pub const MAP_FIXED: i32 = 0x10;
    pub const MAP_ANONYMOUS: i32 = 0x20;
    pub const MAP_ANON: i32 = MAP_ANONYMOUS;
    pub const MAP_GROWSDOWN: i32 = 0x0100;
    pub const MAP_STACK: i32 = 0x20000;
}

/// Mremap flags
pub mod mremap_flags {
    pub const MREMAP_MAYMOVE: i32 = 1;
    pub const MREMAP_FIXED: i32 = 2;
}

/// Failed mmap result
pub const MAP_FAILED: *mut u8 = usize::MAX as *mut u8;

/// sys_mmap - Map memory
///
/// # Arguments
/// * `addr` - Requested address (hint or fixed)
/// * `length` - Size of mapping
/// * `prot` - Protection flags (PROT_READ, PROT_WRITE, PROT_EXEC)
/// * `flags` - Mapping flags (MAP_ANONYMOUS, MAP_PRIVATE, etc.)
/// * `fd` - File descriptor (for file-backed mappings, -1 for anonymous)
/// * `offset` - Offset in file
///
/// # Returns
/// Address of mapping on success, MAP_FAILED on error
pub fn sys_mmap(addr: *mut u8, length: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> *mut u8 {
    let result = syscall6(
        nr::MMAP,
        addr as usize,
        length,
        prot as usize,
        flags as usize,
        fd as usize,
        offset as usize,
    );

    if (result as i64) < 0 {
        MAP_FAILED
    } else {
        result as *mut u8
    }
}

/// sys_munmap - Unmap memory
///
/// # Arguments
/// * `addr` - Start address of mapping
/// * `length` - Size to unmap
///
/// # Returns
/// 0 on success, -1 on error
pub fn sys_munmap(addr: *mut u8, length: usize) -> i32 {
    syscall2(nr::MUNMAP, addr as usize, length) as i32
}

/// sys_mprotect - Change memory protection
///
/// # Arguments
/// * `addr` - Start address
/// * `length` - Size of region
/// * `prot` - New protection flags
///
/// # Returns
/// 0 on success, -1 on error
pub fn sys_mprotect(addr: *mut u8, length: usize, prot: i32) -> i32 {
    syscall3(nr::MPROTECT, addr as usize, length, prot as usize) as i32
}

/// sys_mremap - Remap memory
///
/// # Arguments
/// * `old_addr` - Current address
/// * `old_size` - Current size
/// * `new_size` - New size
/// * `flags` - Remap flags (MREMAP_MAYMOVE, etc.)
///
/// # Returns
/// New address on success, MAP_FAILED on error
pub fn sys_mremap(old_addr: *mut u8, old_size: usize, new_size: usize, flags: i32) -> *mut u8 {
    let result = syscall4(
        nr::MREMAP,
        old_addr as usize,
        old_size,
        new_size,
        flags as usize,
    );

    if (result as i64) < 0 {
        MAP_FAILED
    } else {
        result as *mut u8
    }
}

/// sys_brk - Change data segment size
///
/// # Arguments
/// * `addr` - New end of data segment (null to query current)
///
/// # Returns
/// Current/new end of data segment, or null on error
pub fn sys_brk(addr: *mut u8) -> *mut u8 {
    syscall1(nr::BRK, addr as usize) as *mut u8
}
