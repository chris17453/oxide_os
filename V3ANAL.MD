# V3 Scheduler/Process/Context Switch Audit (x86_64)

## Scope
- Scheduler bridge (`kernel/src/scheduler.rs`), process syscalls (`kernel/src/process.rs`), arch context/interrupt (`crates/arch/arch-x86_64/src/{context.rs,exceptions.rs}`), and scheduler core (`crates/sched/*`).
- Focus: context save/restore, CR3/stack handling, preemption points, multi-task/run-queue behavior, and state duplication leading to crashes or wrong task after switch.

## Architecture in brief
- **Scheduling policy/state**: in `sched` crate (`RunQueue`, `Task` with `TaskContext`, `TaskState`, vruntime, RT, affinity). Per-CPU run queues (single CPU current now). Tasks can be marked runnable/blocked; time slice in tick handler.
- **Interrupt entry**: `arch-x86_64::exceptions::timer_interrupt` saves GPRs + frame, calls registered scheduler callback with RSP; callback returns new RSP; registers restored and `iretq`.
- **Kernel bridge**: `kernel::scheduler::scheduler_tick` converts interrupt frame to `TaskContext`, selects next task, writes new `InterruptFrame` on target kernel stack, switches CR3, updates TSS kernel stack, returns new RSP.
- **Syscall/user switches**: `kernel::process` implements fork/exec/exit/yield with inline asm `sysretq` sequences; manual CR3 writes and stack pointer staging; uses static contexts (`EXIT_CTX`, `FORK_CHILD_CTX`, `YIELD_CTX`).

## Multithreaded/multitask expectations
- System target: many runnable tasks/threads concurrently (background services + interactive sessions), time-sliced fairly rather than blocked by any single task.
- Parent/child semantics: when a child (e.g., `ls`) exits, the parent (e.g., shell) must be made runnable immediately (wait/wakeup) and scheduled next eligible slot; no lingering blocked parent after child exit.
- Preemption must work while multiple runnable tasks exist: timer tick should rotate through ready queue, honoring priorities/vruntime; no single-task pinning due to missing preempt_enable or try-lock skips.
- Threads within a process must each own a kernel stack, TaskContext, and run-queue slot; context switch logic must not assume 1:1 process:thread.
- Background services should remain runnable even when foreground shells are idle; scheduler should avoid starvation and ensure forward progress for all runnable tasks.

## High-risk areas / crash vectors
1) **Dual state sources**: `kernel::scheduler` keeps contexts in `sched` Task structs, but `kernel::process` also stores contexts in static locals for sysret paths; risk of divergence or stale data (e.g., parent/child sysret uses `PARENT_CONTEXT`/statics).
2) **CR3 writes without fences**: Multiple inline `mov cr3, ...` in process and scheduler code lack serialization (no `mfence`/`sfence` or `swapgs` ordering aside from the asm block). Potential stale TLB/shootdown gaps on SMP.
3) **Stack pointer sourcing**: Timer path expects `InterruptFrame` at current RSP; scheduler builds a new frame on *next* task's kernel stack top minus frame size. If kernel_stack_size wrong or not mapped, crash on pop/iretq. Idle task uses placeholder stack (PhysAddr 0) — risk if ever selected with `cs==0x23` path.
4) **GS/base and sysret**: Sysret sequences in fork/exit/yield manually load selectors (ds/es/fs) and rely on `swapgs` just before `sysretq`. Any missing `swapgs` symmetry in interrupt entry/exit can corrupt GS base or TSS.
5) **Preemption gating**: `scheduler_tick` skips kernel preemption unless `arch::is_kernel_preempt_allowed` flag set. If a kernel path forgets to set/clear, tasks may starve or switches occur mid non-preempt-safe section.
6) **Context completeness**: `TaskContext` lacks segment base saving (only CS/SS). No FPU/SSE context save/restore in switch path (could corrupt FP state across tasks).
7) **Locking during interrupts**: `sched::scheduler_tick` uses try-lock; skipped ticks can leave `need_resched` uncleared, delaying switches; deadlock avoided but can cause drift.
8) **Run queue selection**: `pick_next_process` falls back to current if `get_task_meta` missing — can mask bugs where task is missing meta; could return idle placeholders.
9) **Kernel/user distinction**: `scheduler_tick` only updates TSS stack if `next_ctx.cs == 0x23`; if a kernel thread uses kernel CS, TSS not updated — next user switch might still use old RSP0?
10) **Inline asm correctness**: Fork/exit/yield asm manually moves many registers and pushes user RSP; any layout mismatch with `ProcessContext`/`TaskContext` will crash on return. No validation of IF beyond OR-ing rflags.
11) **SMP shadow vars**: CURRENT_CPU and CURRENT_PIDS are singletons (not per-CPU TLS); switching CPUs not fully wired (comment “single CPU for now”). On SMP, stale current_pid_lockfree could pick wrong current in interrupt.
12) **Idle task stack**: Idle task created with PhysAddr::new(0), size 0; if ever context-switched using user path, frame write will fault. Currently idle uses BSP stack, but safety depends on not mapping idle via user CS.

## Misalignments / duplication
- Process lifecycle state split: `sched::Task` is “authority”, but `kernel::process` retains `PARENT_CONTEXT`, `USER_EXITED`, `USER_EXIT_STATUS`, `PARENT_CONTEXT` mutex — duplicated exit/wait state.
- Context storage duplication: `TaskContext` in sched vs static `ProcessContext` copies for sysret; manual copies instead of single source.
- Preempt control split: `arch::allow_kernel_preempt/clear` flag separate from `sched::preempt_disable` counters.

## Recommendations (analysis-only)
1) Single source for context: store user/kern contexts only in `sched::TaskContext`; have fork/exit/yield read/write through sched accessors; remove static `*_CTX` staging or make them transient buffers verified against `TaskContext`.
2) Harden switch path:
   - Validate kernel_stack_phys/size before frame write; guard idle stack special-case.
   - Add barriers around CR3 updates (`swapgs; mov cr3;` plus optional `lfence`/`mfence` or `sfence.vma` equivalent not available; ensure TLB shootdown hooks used).
   - Ensure TSS RSP0 updated on every user-bound switch (even if CS suggests kernel? confirm invariant).
3) Save full CPU state:
   - Add XSAVE/FPU context save/restore per task, or eager fxsave/fxrstor in switch.
4) Preemption model:
   - Unify preempt gating with sched preempt_count; audit kernel sites that set `allow_kernel_preempt`.
5) SMP correctness:
   - Make CURRENT_CPU/PIDS per-CPU, or guard with per-CPU storage; wire AP bring-up to set current CPU and init run queues.
6) Robustness:
   - Add consistency assertions: `get_task_switch_info` must not return zero kernel_stack; fail-fast.
   - In `pick_next_process`, treat missing meta as fatal/error path rather than silently running current.
7) Sysret sequences:
   - Encapsulate the sysret return builder in one arch helper that consumes `TaskContext`; remove duplicate asm blocks; verify segment selectors and IF always set.
8) Blocking/wait:
   - Ensure wake paths clear waiting state and requeue correctly; audit `block_current` + `wake_up` for races when try_lock skips.
9) Multitask fairness:
   - Confirm timer tick enqueues all runnable tasks and advances vruntime; ensure background services keep running when shell is idle; verify parent wakeups requeue immediately after child exit.

## Quick suspicion points for current crashes
- Wrong kernel_stack_phys/size for a task leading to bad InterruptFrame write during switch.
- TaskContext not updated before switch (e.g., interrupt in kernel not allowed to preempt, but need_resched set; next switch uses stale context).
- Fork/exit sysret paths writing incorrect RSP/RIP due to misaligned static context or missing swapgs symmetry.
- Parent wakeup lost or delayed after child exit, leaving shell blocked while no runnable tasks remain; run queue may then select idle and never return to shell.
