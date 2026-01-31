//! System call interface
//!
//! Provides syscall wrappers for OXIDE userspace.
//! Architecture-specific raw syscall implementations are in arch/.

// Re-export arch-specific raw syscall functions
pub use crate::arch::syscall::{
    syscall0, syscall1, syscall2, syscall3, syscall4, syscall5, syscall6,
};

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
    pub const EXECVE: u64 = 13;
    pub const OPEN: u64 = 20;
    pub const CLOSE: u64 = 21;
    pub const LSEEK: u64 = 22;
    pub const FSTAT: u64 = 23;
    pub const STAT: u64 = 24;
    pub const LSTAT: u64 = 28;
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
    pub const SIGRETURN: u64 = 57;
    // Time syscalls
    pub const GETTIMEOFDAY: u64 = 60;
    pub const CLOCK_GETTIME: u64 = 61;
    pub const CLOCK_GETRES: u64 = 62;
    pub const NANOSLEEP: u64 = 63;
    // System info syscalls
    pub const UNAME: u64 = 64;
    pub const STATFS: u64 = 65;
    pub const FSTATFS: u64 = 66;
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
    // Poll/select syscalls (must match kernel - avoid collision with MMAP)
    pub const POLL: u64 = 95;
    pub const PPOLL: u64 = 96;
    pub const SELECT: u64 = 97;
    pub const PSELECT6: u64 = 98;
    // User/group syscalls (MUST match kernel syscall numbers!)
    pub const GETUID: u64 = 14;
    pub const GETGID: u64 = 15;
    pub const GETEUID: u64 = 16;
    pub const GETEGID: u64 = 17;
    pub const SETUID: u64 = 18;
    pub const SETGID: u64 = 19;
    pub const SETEUID: u64 = 20;
    pub const SETEGID: u64 = 21;
    // Memory syscalls (must match kernel)
    pub const MMAP: u64 = 90;
    pub const MUNMAP: u64 = 91;
    pub const MPROTECT: u64 = 92;
    pub const MREMAP: u64 = 93;
    pub const BRK: u64 = 94;
    // Keyboard layout syscalls
    pub const SETKEYMAP: u64 = 120;
    pub const GETKEYMAP: u64 = 121;

    // Process priority syscalls
    pub const NICE: u64 = 122;
    pub const GETPRIORITY: u64 = 123;
    pub const SETPRIORITY: u64 = 124;

    // Timer/alarm syscalls
    pub const ALARM: u64 = 125;
    pub const SETITIMER: u64 = 126;
    pub const GETITIMER: u64 = 127;

    // Scheduler syscalls
    pub const SCHED_YIELD: u64 = 130;

    // File permission syscalls
    pub const CHMOD: u64 = 150;
    pub const FCHMOD: u64 = 151;
    pub const CHOWN: u64 = 152;
    pub const FCHOWN: u64 = 153;
    pub const UTIMES: u64 = 154;
    pub const FUTIMES: u64 = 155;

    // Thread syscalls (Linux-compatible numbers)
    pub const CLONE: u64 = 56;
    pub const GETTID: u64 = 186;
    pub const FUTEX: u64 = 202;
    pub const SET_TID_ADDRESS: u64 = 218;
    pub const EXIT_GROUP: u64 = 231;

    // Firewall syscalls
    pub const FW_ADD_RULE: u64 = 200;
    pub const FW_DEL_RULE: u64 = 201;
    pub const FW_LIST_RULES: u64 = 202;
    pub const FW_SET_POLICY: u64 = 203;
    pub const FW_FLUSH: u64 = 204;
    pub const FW_GET_CONNTRACK: u64 = 205;

    // Random number generation
    pub const GETRANDOM: u64 = 318;

    // Filesystem mount syscalls
    pub const MOUNT: u64 = 165;
    pub const UMOUNT: u64 = 166;
    pub const PIVOT_ROOT: u64 = 167;
}

// Re-export syscall numbers at module level for convenience
pub use nr::BRK as SYS_BRK;
pub use nr::CLOCK_GETRES as SYS_CLOCK_GETRES;
pub use nr::CLOCK_GETTIME as SYS_CLOCK_GETTIME;
pub use nr::CLONE as SYS_CLONE;
pub use nr::CLOSE as SYS_CLOSE;
pub use nr::EXIT_GROUP as SYS_EXIT_GROUP;
pub use nr::FUTEX as SYS_FUTEX;
pub use nr::GETDENTS as SYS_GETDENTS;
pub use nr::GETDENTS64 as SYS_GETDENTS64;
pub use nr::GETEGID as SYS_GETEGID;
pub use nr::GETEUID as SYS_GETEUID;
pub use nr::GETGID as SYS_GETGID;
pub use nr::GETKEYMAP as SYS_GETKEYMAP;
pub use nr::GETTID as SYS_GETTID;
pub use nr::GETTIMEOFDAY as SYS_GETTIMEOFDAY;
pub use nr::GETUID as SYS_GETUID;
pub use nr::IOCTL as SYS_IOCTL;
pub use nr::LSEEK as SYS_LSEEK;
pub use nr::MMAP as SYS_MMAP;
pub use nr::MPROTECT as SYS_MPROTECT;
pub use nr::MREMAP as SYS_MREMAP;
pub use nr::MUNMAP as SYS_MUNMAP;
pub use nr::NANOSLEEP as SYS_NANOSLEEP;
pub use nr::OPEN as SYS_OPEN;
pub use nr::POLL as SYS_POLL;
pub use nr::PPOLL as SYS_PPOLL;
pub use nr::PSELECT6 as SYS_PSELECT6;
pub use nr::READ as SYS_READ;
pub use nr::SELECT as SYS_SELECT;
pub use nr::SET_TID_ADDRESS as SYS_SET_TID_ADDRESS;
pub use nr::SETEGID as SYS_SETEGID;
pub use nr::SETEUID as SYS_SETEUID;
pub use nr::SETGID as SYS_SETGID;
pub use nr::SETKEYMAP as SYS_SETKEYMAP;
pub use nr::SETUID as SYS_SETUID;
pub use nr::WRITE as SYS_WRITE;

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
    syscall4(
        nr::OPEN,
        path.as_ptr() as usize,
        path.len(),
        flags as usize,
        mode as usize,
    ) as i32
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

/// sys_execve - Execute new program with arguments and environment
pub fn sys_execve(path: &str, argv: *const *const u8, envp: *const *const u8) -> i32 {
    syscall4(
        nr::EXECVE,
        path.as_ptr() as usize,
        path.len(),
        argv as usize,
        envp as usize,
    ) as i32
}

/// sys_wait - Wait for any child
pub fn sys_wait(status: &mut i32) -> i32 {
    syscall1(nr::WAIT, status as *mut i32 as usize) as i32
}

/// sys_waitpid - Wait for specific child
pub fn sys_waitpid(pid: i32, status: &mut i32, options: i32) -> i32 {
    syscall3(
        nr::WAITPID,
        pid as usize,
        status as *mut i32 as usize,
        options as usize,
    ) as i32
}

/// sys_getpid - Get process ID
pub fn sys_getpid() -> i32 {
    syscall0(nr::GETPID) as i32
}

/// sys_getppid - Get parent process ID
pub fn sys_getppid() -> i32 {
    syscall0(nr::GETPPID) as i32
}

/// sys_sched_yield - Yield the processor
pub fn sys_sched_yield() -> i32 {
    syscall0(nr::SCHED_YIELD) as i32
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

/// mkdir - Create directory (raw pointer version)
pub fn mkdir(path_ptr: *const u8, path_len: usize, mode: u32) -> i32 {
    syscall3(nr::MKDIR, path_ptr as usize, path_len, mode as usize) as i32
}

/// sys_rmdir - Remove directory
pub fn sys_rmdir(path: &str) -> i32 {
    syscall2(nr::RMDIR, path.as_ptr() as usize, path.len()) as i32
}

/// sys_unlink - Remove file
pub fn sys_unlink(path: &str) -> i32 {
    syscall2(nr::UNLINK, path.as_ptr() as usize, path.len()) as i32
}

/// unlink - Remove file (raw pointer version)
pub fn unlink(path_ptr: *const u8, path_len: usize) -> i32 {
    syscall2(nr::UNLINK, path_ptr as usize, path_len) as i32
}

/// sys_chmod - Change file mode
pub fn sys_chmod(path: &str, mode: u32) -> i32 {
    syscall3(nr::CHMOD, path.as_ptr() as usize, path.len(), mode as usize) as i32
}

/// sys_chown - Change file owner and group
pub fn sys_chown(path: &str, uid: i32, gid: i32) -> i32 {
    syscall4(
        nr::CHOWN,
        path.as_ptr() as usize,
        path.len(),
        uid as usize,
        gid as usize,
    ) as i32
}

/// chown - Change file owner and group (raw pointer version)
pub fn chown(path_ptr: *const u8, path_len: usize, uid: i32, gid: i32) -> i32 {
    syscall4(
        nr::CHOWN,
        path_ptr as usize,
        path_len,
        uid as usize,
        gid as usize,
    ) as i32
}

/// sys_utimes - Set file access and modification times
///
/// # Arguments
/// * `path` - Path to file
/// * `atime_sec` - Access time in seconds since epoch (u64::MAX = don't change)
/// * `mtime_sec` - Modification time in seconds since epoch (u64::MAX = don't change)
pub fn sys_utimes(path: &str, atime_sec: u64, mtime_sec: u64) -> i32 {
    syscall4(
        nr::UTIMES,
        path.as_ptr() as usize,
        path.len(),
        atime_sec as usize,
        mtime_sec as usize,
    ) as i32
}

/// sys_rename - Rename/move file
pub fn sys_rename(old: &str, new: &str) -> i32 {
    syscall4(
        nr::RENAME,
        old.as_ptr() as usize,
        old.len(),
        new.as_ptr() as usize,
        new.len(),
    ) as i32
}

/// sys_link - Create hard link
pub fn sys_link(target: &str, link_name: &str) -> i32 {
    syscall4(
        nr::LINK,
        target.as_ptr() as usize,
        target.len(),
        link_name.as_ptr() as usize,
        link_name.len(),
    ) as i32
}

/// sys_symlink - Create symbolic link
pub fn sys_symlink(target: &str, link_name: &str) -> i32 {
    syscall4(
        nr::SYMLINK,
        target.as_ptr() as usize,
        target.len(),
        link_name.as_ptr() as usize,
        link_name.len(),
    ) as i32
}

/// sys_readlink - Read value of symbolic link
pub fn sys_readlink(path: &str, buf: &mut [u8]) -> i32 {
    syscall4(
        nr::READLINK,
        path.as_ptr() as usize,
        path.len(),
        buf.as_mut_ptr() as usize,
        buf.len(),
    ) as i32
}

/// sys_nanosleep - High resolution sleep
pub fn sys_nanosleep(seconds: u64, nanoseconds: u64) -> i32 {
    // Pack seconds and nanoseconds
    syscall2(nr::NANOSLEEP, seconds as usize, nanoseconds as usize) as i32
}

/// sys_gettimeofday - Get current time
pub fn sys_gettimeofday(tv_sec: &mut i64, tv_usec: &mut i64) -> i32 {
    syscall2(
        nr::GETTIMEOFDAY,
        tv_sec as *mut i64 as usize,
        tv_usec as *mut i64 as usize,
    ) as i32
}

/// sys_getdents - Read directory entries
pub fn sys_getdents(fd: i32, buf: &mut [u8]) -> i32 {
    syscall3(
        nr::GETDENTS,
        fd as usize,
        buf.as_mut_ptr() as usize,
        buf.len(),
    ) as i32
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

/// sys_ftruncate - Truncate file to specified length
pub fn sys_ftruncate(fd: i32, length: i64) -> i32 {
    syscall2(nr::FTRUNCATE, fd as usize, length as usize) as i32
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
pub fn sys_clone(
    flags: u32,
    stack: *mut u8,
    parent_tid: *mut u32,
    child_tid: *mut u32,
    tls: u64,
) -> i32 {
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
pub fn sys_futex(
    addr: *mut u32,
    op: i32,
    val: u32,
    timeout: u64,
    addr2: *mut u32,
    val3: u32,
) -> i32 {
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
    sys_futex(
        addr,
        futex_op::FUTEX_WAIT_PRIVATE,
        expected,
        timeout_ns,
        core::ptr::null_mut(),
        0,
    )
}

/// sys_futex_wake - Wake waiters on a futex
pub fn sys_futex_wake(addr: *mut u32, count: u32) -> i32 {
    sys_futex(
        addr,
        futex_op::FUTEX_WAKE_PRIVATE,
        count,
        0,
        core::ptr::null_mut(),
        0,
    )
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
pub fn sys_mmap(
    addr: *mut u8,
    length: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: i64,
) -> *mut u8 {
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

// ============================================================================
// Process priority syscall wrappers
// ============================================================================

/// Priority target types
pub mod prio {
    pub const PRIO_PROCESS: i32 = 0;
    pub const PRIO_PGRP: i32 = 1;
    pub const PRIO_USER: i32 = 2;
}

/// sys_nice - Change process priority
///
/// # Arguments
/// * `inc` - Priority increment (positive = lower priority)
///
/// # Returns
/// New nice value on success, -1 on error
pub fn sys_nice(inc: i32) -> i32 {
    syscall1(nr::NICE, inc as usize) as i32
}

/// sys_getpriority - Get scheduling priority
///
/// # Arguments
/// * `which` - PRIO_PROCESS, PRIO_PGRP, or PRIO_USER
/// * `who` - Process ID, group ID, or user ID (0 = current)
///
/// # Returns
/// Priority value (0-40) on success, -1 on error
pub fn sys_getpriority(which: i32, who: i32) -> i32 {
    syscall2(nr::GETPRIORITY, which as usize, who as usize) as i32
}

/// sys_setpriority - Set scheduling priority
///
/// # Arguments
/// * `which` - PRIO_PROCESS, PRIO_PGRP, or PRIO_USER
/// * `who` - Process ID, group ID, or user ID (0 = current)
/// * `prio` - New priority (0-40, where 0 is highest)
///
/// # Returns
/// 0 on success, -1 on error
pub fn sys_setpriority(which: i32, who: i32, prio: i32) -> i32 {
    syscall3(nr::SETPRIORITY, which as usize, who as usize, prio as usize) as i32
}

// ============================================================================
// Timer/alarm syscall wrappers
// ============================================================================

/// Interval timer structure
#[repr(C)]
pub struct ITimerVal {
    pub it_interval_sec: i64,  // Timer interval (seconds)
    pub it_interval_usec: i64, // Timer interval (microseconds)
    pub it_value_sec: i64,     // Current value (seconds)
    pub it_value_usec: i64,    // Current value (microseconds)
}

/// Timer types
pub mod itimer {
    pub const ITIMER_REAL: i32 = 0; // Real time (SIGALRM)
    pub const ITIMER_VIRTUAL: i32 = 1; // User time (SIGVTALRM)
    pub const ITIMER_PROF: i32 = 2; // User + system time (SIGPROF)
}

/// sys_alarm - Set alarm signal
///
/// # Arguments
/// * `seconds` - Seconds until SIGALRM (0 = cancel)
///
/// # Returns
/// Seconds remaining from previous alarm
pub fn sys_alarm(seconds: u32) -> u32 {
    syscall1(nr::ALARM, seconds as usize) as u32
}

/// sys_setitimer - Set interval timer
///
/// # Arguments
/// * `which` - ITIMER_REAL, ITIMER_VIRTUAL, or ITIMER_PROF
/// * `new_value` - New timer value
/// * `old_value` - Optional old timer value (may be null)
///
/// # Returns
/// 0 on success, -1 on error
pub fn sys_setitimer(which: i32, new_value: *const ITimerVal, old_value: *mut ITimerVal) -> i32 {
    syscall3(
        nr::SETITIMER,
        which as usize,
        new_value as usize,
        old_value as usize,
    ) as i32
}

/// sys_getitimer - Get interval timer
///
/// # Arguments
/// * `which` - ITIMER_REAL, ITIMER_VIRTUAL, or ITIMER_PROF
/// * `curr_value` - Current timer value
///
/// # Returns
/// 0 on success, -1 on error
pub fn sys_getitimer(which: i32, curr_value: *mut ITimerVal) -> i32 {
    syscall2(nr::GETITIMER, which as usize, curr_value as usize) as i32
}

// ============================================================================
// Firewall syscall wrappers
// ============================================================================

/// Firewall rule structure (matches kernel FwRule)
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FwRule {
    /// Chain: 0=Input, 1=Output, 2=Forward
    pub chain: u8,
    /// Action: 0=Accept, 1=Drop, 2=Reject
    pub action: u8,
    /// Protocol: 0=Any, 1=ICMP, 6=TCP, 17=UDP
    pub protocol: u8,
    /// Connection state: 0=Any, 1=New, 2=Established, 3=Related, 4=Invalid
    pub state: u8,
    /// Source IP address (network byte order)
    pub src_ip: u32,
    /// Source IP prefix length (0-32)
    pub src_prefix: u8,
    /// Destination IP address (network byte order)
    pub dst_ip: u32,
    /// Destination IP prefix length (0-32)
    pub dst_prefix: u8,
    /// Source port start
    pub src_port_start: u16,
    /// Source port end
    pub src_port_end: u16,
    /// Destination port start
    pub dst_port_start: u16,
    /// Destination port end
    pub dst_port_end: u16,
    /// Padding for alignment
    pub _pad: [u8; 2],
}

/// Connection tracking statistics
#[repr(C)]
pub struct ConntrackStats {
    pub count: u64,
    pub max: u64,
}

/// Firewall chain constants
pub mod fw_chain {
    pub const INPUT: u8 = 0;
    pub const OUTPUT: u8 = 1;
    pub const FORWARD: u8 = 2;
    pub const ALL: u8 = 255;
}

/// Firewall action constants
pub mod fw_action {
    pub const ACCEPT: u8 = 0;
    pub const DROP: u8 = 1;
    pub const REJECT: u8 = 2;
}

/// Firewall protocol constants
pub mod fw_proto {
    pub const ANY: u8 = 0;
    pub const ICMP: u8 = 1;
    pub const TCP: u8 = 6;
    pub const UDP: u8 = 17;
}

/// Connection state constants
pub mod fw_state {
    pub const ANY: u8 = 0;
    pub const NEW: u8 = 1;
    pub const ESTABLISHED: u8 = 2;
    pub const RELATED: u8 = 3;
    pub const INVALID: u8 = 4;
}

/// sys_fw_add_rule - Add a firewall rule
///
/// # Arguments
/// * `rule` - Pointer to FwRule structure
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_fw_add_rule(rule: *const FwRule) -> i32 {
    syscall1(nr::FW_ADD_RULE, rule as usize) as i32
}

/// sys_fw_del_rule - Delete a firewall rule by index
///
/// # Arguments
/// * `index` - Rule index to delete
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_fw_del_rule(index: usize) -> i32 {
    syscall1(nr::FW_DEL_RULE, index) as i32
}

/// sys_fw_list_rules - List firewall rules
///
/// # Arguments
/// * `buf` - Buffer for FwRule array (null to just get count)
/// * `buf_len` - Maximum number of rules to return
///
/// # Returns
/// Number of rules copied (or total count if buf is null), negative errno on error
pub fn sys_fw_list_rules(buf: *mut FwRule, buf_len: usize) -> i32 {
    syscall2(nr::FW_LIST_RULES, buf as usize, buf_len) as i32
}

/// sys_fw_set_policy - Set chain default policy
///
/// # Arguments
/// * `chain` - 0=Input, 1=Output, 2=Forward
/// * `policy` - 0=Accept, 1=Drop, 2=Reject
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_fw_set_policy(chain: u8, policy: u8) -> i32 {
    syscall2(nr::FW_SET_POLICY, chain as usize, policy as usize) as i32
}

/// sys_fw_flush - Flush rules from a chain
///
/// # Arguments
/// * `chain` - 0=Input, 1=Output, 2=Forward, 255=All
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_fw_flush(chain: u8) -> i32 {
    syscall1(nr::FW_FLUSH, chain as usize) as i32
}

/// sys_fw_get_conntrack - Get connection tracking statistics
///
/// # Arguments
/// * `stats` - Optional pointer to ConntrackStats structure
///
/// # Returns
/// Number of tracked connections, negative errno on error
pub fn sys_fw_get_conntrack(stats: *mut ConntrackStats) -> i32 {
    syscall1(nr::FW_GET_CONNTRACK, stats as usize) as i32
}

// ============================================================================
// Random number generation syscall wrappers
// ============================================================================

/// Flags for getrandom()
pub mod grnd_flags {
    /// Non-blocking mode - return EAGAIN if not enough entropy
    pub const GRND_NONBLOCK: u32 = 0x0001;
    /// Use /dev/random pool (blocking pool, more conservative)
    pub const GRND_RANDOM: u32 = 0x0002;
    /// Use insecure pool (for early boot, before initialization)
    pub const GRND_INSECURE: u32 = 0x0004;
}

/// Re-export getrandom syscall number
pub use nr::GETRANDOM as SYS_GETRANDOM;

/// sys_getrandom - Get random bytes from kernel CSPRNG
///
/// # Arguments
/// * `buf` - Buffer to fill with random bytes
/// * `buflen` - Size of buffer
/// * `flags` - GRND_NONBLOCK, GRND_RANDOM, etc.
///
/// # Returns
/// Number of bytes written on success, negative errno on error
pub fn sys_getrandom(buf: &mut [u8], flags: u32) -> isize {
    syscall3(
        nr::GETRANDOM,
        buf.as_mut_ptr() as usize,
        buf.len(),
        flags as usize,
    ) as isize
}

/// getrandom - Get random bytes (raw pointer version)
///
/// # Arguments
/// * `buf` - Pointer to buffer
/// * `buflen` - Size of buffer
/// * `flags` - GRND_NONBLOCK, GRND_RANDOM, etc.
///
/// # Returns
/// Number of bytes written on success, negative errno on error
pub fn getrandom(buf: *mut u8, buflen: usize, flags: u32) -> isize {
    syscall3(nr::GETRANDOM, buf as usize, buflen, flags as usize) as isize
}

// ============================================================================
// System Information Syscalls
// ============================================================================

/// UtsName structure for uname syscall
#[repr(C)]
pub struct UtsName {
    /// Operating system name
    pub sysname: [u8; 65],
    /// Network node hostname
    pub nodename: [u8; 65],
    /// Operating system release
    pub release: [u8; 65],
    /// Operating system version
    pub version: [u8; 65],
    /// Hardware identifier (machine)
    pub machine: [u8; 65],
    /// Domain name (Linux extension)
    pub domainname: [u8; 65],
}

impl UtsName {
    /// Create a zeroed UtsName
    pub const fn new() -> Self {
        UtsName {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        }
    }

    /// Get a field as a string slice (stops at null terminator)
    pub fn get_str(field: &[u8; 65]) -> &str {
        let len = field.iter().position(|&c| c == 0).unwrap_or(64);
        core::str::from_utf8(&field[..len]).unwrap_or("")
    }
}

/// sys_uname - Get system identification
///
/// # Arguments
/// * `buf` - Pointer to UtsName structure
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_uname(buf: &mut UtsName) -> i32 {
    syscall1(nr::UNAME, buf as *mut UtsName as usize) as i32
}

/// uname - Get system identification (C-style interface)
pub fn uname(buf: *mut UtsName) -> i32 {
    syscall1(nr::UNAME, buf as usize) as i32
}

/// Statfs structure for filesystem statistics
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Statfs {
    /// Filesystem type
    pub f_type: i64,
    /// Optimal transfer block size
    pub f_bsize: i64,
    /// Total data blocks in filesystem
    pub f_blocks: u64,
    /// Free blocks in filesystem
    pub f_bfree: u64,
    /// Free blocks available to unprivileged user
    pub f_bavail: u64,
    /// Total file nodes in filesystem
    pub f_files: u64,
    /// Free file nodes in filesystem
    pub f_ffree: u64,
    /// Filesystem ID
    pub f_fsid: [i32; 2],
    /// Maximum length of filenames
    pub f_namelen: i64,
    /// Fragment size
    pub f_frsize: i64,
    /// Mount flags
    pub f_flags: i64,
    /// Spare bytes
    pub f_spare: [i64; 4],
}

impl Statfs {
    /// Create a zeroed Statfs structure
    pub const fn new() -> Self {
        Statfs {
            f_type: 0,
            f_bsize: 0,
            f_blocks: 0,
            f_bfree: 0,
            f_bavail: 0,
            f_files: 0,
            f_ffree: 0,
            f_fsid: [0, 0],
            f_namelen: 0,
            f_frsize: 0,
            f_flags: 0,
            f_spare: [0; 4],
        }
    }
}

/// statfs - Get filesystem statistics for a path
///
/// # Arguments
/// * `path` - Path to file on the filesystem
/// * `buf` - Pointer to Statfs structure
///
/// # Returns
/// 0 on success, negative errno on error
pub fn statfs(path: &str, buf: &mut Statfs) -> i32 {
    syscall3(
        nr::STATFS,
        path.as_ptr() as usize,
        path.len(),
        buf as *mut Statfs as usize,
    ) as i32
}

/// fstatfs - Get filesystem statistics for a file descriptor
///
/// # Arguments
/// * `fd` - File descriptor
/// * `buf` - Pointer to Statfs structure
///
/// # Returns
/// 0 on success, negative errno on error
pub fn fstatfs(fd: i32, buf: &mut Statfs) -> i32 {
    syscall2(nr::FSTATFS, fd as usize, buf as *mut Statfs as usize) as i32
}

// ============================================================================
// Mount/Umount syscalls
// ============================================================================

/// Mount flags (Linux-compatible values)
pub mod mount_flags {
    /// Read-only mount
    pub const MS_RDONLY: u32 = 1;
    /// Don't allow setuid/setgid
    pub const MS_NOSUID: u32 = 2;
    /// Don't interpret special files
    pub const MS_NODEV: u32 = 4;
    /// Don't allow program execution
    pub const MS_NOEXEC: u32 = 8;
    /// Writes are synced immediately
    pub const MS_SYNCHRONOUS: u32 = 16;
    /// Remount an existing mount
    pub const MS_REMOUNT: u32 = 32;
    /// Allow mandatory locks
    pub const MS_MANDLOCK: u32 = 64;
    /// Directory modifications are synchronous
    pub const MS_DIRSYNC: u32 = 128;
    /// Don't follow symlinks
    pub const MS_NOSYMFOLLOW: u32 = 256;
    /// Don't update access times
    pub const MS_NOATIME: u32 = 1024;
    /// Don't update directory access times
    pub const MS_NODIRATIME: u32 = 2048;
    /// Bind mount
    pub const MS_BIND: u32 = 4096;
    /// Move mount
    pub const MS_MOVE: u32 = 8192;
    /// Recursive mount
    pub const MS_REC: u32 = 16384;
    /// Silent flag
    pub const MS_SILENT: u32 = 32768;
    /// Relative atime updates
    pub const MS_RELATIME: u32 = 1 << 21;
    /// Strict atime updates
    pub const MS_STRICTATIME: u32 = 1 << 24;
    /// Make writes sync lazily
    pub const MS_LAZYTIME: u32 = 1 << 25;
}

/// Umount flags
pub mod umount_flags {
    /// Force unmount even if busy
    pub const MNT_FORCE: u32 = 1;
    /// Detach from filesystem tree (lazy unmount)
    pub const MNT_DETACH: u32 = 2;
    /// Mark for expiry
    pub const MNT_EXPIRE: u32 = 4;
    /// Don't follow symlinks
    pub const UMOUNT_NOFOLLOW: u32 = 8;
}

/// mount - Mount a filesystem
///
/// # Arguments
/// * `source` - Device or directory to mount (e.g., "/dev/sda1")
/// * `target` - Mount point path
/// * `fstype` - Filesystem type (e.g., "ext4", "tmpfs")
/// * `flags` - Mount flags (MS_RDONLY, etc.)
/// * `data` - Filesystem-specific mount options (unused for now)
///
/// # Returns
/// 0 on success, negative errno on error
pub fn mount(source: &str, target: &str, fstype: &str, flags: u32, _data: *const u8) -> i32 {
    // Pack flags into upper 32 bits of the final argument (fstype_len | flags<<32)
    let fstype_len_and_flags = (fstype.len() as u64) | ((flags as u64) << 32);
    syscall6(
        nr::MOUNT,
        source.as_ptr() as usize,
        source.len(),
        target.as_ptr() as usize,
        target.len(),
        fstype.as_ptr() as usize,
        fstype_len_and_flags as usize,
    ) as i32
}

/// umount - Unmount a filesystem
///
/// # Arguments
/// * `target` - Mount point to unmount
///
/// # Returns
/// 0 on success, negative errno on error
pub fn umount(target: &str) -> i32 {
    syscall3(
        nr::UMOUNT,
        target.as_ptr() as usize,
        target.len(),
        0, // flags
    ) as i32
}

/// umount2 - Unmount a filesystem with flags
///
/// # Arguments
/// * `target` - Mount point to unmount
/// * `flags` - Unmount flags (MNT_FORCE, MNT_DETACH, etc.)
///
/// # Returns
/// 0 on success, negative errno on error
pub fn umount2(target: &str, flags: u32) -> i32 {
    syscall3(
        nr::UMOUNT,
        target.as_ptr() as usize,
        target.len(),
        flags as usize,
    ) as i32
}

/// pivot_root - Change the root filesystem
///
/// Makes the filesystem at `new_root` the new `/` and moves the
/// old root to `put_old`.
///
/// # Arguments
/// * `new_root` - Path to new root (must be a mount point)
/// * `put_old` - Where to place old root (must be under new_root)
///
/// # Returns
/// 0 on success, negative errno on error
pub fn pivot_root(new_root: &str, put_old: &str) -> i32 {
    syscall4(
        nr::PIVOT_ROOT,
        new_root.as_ptr() as usize,
        new_root.len(),
        put_old.as_ptr() as usize,
        put_old.len(),
    ) as i32
}

/// mount_move - Move a mount point to a new location
///
/// Equivalent to `mount(source, target, "", MS_MOVE, null)`.
///
/// # Arguments
/// * `source` - Current mount point path
/// * `target` - New mount point path
///
/// # Returns
/// 0 on success, negative errno on error
pub fn mount_move(source: &str, target: &str) -> i32 {
    mount(source, target, "", mount_flags::MS_MOVE, core::ptr::null())
}
