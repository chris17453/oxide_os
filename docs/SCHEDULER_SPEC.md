# EFFLUX Scheduler Specification

**Version:** 1.0
**Status:** Draft
**License:** MIT

---

## 0) Overview

EFFLUX uses a preemptive priority-based scheduler with per-CPU run queues.

---

## Architecture-Specific Documentation

See `docs/arch/<arch>/CONTEXT.md` for context switching details:

- [x86_64](arch/x86_64/CONTEXT.md)
- [i686](arch/i686/CONTEXT.md)
- [AArch64](arch/aarch64/CONTEXT.md)
- [ARM32](arch/arm/CONTEXT.md)
- [MIPS64](arch/mips64/CONTEXT.md)
- [MIPS32](arch/mips32/CONTEXT.md)
- [RISC-V 64](arch/riscv64/CONTEXT.md)
- [RISC-V 32](arch/riscv32/CONTEXT.md)

---

## 1) Design

| Property | Value |
|----------|-------|
| Model | Priority-based + round-robin |
| Preemption | Fully preemptive |
| SMP | Per-CPU run queues + work stealing |
| Time slice | 10ms default |
| Priority levels | 0-31 (0 = highest) |

---

## 2) Thread State

| State | Description |
|-------|-------------|
| Running | Currently executing on a CPU |
| Ready | Runnable, waiting for CPU |
| Blocked | Waiting on I/O, mutex, sleep, etc. |
| Zombie | Terminated, awaiting parent reap |

---

## 3) Priority Levels

| Range | Name | Use |
|-------|------|-----|
| 0-7 | Real-time | Kernel threads, critical |
| 8-15 | High | Interactive |
| 16-23 | Normal | Regular applications |
| 24-31 | Low/Idle | Background |

Default user priority: 20

---

## 4) Data Structures

### Thread

| Field | Description |
|-------|-------------|
| tid | Thread ID |
| process | Parent process |
| state | Running/Ready/Blocked/Zombie |
| priority | Current priority (0-31) |
| cpu_affinity | Allowed CPUs |
| time_slice | Remaining ticks |
| context | Arch-specific saved registers |
| kernel_stack | Per-thread kernel stack |

### Per-CPU Run Queue

| Field | Description |
|-------|-------------|
| current | Currently running thread |
| queues[32] | One queue per priority |
| bitmap | Non-empty queue mask |

---

## 5) Algorithm

1. **Pick next:** Find highest priority non-empty queue, dequeue front
2. **Enqueue:** Add to tail of appropriate priority queue, update bitmap
3. **Preempt:** On timer tick, decrement time slice; if zero, reschedule

---

## 6) Load Balancing

- Periodic check for imbalanced CPUs
- Work stealing from overloaded to underloaded
- Respects CPU affinity

---

## 7) Syscalls

| Syscall | Description |
|---------|-------------|
| sched_yield | Voluntarily yield CPU |
| sched_setscheduler | Set scheduling policy |
| sched_getscheduler | Get scheduling policy |
| sched_setaffinity | Set CPU affinity |
| sched_getaffinity | Get CPU affinity |
| nice | Adjust priority |
| getpriority/setpriority | Get/set priority |

---

## 8) Exit Criteria

- [ ] Preemptive scheduling works
- [ ] Multiple priority levels work
- [ ] Per-CPU run queues implemented
- [ ] Load balancing functional
- [ ] CPU affinity respected
- [ ] Works on all architectures

---

*End of EFFLUX Scheduler Specification*
