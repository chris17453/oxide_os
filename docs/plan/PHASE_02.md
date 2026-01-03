# Phase 2: Interrupts + Timer + Scheduler

**Stage:** 1 - Foundation
**Status:** Not Started
**Dependencies:** Phase 1 (Memory)

---

## Goal

Preemptive multitasking with kernel threads.

---

## Deliverables

| Item | Status |
|------|--------|
| Interrupt controller setup | [ ] |
| Exception handlers | [ ] |
| Timer driver | [ ] |
| Kernel thread creation | [ ] |
| Context switch | [ ] |
| Preemptive scheduler | [ ] |
| Per-CPU run queues | [ ] |

---

## Architecture Status

| Arch | Interrupts | Timer | Context | Scheduler | Done |
|------|------------|-------|---------|-----------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Interrupt Controllers

| Arch | Controller | Notes |
|------|------------|-------|
| x86_64 | APIC + IOAPIC | Also PIC fallback |
| i686 | PIC or APIC | PIC for legacy |
| aarch64 | GIC v2/v3 | Generic Interrupt Controller |
| arm | GIC v2 | Generic Interrupt Controller |
| mips64/mips32 | CP0 Cause/Status | Software dispatch |
| riscv64/riscv32 | PLIC + CLINT | Platform-Level IC |

---

## Timer Sources

| Arch | Timer | Frequency |
|------|-------|-----------|
| x86_64 | APIC Timer / HPET | Variable |
| i686 | PIT / APIC | 1.19318 MHz (PIT) |
| aarch64 | Generic Timer | System counter |
| arm | Generic Timer | System counter |
| mips64/mips32 | CP0 Count/Compare | CPU clock / 2 |
| riscv64/riscv32 | SBI Timer / mtime | Variable |

---

## Key Files to Create

```
kernel/
├── arch/
│   ├── x86_64/
│   │   ├── interrupt.rs        # IDT setup
│   │   ├── apic.rs             # Local APIC
│   │   ├── ioapic.rs           # I/O APIC
│   │   ├── timer.rs            # APIC timer
│   │   └── context.rs          # Register save/restore
│   ├── aarch64/
│   │   ├── exception.rs        # Exception vectors
│   │   ├── gic.rs              # GIC driver
│   │   ├── timer.rs            # Generic timer
│   │   └── context.rs
│   └── ... (other arches)
├── core/sched/
│   ├── mod.rs                  # Scheduler API
│   ├── thread.rs               # Thread structure
│   ├── runqueue.rs             # Per-CPU run queues
│   └── switch.rs               # Context switch logic
```

---

## Thread Structure

```rust
pub struct Thread {
    tid: u64,
    state: ThreadState,      // Running, Ready, Blocked
    priority: u8,            // 0-31 (0 = highest)
    kernel_stack: usize,
    context: arch::Context,  // Saved registers
}
```

---

## Exit Criteria

- [ ] Interrupt handlers installed on all arches
- [ ] Timer fires at 100Hz+
- [ ] Kernel threads can be created
- [ ] Context switch works
- [ ] Multiple threads run concurrently
- [ ] Preemption works (time slice enforced)
- [ ] Works on all 8 architectures

---

## Notes

*(Add implementation notes as work progresses)*

---

*Phase 2 of EFFLUX Implementation*
