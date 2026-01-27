# ARM32 Context Switching

**Architecture:** ARM32
**Parent Spec:** [SCHEDULER_SPEC.md](../../SCHEDULER_SPEC.md)

---

## Context Structure

| Field | Description |
|-------|-------------|
| sp | Stack pointer |
| r4-r11 | Callee-saved registers |
| lr | Link register |
| cpsr | Status register |
| ttbr0 | Page table base |
| tls | TLS pointer (TPIDRURO) |

---

## Switch Procedure

1. Save r4-r11, lr, sp
2. Load from new context
3. Switch TTBR0 if needed
4. DSB + ISB after TTBR
5. Return via lr

---

## VFP State

- Save/restore d8-d15 if VFP used
- FPSCR status register

---

## TLS

- TPIDRURO (user read-only TLS)
- TPIDRPRW (privileged TLS)

---

## Exit Criteria

- [ ] Context switch working
- [ ] VFP preserved
- [ ] TLS switched

---

*End of ARM32 Context Switching*
