# Fork must use parent-runs-first and distribute children across CPUs

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `35d80327f741` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-13T19:48:19.322011+00:00 |
| Condition | When modifying kernel fork/clone implementation |
| Keywords | fork, scheduler, smp, parentrunsfirst, sysretq, cpudistribution |
| Files | `kernel/src/process.rs`, `kernel/sched/sched/src/core.rs` |
| Session | [3bfe90c6680d](../sessions/fix-multi-vt-terminal-spawning-only-1-vt-gets-a-working-shell-debug-and-fix-the.md) |

## Details

kernel_fork() must return child_pid to parent through normal syscall return path. The child gets added to a runqueue (round-robin across CPUs) and scheduled by the next timer tick. NEVER use switch_to + sysretq to jump directly into the child — this causes: (1) all tasks pile onto CPU#0 while CPUs 1-3 idle, (2) parent starvation when rapid-forking, (3) PARENT_CONTEXT single-global is SMP-unsafe. Linux switched to parent-runs-first in 2.6.32 for the same reasons.
