# MIPS32 Boot Implementation

**Architecture:** MIPS32 (R2000+)
**Parent Spec:** [BOOT_SPEC.md](../../BOOT_SPEC.md)

---

## Boot Methods

| Method | Use Case |
|--------|----------|
| U-Boot | Most common |
| YAMON | Malta board |
| Bare Metal | Embedded/routers |

---

## Entry Requirements

- **Mode:** Kernel mode (32-bit)
- **TLB:** Undefined
- **Caches:** May need init
- **Interrupts:** Disabled
- **a0:** argc or DTB
- **a1:** argv or zero
- **a2:** envp or zero

---

## Boot Sequence

1. **Entry** - Receive boot args
2. **CP0 Init** - Status, Config
3. **Cache Init** - I-cache, D-cache
4. **TLB Clear** - Invalidate entries
5. **BSS Clear**
6. **Early Console** - 16550 UART
7. **Memory Map** - From bootloader or probe
8. **TLB Setup** - Kernel wired entries
9. **Exception Vectors** - 0x8000_0000, etc.

---

## Memory Segments

| Segment | Address Range | Cached | Mapped |
|---------|---------------|--------|--------|
| useg | 0x0000_0000 - 0x7FFF_FFFF | TLB | Yes |
| kseg0 | 0x8000_0000 - 0x9FFF_FFFF | Yes | No |
| kseg1 | 0xA000_0000 - 0xBFFF_FFFF | No | No |
| kseg2 | 0xC000_0000 - 0xFFFF_FFFF | TLB | Yes |

---

## Key Points

- Software TLB management (same as MIPS64)
- 4KB pages standard, larger via PageMask
- Exception vectors at fixed addresses
- Big or little endian (check Config register)

---

## Exit Criteria

- [ ] Memory map obtained
- [ ] TLB functional
- [ ] Exception handlers installed
- [ ] Works on QEMU Malta

---

*End of MIPS32 Boot Implementation*
