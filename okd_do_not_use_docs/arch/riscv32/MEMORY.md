# RISC-V 32 Memory Management

**Architecture:** RISC-V 32-bit
**Parent Spec:** [MEMORY_SPEC.md](../../MEMORY_SPEC.md)

---

## Address Translation

- **Sv32 only**
- 2-level page tables
- 32-bit VA, 34-bit PA
- 4KB pages, 4MB superpages

---

## Page Table Entry

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
| 31:10 | PPN |

---

## Virtual Address Layout

| Bits | Level |
|------|-------|
| 31:22 | VPN[1] |
| 21:12 | VPN[0] |
| 11:0 | Offset |

---

## satp Register

| Bits | Field |
|------|-------|
| 31 | Mode (0=bare, 1=Sv32) |
| 30:22 | ASID |
| 21:0 | PPN |

---

## TLB Management

- `sfence.vma`

---

## Memory Layout

```
0x00000000 - 0x7FFFFFFF  User (2GB)
0x80000000 - 0xFFFFFFFF  Kernel (2GB)
```

---

## Exit Criteria

- [ ] Sv32 working
- [ ] 4MB superpages

---

*End of RISC-V 32 Memory Management*
