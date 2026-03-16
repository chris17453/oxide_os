# exec MUST fail if get_task_meta returns None — never continue to enter_usermode with orphaned address space

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `a7eef8dec619` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:37:46.218440+00:00 |
| Keywords | exec, kernel_exec, get_task_meta, address-space, enter_usermode, frame-leak |
| Session | [b27c9dc86d34](../sessions/continue-memory-hardening-fix-remaining-audit-issues-cow-toctou-race-exec-as-lea.md) |

## Details

In kernel_exec (process.rs), after do_exec succeeds and creates a new address space, the code MUST store exec_result.address_space into the ProcessMeta before switching to the new PML4. If get_task_meta returns None (should be impossible for the running PID, but handle it), return an error code (-ESRCH) so exec_result is dropped and UserAddressSpace::Drop frees all frames. Never continue to enter_usermode_with_context with an unstored address space — it will never be dropped since that function never returns.
