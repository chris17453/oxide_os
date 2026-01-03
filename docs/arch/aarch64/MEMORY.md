# AArch64 Memory Management

**Architecture:** AArch64
**Parent Spec:** [MEMORY_SPEC.md](../../MEMORY_SPEC.md)

---

## Page Table Structure

- **Levels:** 3 or 4 depending on granule
- **Granules:** 4KB, 16KB, 64KB
- **Page sizes:** 4KB, 16KB, 64KB + block mappings

---

## Virtual Address Layout (4KB granule, 48-bit)

| Bits | Level | Size |
|------|-------|------|
| 47:39 | L0 index | 512GB |
| 38:30 | L1 index | 1GB |
| 29:21 | L2 index | 2MB |
| 20:12 | L3 index | 4KB |
| 11:0 | Offset | 4KB |

---

## Descriptor Flags

| Bits | Name | Description |
|------|------|-------------|
| 0 | Valid | Entry valid |
| 1 | Table/Block | 1=table, 0=block |
| 6 | AP[1] | Unprivileged access |
| 7 | AP[2] | Read-only |
| 10 | AF | Access flag |
| 53 | PXN | Privileged execute never |
| 54 | UXN | Unprivileged execute never |

---

## Key Registers

| Register | Purpose |
|----------|---------|
| TTBR0_EL1 | User page tables |
| TTBR1_EL1 | Kernel page tables |
| TCR_EL1 | Translation control |
| MAIR_EL1 | Memory attributes |
| SCTLR_EL1.M | MMU enable |

---

## MAIR Attribute Indices

| Index | Typical Use |
|-------|-------------|
| 0 | Device-nGnRnE |
| 1 | Normal, Non-cacheable |
| 2 | Normal, Write-through |
| 3 | Normal, Write-back |

---

## TLB Management

- `TLBI VAE1IS, Xn` - Invalidate by VA
- `TLBI ASIDE1IS, Xn` - Invalidate by ASID
- `TLBI VMALLE1IS` - Invalidate all EL1
- `DSB ISH` + `ISB` - Barriers after TLBI

---

## Memory Layout

```
0x0000_0000_0000_0000+  User (TTBR0)
0xFFFF_0000_0000_0000+  Kernel (TTBR1)
```

---

## Exit Criteria

- [ ] 4KB granule working
- [ ] TTBR0/TTBR1 split
- [ ] MAIR configured
- [ ] ASID support

---

*End of AArch64 Memory Management*
