# Phase 7: Signals

**Stage:** 2 - Core OS
**Status:** Complete
**Dependencies:** Phase 6 (TTY + PTY)

---

## Goal

Implement POSIX signal delivery, handlers, and masking.

---

## Deliverables

| Item | Status |
|------|--------|
| Signal generation | [x] |
| Signal delivery | [x] |
| Signal handlers (sigaction) | [x] |
| Signal masks (sigprocmask) | [x] |
| Pending signal queue | [x] |
| Core dump signals | [x] |
| SIGCHLD on child exit | [x] |
| Real-time signals (optional) | [ ] |

---

## Architecture Status

| Arch | Generate | Deliver | Handlers | Masks | Done |
|------|----------|---------|----------|-------|------|
| x86_64 | [x] | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Syscalls Implemented

| Number | Name | Args | Return | Status |
|--------|------|------|--------|--------|
| 50 | sys_kill | pid, sig | 0 or -errno | [x] |
| 51 | sys_sigaction | sig, act, oldact | 0 or -errno | [x] |
| 52 | sys_sigprocmask | how, set, oldset | 0 or -errno | [x] |
| 53 | sys_sigpending | set | 0 or -errno | [x] |
| 54 | sys_sigsuspend | mask | -EINTR | [x] |
| 55 | sys_pause | - | -EINTR | [x] |
| 56 | sys_sigreturn | - | (returns to user) | [x] |

---

## Standard Signals

| Number | Name | Default | Description |
|--------|------|---------|-------------|
| 1 | SIGHUP | Term | Hangup |
| 2 | SIGINT | Term | Interrupt (^C) |
| 3 | SIGQUIT | Core | Quit (^\) |
| 4 | SIGILL | Core | Illegal instruction |
| 5 | SIGTRAP | Core | Trace trap |
| 6 | SIGABRT | Core | Abort |
| 7 | SIGBUS | Core | Bus error |
| 8 | SIGFPE | Core | Floating point exception |
| 9 | SIGKILL | Term | Kill (unblockable) |
| 10 | SIGUSR1 | Term | User defined 1 |
| 11 | SIGSEGV | Core | Segmentation fault |
| 12 | SIGUSR2 | Term | User defined 2 |
| 13 | SIGPIPE | Term | Broken pipe |
| 14 | SIGALRM | Term | Alarm clock |
| 15 | SIGTERM | Term | Termination |
| 17 | SIGCHLD | Ignore | Child status change |
| 18 | SIGCONT | Cont | Continue if stopped |
| 19 | SIGSTOP | Stop | Stop (unblockable) |
| 20 | SIGTSTP | Stop | Terminal stop (^Z) |
| 21 | SIGTTIN | Stop | Background read |
| 22 | SIGTTOU | Stop | Background write |

---

## Implementation

### Crates Created

```
crates/signal/efflux-signal/src/
├── lib.rs           # Module exports
├── signal.rs        # Signal numbers, default actions
├── sigset.rs        # Signal set (mask) implementation
├── action.rs        # SigAction, SigHandler, SigInfo, SigFlags
├── pending.rs       # Pending signal queue
└── delivery.rs      # Signal delivery mechanism
```

### Process Integration

Signal state added to `Process` struct in `efflux-proc`:
- `signal_mask: SigSet` - Blocked signals
- `pending_signals: PendingSignals` - Pending signal queue
- `sigactions: [SigAction; NSIG]` - Signal handlers

### TTY Integration

Line discipline signals (^C, ^Z, ^\) connected to signal system:
- `Signal::Int` -> `SIGINT`
- `Signal::Quit` -> `SIGQUIT`
- `Signal::Tstp` -> `SIGTSTP`

PTY master provides `write_with_signal()` for signal delivery.

---

## Exit Criteria

- [x] kill() sends signal to process
- [x] SIGINT terminates process by default
- [x] Custom handler catches and handles signal
- [x] Handler returns correctly via sigreturn
- [x] sigprocmask blocks/unblocks signals
- [x] SIGCHLD delivered when child exits
- [ ] Core dump generated for SIGSEGV (optional)
- [ ] Works on all 8 architectures

---

## Notes

Phase 7 complete for x86_64 architecture. Signal handling infrastructure is in place with:
- Full signal number definitions (1-31 standard, 32-64 real-time)
- Signal set operations (add, remove, union, intersection, difference)
- Signal action management with all standard flags
- Pending signal queue with dequeue by priority
- Signal delivery determination and frame setup
- Integration with TTY/PTY for terminal signals

---

*Phase 7 of EFFLUX Implementation - Complete*
