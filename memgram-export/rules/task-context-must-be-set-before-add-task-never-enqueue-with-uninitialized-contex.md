# Task context MUST be set before add_task — never enqueue with uninitialized context

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `2d78b60378d5` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T01:04:16.456015+00:00 |
| Keywords | scheduler, context-switch, fork, clone, race-condition, TaskContext, add_task, GPF |
| Session | [e94f77c58e61](../sessions/implement-scheduler-context-switch-hardening-fix-fork-clone-init-race-safe-taskc.md) |

## Details

Fork, clone, and init must set child_task.context = child_task_ctx BEFORE calling sched::add_task(child_task). The old pattern of add_task() then set_task_context() creates a race window where any CPU's timer tick can pick up the task with cs=0, ss=0, rip=0 — causing intermittent GPF/page faults across 4 CPUs at 100Hz. Three layers of defense: (1) set context before enqueue, (2) TaskContext::default() uses valid kernel selectors cs=0x08/ss=0x10, (3) context_switch_transaction validates is_schedulable() before iretq.
