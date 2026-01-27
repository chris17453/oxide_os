# RISC-V 64 Timer

**Architecture:** RISC-V 64-bit
**Parent Spec:** [TIMER_SPEC.md](../../TIMER_SPEC.md)

---

## Timer Sources

| Source | Notes |
|--------|-------|
| SBI timer | Portable |
| CLINT | Direct access |

---

## SBI Timer

- `sbi_set_timer(time)` sets next interrupt
- Interrupt: scause bit 63 + 5

---

## CSRs

| CSR | Purpose |
|-----|---------|
| time | Current time |
| stimecmp | Compare (Sstc ext) |

---

## Setup

1. Read time
2. sbi_set_timer(time + interval)
3. Handle interrupt, repeat

---

## Exit Criteria

- [ ] SBI timer working

---

*End of RISC-V 64 Timer*
