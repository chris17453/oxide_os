# x86_64 Timer

**Architecture:** x86_64
**Parent Spec:** [TIMER_SPEC.md](../../TIMER_SPEC.md)

---

## Timer Sources

| Source | Type | Notes |
|--------|------|-------|
| LAPIC Timer | Per-CPU | Primary for scheduling |
| HPET | Global | Fallback, ACPI |
| PIT | Global | Legacy |
| TSC | Per-CPU | Timekeeping |

---

## LAPIC Timer

- **Modes:** One-shot, periodic, TSC-deadline
- **TSC-deadline:** Best for modern CPUs

---

## TSC

- `rdtsc` / `rdtscp`
- Calibrate against known source
- Check invariant TSC (CPUID)

---

## HPET

- Memory-mapped (ACPI for address)
- Multiple comparators

---

## PIT (8254)

- Ports 0x40-0x43
- 1.193182 MHz
- Legacy fallback

---

## Typical Setup

1. Calibrate TSC
2. Use LAPIC timer for preemption
3. TSC for timekeeping

---

## Exit Criteria

- [ ] LAPIC timer working
- [ ] TSC calibrated

---

*End of x86_64 Timer*
