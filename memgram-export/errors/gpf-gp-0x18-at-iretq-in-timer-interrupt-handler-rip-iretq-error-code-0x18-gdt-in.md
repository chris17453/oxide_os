# Error: GPF (#GP 0x18) at iretq in timer_interrupt handler. RIP=iretq, error code 0x18 =

| Field | Value |
|-------|-------|
| ID | `2e1639a3d57d` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-02T20:42:43.662268+00:00 |
| Keywords | GPF, iretq, SS, CS, segment, SMP, AP, idle, scheduler, iret frame, context switch, timer interrupt |
| Files | `kernel/src/scheduler.rs`, `kernel/src/smp_init.rs`, `kernel/sched/sched/src/core.rs`, `kernel/sched/sched/src/lib.rs` |

## Error

GPF (#GP 0x18) at iretq in timer_interrupt handler. RIP=iretq, error code 0x18 = GDT index 3 (USER_DATA). Caused by iret frame having CS=0x08 (kernel) with SS=0x1B (user) — iretq same-privilege return requires SS.RPL == CPL(0), but SS=0x1B has RPL=3.

## Cause

Two root causes: (1) scheduler.rs iret frame builder defaulted ss=0→0x1B for ALL contexts, including kernel-mode CS=0x08 tasks. (2) APs had no idle Task in their run queue BTreeMap — pick_next_task returned idle PID 0 but context_switch_transaction failed (get_task(0)→None), corrupting rq.curr and causing cascading scheduler state issues with stale/default task contexts.

## Fix

Three fixes applied: (1) Derive SS from CS in iret frame builder — CS=0x08→SS=0x10, CS=0x23→SS=0x1B. Only two valid combos in our GDT. (2) Create proper per-AP idle Tasks via sched::add_task_to_cpu() in smp_init.rs with cs=0x08, ss=0x10. (3) Fixed AP idle loop to use allow_kernel_preempt()+sti+hlt like BSP, not yield_current+hlt.
