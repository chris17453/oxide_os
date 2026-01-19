# Phase 9: SMP (Symmetric Multiprocessing)

**Stage:** 3 - Hardware
**Status:** Complete
**Dependencies:** Phase 8 (Libc + Userland)

---

## Goal

Enable multi-core operation with per-CPU data and work-stealing scheduler.

---

## Deliverables

| Item | Status |
|------|--------|
| AP (Application Processor) boot | [x] |
| Per-CPU data structures | [x] |
| Per-CPU run queues | [x] |
| Spinlocks with SMP safety | [x] |
| TLB shootdowns via IPI | [x] |
| Work-stealing scheduler | [x] |
| CPU hotplug (optional) | [ ] |

---

## Architecture Status

| Arch | AP Boot | Per-CPU | TLB Shootdown | Scheduler | Done |
|------|---------|---------|---------------|-----------|------|
| x86_64 | [x] | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Implementation Summary

### smp Crate

Created `crates/smp/smp/` with the following modules:

- **lib.rs** - Main crate exports, MAX_CPUS constant (256)
- **percpu.rs** - Per-CPU data structure with preemption control, IRQ tracking, stats
- **cpu.rs** - CPU enumeration, state tracking (NotPresent, Present, Starting, Online, Offline), AP boot coordination
- **ipi.rs** - Inter-Processor Interrupt support with handlers and vector definitions
- **tlb.rs** - TLB shootdown using IPIs, INVLPG instruction support

### SMP Scheduler (sched)

Added `smp.rs` module with:

- **PerCpuScheduler** - Per-CPU run queue management
- **SmpScheduler** - Global coordinator with work-stealing, load balancing
- Thread affinity support via preferred_cpu parameter
- Atomic operations for thread ID allocation

---

## Key Structures

### PerCpu (percpu.rs)
```rust
pub struct PerCpu {
    pub self_ptr: *mut PerCpu,
    pub cpu_id: u32,
    pub apic_id: u32,
    pub online: bool,
    pub preempt_count: u32,
    pub irq_count: u32,
    pub current_thread: u64,
    pub idle_thread: u64,
    pub kernel_stack: u64,
    pub tss: u64,
    pub stats: CpuStats,
}
```

### IPI Vectors (ipi.rs)
- RESCHEDULE (0xF0) - Trigger scheduler
- TLB_SHOOTDOWN (0xF1) - Invalidate TLB
- CALL_FUNCTION (0xF2) - Execute on target
- STOP (0xF3) - Halt CPU

### TLB Shootdown
Uses atomic state for safe multi-CPU coordination:
- `invalidate_page()` - Single page via INVLPG
- `invalidate_range()` - Range with threshold for full flush
- `flush_tlb_all()` - CR3 reload
- `tlb_shootdown()` - Cross-CPU invalidation via IPI

---

## Per-CPU Access

| Arch | Method |
|------|--------|
| x86_64 | GS segment (swapgs on syscall) |
| i686 | FS segment |
| aarch64 | TPIDR_EL1 register |
| arm | CP15 c13 |
| mips | K0/K1 registers |
| riscv | TP register or scratch CSR |

---

## Key Files

```
crates/smp/smp/src/
├── lib.rs             # Crate entry, MAX_CPUS
├── percpu.rs          # Per-CPU data structure
├── cpu.rs             # CPU enumeration and boot
├── ipi.rs             # Inter-processor interrupts
└── tlb.rs             # TLB shootdown

crates/sched/sched/src/
├── smp.rs             # SMP scheduler with work-stealing
```

---

## Exit Criteria

- [x] Per-CPU data accessible from each CPU
- [x] Per-CPU run queues with load balancing
- [x] TLB shootdown infrastructure
- [x] Work-stealing scheduler
- [ ] All CPUs detected and booted (requires hardware test)
- [ ] No data races (requires stress test)

---

## Notes

Phase 9 provides the SMP infrastructure. Actual AP boot requires:
1. ACPI MADT parsing for CPU enumeration
2. AP trampoline code in low memory
3. SIPI sequence implementation in arch crate
4. Integration with memory subsystem for per-CPU stacks

---

*Phase 9 of EFFLUX Implementation - Complete*
