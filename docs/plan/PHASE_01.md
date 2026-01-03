# Phase 1: Memory Management

**Stage:** 1 - Foundation
**Status:** Not Started
**Dependencies:** Phase 0 (Boot + Serial)

---

## Goal

Physical and virtual memory management with kernel heap.

---

## Deliverables

| Item | Status |
|------|--------|
| Memory map from firmware | [ ] |
| Boot-time bump allocator | [ ] |
| Buddy allocator | [ ] |
| Kernel page tables | [ ] |
| Slab allocator (kernel heap) | [ ] |
| Direct physical map | [ ] |

---

## Architecture Status

| Arch | MemMap | Bump | Buddy | PageTables | Slab | Done |
|------|--------|------|-------|------------|------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Memory Map Sources

| Arch | Source |
|------|--------|
| x86_64/i686 | UEFI GetMemoryMap / E820 |
| aarch64 | UEFI GetMemoryMap / Device Tree |
| arm | Device Tree / ATAGs |
| mips64/mips32 | ARCS GetMemoryDescriptor / YAMON |
| riscv64/riscv32 | Device Tree |

---

## Page Table Formats

| Arch | Format | Levels | Page Size |
|------|--------|--------|-----------|
| x86_64 | 4-level (5 with LA57) | 4-5 | 4KB |
| i686 | 2-level or PAE | 2-3 | 4KB |
| aarch64 | TTBR0/TTBR1 | 4 | 4KB/16KB/64KB |
| arm | 2-level | 2 | 4KB |
| mips64 | Software TLB | - | 4KB-16MB |
| mips32 | Software TLB | - | 4KB-16MB |
| riscv64 | Sv39/Sv48/Sv57 | 3-5 | 4KB |
| riscv32 | Sv32 | 2 | 4KB |

---

## Key Files to Create

```
kernel/
├── arch/
│   ├── x86_64/mm/
│   │   ├── mod.rs
│   │   ├── paging.rs           # 4/5-level page tables
│   │   └── tlb.rs              # TLB invalidation
│   ├── aarch64/mm/
│   │   ├── mod.rs
│   │   ├── paging.rs           # TTBR setup
│   │   └── mair.rs             # Memory attributes
│   └── ... (other arches)
├── core/mm/
│   ├── mod.rs                  # Public API
│   ├── buddy.rs                # Buddy allocator
│   ├── slab.rs                 # Slab allocator
│   └── vmm.rs                  # Virtual memory manager
```

---

## Exit Criteria

- [ ] Firmware memory map parsed on all arches
- [ ] Buddy allocator alloc/free works
- [ ] Kernel page tables active on all arches
- [ ] `Box::new()` works in kernel
- [ ] Direct map covers all physical RAM
- [ ] Page fault panics with useful info

---

## Notes

*(Add implementation notes as work progresses)*

---

*Phase 1 of EFFLUX Implementation*
