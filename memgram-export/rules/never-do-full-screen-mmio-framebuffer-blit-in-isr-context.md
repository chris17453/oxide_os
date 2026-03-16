# NEVER do full-screen MMIO framebuffer blit in ISR context

🔴 critical | ❌ dont

| Field | Value |
|-------|-------|
| ID | `b45773387012` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T01:35:08.355047+00:00 |
| Keywords | ISR, MMIO, blit, framebuffer, timer tick, toggle_cursor_blink, mark_all_dirty, triple fault |
| Files | `kernel/tty/terminal/src/lib.rs`, `kernel/tty/terminal/src/renderer.rs`, `kernel/src/console.rs` |
| Session | [f73324620503](../sessions/debug-and-fix-curses-demo-top-triple-fault-crash-qemu-exits-cleanly-during-heavy.md) |

## Details

Any code path reachable from timer ISR (terminal_tick, toggle_cursor_blink, etc.) must NEVER trigger a full-screen MMIO blit. At 1024x768x4bpp = 3MB, rep movsq does 384K MMIO writes. Each MMIO write in KVM = VM exit (~2us). Total: 768ms+ with interrupts disabled. This causes triple faults (QEMU clean exit with -no-reboot). Only blit the minimal dirty region (cursor rows = ~16KB).
