# Phase 2: Interrupts + Timer + Scheduler

**Stage:** 1 - Foundation
**Status:** Complete
**Target:** x86_64 only
**Dependencies:** Phase 1 (Memory Management)

---

## Goal

Preemptive multitasking with kernel threads.

---

## Deliverables

| Item | Status |
|------|--------|
| IDT setup + exception handlers | [x] |
| Local APIC initialization | [x] |
| APIC timer (100Hz+) | [x] |
| Kernel thread structure | [x] |
| Context save/restore | [x] |
| Thread creation API | [x] |
| Round-robin scheduler | [x] |
| Preemption via timer interrupt | [x] |

---

## x86_64 Implementation Details

### Interrupt Descriptor Table (IDT)

- 256 entries (0-255)
- Entries 0-31: CPU exceptions
- Entry 32+: Hardware interrupts (remapped from PIC default)
- Use interrupt gates (IF cleared on entry)

Key exceptions to handle:
| Vector | Name | Notes |
|--------|------|-------|
| 0 | Divide Error | #DE |
| 6 | Invalid Opcode | #UD |
| 8 | Double Fault | #DF (must have IST) |
| 13 | General Protection | #GP |
| 14 | Page Fault | #PF (CR2 has address) |

### Local APIC

- Memory-mapped at `0xFEE00000` (physical)
- Map to virtual address via direct physical map
- Key registers:
  - `0x20`: APIC ID
  - `0x80`: Task Priority (TPR)
  - `0xB0`: End of Interrupt (EOI)
  - `0xF0`: Spurious Interrupt Vector
  - `0x320`: LVT Timer
  - `0x380`: Initial Count
  - `0x390`: Current Count
  - `0x3E0`: Divide Configuration

### APIC Timer

- One-shot or periodic mode
- Calibrate against known time source (PIT or TSC)
- Target: 100Hz (10ms tick)
- Timer interrupt triggers scheduler

### Context Structure

```rust
#[repr(C)]
pub struct Context {
    // Callee-saved registers (System V ABI)
    pub rbx: u64,
    pub rbp: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rsp: u64,
    pub rip: u64,  // Return address
    pub rflags: u64,
}
```

### Thread Structure

```rust
pub struct Thread {
    pub tid: u64,
    pub state: ThreadState,      // Running, Ready, Blocked, Zombie
    pub priority: u8,            // 0-31 (0 = highest)
    pub kernel_stack: VirtAddr,  // Top of kernel stack
    pub kernel_stack_size: usize,
    pub context: Context,        // Saved registers
}

pub enum ThreadState {
    Running,
    Ready,
    Blocked,
    Zombie,
}
```

---

## Crates to Create

**Architecture Rule:** ALL assembly and hardware-specific code goes in `arch-*` crates.
The scheduler crate uses traits, never inline assembly.

```
crates/
├── arch/
│   ├── arch-traits/     # Trait definitions (already exists)
│   │   └── src/lib.rs          # Add: InterruptController, Timer, Context traits
│   └── arch-x86_64/     # x86_64 implementation (already exists)
│       └── src/
│           ├── lib.rs          # Arch trait impl
│           ├── idt.rs          # IDT setup (assembly here)
│           ├── apic.rs         # Local APIC driver
│           ├── exceptions.rs   # Exception handlers (assembly stubs)
│           ├── timer.rs        # APIC timer
│           └── context.rs      # Context switch (assembly here)
├── sched/
│   ├── sched-traits/    # Scheduler trait definitions
│   │   └── src/lib.rs          # Scheduler, Thread traits
│   └── sched/           # Generic scheduler (NO assembly)
│       └── src/
│           ├── lib.rs          # Public API
│           ├── thread.rs       # Thread structure
│           ├── scheduler.rs    # Round-robin logic
│           └── runqueue.rs     # Run queue management
```

**Key Traits to Define:**

```rust
// In arch-traits
pub trait InterruptController {
    fn init();
    fn enable();
    fn disable();
    fn end_of_interrupt(vector: u8);
}

pub trait Timer {
    fn init(frequency_hz: u32);
    fn set_handler(handler: fn());
}

pub trait ContextOps {
    type Context;
    fn new_context(entry: fn(), stack_top: usize) -> Self::Context;
    unsafe fn switch(old: &mut Self::Context, new: &Self::Context);
}
```

---

## Implementation Order

1. **IDT + Exception Handlers**
   - Set up 256-entry IDT
   - Install handlers for CPU exceptions
   - Verify page fault handler works

2. **Local APIC**
   - Detect and enable APIC
   - Map APIC registers
   - Configure spurious interrupt

3. **APIC Timer**
   - Calibrate timer frequency
   - Set up periodic interrupt at 100Hz
   - Verify timer interrupt fires

4. **Thread Infrastructure**
   - Thread structure and state machine
   - Kernel stack allocation
   - Thread creation function

5. **Context Switch**
   - Save/restore assembly
   - Switch function
   - Test manual switching

6. **Scheduler**
   - Run queue (simple linked list)
   - Schedule function (round-robin)
   - Hook timer to call scheduler

7. **Integration**
   - Multiple threads running
   - Preemption working
   - Proper cleanup/exit

---

## Exit Criteria

- [x] IDT installed, exceptions handled gracefully
- [x] Page fault prints address and halts (not triple fault)
- [x] Local APIC enabled and responding
- [x] Timer interrupt fires at ~100Hz
- [x] Can create kernel threads
- [x] Context switch saves/restores all registers
- [x] Multiple threads execute concurrently
- [x] Preemption works (threads yield on timer)
- [x] No crashes after running for 10+ seconds
- [x] `make test` passes

---

## Test Plan

```rust
// In kernel_main after memory init:

// Create test threads
let thread1 = Thread::spawn(|| {
    loop {
        serial_println!("Thread 1");
        for _ in 0..1000000 { /* busy wait */ }
    }
});

let thread2 = Thread::spawn(|| {
    loop {
        serial_println!("Thread 2");
        for _ in 0..1000000 { /* busy wait */ }
    }
});

// Should see interleaved output:
// Thread 1
// Thread 2
// Thread 1
// Thread 2
// ...
```

---

## Reference Specs

- `docs/SCHEDULER_SPEC.md` - Scheduler design
- `docs/TIMER_SPEC.md` - Timer subsystem
- `docs/arch/x86_64/CONTEXT.md` - Context switch details
- `docs/arch/x86_64/TIMER.md` - x86_64 timer hardware

---

## Notes

### Completed (2026-01-18)

**All deliverables complete:**
- `arch-x86_64`: Full IDT with exception handlers (gdt.rs, idt.rs, exceptions.rs)
- `arch-x86_64`: Local APIC driver with timer (apic.rs)
- `arch-x86_64`: Context switch via timer interrupt (context.rs, exceptions.rs)
- `sched-traits`: Scheduler trait definitions
- `sched`: Round-robin scheduler with thread management
- Kernel integration: arch init, timer @ 100Hz, thread creation
- **Preemptive multitasking working**: Timer interrupt triggers context switches

**Test Results:**
- Two threads running concurrently with interleaved output
- Thread 1: 500 iterations, Thread 2: 466 iterations (both ran!)
- Round-robin scheduling with 10-tick time slices
- No crashes, clean halt after test completion

---

*Phase 2 of OXIDE Implementation - x86_64 Target*
