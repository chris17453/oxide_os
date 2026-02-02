# OXIDE OS Assembly Code Inventory

**Generated:** 2026-02-02
**Total Assembly:** 8,575+ lines across 49 files

## Quick Stats

- **Native assembly files:** 2 (.s/.S files)
- **Rust files with inline asm:** 47 files
- **Total inline asm blocks:** 377+ instances
- **Architecture support:** 100% x86_64 only
- **Target architectures:** x86_64 (little-endian), ARM64 (little-endian), **SGI MIPS64 (big-endian)** ⚠️

## Native Assembly Files

### 1. Application Processor Boot
**File:** `crates/arch/arch-x86_64/src/ap_boot.s`
**Lines:** 101
**Purpose:** AP boot trampoline - Real mode → Protected → Long mode
**Key Operations:**
- GDT setup
- CR0/CR4 manipulation
- EFER MSR configuration
- Paging enablement
- Jump to Rust code

**Instructions Used:**
`cli`, `cld`, `lgdt`, `mov cr0/cr4`, `rdmsr`, `wrmsr`, `ljmp`, `iretq`

**Critical:** Runs at physical address 0x8000

---

### 2. Userspace Init Test
**File:** `userspace/init/init.S`
**Lines:** 108
**Purpose:** Phase 4 fork-wait test program
**Syscalls:** EXIT(0), WRITE(1), FORK(3), WAIT(5)
**Migration:** Can be replaced with Rust inline asm

---

## Inline Assembly Distribution

### Core Architecture Files (9 files)

#### exceptions.rs (1,439 lines total)
- **Assembly blocks:** 28 instances
- **Handlers:** divide, debug, NMI, breakpoint, overflow, page_fault
- **Special:** Timer ISR with context switch
- **Uses:** `naked_asm!`, `push/pop`, `swapgs`, `iretq`

#### syscall.rs (580 lines)
- **Entry:** Naked syscall entry point
- **MSR ops:** EFER, STAR, LSTAR, SFMASK, KERNEL_GS_BASE
- **Security:** SMAP (stac/clac), user context capture
- **Exit:** `sysretq`

#### usermode.rs (516 lines)
- **Purpose:** Ring 0 → Ring 3 transition
- **Method:** IRETQ-based jump to user mode
- **Security:** Register clearing (prevent kernel leaks)

#### lib.rs (468 lines)
- **Port I/O:** `inb/outb/inw/outw/inl/outl`
- **CPU control:** `hlt/cli/sti`
- **TLB:** `invlpg`, `mov cr3`
- **Timing:** `rdtsc`

#### apic.rs (440 lines)
- **CPUID:** Feature detection
- **MSR:** APIC base configuration
- **MMIO:** APIC register access

#### gdt.rs (309 lines)
- **Operation:** `lgdt` (Load GDT)

#### idt.rs (324 lines)
- **Operation:** `lidt` (Load IDT)

#### ap_boot.rs (126 lines)
- **Purpose:** Wraps ap_boot.s

#### serial.rs (195 lines)
- **Minimal:** Port I/O for COM1

---

### Userspace libc (3 files)

#### setjmp.rs
- **Operations:** `setjmp/longjmp` implementation
- **Registers:** rbx, rbp, r12-r15, rsp, rip
- **Uses:** `global_asm!`

#### lib.rs
- **Entry:** `_start` using `naked_asm!`
- **Flow:** Stack setup → init functions → main() → exit

#### arch/x86_64/syscall.rs
- **Purpose:** Syscall stubs (syscall0-syscall6)

---

### Hypervisor (4 files)

Located in `crates/hypervisor/vmx/src/`:
- `lib.rs`, `vmx.rs`, `vmcs.rs`, `ept.rs`
- **Operations:** VMX-specific (vmxon, vmlaunch, etc.)
- **MSRs:** VMX control MSRs

---

### Other Components (31 files)

**Categories:**
- TLB operations: `crates/smp/smp/src/tlb.rs`
- Scheduling: `crates/sched/sched/src/core.rs`
- Drivers: virtio-blk, virtio-net, PS/2, PCI, serial
- Graphics: framebuffer, terminal

---

## x86_64-Specific Instructions

### Control Flow & Interrupts
- `cli/sti` - Disable/enable interrupts
- `hlt` - Halt CPU
- `iretq/sysretq` - Return from interrupt/syscall
- `syscall` - Fast system call
- `swapgs` - Swap GS register

### Memory Management
- `invlpg` - Invalidate TLB entry
- `mov cr0/cr3/cr4` - Control register access
- `mov cr2` - Page fault address
- `lgdt/lidt/lldt/ltr` - Load descriptor tables

### I/O & MSRs
- `in/out` - Port I/O (al/ax/eax)
- `rdmsr/wrmsr` - Model Specific Registers
- `cpuid` - CPU identification

### Performance
- `rdtsc` - Read Time Stamp Counter
- `pushfq/popfq` - RFLAGS manipulation

### Registers
- **General:** rax, rbx, rcx, rdx, rsi, rdi, rbp, rsp
- **Extended:** r8-r15
- **Segment:** gs (via swapgs)
- **Control:** cr0, cr2, cr3, cr4

---

## Architecture-Specific Code Locations

```
crates/arch/
├── arch-traits/          # Trait definitions (EXPAND THIS)
└── arch-x86_64/          # All x86_64 code
    └── src/
        ├── lib.rs        # Port I/O, CPU control
        ├── exceptions.rs # Exception handling
        ├── syscall.rs    # Syscall mechanism
        ├── usermode.rs   # Ring transitions
        ├── apic.rs       # APIC operations
        ├── gdt.rs        # GDT management
        ├── idt.rs        # IDT management
        └── ap_boot.s     # AP boot trampoline

userspace/libc/src/arch/x86_64/
└── syscall.rs            # Userspace syscalls

Future:
crates/arch/
├── arch-aarch64/         # ARM64 implementation
└── arch-mips64/          # MIPS64 implementation
```

---

## Operations Requiring Abstraction

### High Priority (Core functionality)
1. **Endianness** ⚠️ - Little-endian (x86/ARM) vs Big-endian (SGI MIPS)
2. **Port I/O** - x86-only, MMIO for ARM/SGI
3. **Syscall mechanism** - Different per arch
4. **Exception handling** - IDT vs vectors
5. **TLB operations** - Different instructions
6. **Control registers** - Different names/semantics
7. **Context switching** - Different register sets
8. **DMA coherency** ⚠️ - Coherent (x86/ARM) vs Non-coherent (SGI)

### Medium Priority (Subsystems)
9. **Interrupt controllers** - APIC vs GIC vs INT2/INT3
10. **Timers** - APIC timer vs Generic timer vs CP0 Count
11. **Boot sequence** - Real mode vs EL transitions vs ARCS/KSEG
12. **Cache operations** - wbinvd vs DC ops vs manual VIVT (SGI)

### Low Priority (Features)
11. **Virtualization** - VMX vs VHE vs VZ
12. **SIMD** - SSE/AVX vs NEON vs MSA
13. **Atomic operations** - lock prefix vs ldrex/strex vs ll/sc

---

## Migration Priorities

### Phase 1 (Immediate)
- Expand arch-traits with comprehensive interfaces
- Document each operation's purpose and requirements

### Phase 2 (Core)
- Refactor x86_64 to implement new traits
- Update drivers to use trait-based I/O
- Make memory management arch-agnostic

### Phase 3 (Validation)
- Create ARM64 skeleton to validate trait design
- Create MIPS64 skeleton for completeness

### Phase 4 (Integration)
- Make kernel generic over architecture
- Abstract bootloader and userspace

---

## Files Requiring Changes

### Immediate Changes (Phase 1-2)
```
crates/arch/arch-traits/src/lib.rs          # Add 10+ new traits
crates/arch/arch-x86_64/src/lib.rs          # Implement new traits
crates/arch/arch-x86_64/src/exceptions.rs   # Use ExceptionHandler trait
crates/arch/arch-x86_64/src/syscall.rs      # Use SyscallInterface trait
crates/drivers/pci/src/lib.rs               # Use PortIo trait
crates/mm/mm-paging/src/lib.rs              # Use ControlRegisters trait
```

### Future Changes (Phase 3-4)
```
crates/arch/arch-aarch64/                   # New ARM64 implementation
crates/arch/arch-mips64/                    # New MIPS64 implementation
kernel/src/lib.rs                           # Generic kernel entry
userspace/libc/src/arch/aarch64/            # ARM64 userspace
userspace/libc/src/arch/mips64/             # MIPS64 userspace
```

---

## Testing Strategy

### Unit Tests
- Test each trait implementation independently
- Verify register read/write operations
- Test TLB flush operations

### Integration Tests
- Boot sequence (each architecture in QEMU)
- Syscall latency benchmarks
- Context switch performance
- Exception handling correctness

### Validation
- x86_64: Existing test suite must pass
- ARM64: Boot to shell in QEMU virt machine
- MIPS64: Boot to shell in QEMU malta machine

---

## Tools and Resources

### QEMU Testing
```bash
# x86_64
qemu-system-x86_64 -kernel target/x86_64/kernel.elf

# ARM64
qemu-system-aarch64 -M virt -cpu cortex-a57 \
    -kernel target/aarch64/kernel.elf

# MIPS64
qemu-system-mips64 -M malta -cpu MIPS64R2-generic \
    -kernel target/mips64/kernel.elf
```

### Documentation
- ARM Architecture Reference Manual (ARM ARM)
- Intel 64 and IA-32 Architectures Software Developer Manual
- MIPS64 Architecture For Programmers
- UEFI Specification
- Device Tree Specification

### Analysis Tools
- objdump: Disassemble compiled code
- readelf: Examine ELF structure
- gdb-multiarch: Debug across architectures
- ripgrep: Search for asm patterns

---

## Next Actions

1. ✅ **Completed:** Full assembly inventory
2. ⏳ **Next:** Review migration plan with team
3. ⏳ **Phase 1:** Expand arch-traits (Week 1-2)
4. ⏳ **Phase 2:** Refactor x86_64 (Week 3-5)

---

**For detailed migration plan, see:** `docs/arch/MIGRATION_PLAN.md`
**For trait definitions, see:** `crates/arch/arch-traits/src/lib.rs`
**For x86_64 implementation, see:** `crates/arch/arch-x86_64/src/`
