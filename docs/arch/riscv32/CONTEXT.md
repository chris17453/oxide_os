# RISC-V 32 Context Switching

**Architecture:** RISC-V 32-bit
**Parent Spec:** [SCHEDULER_SPEC.md](../../SCHEDULER_SPEC.md)

---

## Context Structure

| Field | Description |
|-------|-------------|
| sp | Stack pointer |
| s0-s11 | Callee-saved |
| ra | Return address |
| satp | Page table + ASID |
| tp | Thread pointer |

---

## Switch Procedure

1. Save ra, sp, s0-s11
2. Load from new context
3. Switch satp if needed
4. sfence.vma
5. Return via ra

---

## TLS

- tp register for TLS

---

## Exit Criteria

- [ ] Context switch working
- [ ] satp switched

---

*End of RISC-V 32 Context Switching*
