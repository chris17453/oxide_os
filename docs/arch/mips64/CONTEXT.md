# MIPS64 Context Switching

**Architecture:** MIPS64
**Parent Spec:** [SCHEDULER_SPEC.md](../../SCHEDULER_SPEC.md)

---

## Context Structure

| Field | Description |
|-------|-------------|
| sp ($29) | Stack pointer |
| s0-s7 ($16-$23) | Callee-saved |
| fp ($30) | Frame pointer |
| ra ($31) | Return address |
| gp ($28) | Global pointer |
| hi, lo | Multiply/divide results |
| fpu regs | If FPU used |

---

## Switch Procedure

1. Save s0-s7, fp, ra, sp, gp
2. Load from new context
3. Update ASID in EntryHi if process change
4. Return via ra

---

## TLB/ASID

- Set EntryHi ASID field on switch
- No full TLB flush needed

---

## FPU State

- CP1 registers if Status.CU1 set
- Check Status.FR for 32 vs 64 FPRs

---

## TLS

- Typically use k0 or reserved register
- Or use UserLocal CP0 register

---

## Exit Criteria

- [ ] Context switch working
- [ ] ASID updated
- [ ] FPU preserved

---

*End of MIPS64 Context Switching*
