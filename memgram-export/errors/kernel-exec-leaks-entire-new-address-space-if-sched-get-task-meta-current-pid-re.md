# Error: kernel_exec leaks entire new address space if sched::get_task_meta(current_pid) 

| Field | Value |
|-------|-------|
| ID | `b79ebc70c04c` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:37:41.143199+00:00 |
| Keywords | exec, address-space-leak, get_task_meta, enter_usermode, Drop, frame-leak |
| Session | [b27c9dc86d34](../sessions/continue-memory-hardening-fix-remaining-audit-issues-cow-toctou-race-exec-as-lea.md) |

## Error

kernel_exec leaks entire new address space if sched::get_task_meta(current_pid) returns None — enter_usermode_with_context never returns so exec_result is never dropped

## Cause

The code used `if let Some(meta) = get_task_meta(...)` and continued to switch to the new PML4 even when meta was None. Since enter_usermode_with_context never returns, exec_result (containing the new UserAddressSpace) was never dropped — all frames in the new address space leaked permanently.

## Fix

Changed to `match` that returns -ESRCH (error) when meta is None. exec_result is dropped in the error path, triggering UserAddressSpace::Drop which frees all frames properly.
