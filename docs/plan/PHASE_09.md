# Phase 9: SMP (Symmetric Multiprocessing)

**Stage:** 3 - Hardware
**Status:** Not Started
**Dependencies:** Phase 8 (Libc + Userland)

---

## Goal

Enable multi-core operation with per-CPU data and work-stealing scheduler.

---

## Deliverables

| Item | Status |
|------|--------|
| AP (Application Processor) boot | [ ] |
| Per-CPU data structures | [ ] |
| Per-CPU run queues | [ ] |
| Spinlocks with SMP safety | [ ] |
| TLB shootdowns via IPI | [ ] |
| Work-stealing scheduler | [ ] |
| CPU hotplug (optional) | [ ] |

---

## Architecture Status

| Arch | AP Boot | Per-CPU | TLB Shootdown | Scheduler | Done |
|------|---------|---------|---------------|-----------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## AP Boot Methods

| Arch | Method | Description |
|------|--------|-------------|
| x86_64/i686 | SIPI | Startup IPI sequence via APIC |
| aarch64 | PSCI | Power State Coordination Interface |
| arm | PSCI / spin-table | Depends on platform |
| mips64/mips32 | Platform-specific | Usually mailbox |
| riscv64/riscv32 | HSM | Hart State Management (SBI) |

---

## x86_64 AP Boot Sequence

```
1. BSP allocates AP boot code page (< 1MB)
2. Copy AP trampoline to low memory
3. Initialize LAPIC on BSP
4. For each AP:
   a. Send INIT IPI
   b. Wait 10ms
   c. Send SIPI with vector = page >> 12
   d. Wait for AP to signal ready
5. AP executes trampoline:
   a. Switch to protected mode
   b. Switch to long mode
   c. Load GDT, IDT
   d. Jump to kernel AP entry
6. AP initializes per-CPU data
7. AP enters scheduler
```

---

## Per-CPU Data Structure

```rust
#[repr(C)]
pub struct PerCpu {
    /// Self pointer (for fast access via segment)
    pub self_ptr: *mut PerCpu,

    /// CPU ID
    pub cpu_id: u32,

    /// Current thread
    pub current_thread: *mut Thread,

    /// Idle thread for this CPU
    pub idle_thread: *mut Thread,

    /// Run queue
    pub run_queue: RunQueue,

    /// Preemption count (0 = preemptible)
    pub preempt_count: u32,

    /// Interrupt nesting level
    pub irq_count: u32,

    /// Per-CPU statistics
    pub stats: CpuStats,
}
```

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

## TLB Shootdown

```
CPU 0                           CPU 1, 2, 3
  │                                  │
  │ Unmap page P                     │
  │                                  │
  ├─► Send IPI ────────────────────► │
  │                                  │ Receive IPI
  │                                  │ Flush TLB for P
  │                                  │ Signal done
  │ ◄─────────────────────────────── │
  │ Wait for all                     │
  │                                  │
  ▼                                  ▼
  Continue                         Continue
```

---

## Work-Stealing Scheduler

```
CPU 0 Queue: [T1, T2, T3]     CPU 1 Queue: []
                                    │
                                    │ Queue empty!
                                    │
                                    ▼
                              Steal from CPU 0
                                    │
                                    ▼
CPU 0 Queue: [T1, T2]         CPU 1 Queue: [T3]
```

**Rules:**
1. Each CPU has local run queue
2. Push new threads to local queue
3. Pop from local queue (LIFO for cache)
4. If empty, steal from random CPU (FIFO)
5. Stealing is lock-free (CAS)

---

## Key Files

```
crates/smp/efflux-smp/src/
├── lib.rs
├── percpu.rs          # Per-CPU data
├── boot.rs            # AP boot coordination
├── ipi.rs             # Inter-processor interrupts
└── tlb.rs             # TLB shootdown

crates/arch/efflux-arch-x86_64/src/
├── smp/
│   ├── mod.rs
│   ├── apboot.rs      # AP boot trampoline
│   ├── apic_ipi.rs    # IPI via APIC
│   └── percpu.rs      # GS-based per-CPU
```

---

## Synchronization Primitives

| Primitive | Use Case |
|-----------|----------|
| Spinlock | Short critical sections |
| Ticket lock | Fair spinlock |
| RwLock | Reader-writer scenarios |
| Seqlock | Read-mostly data |
| RCU | Read-heavy with rare updates |
| Per-CPU | No synchronization needed |

---

## Exit Criteria

- [ ] All CPUs detected and booted
- [ ] Per-CPU data accessible from each CPU
- [ ] Threads scheduled across all CPUs
- [ ] TLB shootdown works correctly
- [ ] No data races (verified with stress test)
- [ ] Works on all 8 architectures

---

## Test Program

```c
// Stress test: spawn threads that increment shared counter
#define NUM_THREADS 100
#define INCREMENTS 10000

atomic_int counter = 0;

void* worker(void* arg) {
    for (int i = 0; i < INCREMENTS; i++) {
        atomic_fetch_add(&counter, 1);
    }
    return NULL;
}

int main() {
    pthread_t threads[NUM_THREADS];

    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_create(&threads[i], NULL, worker, NULL);
    }

    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_join(threads[i], NULL);
    }

    int expected = NUM_THREADS * INCREMENTS;
    printf("Counter: %d (expected %d)\n", counter, expected);

    return counter == expected ? 0 : 1;
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 9 of EFFLUX Implementation*
