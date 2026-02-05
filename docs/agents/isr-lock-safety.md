# ISR Lock Safety: Comprehensive Blocking Lock Audit

**Author:** GraveShift
**Last audit:** 2026-02-05

## The Rule

**ISR context MUST NEVER acquire blocking locks.** Use non-blocking
`try_lock()` / `try_with_rq()` and skip-or-retry on contention.

The CPU that took the interrupt cannot run the interrupted code to release
the lock. If the ISR spins, the system is permanently stuck.

## Blocking vs Non-Blocking APIs

| Function | Lock type | ISR-safe? |
|----------|-----------|-----------|
| `.lock()` on any `spin::Mutex` | Blocking spin | **NO** |
| `.try_lock()` on any `spin::Mutex` | Non-blocking | **YES** |
| `with_rq()` | Blocking spin on RQ | **NO** |
| `try_with_rq()` | Non-blocking try-lock on RQ | **YES** |
| `sched::wake_up()` | Uses `with_rq` | **NO** — must not call from ISR |
| `sched::try_wake_up()` | Uses `try_with_rq` | **YES** — returns false if contended |
| `sched::block_current()` | Uses `with_rq` | **NO** |
| `sched::scheduler_tick()` | Uses `try_with_rq` | **YES** |
| `sched::need_resched()` | Uses `try_with_rq` | **YES** |

## Comprehensive ISR Audit

### ISR #1: Timer Interrupt (`handle_timer`)

| Path | Status | Notes |
|------|--------|-------|
| `check_sleepers()` → `try_wake_up()` | **SAFE** | Fixed: was `wake_up` (blocking) |
| `check_sleepers()` BSP-only gate | **SAFE** | Avoids redundant cross-CPU IPIs |
| Scheduler dump → `debug_dump_all()` | **SAFE** | Uses `try_with_rq` |
| Signal delivery → `wake_up(ppid)` | **SAFE** | Only for user-mode frames (`cs==0x23`); user code never holds kernel locks |
| Signal SIGSTOP → `block_current()` | **SAFE** | Same: user-mode only |
| `sched::scheduler_tick()` | **SAFE** | Uses `try_with_rq` |
| `sched::need_resched()` | **SAFE** | Uses `try_with_rq` |
| `pick_next_task()` → `with_rq()` | **SAFE** | Only reached past preemption gate (user mode or `sti;hlt` — no locks held) |
| Terminal tick callback | **SAFE** | Uses lock-free ring buffer, `try_lock` on input |

### ISR #2: Keyboard Interrupt (`handle_keyboard`)

| Path | Status | Notes |
|------|--------|-------|
| PS/2 `KEYBOARD.try_lock()` | **SAFE** | Non-blocking outer lock |
| `handle_scancode()` → `keymap.lock()` | **SAFE** | Inner lock protected by outer `try_lock` — no external access |
| `input::report_event()` → `try_get_device()` | **SAFE** | Fixed: was `get_device()` → `DEVICES.lock()` (blocking) |
| `input::report_event()` → `try_push_event()` | **SAFE** | Fixed: was `push_event()` → `events.lock()` (blocking) |
| `input::report_event()` → `wake_blocked_reader()` | **SAFE** | Fixed: `BLOCKED_READERS.try_lock()` + `try_wake_up` wrapper |

### ISR #3: Mouse Interrupt (`handle_mouse`)

| Path | Status | Notes |
|------|--------|-------|
| PS/2 `MOUSE.try_lock()` | **SAFE** | Non-blocking outer lock |
| `handle_byte()` → `packet.lock()` | **SAFE** | Inner lock protected by outer `try_lock` |
| `input::report_event()` → `try_get_device()` | **SAFE** | Same fix as keyboard |
| `input::report_event()` → `try_push_event()` | **SAFE** | Same fix as keyboard |
| `input::report_event()` → `wake_blocked_reader()` | **SAFE** | Same fix as keyboard |

### ISR #4: Reschedule IPI (`ipi_reschedule`)

| Path | Status | Notes |
|------|--------|-------|
| `end_of_interrupt()` only | **SAFE** | No locks |

### ISR #5: TLB Shootdown IPI (`ipi_tlb_shootdown`)

| Path | Status | Notes |
|------|--------|-------|
| Callback + `end_of_interrupt()` | **SAFE** | No locks in callback |

## Fixes Applied

### 1. `check_sleepers` → `try_wake_up` (timer ISR)
- **File:** `kernel/syscall/syscall/src/time.rs`
- **Was:** `sched::wake_up(pid)` (blocking)
- **Now:** `sched::try_wake_up(pid)` (non-blocking). Entry stays if lock contended.

### 2. Input subsystem wake callback (keyboard/mouse ISR)
- **File:** `kernel/src/init.rs` — changed `sched::wake_up` → `isr_safe_wake` wrapper (calls `try_wake_up`)
- **File:** `kernel/input/input/src/lib.rs` — changed `BLOCKED_READERS.lock()` → `.try_lock()` in `wake_blocked_reader()`

### 3. IPI_RESCHEDULE handler (required by `wake_up`/`try_wake_up`)
- **File:** `kernel/arch/arch-x86_64/src/exceptions.rs` — `ipi_reschedule` handler
- **File:** `kernel/arch/arch-x86_64/src/idt.rs` — IDT entry for vector 0xF0
- Without this, the IPI causes a GPF (error code 0x783).

### 4. `pick_next_process` — don't reject `pick_next_task` results
- **File:** `kernel/src/scheduler.rs`
- `pick_next_task()` mutates state (pops CFS tree, changes `rq.curr`). Rejecting
  the result leaves the scheduler in a corrupted state.

### 5. Input subsystem `report_event()` — fully ISR-safe (keyboard/mouse ISR)
- **File:** `kernel/input/input/src/lib.rs`
- **Was:** `get_device()` → `DEVICES.lock()` (blocking) + `push_event()` → `events.lock()` (blocking)
- **Now:** `try_get_device()` → `DEVICES.try_lock()` + `try_push_event()` → `events.try_lock()`
- If either lock is contended, the event is dropped. Next keystroke retries.

### 6. Serial output bounded spin — prevent ISR stall on UART saturation
- **File:** `kernel/arch/arch-x86_64/src/serial.rs` — `write_byte_unsafe()` and `write_byte()`
- **File:** `kernel/tty/vt/src/lib.rs` — `dbg_serial()` → delegates to `os_log::write_str_raw()`
- **File:** `kernel/vfs/devfs/src/devices.rs` — `dbg_serial()` + `raw_serial_str()` → delegates to `os_log`
- **File:** `kernel/vfs/vfs/src/file.rs` — inline `write_byte()` bounded spin
- **File:** `kernel/syscall/syscall/src/vfs.rs` — inline `write_byte()` bounded spin (4 copies)
- **Was:** Unbounded `while (LSR & THRE) == 0` spin — ISR blocks forever if UART FIFO full
- **Now:** 2048-iteration spin limit. If FIFO still full, drop the byte. Debug is best-effort.

### 7. Scheduler frame pointer alignment — prevent panic on corrupted RSP
- **File:** `kernel/src/scheduler.rs`
- Align `new_frame_ptr` down to 8-byte boundary before writing `InterruptFrame`.
- Corrupted saved RSP (e.g., from preemption race) no longer panics the scheduler.

## Why Signal Delivery is Safe (despite using `with_rq`)

Signal delivery in the timer ISR (`scheduler.rs:346`) is gated by:
```rust
if in_user_mode && current_pid > 1 { ... }
```
User-mode code (`frame.cs == 0x23`) **never holds kernel locks**. Therefore
`with_rq(this_cpu, ...)` will always acquire immediately — no contention.

## Why PS/2 Inner Locks are Safe (despite using `.lock()`)

`handle_scancode()` calls `self.keymap.lock()` and `handle_byte()` calls
`self.packet.lock()`. These are fields **inside** structs already guarded by
`KEYBOARD.try_lock()` / `MOUSE.try_lock()`. No external code can access the
inner lock without first holding the outer lock, so the inner `.lock()` has
zero contention and completes immediately.

## Adding New ISR Code — Checklist

1. **Never** use `.lock()`, `with_rq()`, `wake_up()`, or `block_current()`.
2. **Always** use `.try_lock()`, `try_with_rq()`, `try_wake_up()`.
3. If the lock is contended, **skip** the work — the next ISR invocation retries.
4. For serial output, use `write_str_unsafe` / `write_byte_unsafe` (no lock).
5. Test with `make run` (4 CPUs, debug-all) to maximize contention window.

## Files Reference

| File | What to check |
|------|---------------|
| `kernel/sched/sched/src/core.rs` | `try_wake_up()` vs `wake_up()` |
| `kernel/syscall/syscall/src/time.rs` | `check_sleepers()` — must use `try_wake_up` |
| `kernel/src/scheduler.rs` | `scheduler_tick()` — ISR entry point |
| `kernel/input/input/src/lib.rs` | `wake_blocked_reader()` — must use `try_lock` + `try_wake_up` |
| `kernel/src/init.rs` | Wake callback registration — must use ISR-safe wrapper |
| `kernel/arch/arch-x86_64/src/exceptions.rs` | All ISR handlers |
| `kernel/arch/arch-x86_64/src/idt.rs` | IDT vector registration (0xF0 IPI_RESCHEDULE) |
| `kernel/drivers/input/ps2/src/lib.rs` | `handle_keyboard_irq` / `handle_mouse_irq` outer `try_lock` |
