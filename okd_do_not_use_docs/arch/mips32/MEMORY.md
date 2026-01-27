# MIPS32 Memory Management

**Architecture:** MIPS32
**Parent Spec:** [MEMORY_SPEC.md](../../MEMORY_SPEC.md)

---

## TLB Architecture

- Software-managed TLB
- Paired entries (even/odd pages)
- ASID for address space tagging

---

## Memory Segments

| Segment | Address | Cached | Mapped |
|---------|---------|--------|--------|
| useg | 0x00000000 | TLB | Yes |
| kseg0 | 0x80000000 | Yes | No |
| kseg1 | 0xA0000000 | No | No |
| kseg2 | 0xC0000000 | TLB | Yes |

---

## Key CP0 Registers

| Register | Purpose |
|----------|---------|
| Index | TLB index |
| Random | Random index |
| EntryLo0/1 | Page frame + flags |
| EntryHi | VPN + ASID |
| PageMask | Page size mask |
| Wired | Wired entries |

---

## TLB Instructions

- TLBR, TLBWI, TLBWR, TLBP

---

## TLB Refill

- Exception vector at 0x80000000
- Handler must fit in 32 instructions (ideal)

---

## Exit Criteria

- [ ] TLB refill handler
- [ ] kseg0/kseg1 used for kernel
- [ ] useg mapped for user

---

*End of MIPS32 Memory Management*
