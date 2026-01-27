# MIPS64 Timer

**Architecture:** MIPS64
**Parent Spec:** [TIMER_SPEC.md](../../TIMER_SPEC.md)

---

## CP0 Count/Compare

- **Count:** Free-running counter
- **Compare:** Triggers interrupt when Count == Compare
- **Interrupt:** Cause.TI, IP7

---

## Registers

| Register | Purpose |
|----------|---------|
| Count | Counter |
| Compare | Compare value |
| Cause.TI | Timer pending |
| Status.IM7 | Timer enable |

---

## Usage

1. Read Count
2. Add ticks, write Compare
3. Enable Status.IM7
4. Write Compare to clear interrupt

---

## SGI-Specific

- Additional timers in CRIME/MACE

---

## Exit Criteria

- [ ] Count/Compare working

---

*End of MIPS64 Timer*
