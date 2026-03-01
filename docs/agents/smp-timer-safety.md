# SMP Timer ISR Safety Rules

**Author:** SableWire
**Context:** Timer interrupt handler (`handle_timer` in `arch-x86_64/exceptions.rs`)

## Problem

With `-smp N`, all N CPUs fire local APIC timer interrupts. The timer ISR must
be SMP-safe:

- **Shared mutable state** in the ISR is a data race unless atomic or
  single-writer.
- **Serial output** from multiple CPUs interleaves byte-by-byte through
  `write_byte_unsafe` (no per-CPU coordination).
- **Per-CPU run queues** mean a scheduler dump on CPU *k* only sees tasks
  assigned to CPU *k*. If all tasks live on CPU 0, APs dump empty tables.

## Rules

1. **Global tick counter (`TIMER_TICKS`) must be `AtomicU64`.**
   Only BSP (APIC ID 0) increments it. APs read-only. This keeps the tick rate
   at the intended 100 Hz regardless of CPU count.

2. **Terminal tick callback runs on BSP only.**
   Console I/O is single-threaded (VT state, serial writer). Running it from
   multiple CPUs races on VT buffers and quadruples serial output in ISR context.

3. **Scheduler debug dumps run on BSP only (CPU 0).**
   Until task migration is implemented, all user tasks live on CPU 0. AP dumps
   produce empty output and interleave with BSP output on serial.

4. **`static mut` in ISR context is forbidden on SMP.**
   Use `AtomicU64`/`AtomicBool` or gate access to a single CPU. Every `static
   mut` touched by the timer handler must be audited for SMP safety.

5. **Scheduler callback runs on ALL CPUs, but GLOBAL_CLOCK is BSP-only.**
   Each CPU manages its own run queue and needs preemption ticks. However,
   `GLOBAL_CLOCK.fetch_add(TICK_NS)` MUST only run on BSP (CPU 0). APs
   call `GLOBAL_CLOCK.load()`. With N CPUs all doing fetch_add, the clock
   advances N× too fast — corrupting all timing (nanosleep, vruntime, etc.).
   **Location:** `kernel/sched/sched/src/core.rs`, `scheduler_tick_ex()`.

6. **ISR serial output (`write_byte_unsafe`) has no inter-CPU lock.**
   Minimize ISR serial writes. Gate debug output to BSP when possible.
   Multi-line dumps from different CPUs will garble each other.

7. **Per-CPU state in ISR must use per-CPU arrays, not global statics.**
   The KPO grace period streak tracker (`KPO_STREAK_PID`, `KPO_STREAK_COUNT`)
   must be indexed by `this_cpu()`. Global statics get trashed when 4 CPUs
   hit the timer ISR simultaneously — CPU 0 tracks PID 3's streak, CPU 1
   overwrites with its idle task, streak never accumulates, forced preemption
   never fires, and one task monopolizes CPU 0 forever.
   **Location:** `kernel/src/scheduler.rs`, kpo grace period gate.

## Files

| File | What to check |
|------|--------------|
| `kernel/arch/arch-x86_64/src/exceptions.rs` | `TIMER_TICKS`, `LAST_TERMINAL_TICK`, `handle_timer` |
| `kernel/src/scheduler.rs` | Debug dump CPU gate, `scheduler_tick` |
| `kernel/sched/sched/src/core.rs` | `GLOBAL_CLOCK`, `try_with_rq` vs `with_rq` |
| `kernel/syscall/syscall/src/time.rs` | `check_sleepers` (already atomic CAS — safe) |
