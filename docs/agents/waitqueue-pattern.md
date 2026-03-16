# Agent Rule: WaitQueue Pattern for fd Blocking

## Rule
All fd-blocking operations (poll, select, pipe read/write, VT read) MUST use
WaitQueue registration + TASK_INTERRUPTIBLE block instead of yield+HLT polling.

## Why
Yield+HLT polling at 100Hz timer tick rate burns one scheduler pick per tick
per polling process. With 6 daemons on 4 CPUs, that's 600 spurious wakeups/sec.
WaitQueues give true event-driven wake: zero CPU while waiting.

## Pattern
```rust
// 1. Check if ready (optimistic fast path)
if fd_ready() { return; }

// 2. Register on WaitQueue
let slot = wq.register(pid);

// 3. Re-check (lost-wake window)
if fd_ready() { wq.unregister(slot); return; }

// 4. Block
sched::block_current(TASK_INTERRUPTIBLE);
wait_for_interrupt();

// 5. Unregister and re-check
wq.unregister(slot);
```

## Wake Sites
Drivers call `wq.wake_all()` when state changes:
- Pipe write → wake read_wq
- Pipe read (was full) → wake write_wq
- VT push_input → wake vt_wq
- VT switch_to → wake new VT's wq

## How to Apply
- New fd types (sockets, /dev/kmsg) should embed a `WaitQueue` and implement
  `poll_register_wait()` on their VnodeOps.
- Never add new yield+HLT poll loops. Always use WaitQueue.
