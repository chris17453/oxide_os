# OXIDE OS Architecture Porting Guide

**Last Updated:** 2026-02-02
**Supported Architectures:** x86_64, ARM64 (aarch64), SGI MIPS64

## Overview

This guide explains how to port OXIDE OS to a new architecture using the trait-based abstraction layer defined in `arch-traits`. Each architecture must implement all required traits to provide a complete system.

## Quick Start

1. Create new crate: `crates/arch/arch-{name}/`
2. Implement all required traits from `arch-traits`
3. Add target specification (`.json` if needed)
4. Update build system (`Makefile`, `Cargo.toml`)
5. Test in QEMU
6. Test on real hardware

## Required Traits

Every architecture MUST implement these traits:

| Trait | Purpose | Critical |
|-------|---------|----------|
| `Arch` | Core architecture identity | ✅ Required |
| `ControlRegisters` | MMU/page table control | ✅ Required |
| `TlbControl` | TLB management | ✅ Required |
| `SystemRegisters` | Special register access | ✅ Required |
| `SyscallInterface` | System call mechanism | ✅ Required |
| `ExceptionHandler` | Exception/interrupt handling | ✅ Required |
| `ContextSwitch` | Thread context switching | ✅ Required |
| `CacheOps` | Cache management | ✅ Required |
| `Endianness` | Byte order conversions | ✅ Required |
| `DmaOps` | DMA synchronization | ✅ Required |
| `BootProtocol` | Boot firmware interface | ✅ Required |
| `PortIo` | Port-based I/O | ⚠️ x86 only |
| `AtomicOps` | Atomic operations | Optional |
| `VirtualizationExt` | Hypervisor support | Optional |
| `Serial` | Early console | Recommended |
| `InterruptController` | Interrupt controller | Recommended |
| `Timer` | System timer | Recommended |

## Register Mapping Tables

### General Purpose Registers

| Purpose | x86_64 | ARM64 | SGI MIPS64 |
|---------|--------|-------|------------|
| **Syscall Number** | rax | x8 | v0 ($2) |
| **Return Value** | rax | x0 | v0 ($2) |
| **Arg 0** | rdi | x0 | a0 ($4) |
| **Arg 1** | rsi | x1 | a1 ($5) |
| **Arg 2** | rdx | x2 | a2 ($6) |
| **Arg 3** | r10 (syscall) / rcx | x3 | a3 ($7) |
| **Arg 4** | r8 | x4 | a4 ($8) |
| **Arg 5** | r9 | x5 | a5 ($9) |
| **Stack Pointer** | rsp | sp (x31) | sp ($29) |
| **Frame Pointer** | rbp | x29 (fp) | fp ($30) |
| **Link Register** | (use stack) | x30 (lr) | ra ($31) |
| **Program Counter** | rip | pc | pc |

### Control/System Registers

| Function | x86_64 | ARM64 | SGI MIPS64 |
|----------|--------|-------|------------|
| **Page Table Root** | CR3 | TTBR0_EL1/TTBR1_EL1 | CP0 Context ($4) |
| **Interrupt Enable** | RFLAGS.IF | DAIF | CP0 Status.IE |
| **Exception PC** | (pushed to stack) | ELR_EL1 | CP0 EPC ($14) |
| **Fault Address** | CR2 | FAR_EL1 | CP0 BadVAddr ($8) |
| **Exception Cause** | (vector number) | ESR_EL1 | CP0 Cause ($13) |
| **ASID/Context** | PCID (CR3) | ASID (TTBR) | CP0 EntryHi.ASID |

## Instruction Equivalence Tables

### CPU Control

| Operation | x86_64 | ARM64 | SGI MIPS64 |
|-----------|--------|-------|------------|
| **Halt CPU** | `hlt` | `wfi` | `wait` |
| **Disable Interrupts** | `cli` | `msr daifset, #2` | `di` / clear Status.IE |
| **Enable Interrupts** | `sti` | `msr daifclr, #2` | `ei` / set Status.IE |
| **No Operation** | `nop` | `nop` | `nop` |
| **Memory Barrier** | `mfence` | `dsb sy` | `sync` |
| **Read Barrier** | `lfence` | `dsb ld` | `sync` |
| **Write Barrier** | `sfence` | `dsb st` | `sync` |

### TLB Operations

| Operation | x86_64 | ARM64 | SGI MIPS64 |
|-----------|--------|-------|------------|
| **Flush Single Entry** | `invlpg [addr]` | `tlbi vae1, addr` | `tlbp` + `tlbwi` |
| **Flush All** | `mov cr3, cr3` | `tlbi vmalle1` | `mtc0 $0, Index` + loop `tlbwi` |
| **Flush ASID** | (reload CR3) | `tlbi aside1, asid` | Change EntryHi.ASID |

### System Calls

| Operation | x86_64 | ARM64 | SGI MIPS64 |
|-----------|--------|-------|------------|
| **Enter Kernel** | `syscall` | `svc #0` | `syscall` |
| **Return to User** | `sysretq` | `eret` | `eret` |

### Context Switch

| Operation | x86_64 | ARM64 | SGI MIPS64 |
|-----------|--------|-------|------------|
| **Save Context** | Push all GPRs | `stp` pairs | `sd` instructions |
| **Load Context** | Pop all GPRs | `ldp` pairs | `ld` instructions |
| **Switch Stack** | `mov rsp, new_sp` | `mov sp, new_sp` | `move $sp, new_sp` |

### Cache Operations

| Operation | x86_64 | ARM64 | SGI MIPS64 |
|-----------|--------|-------|------------|
| **Flush D-Cache Line** | (coherent) | `dc cvac, addr` | `cache 0x15, addr` |
| **Invalidate I-Cache** | (coherent) | `ic ivau, addr` | `cache 0x10, addr` |
| **Write-back D-Cache** | `clflush [addr]` | `dc civac, addr` | `cache 0x15, addr` |
| **Full Cache Flush** | `wbinvd` | Loop dc/ic ops | Loop cache ops |

## Memory Models

### Address Space Layout

**x86_64 (Canonical Addressing):**
```
0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF : User space (128 TB)
0x0000_8000_0000_0000 - 0xFFFF_7FFF_FFFF_FFFF : Non-canonical (invalid)
0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF : Kernel space (128 TB)
```

**ARM64 (Two TTBR model):**
```
0x0000_0000_0000_0000 - 0x0000_FFFF_FFFF_FFFF : User (TTBR0_EL1) (256 TB)
0xFFFF_0000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF : Kernel (TTBR1_EL1) (256 TB)
```

**SGI MIPS64 (Segmented):**
```
0x0000_0000_0000_0000 - 0x0000_00FF_FFFF_FFFF : User (KUSEG) mapped
0xFFFF_FFFF_8000_0000 - 0xFFFF_FFFF_9FFF_FFFF : Kernel (KSEG0) unmapped cached
0xFFFF_FFFF_A000_0000 - 0xFFFF_FFFF_BFFF_FFFF : Kernel (KSEG1) unmapped uncached
0xFFFF_FFFF_C000_0000 - 0xFFFF_FFFF_FFFF_FFFF : Kernel (KSSEG/KSEG3) mapped
0x8000_0000_0000_0000 - 0xBFFF_FFFF_FFFF_FFFF : XKPHYS (direct phys, cache attr)
```

### Page Sizes

| Architecture | Supported Sizes | Default |
|--------------|-----------------|---------|
| **x86_64** | 4K, 2M, 1G | 4K |
| **ARM64** | 4K, 16K, 64K, 2M, 32M, 512M, 1G | 4K |
| **MIPS64** | 4K, 16K, 64K, 256K, 1M, 4M, 16M | 4K |

## Calling Conventions

### Function Calls

**x86_64 (System V ABI):**
- Args: rdi, rsi, rdx, rcx, r8, r9, stack...
- Return: rax, rdx
- Caller-saved: rax, rcx, rdx, rsi, rdi, r8-r11
- Callee-saved: rbx, rsp, rbp, r12-r15

**ARM64 (AAPCS64):**
- Args: x0-x7, stack...
- Return: x0, x1
- Caller-saved: x0-x18
- Callee-saved: x19-x29, sp

**MIPS64 (N64 ABI):**
- Args: a0-a7 ($4-$11), stack...
- Return: v0, v1 ($2-$3)
- Caller-saved: v0-v1, t0-t9, a0-a7
- Callee-saved: s0-s7, gp, sp, fp, ra

### System Calls

**x86_64:**
- Number: rax
- Args: rdi, rsi, rdx, r10, r8, r9
- Return: rax
- Clobbered: rcx, r11

**ARM64:**
- Number: x8
- Args: x0-x5
- Return: x0
- Clobbered: x0-x18

**MIPS64:**
- Number: v0 ($2)
- Args: a0-a5 ($4-$9)
- Return: v0 ($2), v1 ($3) for 128-bit
- Clobbered: t0-t9, v0-v1, a0-a7

## Exception Handling

### Exception Vectors

**x86_64 (IDT):**
- 256-entry Interrupt Descriptor Table
- Each entry: offset, selector, IST, type, DPL, present
- Loaded via `lidt` instruction

**ARM64 (Vector Table):**
- 16 entries × 128 bytes = 2048 bytes
- 4 exception levels × 4 exception types
- Must be 2048-byte aligned
- Loaded via VBAR_EL1

**MIPS64 (Exception Vectors):**
- General exception: 0x180
- TLB refill: 0x000 (separate vector)
- XTLB refill: 0x080 (64-bit)
- Cache error: 0x100
- Base address in CP0 EBase ($15, sel 1)

### Context Save/Restore Pattern

```rust
// x86_64 ISR entry
naked_asm!(
    "push rax",
    "push rcx",
    // ... push all registers
    "mov rdi, rsp",      // Pass frame pointer
    "call handler",
    // ... pop all registers
    "iretq"
);

// ARM64 ISR entry
naked_asm!(
    "sub sp, sp, #272",  // Make space for frame
    "stp x0, x1, [sp]",
    "stp x2, x3, [sp, #16]",
    // ... save all registers
    "mov x0, sp",        // Pass frame pointer
    "bl handler",
    // ... restore all registers
    "eret"
);

// MIPS64 ISR entry
naked_asm!(
    "sd $at, 0($sp)",
    "sd $v0, 8($sp)",
    // ... save all registers
    "move $a0, $sp",     // Pass frame pointer
    "jal handler",
    "nop",               // Delay slot
    // ... restore all registers
    "eret"
);
```

## Endianness Handling

### Conversion Patterns

**Little-endian (x86/ARM):**
```rust
impl Endianness for LittleEndian {
    fn is_big_endian() -> bool { false }

    // No-ops for little-endian
    fn to_le32(val: u32) -> u32 { val }
    fn from_le32(val: u32) -> u32 { val }

    // Swaps for big-endian data
    fn to_be32(val: u32) -> u32 { val.swap_bytes() }
    fn from_be32(val: u32) -> u32 { val.swap_bytes() }
}
```

**Big-endian (SGI MIPS):**
```rust
impl Endianness for BigEndian {
    fn is_big_endian() -> bool { true }

    // Swaps for little-endian data
    fn to_le32(val: u32) -> u32 { val.swap_bytes() }
    fn from_le32(val: u32) -> u32 { val.swap_bytes() }

    // No-ops for big-endian
    fn to_be32(val: u32) -> u32 { val }
    fn from_be32(val: u32) -> u32 { val }
}
```

### Usage in Drivers

```rust
// Reading little-endian PCI config space
fn read_pci_config<E: Endianness>(addr: usize) -> u32 {
    let raw = unsafe { ptr::read_volatile(addr as *const u32) };
    E::from_le32(raw)  // No-op on LE, swap on BE
}

// Reading network packet (big-endian)
fn read_ip_header<E: Endianness>(data: &[u8]) -> u32 {
    let raw = u32::from_ne_bytes([data[0], data[1], data[2], data[3]]);
    E::from_be32(raw)  // No-op on BE, swap on LE
}
```

## Cache Coherency

### Coherent Systems (x86, most ARM)

```rust
impl CacheOps for X86_64 {
    unsafe fn flush_cache_range(_start: VirtAddr, _len: usize) {
        // No-op: caches are coherent
    }

    fn is_cache_coherent() -> bool {
        true
    }
}

impl DmaOps for X86_64 {
    unsafe fn dma_sync_for_device(_addr: PhysAddr, _len: usize) {
        // No-op: DMA is coherent
    }

    fn is_dma_coherent() -> bool {
        true
    }
}
```

### Non-Coherent Systems (SGI MIPS)

```rust
impl CacheOps for SgiMips64 {
    unsafe fn flush_cache_range(start: VirtAddr, len: usize) {
        let line_size = 32;
        let mut addr = start.align_down(line_size);
        let end = (start + len).align_up(line_size);

        while addr < end {
            asm!("cache 0x15, 0({0})", in(reg) addr.as_u64());
            addr += line_size;
        }
    }

    fn is_cache_coherent() -> bool {
        false
    }
}

impl DmaOps for SgiMips64 {
    unsafe fn dma_sync_for_device(addr: PhysAddr, len: usize) {
        // Write-back dirty cache lines before DMA write
        let vaddr = phys_to_virt(addr);
        Self::flush_cache_range(vaddr, len);
    }

    unsafe fn dma_sync_for_cpu(addr: PhysAddr, len: usize) {
        // Invalidate cache lines after DMA read
        let vaddr = phys_to_virt(addr);
        Self::invalidate_cache_range(vaddr, len);
    }

    fn is_dma_coherent() -> bool {
        false
    }
}
```

## Boot Protocol Integration

### UEFI (x86, ARM)

```rust
impl BootProtocol for UefiBootProtocol {
    type BootInfo = UefiBootInfo;

    unsafe fn early_init(info: &Self::BootInfo) {
        // Parse UEFI memory map
        // Set up page tables
        // Exit boot services
    }
}
```

### ARCS (SGI MIPS)

```rust
impl BootProtocol for ArcsBootProtocol {
    type BootInfo = ArcsBootInfo;

    unsafe fn early_init(info: &Self::BootInfo) {
        // Parse ARCS memory descriptors (big-endian!)
        // Set up TLB entries
        // Initialize KSEG0/1 mappings
    }
}
```

## Testing Checklist

When porting to a new architecture:

- [ ] Boot to kernel entry point
- [ ] Print to serial console
- [ ] Initialize MMU and page tables
- [ ] Handle timer interrupts
- [ ] Handle keyboard/device interrupts
- [ ] Perform syscall from userspace
- [ ] Context switch between threads
- [ ] Page fault handling works
- [ ] DMA operations work correctly
- [ ] Endianness conversions are correct
- [ ] All 100+ kernel tests pass
- [ ] Userspace programs run correctly

## Common Pitfalls

1. **Forgetting delay slots** (MIPS) - branch/jump instructions have delay slots
2. **Cache not flushed** (MIPS) - forgetting manual cache management
3. **Wrong endianness** - reading disk/network data without conversion
4. **TLB too small** (MIPS) - only 48-64 entries vs 1536+ on x86
5. **ASID exhaustion** - only 256 ASIDs available
6. **Stack alignment** - some architectures require 16-byte alignment
7. **Calling convention** - wrong register usage for syscalls
8. **Cache aliases** (VIVT) - same physical page at multiple virtual addresses

## References

- [OXIDE Migration Plan](./MIGRATION_PLAN.md)
- [Architecture Comparison](./ARCH_COMPARISON.md)
- [SGI MIPS Notes](./SGI_MIPS_NOTES.md)
- x86_64: Intel SDM, AMD APM
- ARM64: ARM Architecture Reference Manual
- MIPS64: MIPS64 Architecture For Programmers
- ARCS: Advanced RISC Computing Specification

---

**Next Steps:**
1. Choose your target architecture
2. Review existing implementations in `crates/arch/arch-{x86_64,aarch64,sgi-mips64}`
3. Create skeleton implementation
4. Test in QEMU
5. Gradually implement all traits
6. Test on real hardware

For questions, see `docs/arch/` or examine working implementations.
