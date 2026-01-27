# RISC-V 64 ABI

**Architecture:** RISC-V 64-bit
**Parent Spec:** [ABI_SPEC.md](../../ABI_SPEC.md)

---

## Registers

| Register | ABI | Usage | Callee-Saved |
|----------|-----|-------|--------------|
| x0 | zero | Zero | N/A |
| x1 | ra | Return addr | No |
| x2 | sp | Stack ptr | Yes |
| x4 | tp | Thread ptr | N/A |
| x8-x9 | s0-s1 | Saved | Yes |
| x10-x11 | a0-a1 | Args/return | No |
| x12-x17 | a2-a7 | Args | No |
| x18-x27 | s2-s11 | Saved | Yes |

---

## Calling Convention

- **Args:** a0-a7, then stack
- **Return:** a0 (a0-a1 for 128-bit)
- **Stack:** 16-byte aligned

---

## Syscall

| Register | Usage |
|----------|-------|
| a7 | Syscall number |
| a0-a5 | Args |
| a0 | Return |

Use `ecall`.

---

## TLS

- tp register (x4)

---

## Exit Criteria

- [ ] Calls working
- [ ] ecall working

---

*End of RISC-V 64 ABI*
