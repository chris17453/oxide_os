# exec must update scheduler metadata BEFORE replacing address space in ProcessMeta

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `0399036fac75` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T21:08:02.494314+00:00 |
| Keywords | exec, PML4, scheduler, update_task_exec_info, address_space, race-condition |
| Session | [010a3158f67b](../sessions/clean-up-diagnostic-traces-record-pml4-fix-and-mcp-monitorcommand-fix-in-memgram.md) |

## Details

In kernel_exec, call sched::update_task_exec_info(pid, entry_point, user_stack) BEFORE storing exec_result.address_space into ProcessMeta. The scheduler can context-switch the task at any timer tick — if the new PML4 is already installed but the scheduler still has the old RIP/RSP, the CPU jumps to the old instruction pointer in the new address space, which is typically unmapped → GPF or INVALID OPCODE at wild addresses.
