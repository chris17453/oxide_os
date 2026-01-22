# Task Status - January 22, 2026

## Problem
`cat group` (and other commands) hang after exec because libc's `_start` function doesn't pass argc/argv to main().

## Root Cause Found
The libc `_start` function calls `init_env()` which clobbers the argument registers (rdi, rsi, rdx), then calls `_main_wrapper()` which doesn't pass argc/argv to `main()`. Programs receive garbage values.

## Fix Applied (in userspace/libc/src/lib.rs)
Changed `_start` to:
1. Save argc/argv from stack to callee-saved registers (r12, r13)
2. Call init_env
3. Restore and pass argc/argv to main

```rust
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mov r12, [rsp]",        // argc from stack
        "lea r13, [rsp + 8]",    // argv = &stack[1]
        "call {init_env}",
        "mov edi, r12d",         // argc
        "mov rsi, r13",          // argv
        "call {main}",
        "mov edi, eax",
        "call {exit}",
        "ud2",
        ...
    )
}
```

## Build Issues (blocking rebuild)

### 1. Stable Rust vs Nightly
- Fedora has stable Rust 1.92.0, not nightly
- Commented out `[unstable]` section in `.cargo/config.toml`
- Changed Makefile to use `x86_64-unknown-none` target (Fedora has pre-built std)

### 2. Global Allocator Conflicts
- Added bump allocator to libc for `alloc` crate support
- But gwbasic has its own allocator in `apps/gwbasic/src/oxide_main.rs:270`
- **Need to remove gwbasic's allocator** or make libc's optional

### 3. Disabled Files (incomplete reboot support)
Moved to .disabled:
- `userspace/coreutils/src/bin/poweroff.rs.disabled`
- `userspace/coreutils/src/bin/reboot.rs.disabled`
- `userspace/coreutils/src/bin/shutdown.rs.disabled`

These need `reboot()` and `reboot_cmd` added to libc.

## Files Modified
- `/home/nd/repos/oxide_os/userspace/libc/src/lib.rs` - Fixed _start, added allocator
- `/home/nd/repos/oxide_os/.cargo/config.toml` - Commented unstable section
- `/home/nd/repos/oxide_os/Makefile` - Changed USERSPACE_TARGET to x86_64-unknown-none, added RUSTFLAGS
- `/home/nd/repos/oxide_os/userspace/coreutils/src/bin/gzip.rs` - Removed duplicate allocator
- `/home/nd/repos/oxide_os/userspace/coreutils/src/bin/gunzip.rs` - Removed duplicate allocator
- `/home/nd/repos/oxide_os/kernel/src/main.rs` - Added `#[repr(C)]` to ParentContext struct

## Next Steps to Complete Build
1. Remove allocator from `apps/gwbasic/src/oxide_main.rs` (around line 270)
2. Run `make initramfs`
3. Run `make run-no-net` to test

## Other Fix Applied
Added `#[repr(C)]` to `ParentContext` struct in kernel to fix register corruption on waitpid return. Without it, Rust reordered struct fields and assembly offsets were wrong.
