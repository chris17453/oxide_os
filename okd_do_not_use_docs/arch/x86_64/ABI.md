# x86_64 ABI

**Architecture:** x86_64
**Parent Spec:** [ABI_SPEC.md](../../ABI_SPEC.md)

---

## Registers

| Register | Usage | Callee-Saved |
|----------|-------|--------------|
| rax | Return value, syscall number | No |
| rdi, rsi, rdx, rcx, r8, r9 | Args 1-6 | No |
| rbx, rbp, r12-r15 | General purpose | Yes |
| rsp | Stack pointer | Yes |
| r10, r11 | Scratch | No |

---

## Calling Convention (System V AMD64)

- **Args:** rdi, rsi, rdx, rcx, r8, r9, then stack
- **Return:** rax (and rdx for 128-bit)
- **Stack:** 16-byte aligned at call
- **Red zone:** 128 bytes below rsp

---

## Syscall Convention

| Register | Usage |
|----------|-------|
| rax | Syscall number |
| rdi, rsi, rdx, r10, r8, r9 | Args 1-6 |
| rax | Return value |
| rcx, r11 | Clobbered by syscall |

Use `syscall` instruction.

---

## TLS

- FS base for user TLS
- GS base for kernel per-CPU
- FSGSBASE instructions or MSR

---

## Exit Criteria

- [ ] Function calls working
- [ ] Syscalls via SYSCALL working
- [ ] TLS via FS working

---

*End of x86_64 ABI*
