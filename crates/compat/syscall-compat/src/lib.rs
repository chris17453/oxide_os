//! Syscall Compatibility Layer for EFFLUX OS
//!
//! Provides syscall translation for running foreign binaries.

#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use spin::RwLock;

/// Syscall argument conversion
pub struct SyscallArgs {
    /// Arguments (up to 6)
    pub args: [u64; 6],
}

impl SyscallArgs {
    /// Create new args
    pub fn new(args: [u64; 6]) -> Self {
        SyscallArgs { args }
    }

    /// Get argument by index
    pub fn get(&self, index: usize) -> u64 {
        if index < 6 {
            self.args[index]
        } else {
            0
        }
    }

    /// Set argument by index
    pub fn set(&mut self, index: usize, value: u64) {
        if index < 6 {
            self.args[index] = value;
        }
    }
}

/// Syscall result
pub struct SyscallResult {
    /// Return value
    pub value: i64,
    /// Error occurred
    pub is_error: bool,
}

impl SyscallResult {
    /// Success result
    pub fn ok(value: i64) -> Self {
        SyscallResult {
            value,
            is_error: false,
        }
    }

    /// Error result
    pub fn err(errno: i64) -> Self {
        SyscallResult {
            value: -errno,
            is_error: true,
        }
    }
}

/// Syscall handler function type
pub type SyscallHandler = fn(&mut SyscallArgs) -> SyscallResult;

/// Linux x86_64 syscall numbers
pub mod linux {
    pub const READ: u64 = 0;
    pub const WRITE: u64 = 1;
    pub const OPEN: u64 = 2;
    pub const CLOSE: u64 = 3;
    pub const STAT: u64 = 4;
    pub const FSTAT: u64 = 5;
    pub const LSTAT: u64 = 6;
    pub const POLL: u64 = 7;
    pub const LSEEK: u64 = 8;
    pub const MMAP: u64 = 9;
    pub const MPROTECT: u64 = 10;
    pub const MUNMAP: u64 = 11;
    pub const BRK: u64 = 12;
    pub const RT_SIGACTION: u64 = 13;
    pub const RT_SIGPROCMASK: u64 = 14;
    pub const RT_SIGRETURN: u64 = 15;
    pub const IOCTL: u64 = 16;
    pub const PREAD64: u64 = 17;
    pub const PWRITE64: u64 = 18;
    pub const READV: u64 = 19;
    pub const WRITEV: u64 = 20;
    pub const ACCESS: u64 = 21;
    pub const PIPE: u64 = 22;
    pub const SELECT: u64 = 23;
    pub const SCHED_YIELD: u64 = 24;
    pub const MREMAP: u64 = 25;
    pub const MSYNC: u64 = 26;
    pub const MINCORE: u64 = 27;
    pub const MADVISE: u64 = 28;
    pub const DUP: u64 = 32;
    pub const DUP2: u64 = 33;
    pub const PAUSE: u64 = 34;
    pub const NANOSLEEP: u64 = 35;
    pub const GETITIMER: u64 = 36;
    pub const ALARM: u64 = 37;
    pub const SETITIMER: u64 = 38;
    pub const GETPID: u64 = 39;
    pub const SOCKET: u64 = 41;
    pub const CONNECT: u64 = 42;
    pub const ACCEPT: u64 = 43;
    pub const SENDTO: u64 = 44;
    pub const RECVFROM: u64 = 45;
    pub const SENDMSG: u64 = 46;
    pub const RECVMSG: u64 = 47;
    pub const SHUTDOWN: u64 = 48;
    pub const BIND: u64 = 49;
    pub const LISTEN: u64 = 50;
    pub const GETSOCKNAME: u64 = 51;
    pub const GETPEERNAME: u64 = 52;
    pub const SOCKETPAIR: u64 = 53;
    pub const CLONE: u64 = 56;
    pub const FORK: u64 = 57;
    pub const VFORK: u64 = 58;
    pub const EXECVE: u64 = 59;
    pub const EXIT: u64 = 60;
    pub const WAIT4: u64 = 61;
    pub const KILL: u64 = 62;
    pub const UNAME: u64 = 63;
    pub const FCNTL: u64 = 72;
    pub const FLOCK: u64 = 73;
    pub const FSYNC: u64 = 74;
    pub const FDATASYNC: u64 = 75;
    pub const TRUNCATE: u64 = 76;
    pub const FTRUNCATE: u64 = 77;
    pub const GETDENTS: u64 = 78;
    pub const GETCWD: u64 = 79;
    pub const CHDIR: u64 = 80;
    pub const FCHDIR: u64 = 81;
    pub const RENAME: u64 = 82;
    pub const MKDIR: u64 = 83;
    pub const RMDIR: u64 = 84;
    pub const CREAT: u64 = 85;
    pub const LINK: u64 = 86;
    pub const UNLINK: u64 = 87;
    pub const SYMLINK: u64 = 88;
    pub const READLINK: u64 = 89;
    pub const CHMOD: u64 = 90;
    pub const FCHMOD: u64 = 91;
    pub const CHOWN: u64 = 92;
    pub const FCHOWN: u64 = 93;
    pub const LCHOWN: u64 = 94;
    pub const UMASK: u64 = 95;
    pub const GETTIMEOFDAY: u64 = 96;
    pub const GETRLIMIT: u64 = 97;
    pub const GETRUSAGE: u64 = 98;
    pub const SYSINFO: u64 = 99;
    pub const TIMES: u64 = 100;
    pub const GETUID: u64 = 102;
    pub const GETGID: u64 = 104;
    pub const SETUID: u64 = 105;
    pub const SETGID: u64 = 106;
    pub const GETEUID: u64 = 107;
    pub const GETEGID: u64 = 108;
    pub const GETPPID: u64 = 110;
    pub const GETPGRP: u64 = 111;
    pub const SETSID: u64 = 112;
    pub const SETREUID: u64 = 113;
    pub const SETREGID: u64 = 114;
    pub const GETGROUPS: u64 = 115;
    pub const SETGROUPS: u64 = 116;
    pub const SETRESUID: u64 = 117;
    pub const GETRESUID: u64 = 118;
    pub const SETRESGID: u64 = 119;
    pub const GETRESGID: u64 = 120;
    pub const ARCH_PRCTL: u64 = 158;
    pub const GETTID: u64 = 186;
    pub const FUTEX: u64 = 202;
    pub const SET_TID_ADDRESS: u64 = 218;
    pub const CLOCK_GETTIME: u64 = 228;
    pub const CLOCK_GETRES: u64 = 229;
    pub const CLOCK_NANOSLEEP: u64 = 230;
    pub const EXIT_GROUP: u64 = 231;
    pub const OPENAT: u64 = 257;
    pub const MKDIRAT: u64 = 258;
    pub const MKNODAT: u64 = 259;
    pub const FCHOWNAT: u64 = 260;
    pub const NEWFSTATAT: u64 = 262;
    pub const UNLINKAT: u64 = 263;
    pub const RENAMEAT: u64 = 264;
    pub const LINKAT: u64 = 265;
    pub const SYMLINKAT: u64 = 266;
    pub const READLINKAT: u64 = 267;
    pub const FCHMODAT: u64 = 268;
    pub const FACCESSAT: u64 = 269;
    pub const PSELECT6: u64 = 270;
    pub const PPOLL: u64 = 271;
    pub const SET_ROBUST_LIST: u64 = 273;
    pub const GET_ROBUST_LIST: u64 = 274;
    pub const ACCEPT4: u64 = 288;
    pub const DUP3: u64 = 292;
    pub const PIPE2: u64 = 293;
    pub const PRLIMIT64: u64 = 302;
    pub const GETRANDOM: u64 = 318;
}

/// Linux errno values
pub mod errno {
    pub const EPERM: i64 = 1;
    pub const ENOENT: i64 = 2;
    pub const ESRCH: i64 = 3;
    pub const EINTR: i64 = 4;
    pub const EIO: i64 = 5;
    pub const ENXIO: i64 = 6;
    pub const E2BIG: i64 = 7;
    pub const ENOEXEC: i64 = 8;
    pub const EBADF: i64 = 9;
    pub const ECHILD: i64 = 10;
    pub const EAGAIN: i64 = 11;
    pub const ENOMEM: i64 = 12;
    pub const EACCES: i64 = 13;
    pub const EFAULT: i64 = 14;
    pub const EBUSY: i64 = 16;
    pub const EEXIST: i64 = 17;
    pub const EXDEV: i64 = 18;
    pub const ENODEV: i64 = 19;
    pub const ENOTDIR: i64 = 20;
    pub const EISDIR: i64 = 21;
    pub const EINVAL: i64 = 22;
    pub const ENFILE: i64 = 23;
    pub const EMFILE: i64 = 24;
    pub const ENOTTY: i64 = 25;
    pub const EFBIG: i64 = 27;
    pub const ENOSPC: i64 = 28;
    pub const ESPIPE: i64 = 29;
    pub const EROFS: i64 = 30;
    pub const EMLINK: i64 = 31;
    pub const EPIPE: i64 = 32;
    pub const EDOM: i64 = 33;
    pub const ERANGE: i64 = 34;
    pub const ENOSYS: i64 = 38;
    pub const ENOTEMPTY: i64 = 39;
    pub const ELOOP: i64 = 40;
    pub const EWOULDBLOCK: i64 = EAGAIN;
}

/// Linux compatibility layer
pub struct LinuxCompat {
    /// Syscall handlers
    handlers: RwLock<BTreeMap<u64, SyscallHandler>>,
}

impl LinuxCompat {
    /// Create new compatibility layer
    pub fn new() -> Self {
        LinuxCompat {
            handlers: RwLock::new(BTreeMap::new()),
        }
    }

    /// Register syscall handler
    pub fn register_handler(&self, syscall: u64, handler: SyscallHandler) {
        self.handlers.write().insert(syscall, handler);
    }

    /// Handle syscall
    pub fn handle_syscall(&self, syscall: u64, args: &mut SyscallArgs) -> SyscallResult {
        if let Some(handler) = self.handlers.read().get(&syscall) {
            handler(args)
        } else {
            // Syscall not implemented
            SyscallResult::err(errno::ENOSYS)
        }
    }
}

impl Default for LinuxCompat {
    fn default() -> Self {
        Self::new()
    }
}

/// Open flags translation
pub mod open_flags {
    /// Linux open flags
    pub mod linux {
        pub const O_RDONLY: i32 = 0;
        pub const O_WRONLY: i32 = 1;
        pub const O_RDWR: i32 = 2;
        pub const O_CREAT: i32 = 0o100;
        pub const O_EXCL: i32 = 0o200;
        pub const O_NOCTTY: i32 = 0o400;
        pub const O_TRUNC: i32 = 0o1000;
        pub const O_APPEND: i32 = 0o2000;
        pub const O_NONBLOCK: i32 = 0o4000;
        pub const O_DIRECTORY: i32 = 0o200000;
        pub const O_CLOEXEC: i32 = 0o2000000;
    }

    /// Translate Linux open flags to EFFLUX
    pub fn translate_open_flags(linux_flags: i32) -> i32 {
        // Direct mapping for now (would translate to EFFLUX flags)
        linux_flags
    }
}

/// Memory protection flags translation
pub mod mmap_flags {
    /// Linux mmap flags
    pub mod linux {
        pub const PROT_NONE: i32 = 0;
        pub const PROT_READ: i32 = 1;
        pub const PROT_WRITE: i32 = 2;
        pub const PROT_EXEC: i32 = 4;

        pub const MAP_SHARED: i32 = 0x01;
        pub const MAP_PRIVATE: i32 = 0x02;
        pub const MAP_FIXED: i32 = 0x10;
        pub const MAP_ANONYMOUS: i32 = 0x20;
    }

    /// Translate Linux mmap protection
    pub fn translate_prot(linux_prot: i32) -> i32 {
        linux_prot // Direct mapping
    }

    /// Translate Linux mmap flags
    pub fn translate_flags(linux_flags: i32) -> i32 {
        linux_flags // Direct mapping
    }
}

/// Signal number translation
pub mod signals {
    pub const SIGHUP: i32 = 1;
    pub const SIGINT: i32 = 2;
    pub const SIGQUIT: i32 = 3;
    pub const SIGILL: i32 = 4;
    pub const SIGTRAP: i32 = 5;
    pub const SIGABRT: i32 = 6;
    pub const SIGBUS: i32 = 7;
    pub const SIGFPE: i32 = 8;
    pub const SIGKILL: i32 = 9;
    pub const SIGUSR1: i32 = 10;
    pub const SIGSEGV: i32 = 11;
    pub const SIGUSR2: i32 = 12;
    pub const SIGPIPE: i32 = 13;
    pub const SIGALRM: i32 = 14;
    pub const SIGTERM: i32 = 15;
    pub const SIGCHLD: i32 = 17;
    pub const SIGCONT: i32 = 18;
    pub const SIGSTOP: i32 = 19;
    pub const SIGTSTP: i32 = 20;
    pub const SIGTTIN: i32 = 21;
    pub const SIGTTOU: i32 = 22;

    /// Translate Linux signal to EFFLUX
    pub fn translate_signal(linux_sig: i32) -> i32 {
        linux_sig // Direct mapping for most signals
    }
}
