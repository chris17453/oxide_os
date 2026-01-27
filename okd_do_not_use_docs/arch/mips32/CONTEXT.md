# MIPS32 Context Switching

**Architecture:** MIPS32
**Parent Spec:** [SCHEDULER_SPEC.md](../../SCHEDULER_SPEC.md)

---

## Context Structure

| Field | Description |
|-------|-------------|
| sp ($29) | Stack pointer |
| s0-s7 ($16-$23) | Callee-saved |
| fp ($30) | Frame pointer |
| ra ($31) | Return address |
| hi, lo | Multiply results |

---

## Switch Procedure

1. Save s0-s7, fp, ra, sp
2. Load from new context
3. Update EntryHi ASID
4. Return via ra

---

## FPU State

- Save if CP1 enabled

---

## Exit Criteria

- [ ] Context switch working
- [ ] ASID switched

---

*End of MIPS32 Context Switching*
