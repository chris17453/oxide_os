# Phase 4: Process Model

**Stage:** 2 - Core OS
**Status:** In Progress
**Dependencies:** Phase 3 (User Mode + Syscalls)

---

## Goal

Implement full UNIX-like process model with fork, exec, wait, and process groups.

---

## Deliverables

| Item | Status |
|------|--------|
| Process structure (PID, PPID, credentials) | [x] |
| Process table and PID allocation | [x] |
| fork() with Copy-on-Write | [x] |
| exec() replaces process image | [x] |
| wait()/waitpid() reaps children | [x] |
| Process groups and sessions | [x] |

---

## Architecture Status

| Arch | Process | fork | exec | wait | Groups | Done |
|------|---------|------|------|------|--------|------|
| x86_64 | [x] | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Syscalls to Implement

| Number | Name | Args | Return |
|--------|------|------|--------|
| 3 | sys_fork | - | child PID (parent), 0 (child) |
| 4 | sys_exec | path, argv, envp | -1 on error (doesn't return on success) |
| 5 | sys_wait | status_ptr | child PID |
| 6 | sys_waitpid | pid, status_ptr, options | child PID |
| 7 | sys_getpid | - | current PID |
| 8 | sys_getppid | - | parent PID |
| 9 | sys_setpgid | pid, pgid | 0 or -errno |
| 10 | sys_getpgid | pid | pgid or -errno |
| 11 | sys_setsid | - | session ID or -errno |
| 12 | sys_getsid | pid | session ID or -errno |

---

## Copy-on-Write Implementation

1. On fork:
   - Copy page table structure (PML4, PDPT, PD, PT)
   - For each present page in user space:
     - Mark as read-only in BOTH parent and child
     - Increment reference count on physical frame
   - Parent and child share the same physical frames initially

2. On write fault (page fault with write bit set):
   - Check if faulting address is in a COW page
   - If reference count > 1:
     - Allocate new frame
     - Copy contents from old frame
     - Map new frame as writable
     - Decrement old frame's reference count
   - If reference count == 1:
     - Just make the page writable (we're the only owner)

3. Reference counting:
   - Maintain per-frame reference count
   - Increment on fork COW mapping
   - Decrement on unmap or COW copy
   - Free frame when count reaches 0

---

## Process Group/Session Model

```
Session (sid)
├── Foreground Process Group (pgid)
│   ├── Process A (leader)
│   └── Process B
└── Background Process Group
    └── Process C
```

- Process group: collection of related processes (e.g., a pipeline)
- Session: collection of process groups (e.g., a login session)
- Session leader: first process in session (usually shell)
- Controlling terminal: TTY associated with session

---

## Key Files

```
crates/proc/efflux-proc/src/
├── lib.rs
├── address_space.rs     # User address space (existing)
├── process.rs           # Process structure (new)
├── fork.rs              # fork implementation (new)
├── exec.rs              # exec implementation (new)
└── cow.rs               # COW page tracking (new)

crates/syscall/efflux-syscall/src/
├── lib.rs               # Syscall dispatch (update)
├── process.rs           # fork/exec/wait handlers (new)
└── errno.rs             # Error codes (new)
```

---

## Exit Criteria

- [ ] fork() creates child process with COW pages
- [ ] Child gets return value 0, parent gets child PID
- [ ] exec() loads new ELF and starts execution
- [ ] wait()/waitpid() blocks until child exits
- [ ] Zombie processes are reaped correctly
- [ ] Process groups can be created and managed
- [ ] Test program runs fork-exec-wait cycle
- [ ] Works on all 8 architectures

---

## Test Program

```c
int main() {
    pid_t pid = fork();
    if (pid == 0) {
        // Child process
        exec("/bin/hello", NULL, NULL);
        exit(1);  // exec failed
    } else if (pid > 0) {
        // Parent process
        int status;
        waitpid(pid, &status, 0);
        printf("Child exited with status %d\n", status);
    } else {
        perror("fork");
    }
    return 0;
}
```

---

## Notes

**Process structure implemented (2025-01-18):**
- Process type with PID, PPID, credentials, state, address space
- ProcessTable for global process management
- PID allocator (atomic counter starting at 1)
- Credentials (uid, gid, euid, egid)
- Process groups and sessions (pgid, sid fields)

---

*Phase 4 of EFFLUX Implementation*
