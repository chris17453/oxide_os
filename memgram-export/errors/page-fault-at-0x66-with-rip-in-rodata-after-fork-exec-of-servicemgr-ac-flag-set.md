# Error: Page fault at 0x66 with RIP in .rodata after fork+exec of servicemgr. AC flag SE

| Field | Value |
|-------|-------|
| ID | `667ebffe152c` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-13T13:24:37.314696+00:00 |
| Keywords | fork, sysretq, timer, interrupt, race, cli, pagefault, rodata, ist, rsp, stackcorruption |
| Files | `kernel/src/process.rs`, `kernel/arch/arch-x86_64/src/exceptions.rs`, `kernel/arch/arch-x86_64/src/syscall.rs` |
| Session | [cb4b0c71822b](../sessions/fix-fork-sysretq-timer-interrupt-race-diagnose-and-fix-page-fault-crash-after-fo.md) |

## Error

Page fault at 0x66 with RIP in .rodata after fork+exec of servicemgr. AC flag SET (inside STAC/CLAC window). Corrupted control flow — RIP points to string data in .rodata, not executable code.

## Cause

Timer interrupt race in fork's manual sysretq path. The fork handler (process.rs) runs with interrupts ENABLED. After `pop rsp` switches to user stack but before sysretq, a timer interrupt fires. Since timer ISR has no IST, ring-0→ring-0 interrupts use current RSP — which now points to user memory. This corrupts the scheduler's interrupt frame, causing RIP to land in .rodata on the next context switch back.

## Fix

Added `cli` as first instruction in the fork handler's inline asm block that does manual sysretq to child. sysretq atomically restores IF from R11 (RFLAGS with IF set), so interrupts are re-enabled on ring transition without a race window.
