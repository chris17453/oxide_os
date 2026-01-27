# x86_64 Context Switching

**Architecture:** x86_64
**Parent Spec:** [SCHEDULER_SPEC.md](../../SCHEDULER_SPEC.md)

---

## Context Structure

| Field | Description |
|-------|-------------|
| rsp | Stack pointer |
| rbp, rbx, r12-r15 | Callee-saved registers |
| rip | Return address (on stack) |
| cr3 | Page table base |
| fs_base, gs_base | TLS bases |
| fpu_state | FXSAVE/XSAVE area |

---

## Switch Procedure

1. Push callee-saved regs (rbp, rbx, r12-r15)
2. Save RSP to old context
3. Load RSP from new context
4. Switch CR3 if address space differs
5. Pop callee-saved regs
6. Return (RIP on stack)

---

## FPU State

- Use FXSAVE/FXRSTOR (SSE) or XSAVE/XRSTOR (AVX+)
- Lazy FPU: set CR0.TS, handle #NM
- Or eager: always save/restore

---

## Kernel Stack

- Per-thread kernel stack
- TSS.RSP0 points to current thread's kernel stack
- Update TSS on context switch

---

## TLS

- FS base for user TLS (via FSGSBASE or MSR)
- GS base for kernel per-CPU data
- SWAPGS on kernel entry/exit

---

## Exit Criteria

- [ ] Context switch working
- [ ] FPU state preserved
- [ ] TSS.RSP0 updated
- [ ] TLS bases switched

---

*End of x86_64 Context Switching*
