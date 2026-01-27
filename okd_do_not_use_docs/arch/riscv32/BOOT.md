# RISC-V 32 Boot Implementation

**Architecture:** RISC-V 32-bit (RV32GC)
**Parent Spec:** [BOOT_SPEC.md](../../BOOT_SPEC.md)

---

## Boot Methods

| Method | Use Case |
|--------|----------|
| OpenSBI + DTB | Primary |
| U-Boot | Common |
| Bare Metal | Embedded |

---

## Entry Requirements

- **Mode:** S-mode (or M-mode if no OpenSBI)
- **MMU:** Off
- **Interrupts:** Disabled
- **a0:** Hart ID
- **a1:** DTB pointer

---

## Boot Sequence

1. **Entry** - Hart ID (a0), DTB (a1)
2. **BSS Clear**
3. **Early Console** - SBI or direct UART
4. **DTB Parse** - Memory, devices
5. **Page Tables** - Sv32 (2-level)
6. **Enable MMU** - satp
7. **Trap Vectors** - stvec
8. **Interrupts** - PLIC or CLINT
9. **SMP** - SBI HSM

---

## Address Translation

**Sv32 only:**
- 2-level page tables
- 32-bit virtual, 34-bit physical
- 4KB pages, 4MB superpages

---

## Memory Layout

- **User:** 0x0000_0000 - 0x7FFF_FFFF (2GB)
- **Kernel:** 0x8000_0000 - 0xFFFF_FFFF (2GB)

---

## Key Differences from RV64

- Sv32 instead of Sv39/48/57
- 32-bit CSRs
- Smaller physical address space
- Same SBI interface

---

## Exit Criteria

- [ ] DTB parsed
- [ ] Sv32 MMU enabled
- [ ] Traps working
- [ ] Works on QEMU virt

---

*End of RISC-V 32 Boot Implementation*
