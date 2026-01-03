# MIPS64 Memory Management

**Architecture:** MIPS64
**Parent Spec:** [MEMORY_SPEC.md](../../MEMORY_SPEC.md)

---

## TLB Architecture

- **Software-managed TLB** (no hardware page walk)
- TLB entries: typically 32-64 (CPU dependent)
- Wired entries: reserved for kernel
- Random replacement for non-wired

---

## TLB Entry Structure

| Field | Description |
|-------|-------------|
| VPN2 | Virtual page number / 2 |
| ASID | Address space ID (8 bits) |
| PageMask | Variable page size |
| EntryLo0 | Even page mapping |
| EntryLo1 | Odd page mapping |
| G | Global (ignore ASID) |

---

## EntryLo Flags

| Bits | Name | Description |
|------|------|-------------|
| 0 | G | Global |
| 1 | V | Valid |
| 2 | D | Dirty (writable) |
| 5:3 | C | Cache coherency |

---

## Memory Segments

| Segment | Address | Cached | Mapped |
|---------|---------|--------|--------|
| xuseg | 0x0000... | TLB | Yes |
| xkphys | 0x8000... | varies | No |
| ckseg0 | 0xFFFF_FFFF_8000_0000 | Yes | No |
| ckseg1 | 0xFFFF_FFFF_A000_0000 | No | No |
| xkseg | 0xC000... | TLB | Yes |

---

## Key CP0 Registers

| Register | Purpose |
|----------|---------|
| Index | TLB index for read/write |
| Random | Random index for TLBWR |
| EntryLo0/1 | TLB entry low parts |
| EntryHi | VPN2 + ASID |
| PageMask | Page size |
| Wired | Wired entry count |
| Context | TLB refill assist |

---

## TLB Instructions

| Instruction | Description |
|-------------|-------------|
| TLBR | Read TLB entry |
| TLBWI | Write indexed |
| TLBWR | Write random |
| TLBP | Probe for entry |

---

## TLB Refill

- Exception at 0xFFFF_FFFF_8000_0000 (32-bit compat)
- XTLB at 0xFFFF_FFFF_8000_0080 (64-bit)
- Must be fast (no nested exceptions)

---

## Exit Criteria

- [ ] TLB refill handler working
- [ ] Wired entries for kernel
- [ ] ASID support
- [ ] Variable page sizes

---

*End of MIPS64 Memory Management*
