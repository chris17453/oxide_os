# Error: PML4 corruption race in kernel_exec — scheduler could context-switch to task wit

| Field | Value |
|-------|-------|
| ID | `ef2186808604` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T21:07:54.416560+00:00 |
| Keywords | PML4, exec, kernel_exec, race-condition, context-switch, address-space |
| Session | [010a3158f67b](../sessions/clean-up-diagnostic-traces-record-pml4-fix-and-mcp-monitorcommand-fix-in-memgram.md) |

## Error

PML4 corruption race in kernel_exec — scheduler could context-switch to task with stale PML4

## Cause

In kernel_exec (process.rs), the old order was: (1) store new address space in ProcessMeta, (2) call update_task_exec_info to reset scheduler state. Between steps 1 and 2, a timer tick could context-switch the task with the new address space but stale scheduler metadata (old instruction pointer, old stack). This created a race window where the CPU could jump to invalid addresses in the new address space.

## Fix

Reorder: call sched::update_task_exec_info() BEFORE storing the new address space in ProcessMeta. This ensures scheduler state is consistent before the task becomes eligible for scheduling with the new PML4. File: kernel/src/process.rs
