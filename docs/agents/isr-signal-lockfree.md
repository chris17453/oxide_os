# ISR Signal Delivery Must Be Lock-Free

## Rule
The keyboard IRQ signal fast-path (Ctrl+C → SIGINT, Ctrl+\ → SIGQUIT, Ctrl+Z → SIGTSTP)
MUST be fully lock-free. NEVER use try_lock() to read ISIG or foreground PGID.

## Why
During heavy terminal output (`cat /dev/urandom`, large compiler output, etc.):
1. `write_vt()` holds VT_TERMINALS lock with preemption disabled
2. The TTY ldisc and foreground_pgid have their own Mutexes
3. Keyboard IRQ fires during the render
4. `push_input()` signal fast-path tries to check ISIG and read PGID
5. If ANY try_lock in the chain fails → Ctrl+C byte enters ring but signal is dropped
6. If the process isn't reading from the TTY (reading from urandom/pipes/sockets),
   nobody drains the ring buffer → signal byte rots forever → process unkillable

## Solution
Per-VT atomic caches on VtManager:
- `cached_pgid: [AtomicI32; MAX_VTS]` — shadow of foreground PGID
- `cached_isig: [AtomicBool; MAX_VTS]` — shadow of ISIG flag

Updated atomically (Release ordering) when:
- `TIOCSPGRP` ioctl sets foreground process group
- `TCSETS/TCSETSW/TCSETSF` ioctl changes termios

Read by ISR (Acquire ordering) with zero locks:
```rust
let isig = self.cached_isig[active].load(Ordering::Acquire);
let pgid = self.cached_pgid[active].load(Ordering::Acquire);
if isig && pgid > 0 {
    callback(pgid, signo); // Guaranteed delivery
}
```

## TTY VT Number
Each TTY now knows its VT index (`tty.vt_num`, like Linux's `vc_data.vc_num`).
Set via `Tty::with_vt_num()` during VT init. PTYs get -1.

## Files
- `kernel/tty/vt/src/lib.rs` — VtManager cached_pgid/cached_isig, push_input fast-path
- `kernel/tty/tty/src/tty.rs` — Tty.vt_num field, with_vt_num constructor

— GraveShift
