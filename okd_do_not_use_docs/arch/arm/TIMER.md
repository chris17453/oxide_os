# ARM32 Timer

**Architecture:** ARM32
**Parent Spec:** [TIMER_SPEC.md](../../TIMER_SPEC.md)

---

## Timer Options

| Source | Notes |
|--------|-------|
| Generic Timer | ARMv7+ |
| SP804 | Common in QEMU |
| Platform-specific | SoC varies |

---

## Generic Timer

- Same as AArch64, CP15 access
- `mrc/mcr p15, 0, rN, c14, ...`

---

## SP804

- Memory-mapped dual timer
- Load, control, clear registers

---

## Exit Criteria

- [ ] Timer interrupt working

---

*End of ARM32 Timer*
