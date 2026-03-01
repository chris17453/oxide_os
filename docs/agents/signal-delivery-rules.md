# Signal Delivery Rules

**Author:** GraveShift, ThreadRogue
**Status:** ACTIVE RULE

## Signal Delivery Path

Signals are delivered in the timer ISR when returning to user mode:

```
Timer ISR → check in_user_mode (cs==0x23) → try_lock ProcessMeta → dequeue_pending → determine_action → dispatch
```

### Key Rules

1. **ISR MUST use try_lock for ProcessMeta** — Blocking lock in ISR = deadlock if interrupted code holds meta. If try_lock fails, signal delivery is deferred to next tick.

2. **Terminate/CoreDump MUST drop ProcessMeta before calling scheduler** — `set_task_exit_status()`, `try_wake_up()`, and `set_need_resched()` all acquire RQ locks. Holding ProcessMeta + RQ lock simultaneously invites deadlock.

3. **Wake parent on child death** — After setting exit status, MUST call `try_wake_up(ppid)` so parent's `waitpid()` returns. Use `try_wake_up` (non-blocking), not `wake_up` (blocking).

4. **ISR signal delivery only runs in user mode** — The `in_user_mode && current_pid > 1` gate ensures we never deliver signals to kernel threads or init.

5. **exec MUST reset caught signal handlers** — After exec(), the old process image's handler addresses are invalid. All handlers with `SigAction::handler != SIG_DFL && handler != SIG_IGN` must be reset to SIG_DFL.

6. **Signal wakeup after send_signal** — After queuing a signal, MUST wake the target process: `sched::wake_up(pid)` from syscall context, `sched::try_wake_up(pid)` from ISR context. Without this, sleeping processes (nanosleep, poll) won't notice the signal until they wake naturally.

7. **nanosleep EINTR path** — When a signal interrupts nanosleep, the sleep queue entry is cleared and -EINTR is returned. The process then gets scheduled, enters the timer ISR, and the signal is delivered.

## Signal Frame Layout (User Handler)

```
[user stack grows down]
  ← original RSP
  [padding for 16-byte alignment]
  [SignalFrame]
    - saved_rip, saved_rsp, saved_rflags
    - saved registers (rax..r15)
    - signal_mask (for restoring after handler)
    - signo
  [return address → __oxide_sigreturn trampoline]
  ← new RSP (frame.rsp redirected here)
```

RIP is redirected to the user handler. When the handler returns, `__oxide_sigreturn` calls `SYS_SIGRETURN` to restore registers and resume original execution.

## Verified Test Cases (sigtest)

1. **Direct kill(pid, SIGINT)** — Parent forks child, child sleeps, parent kills with SIGINT
2. **PGID kill(-pgid, SIGINT)** — Process group signal delivery
3. **Signal during nanosleep (EINTR)** — Child in nanosleep interrupted by signal
4. **Self group kill(0, SIGINT)** — kill(0, sig) sends to own process group
