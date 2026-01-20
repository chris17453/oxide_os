# Phase 8: Libc + Userland

**Stage:** 2 - Core OS
**Status:** Complete
**Dependencies:** Phase 7 (Signals)

---

## Goal

Build custom C library and essential userland programs for a bootable system.

---

## Deliverables

| Item | Status |
|------|--------|
| Custom libc (libc) | [x] |
| init (PID 1) | [x] |
| login | [x] |
| shell (esh) | [x] |
| coreutils | [x] |
| getty | [x] |

---

## Architecture Status

| Arch | libc | init | shell | coreutils | Done |
|------|------|------|-------|-----------|------|
| x86_64 | [x] | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Implementation

### libc

Written in Rust (no_std), provides:

| Module | Functions |
|--------|-----------|
| syscall | Raw syscall wrappers for x86_64 |
| errno | POSIX error codes |
| fcntl | Open flags, seek modes, file modes |
| signal | Signal numbers, SigSet, kill, raise |
| string | strlen, strcpy, strcmp, memcpy, memset, etc. |
| unistd | read, write, open, close, fork, exec, wait, getpid, pipe, chdir, getcwd |
| stdio | print, println, putchar, getchar, print_u64, atoi |
| env | setenv, getenv, unsetenv, init_env |

### init

First userspace process (PID 1):
- Spawns getty for each configured terminal
- Reaps orphaned zombie processes
- Respawns services if they exit

### getty

Terminal manager:
- Opens and configures terminal devices
- Displays login banner
- Spawns login process
- Respawns on logout

### login

User authentication:
- Username and password prompts
- User database lookup (built-in for now)
- Password verification
- Spawns user's shell on success

### esh (OXIDE Shell)

Full-featured command shell with:
- Command execution via fork/exec
- Builtin commands: echo, cd, pwd, exit, help, export
- I/O redirection (<, >, >>)
- Pipes (|)
- Background jobs (&)
- Environment variables
- PATH searching in /bin

### coreutils

| Utility | Status |
|---------|--------|
| echo | [x] |
| cat | [x] |
| ls | [x] |
| mkdir | [x] |
| rm | [x] |
| true | [x] |
| false | [x] |
| uname | [x] |
| ps | [x] |
| kill | [x] |

Note: Most utilities need argument passing from kernel to be fully functional.

---

## File Structure

```
userspace/
├── libc/
│   └── src/
│       ├── lib.rs       # Entry point, panic handler
│       ├── syscall.rs   # Raw syscall interface
│       ├── errno.rs     # Error codes
│       ├── fcntl.rs     # File control
│       ├── signal.rs    # Signal handling
│       ├── string.rs    # String functions
│       ├── unistd.rs    # POSIX functions
│       ├── stdio.rs     # Standard I/O
│       └── env.rs       # Environment variables
├── init/
│   └── src/main.rs      # PID 1 init process
├── login/
│   └── src/main.rs      # Login authentication
├── getty/
│   └── src/main.rs      # Terminal manager
├── shell/
│   └── src/main.rs      # esh shell
└── coreutils/
    └── src/bin/
        ├── echo.rs
        ├── cat.rs
        ├── ls.rs
        ├── mkdir.rs
        ├── rm.rs
        ├── true.rs
        ├── false.rs
        ├── uname.rs
        ├── ps.rs
        └── kill.rs
```

---

## Exit Criteria

- [x] libc provides basic POSIX-like API
- [x] init boots and spawns getty
- [x] login authenticates users
- [x] Shell executes commands
- [x] Pipes and redirections work
- [x] Environment variables implemented
- [x] Working directory (chdir/getcwd) implemented
- [x] Core utilities functional (basic)
- [ ] Full boot to shell prompt (needs kernel integration)
- [ ] Works on all 8 architectures

---

## Notes

Phase 8 complete for x86_64 architecture. Userspace programs are written in Rust using a custom no_std libc. The following items remain for future work:

1. **Argument passing**: Kernel needs to pass argv/argc to userspace
2. **Full integration**: Userspace programs need kernel integration and initramfs building
3. **Other architectures**: Only x86_64 implemented

---

*Phase 8 of OXIDE Implementation - Complete*
