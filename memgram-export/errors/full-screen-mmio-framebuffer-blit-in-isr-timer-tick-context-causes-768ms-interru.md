# Error: Full-screen MMIO framebuffer blit in ISR (timer tick) context causes 768ms+ inte

| Field | Value |
|-------|-------|
| ID | `ca0fbf4cc27a` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T01:34:42.259362+00:00 |
| Keywords | toggle_cursor_blink, MMIO blit, ISR context, timer tick, triple fault, framebuffer, rep movsq, KVM exit, interrupt latency, renderer.render, mark_all_dirty, blink_counter |
| Files | `kernel/tty/terminal/src/lib.rs`, `kernel/tty/terminal/src/renderer.rs`, `kernel/src/console.rs` |
| Session | [f73324620503](../sessions/debug-and-fix-curses-demo-top-triple-fault-crash-qemu-exits-cleanly-during-heavy.md) |

## Error

Full-screen MMIO framebuffer blit in ISR (timer tick) context causes 768ms+ interrupt blackout, leading to triple faults and QEMU clean exit

## Cause

toggle_cursor_blink() called renderer.render() which incremented blink_counter. Every 15 frames, blink transition called mark_all_dirty() → full-screen back-buffer render → 3MB rep movsq MMIO blit. Each MMIO write = KVM exit (~2us). At 384K writes = 768ms with interrupts disabled. The BSP goes dark for nearly a second. Perf counter showed max timer ISR = 1.7 BILLION cycles (~850ms).

## Fix

Changed toggle_cursor_blink() to only repaint cursor cell: erase_cursor() + paint_cursor() + update_cursor_tracking() + flush_fb(). This blits only 1-2 rows (~16KB) instead of full screen (~3MB). Text BLINK attribute transitions are deferred to process-context writes. Max timer IRQ dropped from 1.7B to 134M cycles (12x improvement).
