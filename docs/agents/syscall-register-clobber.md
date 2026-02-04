# Syscall Register Clobber Rule

## Rule

All userspace syscall wrapper functions in `userspace/libs/libc/src/arch/x86_64/syscall.rs` MUST declare **every caller-saved register** that is not used as an input as a `lateout` clobber.

## Why

Rust's inline asm treats `in()` registers as consumed after the asm block. The compiler is free to reuse any register not mentioned in constraints. When `#[inline(always)]` syscall wrappers are inlined into a function with multiple consecutive syscalls (e.g., `close(0); close(1); close(2); dup2(fd, 0);`), the compiler may place local variables in registers it considers "free" - then clobber them when setting up the next syscall's inputs.

The x86_64 `syscall` instruction clobbers RCX and R11. The OXIDE kernel's `syscall_entry` preserves and restores all other user registers. But Rust's optimizer doesn't know this - it only sees the constraints declared in the asm block.

## Registers to Clobber

For each `syscallN` function, declare all of these registers as `lateout(...) _` if they are NOT already used as `in(...)`:

- `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9` (syscall argument registers)
- `rcx`, `r11` (always clobbered by `syscall` instruction)

## Example Bug

Getty's `setup_terminal()` called `open2()` (returns fd=3), then `close(0); close(1); close(2); dup2(fd, 0)`. Without proper clobber declarations, the compiler reused the register holding `fd` (value 3) as scratch during the inlined `close()` calls. By the time `dup2` executed, `fd` contained `1` instead of `3`, causing `dup2(1, 0)` to fail with EBADF since fd 1 had been closed. This cascaded: login inherited no fds 0/1/2, causing `read(0)` to return EBADF ~1,166 times/second.

## Files

- `userspace/libs/libc/src/arch/x86_64/syscall.rs` - the fix location
- `kernel/arch/arch-x86_64/src/syscall.rs` - kernel side (preserves all user regs correctly)
