# AArch64 Context Switching

**Architecture:** AArch64
**Parent Spec:** [SCHEDULER_SPEC.md](../../SCHEDULER_SPEC.md)

---

## Context Structure

| Field | Description |
|-------|-------------|
| sp | Stack pointer |
| x19-x28 | Callee-saved registers |
| x29 (fp) | Frame pointer |
| x30 (lr) | Link register |
| ttbr0_el1 | User page table |
| tpidr_el0 | User TLS pointer |
| fpsimd | FP/SIMD state (v8-v15 callee-saved) |

---

## Switch Procedure

1. Save x19-x30, sp to old context
2. Load x19-x30, sp from new context
3. Switch TTBR0 if address space differs
4. ISB after TTBR switch
5. Return via x30

---

## FP/SIMD State

- v8-v15 lower 64 bits are callee-saved
- Full state save/restore if thread uses SIMD

---

## TLS

- TPIDR_EL0 for user TLS
- TPIDR_EL1 for kernel per-CPU

---

## Kernel Stack

- SP_EL1 for exception entry
- Each thread has kernel stack

---

## Exit Criteria

- [ ] Context switch working
- [ ] TTBR0 switched correctly
- [ ] FP/SIMD preserved
- [ ] TLS working

---

*End of AArch64 Context Switching*
