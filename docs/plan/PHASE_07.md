# Phase 7: Signals

**Stage:** 2 - Core OS
**Status:** Not Started
**Dependencies:** Phase 6 (TTY + PTY)

---

## Goal

Implement POSIX signal delivery, handlers, and masking.

---

## Deliverables

| Item | Status |
|------|--------|
| Signal generation | [ ] |
| Signal delivery | [ ] |
| Signal handlers (sigaction) | [ ] |
| Signal masks (sigprocmask) | [ ] |
| Pending signal queue | [ ] |
| Core dump signals | [ ] |
| SIGCHLD on child exit | [ ] |
| Real-time signals (optional) | [ ] |

---

## Architecture Status

| Arch | Generate | Deliver | Handlers | Masks | Done |
|------|----------|---------|----------|-------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Syscalls to Implement

| Number | Name | Args | Return |
|--------|------|------|--------|
| 40 | sys_kill | pid, sig | 0 or -errno |
| 41 | sys_sigaction | sig, act, oldact | 0 or -errno |
| 42 | sys_sigprocmask | how, set, oldset | 0 or -errno |
| 43 | sys_sigpending | set | 0 or -errno |
| 44 | sys_sigsuspend | mask | -EINTR |
| 45 | sys_sigreturn | - | (returns to user) |
| 46 | sys_sigaltstack | ss, old_ss | 0 or -errno |
| 47 | sys_pause | - | -EINTR |
| 48 | sys_alarm | seconds | remaining seconds |

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

## Signal Delivery Flow

```
1. Signal Generated
   ├── kill() syscall
   ├── Terminal (^C, ^Z)
   ├── Exception (SIGSEGV, SIGFPE)
   └── Kernel event (SIGCHLD)
            │
            ▼
2. Check if blocked
   ├── If blocked: add to pending set
   └── If not blocked: continue
            │
            ▼
3. Check handler
   ├── SIG_DFL: default action
   ├── SIG_IGN: ignore
   └── User handler: deliver
            │
            ▼
4. Deliver to user handler
   ├── Save current context to signal stack
   ├── Set up signal frame
   ├── Jump to handler
   └── Handler returns via sigreturn
```

---

## Signal Frame (x86_64)

```
┌─────────────────────────────┐ High address
│   siginfo_t (optional)      │
├─────────────────────────────┤
│   ucontext_t                │
│   - Saved registers         │
│   - Signal mask             │
├─────────────────────────────┤
│   Return address            │
│   (points to sigreturn)     │
├─────────────────────────────┤
│   Red zone (128 bytes)      │
└─────────────────────────────┘ Stack pointer
```

---

## Key Files

```
crates/signal/efflux-signal/src/
├── lib.rs
├── signal.rs          # Signal numbers and info
├── action.rs          # sigaction handling
├── mask.rs            # Signal masks
├── pending.rs         # Pending signal queue
├── deliver.rs         # Signal delivery
└── frame.rs           # Signal stack frame (arch-specific)

kernel/src/
└── signal.rs          # Kernel signal integration
```

---

## sigaction Structure

```rust
pub struct SigAction {
    pub sa_handler: SigHandler,
    pub sa_flags: SigFlags,
    pub sa_mask: SigSet,
    pub sa_restorer: Option<fn()>,
}

pub enum SigHandler {
    Default,           // SIG_DFL
    Ignore,            // SIG_IGN
    Handler(fn(i32)),  // Simple handler
    SigAction(fn(i32, &SigInfo, &mut UContext)), // SA_SIGINFO
}

bitflags! {
    pub struct SigFlags: u32 {
        const SA_NOCLDSTOP = 0x00000001;
        const SA_NOCLDWAIT = 0x00000002;
        const SA_SIGINFO   = 0x00000004;
        const SA_ONSTACK   = 0x08000000;
        const SA_RESTART   = 0x10000000;
        const SA_NODEFER   = 0x40000000;
        const SA_RESETHAND = 0x80000000;
    }
}
```

---

## Exit Criteria

- [ ] kill() sends signal to process
- [ ] SIGINT terminates process by default
- [ ] Custom handler catches and handles signal
- [ ] Handler returns correctly via sigreturn
- [ ] sigprocmask blocks/unblocks signals
- [ ] SIGCHLD delivered when child exits
- [ ] Core dump generated for SIGSEGV (optional)
- [ ] Works on all 8 architectures

---

## Test Program

```c
volatile int got_signal = 0;

void handler(int sig) {
    got_signal = sig;
}

int main() {
    struct sigaction sa = {
        .sa_handler = handler,
        .sa_flags = 0,
    };
    sigemptyset(&sa.sa_mask);

    sigaction(SIGUSR1, &sa, NULL);

    printf("Sending SIGUSR1 to self...\n");
    kill(getpid(), SIGUSR1);

    if (got_signal == SIGUSR1) {
        printf("Handler caught SIGUSR1!\n");
        return 0;
    } else {
        printf("Signal not received!\n");
        return 1;
    }
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 7 of EFFLUX Implementation*
