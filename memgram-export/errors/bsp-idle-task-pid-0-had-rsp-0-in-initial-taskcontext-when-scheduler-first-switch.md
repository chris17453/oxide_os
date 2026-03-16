# Error: BSP idle task (PID 0) had rsp=0 in initial TaskContext. When scheduler first swi

| Field | Value |
|-------|-------|
| ID | `47eaf8f22705` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T02:15:36.434908+00:00 |
| Keywords | scheduler, idle, BSP, rsp, underflow, context-switch, memory-corruption |
| Session | [e94f77c58e61](../sessions/implement-scheduler-context-switch-hardening-fix-fork-clone-init-race-safe-taskc.md) |

## Error

BSP idle task (PID 0) had rsp=0 in initial TaskContext. When scheduler first switches to idle before a timer tick overwrites the context, frame builder computes rsp(0) - frame_size → underflow to 0xFFFFFFFFFFFFFF60 → writes iretq frame to arbitrary kernel memory → cascading corruption causing random GPF/PF crashes.

## Cause

scheduler::init() set idle task context rip, cs, ss, rflags but NOT rsp. TaskContext::default() has rsp=0. AP idle code in smp_init.rs already had the fix (captures boot RSP), but BSP idle was missed.

## Fix

Capture boot RSP via inline asm before creating idle task context, set ctx.rsp = boot_rsp. Same pattern as AP idle in smp_init.rs.
