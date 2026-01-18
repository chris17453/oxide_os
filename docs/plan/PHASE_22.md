# Phase 22: Async I/O

**Stage:** 5 - Polish
**Status:** Not Started
**Dependencies:** Phase 12 (Networking)

---

## Goal

Implement efficient async I/O for high-performance applications.

---

## Deliverables

| Item | Status |
|------|--------|
| epoll-like interface | [ ] |
| Async file I/O | [ ] |
| Async network I/O | [ ] |
| io_uring-style submission | [ ] |
| Event loop support | [ ] |
| Timer events | [ ] |

---

## Architecture Status

| Arch | epoll | io_uring | File AIO | Net AIO | Done |
|------|-------|----------|----------|---------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Async I/O Architecture

```
┌─────────────────────────────────────────────────────┐
│                  Application                         │
│              (async/await, tokio)                    │
└──────────────────────┬──────────────────────────────┘
                       │
          ┌────────────┴────────────┐
          ▼                         ▼
┌──────────────────┐      ┌──────────────────┐
│      epoll       │      │    io_uring      │
│  (event-based)   │      │ (submission/cq)  │
└────────┬─────────┘      └────────┬─────────┘
         │                         │
         └────────────┬────────────┘
                      ▼
┌─────────────────────────────────────────────────────┐
│                   Kernel                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
│  │   Sockets   │  │    Files    │  │   Timers    │ │
│  └─────────────┘  └─────────────┘  └─────────────┘ │
└─────────────────────────────────────────────────────┘
```

---

## epoll Interface

```rust
// Syscalls
pub fn epoll_create(flags: i32) -> Result<Fd>;
pub fn epoll_ctl(epfd: Fd, op: i32, fd: Fd, event: &EpollEvent) -> Result<()>;
pub fn epoll_wait(epfd: Fd, events: &mut [EpollEvent], timeout: i32) -> Result<usize>;

// Event structure
#[repr(C)]
pub struct EpollEvent {
    pub events: u32,     // EPOLLIN, EPOLLOUT, etc.
    pub data: u64,       // User data
}

// Event flags
pub const EPOLLIN: u32 = 0x001;      // Read ready
pub const EPOLLOUT: u32 = 0x004;     // Write ready
pub const EPOLLERR: u32 = 0x008;     // Error
pub const EPOLLHUP: u32 = 0x010;     // Hangup
pub const EPOLLET: u32 = 0x80000000; // Edge triggered
pub const EPOLLONESHOT: u32 = 0x40000000;

// Operations
pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;
```

---

## io_uring Interface

```rust
// io_uring setup
pub fn io_uring_setup(entries: u32, params: &mut IoUringParams) -> Result<Fd>;
pub fn io_uring_register(fd: Fd, opcode: u32, arg: *const c_void, nr_args: u32) -> Result<i32>;
pub fn io_uring_enter(fd: Fd, to_submit: u32, min_complete: u32, flags: u32) -> Result<i32>;

// Submission Queue Entry
#[repr(C)]
pub struct IoUringSqe {
    pub opcode: u8,       // Operation code
    pub flags: u8,        // Flags
    pub ioprio: u16,      // I/O priority
    pub fd: i32,          // File descriptor
    pub off: u64,         // Offset
    pub addr: u64,        // Buffer address
    pub len: u32,         // Length
    pub op_flags: u32,    // Operation-specific flags
    pub user_data: u64,   // User data (returned in CQE)
    pub buf_index: u16,   // Buffer index for fixed buffers
    pub personality: u16, // Credentials to use
    pub splice_fd_in: i32,
    pub __pad2: [u64; 2],
}

// Completion Queue Entry
#[repr(C)]
pub struct IoUringCqe {
    pub user_data: u64,   // From SQE
    pub res: i32,         // Result (bytes or -errno)
    pub flags: u32,       // Flags
}

// Opcodes
pub const IORING_OP_NOP: u8 = 0;
pub const IORING_OP_READV: u8 = 1;
pub const IORING_OP_WRITEV: u8 = 2;
pub const IORING_OP_READ: u8 = 22;
pub const IORING_OP_WRITE: u8 = 23;
pub const IORING_OP_ACCEPT: u8 = 13;
pub const IORING_OP_CONNECT: u8 = 16;
pub const IORING_OP_SEND: u8 = 26;
pub const IORING_OP_RECV: u8 = 27;
pub const IORING_OP_TIMEOUT: u8 = 11;
```

---

## io_uring Memory Layout

```
┌─────────────────────────────────────────────────────┐
│               Submission Queue (SQ)                  │
│  ┌─────────────────────────────────────────────┐   │
│  │ Ring: [head] [tail] [mask] [entries] [flags]│   │
│  │ Array: [sqe_idx_0] [sqe_idx_1] ...          │   │
│  └─────────────────────────────────────────────┘   │
│                                                     │
│  SQE Array: [sqe_0] [sqe_1] [sqe_2] ...            │
│  (Indexed by SQ array entries)                      │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│              Completion Queue (CQ)                   │
│  ┌─────────────────────────────────────────────┐   │
│  │ Ring: [head] [tail] [mask] [entries] [flags]│   │
│  └─────────────────────────────────────────────┘   │
│                                                     │
│  CQE Array: [cqe_0] [cqe_1] [cqe_2] ...            │
│  (Directly indexed by CQ head/tail)                 │
└─────────────────────────────────────────────────────┘

Producer/Consumer:
- SQ: User produces (tail++), kernel consumes (head++)
- CQ: Kernel produces (tail++), user consumes (head++)
```

---

## Kernel Implementation

```rust
// epoll instance
pub struct EpollInstance {
    /// Monitored file descriptors
    interest_list: BTreeMap<Fd, EpollInterest>,

    /// Ready list (fds with pending events)
    ready_list: VecDeque<(Fd, u32)>,

    /// Waiters blocked on epoll_wait
    waiters: WaitQueue,
}

// io_uring instance
pub struct IoUringInstance {
    /// Submission queue
    sq: SubmissionQueue,

    /// Completion queue
    cq: CompletionQueue,

    /// SQE array
    sqes: Vec<IoUringSqe>,

    /// Pending operations
    pending: VecDeque<PendingOp>,

    /// Fixed buffers (optional)
    fixed_buffers: Option<Vec<FixedBuffer>>,
}
```

---

## Key Files

```
crates/async/efflux-epoll/src/
├── lib.rs
├── instance.rs        # epoll instance
├── interest.rs        # Interest management
└── ready.rs           # Ready list

crates/async/efflux-iouring/src/
├── lib.rs
├── instance.rs        # io_uring instance
├── submit.rs          # Submission handling
├── complete.rs        # Completion handling
└── ops.rs             # Operation implementations

kernel/src/async/
├── mod.rs
├── epoll.rs           # epoll syscalls
└── iouring.rs         # io_uring syscalls
```

---

## Syscalls

| Number | Name | Args | Return |
|--------|------|------|--------|
| 100 | sys_epoll_create1 | flags | epfd or -errno |
| 101 | sys_epoll_ctl | epfd, op, fd, event | 0 or -errno |
| 102 | sys_epoll_wait | epfd, events, maxevents, timeout | count or -errno |
| 103 | sys_epoll_pwait | epfd, events, maxevents, timeout, sigmask | count or -errno |
| 110 | sys_io_uring_setup | entries, params | fd or -errno |
| 111 | sys_io_uring_enter | fd, to_submit, min_complete, flags, sig | count or -errno |
| 112 | sys_io_uring_register | fd, opcode, arg, nr_args | 0 or -errno |

---

## Exit Criteria

- [ ] epoll_create/ctl/wait functional
- [ ] Edge-triggered and level-triggered modes
- [ ] io_uring setup and basic ops
- [ ] Async read/write via io_uring
- [ ] Async network I/O via io_uring
- [ ] Works with Rust async runtimes
- [ ] Works on all 8 architectures

---

## Test: TCP Echo Server

```c
// epoll-based echo server
int main() {
    int listen_fd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
    bind(listen_fd, ...);
    listen(listen_fd, 128);

    int epfd = epoll_create1(0);

    struct epoll_event ev = { .events = EPOLLIN, .data.fd = listen_fd };
    epoll_ctl(epfd, EPOLL_CTL_ADD, listen_fd, &ev);

    struct epoll_event events[64];
    while (1) {
        int n = epoll_wait(epfd, events, 64, -1);
        for (int i = 0; i < n; i++) {
            if (events[i].data.fd == listen_fd) {
                int client = accept4(listen_fd, NULL, NULL, SOCK_NONBLOCK);
                ev.events = EPOLLIN | EPOLLET;
                ev.data.fd = client;
                epoll_ctl(epfd, EPOLL_CTL_ADD, client, &ev);
            } else {
                char buf[4096];
                ssize_t len = read(events[i].data.fd, buf, sizeof(buf));
                if (len > 0) {
                    write(events[i].data.fd, buf, len);
                } else {
                    close(events[i].data.fd);
                }
            }
        }
    }
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 22 of EFFLUX Implementation*
