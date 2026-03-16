# Manual sysretq/iretq paths MUST disable interrupts (cli) before touching RSP or swapgs

📌 Pinned | 🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `67225f8c693f` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-13T13:24:43.777302+00:00 |
| Condition | When writing inline asm that does manual sysretq or iretq to return to userspace, especially in fork, exec, or signal return paths |
| Keywords | sysretq, iretq, cli, interrupt, fork, rsp, ist, timer, race |
| Files | `kernel/src/process.rs`, `kernel/arch/arch-x86_64/src/syscall.rs`, `docs/agents/fork-sysretq-cli-requirement.md` |
| Session | [cb4b0c71822b](../sessions/fix-fork-sysretq-timer-interrupt-race-diagnose-and-fix-page-fault-crash-after-fo.md) |

## Details

Any code that manually performs sysretq or iretq to return to userspace MUST have CLI as the first instruction in the asm block. Without CLI, a timer interrupt can fire between `pop rsp` (which switches to user stack) and sysretq/iretq. Since the timer ISR has no IST, ring-0→ring-0 interrupts use current RSP — if RSP points to user memory, the interrupt frame gets pushed there, corrupting scheduler state. sysretq atomically restores IF from R11, and iretq restores from the saved RFLAGS, so interrupts are safely re-enabled on the ring transition itself.
