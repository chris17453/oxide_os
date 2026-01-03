# AArch64 ABI

**Architecture:** AArch64
**Parent Spec:** [ABI_SPEC.md](../../ABI_SPEC.md)
**Reference:** AAPCS64

---

## Registers

| Register | Usage | Callee-Saved |
|----------|-------|--------------|
| x0-x7 | Args / return | No |
| x8 | Indirect result / syscall # | No |
| x9-x15 | Scratch | No |
| x16-x17 | Intra-procedure scratch | No |
| x18 | Platform register | Reserved |
| x19-x28 | General purpose | Yes |
| x29 (fp) | Frame pointer | Yes |
| x30 (lr) | Link register | Special |
| sp | Stack pointer | Yes |

---

## Calling Convention

- **Args:** x0-x7, then stack
- **Return:** x0 (x0-x1 for 128-bit)
- **Stack:** 16-byte aligned always
- **No red zone**

---

## Syscall Convention

| Register | Usage |
|----------|-------|
| x8 | Syscall number |
| x0-x5 | Args 1-6 |
| x0 | Return value |

Use `svc #0`.

---

## TLS

- TPIDR_EL0 for user thread pointer

---

## Exit Criteria

- [ ] AAPCS64 calls working
- [ ] SVC syscalls working
- [ ] TPIDR_EL0 TLS working

---

*End of AArch64 ABI*
