# RISC-V 64 Memory Management

**Architecture:** RISC-V 64-bit
**Parent Spec:** [MEMORY_SPEC.md](../../MEMORY_SPEC.md)

---

## Address Translation Modes

| Mode | Levels | VA Bits | PA Bits |
|------|--------|---------|---------|
| Sv39 | 3 | 39 | 56 |
| Sv48 | 4 | 48 | 56 |
| Sv57 | 5 | 57 | 56 |

---

## Page Table Entry (Sv39)

| Bits | Field |
|------|-------|
| 0 | V (Valid) |
| 1 | R (Read) |
| 2 | W (Write) |
| 3 | X (Execute) |
| 4 | U (User) |
| 5 | G (Global) |
| 6 | A (Accessed) |
| 7 | D (Dirty) |
| 53:10 | PPN |

---

## Virtual Address Layout (Sv39)

| Bits | Level |
|------|-------|
| 38:30 | VPN[2] |
| 29:21 | VPN[1] |
| 20:12 | VPN[0] |
| 11:0 | Offset |

---

## Key CSRs

| CSR | Purpose |
|-----|---------|
| satp | Mode + ASID + PPN |
| sstatus.SUM | Supervisor user memory access |
| sstatus.MXR | Make executable readable |

---

## satp Register

| Bits | Field |
|------|-------|
| 63:60 | Mode (0=bare, 8=Sv39, 9=Sv48) |
| 59:44 | ASID |
| 43:0 | PPN of root page table |

---

## TLB Management

- `sfence.vma` - Flush TLB
- `sfence.vma rs1, rs2` - Flush by addr/ASID

---

## Memory Layout

```
0x0000_0000_0000_0000+  User (low half)
0xFFFF_FFC0_0000_0000+  Kernel (Sv39 high)
```

---

## Exit Criteria

- [ ] Sv39 working
- [ ] Superpages (2MB, 1GB)
- [ ] ASID support
- [ ] sfence.vma used

---

*End of RISC-V 64 Memory Management*
