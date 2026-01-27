# i686 Context Switching

**Architecture:** i686
**Parent Spec:** [SCHEDULER_SPEC.md](../../SCHEDULER_SPEC.md)

---

## Context Structure

| Field | Description |
|-------|-------------|
| esp | Stack pointer |
| ebp, ebx, esi, edi | Callee-saved registers |
| eip | Return address (on stack) |
| cr3 | Page directory base |
| fpu_state | FNSAVE/FXSAVE area |

---

## Switch Procedure

1. Push callee-saved regs
2. Save ESP to old context
3. Load ESP from new context
4. Switch CR3 if needed
5. Pop callee-saved regs
6. Return

---

## FPU State

- FNSAVE/FRSTOR (x87) or FXSAVE/FXRSTOR (SSE)
- Lazy or eager switching

---

## Kernel Stack

- TSS.ESP0 for ring 3 → ring 0 transitions
- Update on context switch

---

## TLS

- GS segment for TLS
- Load GS base on switch

---

## Exit Criteria

- [ ] Context switch working
- [ ] FPU preserved
- [ ] TSS.ESP0 updated

---

*End of i686 Context Switching*
