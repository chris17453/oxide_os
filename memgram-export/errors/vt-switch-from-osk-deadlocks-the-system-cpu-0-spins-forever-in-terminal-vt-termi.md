# Error: VT switch from OSK deadlocks the system. CPU#0 spins forever in terminal::VT_TER

| Field | Value |
|-------|-------|
| ID | `dc4f8f21967a` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-13T19:39:32.398871+00:00 |
| Keywords | vtswitch, deadlock, isr, spinlock, compositor, terminal, timer, vkbd, osk |
| Files | `kernel/src/console.rs`, `kernel/tty/compositor/src/lib.rs`, `kernel/tty/terminal/src/lib.rs` |
| Session | [3bfe90c6680d](../sessions/fix-multi-vt-terminal-spawning-only-1-vt-gets-a-working-shell-debug-and-fix-the.md) |

## Error

VT switch from OSK deadlocks the system. CPU#0 spins forever in terminal::VT_TERMINALS[0].lock() called from resize_vt() inside timer ISR context. Full chain: timer ISR → terminal_tick → handle_tap → handle_vkbd_action(SwitchVt) → compositor::focus_vt → apply_layout_change → resize_vt → VT_TERMINALS.lock() — deadlock because a userspace process on the same CPU already holds VT_TERMINALS lock.

## Cause

handle_vkbd_action called focus_vt() synchronously from ISR context. focus_vt → apply_layout_change → resize_vt uses blocking spin locks (COMPOSITOR.lock(), VT_TERMINALS.lock()). If any process on this CPU holds either lock when timer fires, permanent deadlock.

## Fix

Defer VT switch actions from ISR. Store target VT in PENDING_VT_SWITCH atomic. Process from safe context (syscall or dedicated softirq-like path). ISR only sets atomic + updates vkbd highlight (no locks).
