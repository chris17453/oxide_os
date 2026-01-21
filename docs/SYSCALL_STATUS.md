# OXIDE OS Syscall Implementation Status

**Last Updated:** 2026-01-21

This document tracks the implementation status of all syscalls in OXIDE OS.

---

## Summary

| Category | Implemented | Total | Status |
|----------|-------------|-------|--------|
| Process Management | 15 | 15 | ✅ Complete |
| File Operations | 15 | 15 | ✅ Complete |
| Directory Operations | 9 | 9 | ✅ Complete |
| Signals | 7 | 7 | ✅ Complete |
| Memory Management | 5 | 5 | ✅ Complete |
| Network/Sockets | 14 | 14 | ✅ Complete |
| TTY/Device | 1 | 1 | ✅ Complete |
| Keyboard | 2 | 2 | ✅ Complete |
| Priority | 3 | 3 | ✅ Complete |
| Timers/Alarms | 3 | 3 | ✅ Complete |
| Threads | 5 | 5 | ✅ Complete |
| **Total** | **79** | **79** | **100%** |

---

## Process Management Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 0 | sys_exit | status | ✅ | Terminate process |
| 3 | sys_fork | - | ✅ | Clone process with COW |
| 4 | sys_exec | path, argv, envp | ✅ | Replace process image |
| 5 | sys_wait | status_ptr | ✅ | Wait for any child |
| 6 | sys_waitpid | pid, status, opts | ✅ | Wait for specific child |
| 7 | sys_getpid | - | ✅ | Get process ID |
| 8 | sys_getppid | - | ✅ | Get parent PID |
| 9 | sys_setpgid | pid, pgid | ✅ | Set process group |
| 10 | sys_getpgid | pid | ✅ | Get process group |
| 11 | sys_setsid | - | ✅ | Create new session |
| 12 | sys_getsid | pid | ✅ | Get session ID |
| 13 | sys_execve | path, argv, envp | ✅ | Execute with args/env |
| 14 | sys_getuid | - | ✅ | Get real user ID |
| 15 | sys_getgid | - | ✅ | Get real group ID |
| 16 | sys_geteuid | - | ✅ | Get effective user ID |
| 17 | sys_getegid | - | ✅ | Get effective group ID |
| 18 | sys_setuid | uid | ✅ | Set user ID |
| 19 | sys_setgid | gid | ✅ | Set group ID |

---

## File Operations Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 1 | sys_write | fd, buf, len | ✅ | Write to file descriptor |
| 2 | sys_read | fd, buf, len | ✅ | Read from file descriptor |
| 20 | sys_open | path, flags, mode | ✅ | Open file |
| 21 | sys_close | fd | ✅ | Close file descriptor |
| 22 | sys_lseek | fd, offset, whence | ✅ | Seek in file |
| 23 | sys_fstat | fd, stat_ptr | ✅ | Get file status by FD |
| 24 | sys_stat | path, stat_ptr | ✅ | Get file status by path |
| 25 | sys_dup | fd | ✅ | Duplicate file descriptor |
| 26 | sys_dup2 | oldfd, newfd | ✅ | Duplicate FD to specific number |
| 27 | sys_ftruncate | fd, length | ✅ | Truncate file to length |
| 37 | sys_pipe | pipefd[2] | ✅ | Create pipe |
| 38 | sys_link | target, link | ✅ | Create hard link |
| 39 | sys_symlink | target, link | ✅ | Create symbolic link |
| 40 | sys_ioctl | fd, request, arg | ✅ | Device control |
| 41 | sys_readlink | path, buf, size | ✅ | Read symbolic link |
| 150 | sys_chmod | path, mode | ✅ | Change file permissions |
| 151 | sys_fchmod | fd, mode | ✅ | Change permissions by FD |
| 152 | sys_chown | path, uid, gid | ✅ | Change file owner |
| 153 | sys_fchown | fd, uid, gid | ✅ | Change owner by FD |

---

## Directory Operations Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 30 | sys_mkdir | path, mode | ✅ | Create directory |
| 31 | sys_rmdir | path | ✅ | Remove directory |
| 32 | sys_unlink | path | ✅ | Remove file |
| 33 | sys_rename | old, new | ✅ | Rename/move file |
| 34 | sys_getdents | fd, buf, count | ✅ | Get directory entries |
| 35 | sys_chdir | path | ✅ | Change working directory |
| 36 | sys_getcwd | buf, size | ✅ | Get working directory |

---

## Signal Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 50 | sys_kill | pid, sig | ✅ | Send signal to process |
| 51 | sys_sigaction | sig, act, oldact | ✅ | Set signal handler |
| 52 | sys_sigprocmask | how, set, oldset | ✅ | Change signal mask |
| 53 | sys_sigpending | set | ✅ | Get pending signals |
| 54 | sys_sigsuspend | mask | ✅ | Wait for signal |
| 55 | sys_pause | - | ✅ | Wait for any signal |
| 56 | sys_sigreturn | - | ✅ | Return from signal handler |

---

## Memory Management Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 90 | sys_mmap | addr, len, prot, flags, fd, off | ✅ | Map memory (anonymous only) |
| 91 | sys_munmap | addr, length | ✅ | Unmap memory |
| 92 | sys_mprotect | addr, len, prot | ✅ | Change memory protection |
| 93 | sys_mremap | addr, old_sz, new_sz, flags | ✅ | Remap memory |
| 94 | sys_brk | addr | ✅ | Change data segment size |

---

## Network/Socket Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 70 | sys_socket | domain, type, protocol | ✅ | Create socket |
| 71 | sys_bind | fd, addr, addrlen | ✅ | Bind socket to address |
| 72 | sys_listen | fd, backlog | ✅ | Listen for connections |
| 73 | sys_accept | fd, addr, addrlen | ✅ | Accept connection |
| 74 | sys_connect | fd, addr, addrlen | ✅ | Connect to address |
| 75 | sys_send | fd, buf, len, flags | ✅ | Send data |
| 76 | sys_recv | fd, buf, len, flags | ✅ | Receive data |
| 77 | sys_sendto | fd, buf, len, flags, addr, addrlen | ✅ | Send to address |
| 78 | sys_recvfrom | fd, buf, len, flags, addr, addrlen | ✅ | Receive from address |
| 79 | sys_shutdown | fd, how | ✅ | Shutdown socket |
| 80 | sys_getsockname | fd, addr, addrlen | ✅ | Get socket name |
| 81 | sys_getpeername | fd, addr, addrlen | ✅ | Get peer name |
| 82 | sys_setsockopt | fd, level, opt, val, len | ✅ | Set socket option |
| 83 | sys_getsockopt | fd, level, opt, val, len | ✅ | Get socket option |

---

## TTY/Device Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 40 | sys_ioctl | fd, request, arg | ✅ | Device control (includes TTY) |

---

## Keyboard Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 120 | sys_setkeymap | layout_name, len | ✅ | Set keyboard layout |
| 121 | sys_getkeymap | buf, len | ✅ | Get keyboard layout |

---

## Process Priority Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 122 | sys_nice | increment | ✅ | Adjust process priority |
| 123 | sys_getpriority | which, who | ✅ | Get scheduling priority |
| 124 | sys_setpriority | which, who, prio | ✅ | Set scheduling priority |

**Implementation Notes:**
- Supports PRIO_PROCESS mode
- PRIO_PGRP and PRIO_USER return ENOSYS
- Permission checks for priority increases (requires root)
- Nice values: -20 (highest) to +19 (lowest)

---

## Timer/Alarm Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 125 | sys_alarm | seconds | ✅ | Set alarm signal |
| 126 | sys_setitimer | which, new_val, old_val | ✅ | Set interval timer |
| 127 | sys_getitimer | which, curr_val | ✅ | Get interval timer |

**Implementation Notes:**
- Supports ITIMER_REAL mode (delivers SIGALRM)
- ITIMER_VIRTUAL and ITIMER_PROF return ENOSYS
- Timer tracking added to Process struct

---

## Thread Syscalls

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 56 | sys_clone | flags, stack, ptid, ctid, tls | ✅ | Create thread/process (partial) |
| 186 | sys_gettid | - | ✅ | Get thread ID |
| 202 | sys_futex | addr, op, val, timeout, addr2, val3 | ✅ | Fast userspace mutex |
| 218 | sys_set_tid_address | tidptr | ✅ | Set clear_child_tid |
| 231 | sys_exit_group | status | ✅ | Exit all threads |

**Implementation Notes:**
- sys_clone with CLONE_VM returns ENOSYS (full threading support pending)
- Basic process cloning works
- Futex operations implemented for userspace locking

---

## Module Syscalls (Stub)

| # | Name | Arguments | Status | Notes |
|---|------|-----------|--------|-------|
| 60 | sys_init_module | image, len, params | ⚠️ | Returns ENOSYS |
| 61 | sys_delete_module | name, flags | ⚠️ | Returns ENOSYS |
| 62 | sys_query_module | - | ⚠️ | Deprecated |

---

## Filesystem Support

### Implemented Filesystems

| Filesystem | Type | Status | Mount Point | Notes |
|------------|------|--------|-------------|-------|
| **devfs** | Virtual | ✅ | /dev | Device nodes |
| **procfs** | Virtual | ✅ | /proc | Process information |
| **tmpfs** | RAM | ✅ | /tmp | Temporary files |
| **oxidefs** | Disk | ⚠️ | - | Exists but not mounted |
| **FAT32** | Disk | ⚠️ | - | Exists but not integrated |

### procfs Implementation

Supports:
- `/proc/self` - Symlink to current process
- `/proc/[pid]/status` - Process status
- `/proc/[pid]/cmdline` - Command line
- `/proc/[pid]/exe` - Executable path (symlink)
- `/proc/[pid]/cwd` - Current directory (symlink)
- `/proc/meminfo` - Memory information

---

## Userspace Libraries

### libc (`userspace/libc/`)

Provides POSIX-compatible wrappers for all syscalls:
- Standard I/O (stdio.rs)
- File operations (fcntl.rs, stat.rs)
- Directory operations (dirent.rs)
- Process management (unistd.rs)
- Signals (signal.rs)
- Sockets (socket.rs)
- Memory management (syscall.rs)
- String operations (string.rs)
- Time functions (time.rs)

### compression (`userspace/compression/`)

Provides compression/archive support:
- **DEFLATE/INFLATE**: GZIP compression (deflate.rs)
  - Uncompressed mode implemented
  - Full DEFLATE algorithm pending
  - CRC32 checksum verification
- **TAR**: POSIX ustar format (tar.rs)
  - Archive reader and builder
  - Full TAR header support
  - File/directory/symlink handling

---

## Application Status

| Application | Required Features | Status | Notes |
|-------------|-------------------|--------|-------|
| **ls** | open, getdents, close | ✅ | Ready to use |
| **cat** | read, write | ✅ | Ready to use |
| **ps** | procfs, getpid | ✅ | Ready to use |
| **chown** | sys_chown | ✅ | Syscall implemented |
| **pgrep/pkill** | procfs, sys_kill | ✅ | All features available |
| **nice** | priority syscalls | ✅ | All syscalls implemented |
| **nohup** | signals, fork, exec | ✅ | All features available |
| **timeout** | alarm, setitimer | ✅ | All syscalls implemented |
| **gzip** | DEFLATE compression | ⚠️ | Uncompressed mode only |
| **gunzip** | DEFLATE decompression | ⚠️ | Uncompressed mode only |
| **tar** | TAR format | ✅ | Full format support |

---

## Architecture Support

All syscalls are currently implemented for **x86_64** only.

Future architectures (not yet implemented):
- i686
- aarch64
- arm
- mips64
- mips32
- riscv64
- riscv32

---

## Recent Additions (2026-01-21)

### Link Operations
- **sys_link** (#38) - Create hard links
- **sys_symlink** (#39) - Create symbolic links
- **sys_readlink** (#41) - Read symbolic link targets

### Process Priority
- **sys_nice** (#122) - Adjust process priority
- **sys_getpriority** (#123) - Get scheduling priority
- **sys_setpriority** (#124) - Set scheduling priority

### Timers/Alarms
- **sys_alarm** (#125) - Set alarm signal (SIGALRM)
- **sys_setitimer** (#126) - Set interval timer
- **sys_getitimer** (#127) - Get interval timer value

### Process Struct Enhancements
- Added `nice` field (priority: -20 to +19)
- Added `alarm_remaining` field
- Added `itimer_*` fields for interval timers

### Userspace Libraries
- Created `compression` library with DEFLATE/INFLATE and TAR support
- Added priority/timer syscall wrappers to libc

---

*OXIDE OS - A from-scratch operating system in Rust*
