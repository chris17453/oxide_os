# AArch64 Boot Implementation

**Architecture:** AArch64 (ARM64)
**Parent Spec:** [BOOT_SPEC.md](../../BOOT_SPEC.md)

---

## Boot Methods

| Method | Use Case |
|--------|----------|
| UEFI + DTB | Primary (servers, RPi with UEFI) |
| Linux Boot Protocol | Alternative (U-Boot) |
| Bare Metal + DTB | Embedded |

---

## Entry Requirements

- **Exception Level:** EL1 (or EL2 with virtualization)
- **MMU:** Off or identity mapped
- **Caches:** May be on or off
- **Interrupts:** Disabled (DAIF masked)
- **x0:** DTB pointer (physical)
- **x1-x3:** Reserved (zero)

---

## Boot Sequence

1. **Entry** - Receive DTB pointer in x0
2. **EL Check** - Drop from EL2 to EL1 if needed (via ERET)
3. **BSS Clear** - Zero BSS section
4. **Early Console** - PL011 UART (address from DTB or hardcoded for platform)
5. **DTB Parse** - Extract memory map, chosen node, devices
6. **MMU Setup** - Configure MAIR, TCR, TTBR0/TTBR1, enable MMU
7. **Exception Vectors** - Set VBAR_EL1
8. **GIC Init** - Initialize interrupt controller (GICv2 or GICv3)
9. **Timer Init** - Generic timer via CNTV or CNTP
10. **SMP** - Boot secondaries via PSCI or spin-table

---

## Key System Registers

| Register | Purpose |
|----------|---------|
| SCTLR_EL1 | System control (MMU enable, caches) |
| TCR_EL1 | Translation control (granule, size) |
| MAIR_EL1 | Memory attribute indirection |
| TTBR0_EL1 | User page tables |
| TTBR1_EL1 | Kernel page tables |
| VBAR_EL1 | Exception vector base |
| SP_EL1 | Kernel stack pointer |

---

## Memory Layout

- **TTBR0:** User space (0x0000_0000_0000_0000 - 0x0000_FFFF_FFFF_FFFF)
- **TTBR1:** Kernel space (0xFFFF_0000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF)
- **Page sizes:** 4KB, 16KB, or 64KB granules
- **Levels:** 4-level (4KB) or 3-level (64KB)

---

## SMP Boot

- **PSCI** (preferred): Use SMC/HVC to CPU_ON secondary cores
- **Spin-table** (fallback): Write entry address to per-CPU release address

---

## Platform Variants

| Platform | Console | Notes |
|----------|---------|-------|
| QEMU virt | PL011 @ 0x0900_0000 | GICv3, virtio |
| Raspberry Pi | Mini UART or PL011 | VideoCore bootloader |
| Server (SBSA) | UEFI + ACPI | May have ACPI instead of DTB |

---

## Exit Criteria

- [ ] DTB parsed for memory and devices
- [ ] MMU enabled with TTBR0/TTBR1 split
- [ ] Exception vectors installed
- [ ] GIC initialized
- [ ] SMP via PSCI working
- [ ] Works on QEMU virt

---

*End of AArch64 Boot Implementation*
