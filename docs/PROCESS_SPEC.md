# EFFLUX Process Specification

**Version:** 1.0
**Status:** Draft
**License:** MIT

---

## 0) Overview

Processes are isolated execution environments containing one or more threads, an address space, and resources (file descriptors, signals, etc.).

---

## 1) Process States

| State | Description |
|-------|-------------|
| Created | Allocated, not yet runnable |
| Running | Has at least one running thread |
| Stopped | SIGSTOP/SIGTSTP received |
| Zombie | Exited, awaiting parent reap |

---

## 2) Process Structure

| Field | Description |
|-------|-------------|
| pid | Process ID |
| ppid | Parent PID |
| pgid | Process group ID |
| sid | Session ID |
| uid/gid | User/group credentials |
| address_space | Virtual memory mappings |
| threads | Thread list |
| fds | File descriptor table |
| cwd | Current working directory |
| signal_handlers | Signal disposition table |
| exit_code | Exit status (when zombie) |

---

## 3) Lifecycle

### Creation

1. **fork()** - Clone parent process
   - Copy address space (COW)
   - Duplicate file descriptors
   - Child gets pid=0 return, parent gets child pid

2. **exec()** - Replace process image
   - Parse ELF, load segments
   - Reset signal handlers to default
   - Close O_CLOEXEC file descriptors
   - Jump to entry point

3. **clone()** - Fine-grained creation
   - Select what to share (memory, files, signals)
   - Used for thread creation

### Termination

1. **exit(code)** - Voluntary termination
   - Clean up resources
   - Reparent children to init
   - Become zombie
   - Signal parent (SIGCHLD)

2. **wait()/waitpid()** - Reap child
   - Collect exit status
   - Free zombie process entry

---

## 4) Signals

### Standard Signals

| Signal | Default | Description |
|--------|---------|-------------|
| SIGKILL | Term | Uncatchable kill |
| SIGSTOP | Stop | Uncatchable stop |
| SIGTERM | Term | Termination request |
| SIGINT | Term | Interrupt (Ctrl+C) |
| SIGSEGV | Core | Segmentation fault |
| SIGCHLD | Ignore | Child status change |
| SIGCONT | Cont | Continue if stopped |

### Signal Handling

- **SIG_DFL** - Default action
- **SIG_IGN** - Ignore signal
- **Handler** - User function (runs on user stack)

### Delivery

1. Set pending bit
2. On return to userspace, check pending
3. Push signal frame, redirect to handler
4. Handler returns via sigreturn()

---

## 5) Process Groups & Sessions

| Concept | Purpose |
|---------|---------|
| Process group | Job control (foreground/background) |
| Session | Login session, controlling terminal |

### Key Operations

- **setpgid()** - Set process group
- **setsid()** - Create new session
- **tcsetpgrp()** - Set foreground group

---

## 6) Credentials

| Type | Description |
|------|-------------|
| Real UID/GID | Who you are |
| Effective UID/GID | Permission checks |
| Saved UID/GID | For setuid restoration |

### setuid Execution

1. Load executable with setuid bit
2. Set effective UID to file owner
3. Save original UID for restoration

---

## 7) Resource Limits

| Limit | Description |
|-------|-------------|
| RLIMIT_NOFILE | Max open files |
| RLIMIT_AS | Address space size |
| RLIMIT_STACK | Stack size |
| RLIMIT_NPROC | Max processes |
| RLIMIT_CORE | Core dump size |

---

## 8) Syscalls

| Syscall | Description |
|---------|-------------|
| fork | Clone process |
| exec* | Replace process image |
| exit | Terminate process |
| wait* | Wait for child |
| getpid/getppid | Get process IDs |
| kill | Send signal |
| sigaction | Set signal handler |
| setpgid/getpgid | Process groups |
| setsid/getsid | Sessions |
| setuid/getuid | Credentials |
| getrlimit/setrlimit | Resource limits |

---

## 9) Exit Criteria

- [ ] fork() creates new process with COW
- [ ] exec() loads and runs ELF
- [ ] exit()/wait() lifecycle works
- [ ] Signals delivered correctly
- [ ] Signal handlers execute and return
- [ ] Process groups work for job control
- [ ] Credentials and setuid work
- [ ] Resource limits enforced

---

*End of EFFLUX Process Specification*
