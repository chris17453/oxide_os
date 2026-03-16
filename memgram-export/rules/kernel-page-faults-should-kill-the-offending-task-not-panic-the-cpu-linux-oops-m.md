# Kernel page faults should kill the offending task, not panic the CPU — Linux oops model

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `059b54aad1e8` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-14T00:37:03.586951+00:00 |
| Condition | When handling kernel-mode exceptions that could be attributed to a user task |
| Keywords | pagefault, kerneloops, cpurescue, panic, sigsegv |
| Files | `kernel/arch/arch-x86_64/src/exceptions.rs` |

## Details

When a kernel-mode page fault occurs while executing on behalf of a user task, use the USER_FAULT_KILL_CALLBACK to kill the task (SIGSEGV) and let the CPU continue to its idle loop. Only panic if there's no kill callback (early boot before scheduler). Losing 25% of compute because one process triggered a kernel bug is unacceptable. The CPU rescue path is in kernel/arch/arch-x86_64/src/exceptions.rs handle_page_fault.
