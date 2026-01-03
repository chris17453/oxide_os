# AArch64 Timer

**Architecture:** AArch64
**Parent Spec:** [TIMER_SPEC.md](../../TIMER_SPEC.md)

---

## Generic Timer

Always available on AArch64.

---

## Timer Types

| Timer | Use |
|-------|-----|
| CNTP (Physical) | Kernel |
| CNTV (Virtual) | Guests |

---

## Key Registers

| Register | Purpose |
|----------|---------|
| CNTFRQ_EL0 | Frequency |
| CNTPCT_EL0 | Physical count |
| CNTP_TVAL_EL0 | Countdown value |
| CNTP_CTL_EL0 | Control |
| CNTP_CVAL_EL0 | Compare value |

---

## Control Bits

| Bit | Name |
|-----|------|
| 0 | ENABLE |
| 1 | IMASK |
| 2 | ISTATUS |

---

## Setup

1. Read CNTFRQ_EL0
2. Set CVAL or TVAL
3. Enable in CTL
4. Handle IRQ (PPI 30)

---

## Exit Criteria

- [ ] Timer interrupt working
- [ ] Preemption functional

---

*End of AArch64 Timer*
