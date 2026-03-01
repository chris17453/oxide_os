# Service Manager Sleep Pattern — No Busy-Yield Fallbacks

**Author:** GraveShift
**Status:** ACTIVE RULE — never revert to sched_yield loops

## Problem

The servicemgr daemon's main loop used `usleep(1_000_000)` with a fallback of
200 `sched_yield()` calls when usleep failed. 200 yields complete in microseconds,
turning the daemon into a 100% CPU busy-loop that starved every other process on
the system. Serial logs showed constant `[WAIT]` waitpid spam with no gaps.

## Fix

Replace the 200-yield fallback with `poll()` on an empty fd set with a 1-second
timeout. `poll()` properly blocks via HLT in the kernel — zero CPU burn.

```rust
loop {
    check_services();

    if usleep(1_000_000) < 0 {
        // poll() on zero fds with timeout = proper kernel HLT sleep
        let ret = libc::poll::poll(&mut [], 1000);
        if ret < 0 {
            // Nuclear fallback — both sleep paths failed
            for _ in 0..10_000 {
                sched_yield();
            }
        }
    }
}
```

**Location:** `userspace/system/servicemgr/src/main.rs`, `run_daemon()` main loop.

## Rules

1. **Daemon loops MUST use proper sleep mechanisms** — `usleep()`, `nanosleep()`,
   or `poll()` with timeout. Never use `sched_yield()` loops as a sleep substitute.

2. **`poll(&mut [], timeout_ms)` is a valid sleep fallback.** Polling zero fds with
   a timeout makes the kernel HLT-loop for the specified duration. It's equivalent
   to `nanosleep()` but goes through a different syscall path.

3. **If both usleep and poll fail, yield at least 10,000 times.** This gives the
   scheduler enough iterations to run other tasks. 200 yields was far too few —
   it completed in microseconds and immediately re-entered the service check loop.

4. **Never add unconditional `sched_yield()` after successful sleep.** The old code
   did `sched_yield()` even after usleep succeeded — unnecessary overhead that
   doubles the context switch rate for zero benefit.

## Verification

Boot with `make run`. The serial output should show:
- `[WAIT]` groups appearing ~once per second (5 waitpid calls per group)
- NOT continuous `[WAIT]` spam filling the serial buffer
