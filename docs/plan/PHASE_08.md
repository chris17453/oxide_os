# Phase 8: Libc + Userland

**Stage:** 2 - Core OS
**Status:** Not Started
**Dependencies:** Phase 7 (Signals)

---

## Goal

Build custom C library and essential userland programs for a bootable system.

---

## Deliverables

| Item | Status |
|------|--------|
| Custom libc (efflux-libc) | [ ] |
| init (PID 1) | [ ] |
| login | [ ] |
| shell (esh) | [ ] |
| coreutils | [ ] |
| getty | [ ] |

---

## Architecture Status

| Arch | libc | init | shell | coreutils | Done |
|------|------|------|-------|-----------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Libc Components

| Category | Functions |
|----------|-----------|
| String | strlen, strcpy, strncpy, strcmp, strncmp, strcat, strchr, strrchr, strstr, memcpy, memmove, memset, memcmp |
| Stdio | printf, fprintf, sprintf, snprintf, scanf, fopen, fclose, fread, fwrite, fgets, fputs, fflush, fseek, ftell |
| Stdlib | malloc, free, realloc, calloc, exit, abort, atexit, getenv, setenv, atoi, atol, strtol, strtoul, rand, srand, qsort, bsearch |
| Unistd | read, write, open, close, fork, exec*, wait, waitpid, getpid, getppid, chdir, getcwd, dup, dup2, pipe, sleep, usleep |
| Signal | signal, sigaction, kill, raise, sigprocmask, sigsuspend, sigpending |
| Time | time, gettimeofday, localtime, gmtime, strftime, sleep, nanosleep |
| Ctype | isalpha, isdigit, isalnum, isspace, isupper, islower, toupper, tolower |
| Errno | errno, strerror, perror |

---

## Init Process

```
init (PID 1)
├── Mount /proc, /dev, /tmp
├── Set up /dev/console
├── Spawn getty on each terminal
├── Reap orphaned zombies
└── Handle shutdown signals
```

**Responsibilities:**
1. First userspace process
2. Mount essential filesystems
3. Start getty processes
4. Adopt orphaned processes
5. Reap zombies (wait loop)
6. Handle SIGTERM/SIGINT for shutdown

---

## Shell Features (esh)

| Feature | Status |
|---------|--------|
| Command execution | [ ] |
| PATH searching | [ ] |
| Pipes (cmd1 \| cmd2) | [ ] |
| Redirections (<, >, >>) | [ ] |
| Background jobs (&) | [ ] |
| Job control (fg, bg, jobs) | [ ] |
| Environment variables | [ ] |
| Builtins (cd, exit, export) | [ ] |
| Command history | [ ] |
| Line editing | [ ] |
| Wildcards (*, ?) | [ ] |
| Quoting ("", '', \) | [ ] |

---

## Coreutils

| Utility | Description |
|---------|-------------|
| ls | List directory contents |
| cat | Concatenate files |
| echo | Display text |
| mkdir | Create directory |
| rmdir | Remove directory |
| rm | Remove files |
| cp | Copy files |
| mv | Move/rename files |
| pwd | Print working directory |
| cd | Change directory (shell builtin) |
| touch | Create/update file timestamp |
| head | Show first lines |
| tail | Show last lines |
| wc | Word count |
| grep | Search patterns |
| chmod | Change permissions |
| chown | Change ownership |
| ln | Create links |
| date | Display date/time |
| uname | System information |
| ps | Process status |
| kill | Send signals |
| env | Environment variables |
| true | Return success |
| false | Return failure |
| test / [ | Conditional tests |
| sleep | Delay execution |

---

## Key Files

```
userspace/
├── libc/
│   ├── src/
│   │   ├── string.c
│   │   ├── stdio.c
│   │   ├── stdlib.c
│   │   ├── unistd.c
│   │   ├── signal.c
│   │   └── syscall.S      # Arch-specific syscall stubs
│   └── include/
│       ├── stdio.h
│       ├── stdlib.h
│       ├── string.h
│       ├── unistd.h
│       └── ...
├── init/
│   └── init.c
├── login/
│   └── login.c
├── shell/
│   ├── main.c
│   ├── parser.c
│   ├── exec.c
│   └── builtins.c
├── coreutils/
│   ├── ls.c
│   ├── cat.c
│   ├── echo.c
│   └── ...
└── getty/
    └── getty.c
```

---

## Boot Sequence

```
Kernel boots
    │
    ▼
Load initramfs
    │
    ▼
Execute /sbin/init (PID 1)
    │
    ├── Mount /proc
    ├── Mount /dev (devfs)
    ├── Mount /tmp (tmpfs)
    │
    ▼
Spawn /sbin/getty on /dev/console
    │
    ▼
getty: display login prompt
    │
    ▼
User logs in → exec /bin/login
    │
    ▼
login: authenticate → exec /bin/sh
    │
    ▼
Shell ready for commands
```

---

## Exit Criteria

- [ ] libc provides basic POSIX-like API
- [ ] init boots and spawns getty
- [ ] login authenticates (or auto-login for now)
- [ ] Shell executes commands
- [ ] Pipes and redirections work
- [ ] Core utilities functional
- [ ] Full boot to shell prompt
- [ ] Works on all 8 architectures

---

## Test: Boot to Shell

```
EFFLUX v0.1.0

login: root
Password:

Welcome to EFFLUX!

root@efflux:~# echo "Hello World"
Hello World
root@efflux:~# ls /
bin  dev  etc  proc  sbin  tmp  usr
root@efflux:~# cat /proc/self/status
Name:   cat
Pid:    5
PPid:   4
root@efflux:~# ps
  PID TTY          TIME CMD
    1 ?        00:00:00 init
    2 tty1     00:00:00 getty
    4 tty1     00:00:00 sh
    5 tty1     00:00:00 ps
root@efflux:~#
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 8 of EFFLUX Implementation*
