# OXIDE IPC Specification

**Version:** 1.0  
**Status:** Draft  
**License:** MIT  

---

## 0) Overview

OXIDE supports standard POSIX IPC mechanisms:

- Pipes (anonymous and named)
- Unix domain sockets
- Shared memory (POSIX and SysV)
- Semaphores
- Message queues
- Signals (covered in separate spec)

---

## 1) Pipes

### 1.1 Anonymous Pipes

```rust
pub fn sys_pipe(pipefd: *mut [i32; 2]) -> Result<()>;
pub fn sys_pipe2(pipefd: *mut [i32; 2], flags: i32) -> Result<()>;
```

Flags: `O_CLOEXEC`, `O_NONBLOCK`

### 1.2 Implementation

```rust
pub struct Pipe {
    buffer: RingBuffer<u8>,
    capacity: usize,
    readers: AtomicU32,
    writers: AtomicU32,
    read_waiters: WaitQueue,
    write_waiters: WaitQueue,
}

impl Pipe {
    pub fn read(&self, buf: &mut [u8], nonblock: bool) -> Result<usize> {
        loop {
            if let Some(n) = self.buffer.read(buf) {
                self.write_waiters.wake_one();
                return Ok(n);
            }
            if self.writers.load(Ordering::Acquire) == 0 {
                return Ok(0);  // EOF
            }
            if nonblock {
                return Err(Error::WouldBlock);
            }
            self.read_waiters.wait();
        }
    }
    
    pub fn write(&self, buf: &[u8], nonblock: bool) -> Result<usize> {
        loop {
            if self.readers.load(Ordering::Acquire) == 0 {
                send_signal(current_thread(), SIGPIPE);
                return Err(Error::BrokenPipe);
            }
            if let Some(n) = self.buffer.write(buf) {
                self.read_waiters.wake_one();
                return Ok(n);
            }
            if nonblock {
                return Err(Error::WouldBlock);
            }
            self.write_waiters.wait();
        }
    }
}
```

### 1.3 Named Pipes (FIFOs)

```rust
pub fn sys_mkfifo(path: *const u8, mode: u32) -> Result<()>;
pub fn sys_mknod(path: *const u8, mode: u32, dev: u64) -> Result<()>;
```

---

## 2) Unix Domain Sockets

### 2.1 Syscalls

```rust
pub fn sys_socket(AF_UNIX, type_: i32, protocol: i32) -> Result<i32>;
pub fn sys_socketpair(AF_UNIX, type_: i32, protocol: i32, sv: *mut [i32; 2]) -> Result<()>;
pub fn sys_bind(sockfd: i32, addr: *const SockaddrUn, addrlen: u32) -> Result<()>;
pub fn sys_listen(sockfd: i32, backlog: i32) -> Result<()>;
pub fn sys_accept(sockfd: i32, addr: *mut SockaddrUn, addrlen: *mut u32) -> Result<i32>;
pub fn sys_connect(sockfd: i32, addr: *const SockaddrUn, addrlen: u32) -> Result<()>;
```

### 2.2 Socket Types

- `SOCK_STREAM` — Connection-oriented, reliable
- `SOCK_DGRAM` — Connectionless, message-oriented
- `SOCK_SEQPACKET` — Connection-oriented, message-oriented

### 2.3 Address

```rust
#[repr(C)]
pub struct SockaddrUn {
    pub sun_family: u16,        // AF_UNIX
    pub sun_path: [u8; 108],    // Pathname
}
```

Abstract sockets: `sun_path[0] = 0`

### 2.4 Ancillary Data

Pass file descriptors between processes:

```rust
pub fn sys_sendmsg(sockfd: i32, msg: *const Msghdr, flags: i32) -> Result<isize>;
pub fn sys_recvmsg(sockfd: i32, msg: *mut Msghdr, flags: i32) -> Result<isize>;

// Control message for SCM_RIGHTS
pub struct CmsgHdr {
    pub len: usize,
    pub level: i32,     // SOL_SOCKET
    pub type_: i32,     // SCM_RIGHTS
    // Followed by file descriptors
}
```

---

## 3) Shared Memory

### 3.1 POSIX Shared Memory

```rust
pub fn sys_shm_open(name: *const u8, oflag: i32, mode: u32) -> Result<i32>;
pub fn sys_shm_unlink(name: *const u8) -> Result<()>;
```

Then use `ftruncate()` and `mmap()`.

### 3.2 SysV Shared Memory

```rust
pub fn sys_shmget(key: i32, size: usize, shmflg: i32) -> Result<i32>;
pub fn sys_shmat(shmid: i32, shmaddr: *const u8, shmflg: i32) -> Result<*mut u8>;
pub fn sys_shmdt(shmaddr: *const u8) -> Result<()>;
pub fn sys_shmctl(shmid: i32, cmd: i32, buf: *mut ShmidDs) -> Result<i32>;
```

### 3.3 Implementation

```rust
pub struct SharedMemory {
    pub key: i32,
    pub size: usize,
    pub pages: Vec<PhysAddr>,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub attachments: AtomicU32,
}
```

---

## 4) Semaphores

### 4.1 POSIX Named Semaphores

```rust
pub fn sys_sem_open(name: *const u8, oflag: i32, mode: u32, value: u32) -> Result<*mut Sem>;
pub fn sys_sem_close(sem: *mut Sem) -> Result<()>;
pub fn sys_sem_unlink(name: *const u8) -> Result<()>;
pub fn sys_sem_wait(sem: *mut Sem) -> Result<()>;
pub fn sys_sem_trywait(sem: *mut Sem) -> Result<()>;
pub fn sys_sem_timedwait(sem: *mut Sem, abstime: *const Timespec) -> Result<()>;
pub fn sys_sem_post(sem: *mut Sem) -> Result<()>;
pub fn sys_sem_getvalue(sem: *mut Sem, sval: *mut i32) -> Result<()>;
```

### 4.2 SysV Semaphores

```rust
pub fn sys_semget(key: i32, nsems: i32, semflg: i32) -> Result<i32>;
pub fn sys_semop(semid: i32, sops: *mut Sembuf, nsops: usize) -> Result<()>;
pub fn sys_semctl(semid: i32, semnum: i32, cmd: i32, arg: SemunArg) -> Result<i32>;
```

### 4.3 Futex (Fast Userspace Mutex)

```rust
pub fn sys_futex(uaddr: *mut u32, op: i32, val: u32, timeout: *const Timespec,
                 uaddr2: *mut u32, val3: u32) -> Result<i32>;
```

Operations: `FUTEX_WAIT`, `FUTEX_WAKE`, `FUTEX_REQUEUE`, etc.

---

## 5) Message Queues

### 5.1 POSIX Message Queues

```rust
pub fn sys_mq_open(name: *const u8, oflag: i32, mode: u32, attr: *mut MqAttr) -> Result<i32>;
pub fn sys_mq_close(mqdes: i32) -> Result<()>;
pub fn sys_mq_unlink(name: *const u8) -> Result<()>;
pub fn sys_mq_send(mqdes: i32, msg_ptr: *const u8, msg_len: usize, msg_prio: u32) -> Result<()>;
pub fn sys_mq_receive(mqdes: i32, msg_ptr: *mut u8, msg_len: usize, msg_prio: *mut u32) -> Result<isize>;
pub fn sys_mq_getattr(mqdes: i32, attr: *mut MqAttr) -> Result<()>;
pub fn sys_mq_setattr(mqdes: i32, newattr: *const MqAttr, oldattr: *mut MqAttr) -> Result<()>;
pub fn sys_mq_notify(mqdes: i32, notification: *const SigEvent) -> Result<()>;
```

### 5.2 SysV Message Queues

```rust
pub fn sys_msgget(key: i32, msgflg: i32) -> Result<i32>;
pub fn sys_msgsnd(msqid: i32, msgp: *const u8, msgsz: usize, msgflg: i32) -> Result<()>;
pub fn sys_msgrcv(msqid: i32, msgp: *mut u8, msgsz: usize, msgtyp: i64, msgflg: i32) -> Result<isize>;
pub fn sys_msgctl(msqid: i32, cmd: i32, buf: *mut MsqidDs) -> Result<i32>;
```

---

## 6) eventfd

```rust
pub fn sys_eventfd(initval: u32, flags: u32) -> Result<i32>;
```

Fast userspace notification mechanism. 8-byte counter.

---

## 7) Exit Criteria

- [ ] Anonymous pipes work
- [ ] Named pipes (FIFOs) work
- [ ] Unix domain sockets (stream + dgram)
- [ ] File descriptor passing works
- [ ] POSIX shared memory works
- [ ] POSIX semaphores work
- [ ] Futex works
- [ ] Message queues work

---

*End of OXIDE IPC Specification*
