# RISC-V 64 Context Switching

**Architecture:** RISC-V 64-bit
**Parent Spec:** [SCHEDULER_SPEC.md](../../SCHEDULER_SPEC.md)

---

## Context Structure

| Field | Description |
|-------|-------------|
| sp | Stack pointer |
| s0-s11 | Callee-saved registers |
| ra | Return address |
| satp | Page table + ASID |
| tp | Thread pointer (TLS) |
| fp regs | f8-f9, f18-f27 (callee-saved) |

---

## Switch Procedure

1. Save ra, sp, s0-s11
2. Load from new context
3. Switch satp if address space differs
4. sfence.vma after satp change
5. Return via ra

---

## FPU State

- F extension: f8-f9, f18-f27 callee-saved
- Save fcsr (FP control/status)

---

## TLS

- tp register (x4) for thread pointer

---

## ASID

- satp contains ASID field
- sfence.vma with ASID for targeted flush

---

## Exit Criteria

- [ ] Context switch working
- [ ] satp/ASID switched
- [ ] FPU preserved
- [ ] tp set correctly

---

*End of RISC-V 64 Context Switching*
