# RISC-V 32 Timer

**Architecture:** RISC-V 32-bit
**Parent Spec:** [TIMER_SPEC.md](../../TIMER_SPEC.md)

---

## Timer Sources

- SBI timer (portable)
- CLINT (direct)

---

## SBI Timer

Same as RV64:
- `sbi_set_timer()`
- 64-bit time value (read as timeh:time)

---

## Exit Criteria

- [ ] SBI timer working

---

*End of RISC-V 32 Timer*
