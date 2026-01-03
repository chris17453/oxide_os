# ARM32 Memory Management

**Architecture:** ARM32 (ARMv7-A)
**Parent Spec:** [MEMORY_SPEC.md](../../MEMORY_SPEC.md)

---

## Page Table Structure

- **Levels:** 2 (L1 → L2)
- **L1 entries:** 4096 (covers 4GB)
- **L2 entries:** 256 per table
- **Page sizes:** 4KB, 64KB, 1MB sections

---

## Virtual Address Layout

| Bits | Level | Size |
|------|-------|------|
| 31:20 | L1 index | 1MB per entry |
| 19:12 | L2 index | 4KB per entry |
| 11:0 | Offset | 4KB |

---

## L1 Descriptor Types

| Bits [1:0] | Type |
|------------|------|
| 00 | Fault |
| 01 | Page table |
| 10 | Section (1MB) |
| 11 | Reserved |

---

## L2 Descriptor Types

| Bits [1:0] | Type |
|------------|------|
| 00 | Fault |
| 01 | Large page (64KB) |
| 1x | Small page (4KB) |

---

## Key Registers (CP15)

| Register | Purpose |
|----------|---------|
| TTBR0 | Page table base |
| TTBCR | Translation control |
| DACR | Domain access control |
| SCTLR.M | MMU enable |

---

## Domain Access

- 16 domains, 2 bits each in DACR
- 00 = No access, 01 = Client, 11 = Manager

---

## TLB Management

- `MCR p15, 0, Rn, c8, c7, 0` - Invalidate all
- `MCR p15, 0, Rn, c8, c7, 1` - Invalidate by MVA
- `DSB` + `ISB` after TLB ops

---

## Memory Layout

```
0x00000000 - 0xBFFFFFFF  User (3GB)
0xC0000000 - 0xFFFFFFFF  Kernel (1GB)
```

---

## Exit Criteria

- [ ] 2-level paging working
- [ ] 1MB sections for kernel
- [ ] Domains configured

---

*End of ARM32 Memory Management*
