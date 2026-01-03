# ARM32 ABI

**Architecture:** ARM32
**Parent Spec:** [ABI_SPEC.md](../../ABI_SPEC.md)
**Reference:** AAPCS32

---

## Registers

| Register | Usage | Callee-Saved |
|----------|-------|--------------|
| r0-r3 | Args / return | No |
| r4-r11 | General | Yes |
| r12 (ip) | Scratch | No |
| r13 (sp) | Stack pointer | Yes |
| r14 (lr) | Link register | Special |
| r15 (pc) | Program counter | N/A |

---

## Calling Convention

- **Args:** r0-r3, then stack
- **Return:** r0 (r0-r1 for 64-bit)
- **Stack:** 8-byte aligned

---

## Syscall Convention

| Register | Usage |
|----------|-------|
| r7 | Syscall number |
| r0-r5 | Args 1-6 |
| r0 | Return value |

Use `svc #0`.

---

## TLS

- TPIDRURO via CP15

---

## Exit Criteria

- [ ] AAPCS working
- [ ] SVC syscalls working

---

*End of ARM32 ABI*
