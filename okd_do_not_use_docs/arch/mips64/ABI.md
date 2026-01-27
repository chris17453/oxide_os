# MIPS64 ABI

**Architecture:** MIPS64
**Parent Spec:** [ABI_SPEC.md](../../ABI_SPEC.md)
**Reference:** n64 ABI

---

## Registers

| Register | Name | Usage | Callee-Saved |
|----------|------|-------|--------------|
| $0 | zero | Zero | N/A |
| $2-$3 | v0-v1 | Return | No |
| $4-$11 | a0-a7 | Args | No |
| $16-$23 | s0-s7 | Saved | Yes |
| $28 | gp | Global ptr | Yes |
| $29 | sp | Stack ptr | Yes |
| $30 | fp | Frame ptr | Yes |
| $31 | ra | Return addr | Special |

---

## Calling Convention

- **Args:** a0-a7, then stack
- **Return:** v0 (v0-v1 for 128-bit)
- **Stack:** 16-byte aligned

---

## Syscall Convention

| Register | Usage |
|----------|-------|
| v0 | Syscall number |
| a0-a5 | Args 1-6 |
| v0 | Return |
| a3 | Error flag |

Use `syscall`.

---

## Exit Criteria

- [ ] n64 ABI working
- [ ] Syscalls working

---

*End of MIPS64 ABI*
