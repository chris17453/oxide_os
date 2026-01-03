# EFFLUX Async I/O Specification

**Version:** 1.0  
**Status:** Draft  
**License:** MIT  

---

## 0) Overview

EFFLUX provides Linux-compatible async I/O APIs for high-performance applications.

**Supported APIs:**
- epoll (primary)
- poll
- select
- eventfd
- signalfd
- timerfd
- inotify (file watching)

---

## 1) epoll Interface

### 1.1 Syscalls

```rust
pub fn sys_epoll_create1(flags: u32) -> Result<i32>;
pub fn sys_epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut EpollEvent) -> Result<()>;
pub fn sys_epoll_wait(epfd: i32, events: *mut EpollEvent, max: i32, timeout: i32) -> Result<i32>;
pub fn sys_epoll_pwait(epfd: i32, events: *mut EpollEvent, max: i32, timeout: i32, 
                       sigmask: *const SigSet) -> Result<i32>;
```

### 1.2 Data Structures

```rust
#[repr(C)]
pub struct EpollEvent {
    pub events: u32,
    pub data: u64,
}

bitflags! {
    pub struct EpollFlags: u32 {
        const IN        = 0x001;    // Available for read
        const OUT       = 0x004;    // Available for write
        const ERR       = 0x008;    // Error condition
        const HUP       = 0x010;    // Hang up
        const RDHUP     = 0x2000;   // Peer closed connection
        const PRI       = 0x002;    // Priority data
        const ET        = 1 << 31;  // Edge-triggered
        const ONESHOT   = 1 << 30;  // One-shot mode
        const WAKEUP    = 1 << 29;  // Wake on event
        const EXCLUSIVE = 1 << 28;  // Exclusive wake
    }
}

pub enum EpollOp {
    Add = 1,
    Del = 2,
    Mod = 3,
}
```

### 1.3 Kernel Implementation

```rust
pub struct EpollInstance {
    pub fd: i32,
    pub interests: BTreeMap<i32, EpollInterest>,
    pub ready_list: VecDeque<EpollEvent>,
    pub waiters: WaitQueue,
    pub flags: u32,
}

pub struct EpollInterest {
    pub fd: i32,
    pub events: u32,
    pub data: u64,
    pub edge_triggered: bool,
    pub oneshot: bool,
    pub ready: bool,
}
```

### 1.4 Edge-Triggered vs Level-Triggered

| Mode | Behavior |
|------|----------|
| Level (default) | Returns ready while condition true |
| Edge (EPOLLET) | Returns ready only on state change |

---

## 2) eventfd

### 2.1 Syscalls

```rust
pub fn sys_eventfd(initval: u32, flags: u32) -> Result<i32>;
```

### 2.2 Behavior

- 8-byte counter
- write() adds to counter
- read() returns counter, resets to 0 (or decrements by 1 with EFD_SEMAPHORE)
- Blocks when counter is 0 (unless O_NONBLOCK)

### 2.3 Flags

```rust
pub const EFD_CLOEXEC: u32   = 0x80000;
pub const EFD_NONBLOCK: u32  = 0x800;
pub const EFD_SEMAPHORE: u32 = 0x1;
```

---

## 3) signalfd

### 3.1 Syscalls

```rust
pub fn sys_signalfd(fd: i32, mask: *const SigSet, flags: u32) -> Result<i32>;
```

### 3.2 Behavior

- Receives signals via file descriptor
- read() returns signalfd_siginfo structures
- Must block signals with sigprocmask first

### 3.3 Data Structure

```rust
#[repr(C)]
pub struct SignalfdSiginfo {
    pub signo: u32,
    pub errno: i32,
    pub code: i32,
    pub pid: u32,
    pub uid: u32,
    pub fd: i32,
    pub tid: u32,
    pub band: u32,
    pub overrun: u32,
    pub trapno: u32,
    pub status: i32,
    pub int_val: i32,
    pub ptr: u64,
    pub utime: u64,
    pub stime: u64,
    pub addr: u64,
    pub addr_lsb: u16,
    pub pad: [u8; 46],
}
```

---

## 4) timerfd

### 4.1 Syscalls

```rust
pub fn sys_timerfd_create(clockid: i32, flags: u32) -> Result<i32>;
pub fn sys_timerfd_settime(fd: i32, flags: u32, new: *const ItimerSpec, 
                           old: *mut ItimerSpec) -> Result<()>;
pub fn sys_timerfd_gettime(fd: i32, curr: *mut ItimerSpec) -> Result<()>;
```

### 4.2 Behavior

- Timer expiration via file descriptor
- read() returns expiration count (8 bytes)
- Supports one-shot and repeating timers

### 4.3 Data Structure

```rust
#[repr(C)]
pub struct ItimerSpec {
    pub interval: TimeSpec,  // Repeat interval
    pub value: TimeSpec,     // Initial expiration
}

#[repr(C)]
pub struct TimeSpec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}
```

---

## 5) inotify (File Watching)

### 5.1 Syscalls

```rust
pub fn sys_inotify_init() -> Result<i32>;
pub fn sys_inotify_init1(flags: u32) -> Result<i32>;
pub fn sys_inotify_add_watch(fd: i32, path: *const u8, mask: u32) -> Result<i32>;
pub fn sys_inotify_rm_watch(fd: i32, wd: i32) -> Result<()>;
```

### 5.2 Events

```rust
bitflags! {
    pub struct InotifyMask: u32 {
        const ACCESS        = 0x00000001;  // File accessed
        const MODIFY        = 0x00000002;  // File modified
        const ATTRIB        = 0x00000004;  // Metadata changed
        const CLOSE_WRITE   = 0x00000008;  // Writable file closed
        const CLOSE_NOWRITE = 0x00000010;  // Read-only file closed
        const OPEN          = 0x00000020;  // File opened
        const MOVED_FROM    = 0x00000040;  // File moved from
        const MOVED_TO      = 0x00000080;  // File moved to
        const CREATE        = 0x00000100;  // File created
        const DELETE        = 0x00000200;  // File deleted
        const DELETE_SELF   = 0x00000400;  // Watched item deleted
        const MOVE_SELF     = 0x00000800;  // Watched item moved
        const UNMOUNT       = 0x00002000;  // Filesystem unmounted
        const Q_OVERFLOW    = 0x00004000;  // Event queue overflowed
        const IGNORED       = 0x00008000;  // Watch removed
        const ONLYDIR       = 0x01000000;  // Only watch if directory
        const DONT_FOLLOW   = 0x02000000;  // Don't follow symlinks
        const MASK_ADD      = 0x20000000;  // Add to existing mask
        const ISDIR         = 0x40000000;  // Event is for directory
        const ONESHOT       = 0x80000000;  // One-shot watch
    }
}
```

### 5.3 Event Structure

```rust
#[repr(C)]
pub struct InotifyEvent {
    pub wd: i32,        // Watch descriptor
    pub mask: u32,      // Event mask
    pub cookie: u32,    // Cookie for rename correlation
    pub len: u32,       // Length of name
    // Followed by null-terminated name
}
```

---

## 6) poll and select

### 6.1 poll

```rust
pub fn sys_poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> Result<i32>;
pub fn sys_ppoll(fds: *mut PollFd, nfds: u64, timeout: *const TimeSpec,
                 sigmask: *const SigSet) -> Result<i32>;

#[repr(C)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}
```

### 6.2 select

```rust
pub fn sys_select(nfds: i32, readfds: *mut FdSet, writefds: *mut FdSet,
                  exceptfds: *mut FdSet, timeout: *mut TimeVal) -> Result<i32>;
pub fn sys_pselect(nfds: i32, readfds: *mut FdSet, writefds: *mut FdSet,
                   exceptfds: *mut FdSet, timeout: *const TimeSpec,
                   sigmask: *const SigSet) -> Result<i32>;
```

---

## 7) Zero-Copy I/O

### 7.1 sendfile

```rust
pub fn sys_sendfile(out_fd: i32, in_fd: i32, offset: *mut i64, count: usize) -> Result<isize>;
```

Copies data between file descriptors in kernel space.

### 7.2 splice

```rust
pub fn sys_splice(fd_in: i32, off_in: *mut i64, fd_out: i32, off_out: *mut i64,
                  len: usize, flags: u32) -> Result<isize>;
pub fn sys_tee(fd_in: i32, fd_out: i32, len: usize, flags: u32) -> Result<isize>;
pub fn sys_vmsplice(fd: i32, iov: *const IoVec, nr_segs: usize, flags: u32) -> Result<isize>;
```

### 7.3 copy_file_range

```rust
pub fn sys_copy_file_range(fd_in: i32, off_in: *mut i64, fd_out: i32, off_out: *mut i64,
                           len: usize, flags: u32) -> Result<isize>;
```

---

## 8) Kernel Implementation

### 8.1 Wait Queue

```rust
pub struct WaitQueue {
    waiters: SpinLock<VecDeque<WaitEntry>>,
}

pub struct WaitEntry {
    thread: Arc<Thread>,
    woken: AtomicBool,
    exclusive: bool,
}

impl WaitQueue {
    pub fn wait(&self, timeout: Option<Duration>) -> Result<()>;
    pub fn wake_one(&self);
    pub fn wake_all(&self);
}
```

### 8.2 File Readiness

Each file type implements readiness checking:

```rust
pub trait FileOps {
    fn poll(&self, events: u32) -> u32;  // Returns ready events
    fn register_wait(&self, wq: &WaitQueue);
    fn unregister_wait(&self, wq: &WaitQueue);
}
```

### 8.3 epoll Wake Path

1. File becomes ready (e.g., data arrives)
2. File wakes its wait queue
3. epoll checks if file matches any interest
4. If match, adds to ready list
5. Wakes epoll_wait caller

---

## 9) Exit Criteria

- [ ] epoll works with sockets, pipes, files
- [ ] eventfd works for thread signaling
- [ ] signalfd delivers signals correctly
- [ ] timerfd fires accurately
- [ ] inotify detects file changes
- [ ] sendfile achieves zero-copy
- [ ] Benchmarks comparable to Linux

---

*End of EFFLUX Async I/O Specification*
