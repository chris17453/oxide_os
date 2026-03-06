//! Syscall numbers — Linux x86_64 ABI (asm/unistd_64.h).
//!
//! — SableWire: Every number here must match kernel/syscall/syscall/src/lib.rs::nr.
//! Change one without the other and enjoy creative triple-faults.

// Core I/O (0-8)
pub const READ: u64 = 0;
pub const WRITE: u64 = 1;
pub const OPEN: u64 = 2;
pub const CLOSE: u64 = 3;
pub const STAT: u64 = 4;
pub const FSTAT: u64 = 5;
pub const LSTAT: u64 = 6;
pub const POLL: u64 = 7;
pub const LSEEK: u64 = 8;

// Memory (9-12, 25, 28)
pub const MMAP: u64 = 9;
pub const MPROTECT: u64 = 10;
pub const MUNMAP: u64 = 11;
pub const BRK: u64 = 12;
pub const MREMAP: u64 = 25;
pub const MADVISE: u64 = 28;

// Signals (13-15, 34-38, 62, 127-131)
pub const SIGACTION: u64 = 13;
pub const SIGPROCMASK: u64 = 14;
pub const SIGRETURN: u64 = 15;
pub const PAUSE: u64 = 34;
pub const NANOSLEEP: u64 = 35;
pub const GETITIMER: u64 = 36;
pub const ALARM: u64 = 37;
pub const SETITIMER: u64 = 38;
pub const KILL: u64 = 62;
pub const SIGPENDING: u64 = 127;
pub const SIGSUSPEND: u64 = 130;
pub const SIGALTSTACK: u64 = 131;

// I/O operations (16-20)
pub const IOCTL: u64 = 16;
pub const PREAD64: u64 = 17;
pub const PWRITE64: u64 = 18;
pub const READV: u64 = 19;
pub const WRITEV: u64 = 20;

// File access/permissions (21-22, 32-33, 72-93)
pub const ACCESS: u64 = 21;
pub const PIPE: u64 = 22;
pub const SELECT: u64 = 23;
pub const DUP: u64 = 32;
pub const DUP2: u64 = 33;
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
pub const LINK: u64 = 86;
pub const UNLINK: u64 = 87;
pub const SYMLINK: u64 = 88;
pub const READLINK: u64 = 89;
pub const CHMOD: u64 = 90;
pub const FCHMOD: u64 = 91;
pub const CHOWN: u64 = 92;
pub const FCHOWN: u64 = 93;

// Scheduler (24)
pub const SCHED_YIELD: u64 = 24;

// Sendfile (40)
pub const SENDFILE: u64 = 40;

// Process identity (39, 95, 102-126, 186)
pub const GETPID: u64 = 39;
pub const UMASK: u64 = 95;
pub const GETUID: u64 = 102;
pub const GETGID: u64 = 104;
pub const SETUID: u64 = 105;
pub const SETGID: u64 = 106;
pub const GETEUID: u64 = 107;
pub const GETEGID: u64 = 108;
pub const SETPGID: u64 = 109;
pub const GETPPID: u64 = 110;
pub const SETSID: u64 = 112;
pub const SETREUID: u64 = 113;
pub const SETREGID: u64 = 114;
pub const GETGROUPS: u64 = 115;
pub const SETGROUPS: u64 = 116;
pub const SETRESUID: u64 = 117;
pub const GETRESUID: u64 = 118;
pub const SETRESGID: u64 = 119;
pub const GETRESGID: u64 = 120;
pub const GETPGID: u64 = 121;
pub const SETEUID: u64 = 113;  // alias to setreuid
pub const SETEGID: u64 = 114;  // alias to setregid
pub const GETSID: u64 = 124;
pub const CAPGET: u64 = 125;
pub const CAPSET: u64 = 126;
pub const GETTID: u64 = 186;

// Socket syscalls (41-55)
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
pub const SETSOCKOPT: u64 = 54;
pub const GETSOCKOPT: u64 = 55;

// Process creation (56-61)
pub const CLONE: u64 = 56;
pub const FORK: u64 = 57;
pub const EXEC: u64 = 58;
pub const EXECVE: u64 = 59;
pub const EXIT: u64 = 60;
pub const WAIT4: u64 = 61;
pub const WAIT: u64 = 61;
pub const WAITPID: u64 = 61;

// System info (63, 96, 98-100, 228-230)
pub const UNAME: u64 = 63;
pub const GETTIMEOFDAY: u64 = 96;
pub const GETRUSAGE: u64 = 98;
pub const TIMES: u64 = 100;
pub const CLOCK_GETTIME: u64 = 228;
pub const CLOCK_GETRES: u64 = 229;
pub const CLOCK_NANOSLEEP: u64 = 230;

// Filesystem / scheduling (137-176)
pub const STATFS: u64 = 137;
pub const FSTATFS: u64 = 138;
pub const GETPRIORITY: u64 = 140;
pub const SETPRIORITY: u64 = 141;
pub const SCHED_SETPARAM: u64 = 142;
pub const SCHED_GETPARAM: u64 = 143;
pub const SCHED_SETSCHEDULER: u64 = 144;
pub const SCHED_GETSCHEDULER: u64 = 145;
pub const SCHED_GET_PRIORITY_MAX: u64 = 146;
pub const SCHED_GET_PRIORITY_MIN: u64 = 147;
pub const SCHED_RR_GET_INTERVAL: u64 = 148;
pub const MOUNT: u64 = 165;
pub const UMOUNT: u64 = 166;
pub const SETHOSTNAME: u64 = 170;

// Futex / threading (202-204, 218, 231)
pub const FUTEX: u64 = 202;
pub const SCHED_SETAFFINITY: u64 = 203;
pub const SCHED_GETAFFINITY: u64 = 204;
pub const GETDENTS64: u64 = 217;
pub const SET_TID_ADDRESS: u64 = 218;
pub const EXIT_GROUP: u64 = 231;
pub const UTIMES: u64 = 235;

// *at syscalls (257-272, 280)
pub const OPENAT: u64 = 257;
pub const MKDIRAT: u64 = 258;
pub const MKNODAT: u64 = 259;
pub const FCHOWNAT: u64 = 260;
pub const FUTIMENS: u64 = 261;
pub const UNLINKAT: u64 = 263;
pub const RENAMEAT: u64 = 264;
pub const LINKAT: u64 = 265;
pub const SYMLINKAT: u64 = 266;
pub const READLINKAT: u64 = 267;
pub const FCHMODAT: u64 = 268;
pub const FACCESSAT: u64 = 269;
pub const PSELECT6: u64 = 270;
pub const PPOLL: u64 = 271;
pub const UTIMENSAT: u64 = 280;

// Event FDs (288-293)
pub const ACCEPT4: u64 = 288;
pub const EVENTFD2: u64 = 290;
pub const EPOLL_CREATE1: u64 = 291;
pub const DUP3: u64 = 292;
pub const PIPE2: u64 = 293;

// Modern I/O (302, 318, 326, 436)
pub const PRLIMIT: u64 = 302;
pub const GETRANDOM: u64 = 318;
pub const COPY_FILE_RANGE: u64 = 326;
pub const CLOSE_RANGE: u64 = 436;

// Legacy aliases
pub const SEND: u64 = SENDTO;
pub const RECV: u64 = RECVFROM;

// OXIDE-specific (500+)
pub const SETKEYMAP: u64 = 500;
pub const GETKEYMAP: u64 = 501;
pub const NICE: u64 = 502;
pub const FW_ADD_RULE: u64 = 510;
pub const FW_DEL_RULE: u64 = 511;
pub const FW_LIST_RULES: u64 = 512;
pub const FW_SET_POLICY: u64 = 513;
pub const FW_FLUSH: u64 = 514;
pub const FW_GET_CONNTRACK: u64 = 515;
pub const NET_CONTROL: u64 = 520;

/// AT_FDCWD: use current working directory for *at syscalls
pub const AT_FDCWD: i64 = -100;
