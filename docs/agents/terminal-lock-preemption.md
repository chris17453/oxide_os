# Terminal Lock Preemption Safety

## Rule

`terminal::write()` and `terminal::write_immediate()` MUST disable kernel preemption
while holding `TERMINAL.lock()`, and re-enable after releasing. ISR-context callers
(tick, try_flush) MUST use `try_lock()` — never blocking `lock()`.

## Why — Two Problems, Two Fixes

### Problem 1: ISR Deadlock (VT Switch)

The keyboard ISR fires Alt+F1-F6 → `vt::switch_to()` → `terminal::flush()`.
If `sys_write` holds `TERMINAL.lock()` on the same CPU:

```
CPU0: sys_write → TERMINAL.lock() [HELD]
CPU0: ← keyboard ISR fires
CPU0: ISR → switch_to() → ACTIVE_VT.write() [BLOCKS — sys_write holds .read()]
          OR → terminal::flush() → TERMINAL.lock() [BLOCKS — same CPU]
→ DEADLOCK: ISR can never return, lock holder can never release
```

**Fix**: `switch_to()` uses `ACTIVE_VT.try_write()`, `terminal_vt_switch_callback()` uses
`terminal::try_flush()`. Both bail if contended — VT switch deferred to next keypress.

### Problem 2: Preemption While Holding Spinlock

`sys_write` sets `kpo=1` (kernel preemption OK) before calling `terminal::write()`.
If preempted while holding the lock, other tasks spin on it until the holder resumes:

```
CPU0: Task A holds TERMINAL.lock(), kpo=1
CPU0: ← timer ISR, preempts Task A
CPU0: Task B → TERMINAL.lock() → spins (kpo=1, so B is preemptable too)
CPU0: ← timer ISR, preempts Task B
CPU0: Task C → spins... (wasted context switches)
```

**Fix**: Linux `spin_lock()`/`spin_unlock()` pattern:
1. kpo=1 while WAITING for lock (so holder can be scheduled back)
2. kpo=0 once we HOLD the lock (finish fast, no preemption mid-render)
3. kpo=restored after releasing lock

## Implementation

```rust
// terminal::write()
let was_preemptable = arch_x86_64::is_kernel_preempt_allowed();
let mut guard = TERMINAL.lock();       // kpo=1: preemptable while spinning
if was_preemptable {
    arch_x86_64::disallow_kernel_preempt(); // kpo=0: non-preemptable while holding
}
// ... terminal.write(data) ...
drop(guard);                           // release lock
if was_preemptable {
    arch_x86_64::allow_kernel_preempt();   // restore kpo=1
}
```

## Functions and Their Lock Strategy

| Function | Context | Lock Method | Preempt Control |
|----------|---------|-------------|-----------------|
| `write()` | syscall | `TERMINAL.lock()` | disable while held |
| `write_immediate()` | syscall | `TERMINAL.lock()` | disable while held |
| `tick()` | timer ISR | `TERMINAL.try_lock()` | N/A (ISR) |
| `try_flush()` | any ISR | `TERMINAL.try_lock()` | N/A (ISR) |
| `flush()` | syscall only | `TERMINAL.lock()` | caller's responsibility |
| `has_mouse_mode()` | timer ISR | `MOUSE_INPUT.try_lock()` | N/A (ISR) |
| `mouse_event()` | timer ISR | `MOUSE_INPUT.try_lock()` | N/A (ISR) |

## Lock Splitting (P3.5)

Two locks protect different concerns:

- **`TERMINAL: Mutex<Option<TerminalEmulator>>`** — full emulator state (screen
  buffer, renderer, parser, cursor, scrollback, clipboard). Held during all write
  and render operations. ISR uses `try_lock()` for tick/flush only.

- **`MOUSE_INPUT: Mutex<MouseInputState>`** — ISR-facing mouse state mirror
  (`mouse_mode`, `mouse_encoding`, `cell_width`, `cell_height`, `cols`, `rows`).
  Held only for ~6 word copies. Never contended by write path.

**Lock order** (must be respected to prevent inversion):
1. `TERMINAL` (outer — write path holds this)
2. `MOUSE_INPUT` (inner — acquired ONLY after TERMINAL is released)

The write path captures a mouse-state snapshot while holding `TERMINAL`, releases
`TERMINAL`, then updates `MOUSE_INPUT`. ISR only ever acquires `MOUSE_INPUT`.
The two locks are NEVER held simultaneously.

## Key Invariants

- ISR code MUST use `try_lock()` on BOTH `TERMINAL` and `MOUSE_INPUT` — never `lock()`
- `ACTIVE_VT` in ISR: use `try_read()`/`try_write()` — never `read()`/`write()`
- `TERMINAL` and `MOUSE_INPUT` are NEVER held simultaneously — release-before-acquire
- Preemption disable around spinlock hold is an OPTIMIZATION, not correctness
  (without it: wasted context switches; with it: faster lock release)
