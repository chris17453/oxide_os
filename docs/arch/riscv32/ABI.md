# RISC-V 32 ABI

**Architecture:** RISC-V 32-bit
**Parent Spec:** [ABI_SPEC.md](../../ABI_SPEC.md)

---

## Registers

Same as RV64, 32-bit width.

| Register | ABI | Usage | Callee-Saved |
|----------|-----|-------|--------------|
| x0 | zero | Zero | N/A |
| x1 | ra | Return addr | No |
| x2 | sp | Stack ptr | Yes |
| x4 | tp | Thread ptr | N/A |
| x8-x9 | s0-s1 | Saved | Yes |
| x10-x17 | a0-a7 | Args | No |
| x18-x27 | s2-s11 | Saved | Yes |

---

## Calling Convention

- **Args:** a0-a7, then stack
- **Return:** a0 (a0-a1 for 64-bit)
- **Stack:** 16-byte aligned

---

## Syscall

- a7 = number, a0-a5 = args
- `ecall`

---

## Exit Criteria

- [ ] Calls working

---

*End of RISC-V 32 ABI*
