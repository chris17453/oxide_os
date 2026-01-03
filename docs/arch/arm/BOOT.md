# ARM32 Boot Implementation

**Architecture:** ARM32 (ARMv7-A)
**Parent Spec:** [BOOT_SPEC.md](../../BOOT_SPEC.md)

---

## Boot Methods

| Method | Use Case |
|--------|----------|
| U-Boot + DTB | Primary |
| Linux zImage | Compatible with existing bootloaders |
| Bare Metal | Direct from ROM/bootloader |

---

## Entry Requirements

- **Mode:** SVC (Supervisor) mode
- **MMU:** Off
- **Caches:** Off (typically)
- **Interrupts:** Disabled (CPSR I/F bits set)
- **r0:** Zero
- **r1:** Machine type (legacy) or zero
- **r2:** DTB pointer (physical)

---

## Boot Sequence

1. **Entry** - Receive DTB in r2
2. **Mode Check** - Ensure SVC mode
3. **BSS Clear** - Zero BSS section
4. **Early Console** - UART (PL011 or 8250-compatible)
5. **DTB Parse** - Memory, devices
6. **MMU Setup** - TTBR, domain access, enable
7. **Vectors** - Set VBAR or remap to 0xFFFF0000
8. **GIC/VIC** - Initialize interrupt controller
9. **Timer** - ARM generic timer or SP804
10. **SMP** - PSCI or platform-specific

---

## Key Coprocessor Registers (CP15)

| Register | Purpose |
|----------|---------|
| SCTLR (c1, c0, 0) | System control |
| TTBR0 (c2, c0, 0) | Page table base |
| TTBCR (c2, c0, 2) | Translation table control |
| DACR (c3, c0, 0) | Domain access control |
| VBAR (c12, c0, 0) | Vector base address |

---

## Memory Layout

- **User:** 0x00000000 - 0xBFFFFFFF (3GB)
- **Kernel:** 0xC0000000 - 0xFFFFFFFF (1GB)
- **Page sizes:** 4KB small, 64KB large, 1MB sections
- **Levels:** 2-level (section + page) or 1-level (sections only)

---

## SMP Boot

- **PSCI** (modern): SMC to boot secondaries
- **Platform-specific:** Pen release, boot ROM hooks

---

## Exit Criteria

- [ ] DTB parsed
- [ ] MMU enabled
- [ ] Exception vectors set
- [ ] Interrupts working
- [ ] Works on QEMU virt

---

*End of ARM32 Boot Implementation*
