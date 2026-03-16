# Session: Debug RSP-to-RIP clobber in scheduler context switch: trace register flow from ISR save → SwitchInfo → iret frame to find where context gets corrupted

| Field | Value |
|-------|-------|
| ID | `e85527ff0ebb` |
| Agent | claude-code |
| Model | claude-opus-4-6 |
| Project | oxide-os |
| Branch | - |
| Status | completed |
| Started | 2026-03-03T01:58:42.678856+00:00 |
| Ended | 2026-03-03T02:00:11.207050+00:00 |
| Compactions | 0 |

## Summary

Successfully debugged RSP-to-RIP clobber in OXIDE OS scheduler context switch mechanism. Traced register flow from timer ISR → scheduler_tick → context_switch_transaction → iret frame building. Identified root cause: frame.rsp (user RSP from interrupt context) incorrectly used for kernel-mode frame placement, causing new interrupt frame to be written to user space instead of kernel stack.

## Session Summary

**Outcome:** Root cause identified with complete code flow analysis. Bug: scheduler_tick line 674 copies frame.rsp (user RSP) into TaskContext.rsp. Later, line 898-900 uses this user RSP to calculate frame placement for kernel-mode tasks, writing frame to user address space. iretq pops RIP from wrong location, causing page fault. Fix: Add kernel_stack_rsp field to TaskContext to track actual kernel stack location, use it instead of rsp for kernel-mode frame placement.

**Decisions:**

- Root cause is NOT ISR/exception handling — frame layout is correct
- Root cause is NOT syscall path — saves/restores are correct
- Root cause IS scheduler context switch logic — confuses user RSP with kernel stack RSP
- Recommended fix: Approach A (store kernel_stack_rsp separately in TaskContext)
- Three possible fix strategies evaluated; Approach A chosen for clarity and correctness

**Unresolved:**

- Implementation of fix requires modifying TaskContext struct and scheduler_tick logic
- Need to verify all TaskContext instantiations handle new field
- Regression testing needed after fix implementation
