# i686 Timer

**Architecture:** i686
**Parent Spec:** [TIMER_SPEC.md](../../TIMER_SPEC.md)

---

## Timer Sources

| Source | Notes |
|--------|-------|
| PIT | Always available |
| LAPIC Timer | If APIC present |
| HPET | Modern systems |
| TSC | If available |

---

## PIT (8254)

- Ports 0x40-0x43
- 1.193182 MHz
- IRQ 0 → vector 32

---

## LAPIC Timer

- Same as x86_64, 32-bit access

---

## Setup

1. Calibrate with PIT
2. Use LAPIC if available
3. Fall back to PIT

---

## Exit Criteria

- [ ] PIT or LAPIC working
- [ ] Preemption functional

---

*End of i686 Timer*
