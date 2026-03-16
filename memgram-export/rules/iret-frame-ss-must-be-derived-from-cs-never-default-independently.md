# iret frame SS must be derived from CS — never default independently

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `a7599aaae0e8` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-02T20:42:51.797538+00:00 |
| Condition | When building iret frames for context switches in the scheduler |
| Keywords | iret, SS, CS, GPF, scheduler, context switch, segment |
| Files | `kernel/src/scheduler.rs` |

## Details

In the scheduler's iret frame builder, ALWAYS derive SS from CS. Only two valid combos exist in OXIDE's GDT: CS=0x08→SS=0x10 (kernel), CS=0x23→SS=0x1B (user). Never default SS independently (e.g., ss=0→0x1B) because a kernel-mode task with ss=0 would get user SS, causing #GP on iretq.
