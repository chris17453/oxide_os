# i686 ABI

**Architecture:** i686
**Parent Spec:** [ABI_SPEC.md](../../ABI_SPEC.md)

---

## Registers

| Register | Usage | Callee-Saved |
|----------|-------|--------------|
| eax | Return value | No |
| ebx, esi, edi, ebp | General | Yes |
| ecx, edx | Scratch | No |
| esp | Stack pointer | Yes |

---

## Calling Convention (cdecl)

- **Args:** All on stack, right-to-left
- **Return:** eax (edx:eax for 64-bit)
- **Stack:** Caller cleans, 16-byte aligned at call

---

## Syscall Convention

| Register | Usage |
|----------|-------|
| eax | Syscall number |
| ebx, ecx, edx, esi, edi, ebp | Args 1-6 |
| eax | Return value |

Use `int 0x80`.

---

## TLS

- GS segment for TLS

---

## Exit Criteria

- [ ] cdecl working
- [ ] INT 0x80 syscalls working
- [ ] TLS via GS working

---

*End of i686 ABI*
