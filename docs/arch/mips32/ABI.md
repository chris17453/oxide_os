# MIPS32 ABI

**Architecture:** MIPS32
**Parent Spec:** [ABI_SPEC.md](../../ABI_SPEC.md)
**Reference:** o32 ABI

---

## Registers

| Register | Name | Usage | Callee-Saved |
|----------|------|-------|--------------|
| $0 | zero | Zero | N/A |
| $2-$3 | v0-v1 | Return | No |
| $4-$7 | a0-a3 | Args | No |
| $16-$23 | s0-s7 | Saved | Yes |
| $28 | gp | Global ptr | Yes |
| $29 | sp | Stack ptr | Yes |
| $30 | fp | Frame ptr | Yes |
| $31 | ra | Return addr | Special |

---

## Calling Convention

- **Args:** a0-a3, then stack
- **Return:** v0 (v0-v1 for 64-bit)
- **Stack:** 8-byte aligned

---

## Syscall

- v0 = number, a0-a3 = args
- `syscall`

---

## Exit Criteria

- [ ] o32 ABI working

---

*End of MIPS32 ABI*
