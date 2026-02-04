# Kernel Syscall Exit: Register Restore Rule

## Rule

The kernel's syscall exit path in `kernel/arch/arch-x86_64/src/syscall.rs` MUST restore **all caller-saved registers** from `SYSCALL_USER_CONTEXT` after ANY C-ABI function call (e.g., `signal_check`) and before executing `sysretq`.

## Why

The x86_64 syscall ABI contract: the kernel preserves **every user register** except RCX (overwritten with return RIP) and R11 (overwritten with saved RFLAGS). Userspace compilers (LLVM/GCC) rely on this — they freely keep live values in RSI, RDX, R8-R10 across `syscall` instructions.

The System V C calling convention says RAX, RCX, RDX, RSI, RDI, R8-R11 are **caller-saved**. Any `call` instruction to a C function may clobber all of them. If the syscall exit path calls a C function (like `signal_check`) and then returns to userspace without restoring these registers, userspace sees corrupted values.

## Registers to Restore

After any C-ABI call in the syscall exit path, reload from `SYSCALL_USER_CONTEXT`:

- `rdi` (offset 64) — first syscall arg / general purpose
- `rsi` (offset 56) — second syscall arg / general purpose
- `rdx` (offset 48) — third syscall arg / general purpose
- `r8`  (offset 80) — fifth syscall arg / general purpose
- `r9`  (offset 88) — sixth syscall arg / general purpose
- `r10` (offset 96) — fourth syscall arg (substitutes RCX) / general purpose

RAX is the syscall return value (already handled separately). RCX and R11 are restored by `sysretq` itself.

## SYSCALL_USER_CONTEXT Layout

```
offset  0: rip
offset  8: rsp
offset 16: rflags
offset 24: rax
offset 32: rbx
offset 40: rcx
offset 48: rdx
offset 56: rsi
offset 64: rdi
offset 72: rbp
offset 80: r8
offset 88: r9
offset 96: r10
offset 104: r11
offset 112: r12
offset 120: r13
offset 128: r14
offset 136: r15
```

## Example Bug

Servicemgr (PID 2) called `getdents64` (syscall 0x54) with RSI pointing to a stack buffer (`lea 0x1b8(%rsp), %rsi`). The kernel correctly executed the syscall and saved RSI to `SYSCALL_USER_CONTEXT`. Then `signal_check()` was called — a C function that clobbered RSI to 0. The exit path only restored RDI, not RSI. Userspace code did `mov %rsi, %r8` (expecting the buffer pointer) then `movzwl 0xe(%r8), %ebp` — dereferencing address 0xe. Page fault, SIGSEGV, PID 2 killed.

This only manifested in **release builds** because LLVM's optimizer aggressively reuses registers across syscall boundaries (which is correct per the ABI contract). Debug builds happened to reload from the stack, masking the bug.

## Relationship to Userspace Clobber Rule

`docs/agents/syscall-register-clobber.md` documents the **userspace side**: inline asm `lateout` declarations so the Rust compiler knows registers are clobbered. That rule is **defense in depth** — it protects against compiler assumptions even if the kernel has a bug. This rule documents the **kernel side**: the kernel must actually preserve the registers per the ABI contract.

Both rules must hold simultaneously for correct operation.

## Files

- `kernel/arch/arch-x86_64/src/syscall.rs` — the fix location (register restore block after `signal_check`)
- `userspace/libs/libc/src/arch/x86_64/syscall.rs` — userspace side (companion rule)
- `docs/agents/syscall-register-clobber.md` — companion userspace rule
