# EXIT Syscall Analysis v1
**Date:** 2026-02-02
**Issue:** Userspace processes never exit - syscall instruction doesn't execute or reach kernel

## Problem Statement

Userspace processes hang when attempting to exit. Debug output shows:
1. `main()` executes and returns successfully
2. `_main_wrapper` returns with exit code
3. `_start` calls `sys_exit()`
4. `sys_exit` prints debug messages up to "About to call syscall_exit"
5. **syscall instruction never executes or never reaches kernel**
6. Process hangs indefinitely

**Critical observation:** No `[SYSCALL] exit (N)` message ever appears in kernel log.

## Current Code Flow

### 1. Program Entry (`userspace/libc/src/arch/x86_64/start.rs`)

```rust
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe {
        let argc: i32;
        let argv: *const *const u8;

        // Read argc/argv from stack per x86_64 ELF ABI
        core::arch::asm!(
            "mov {argc:e}, [rsp]",
            "lea {argv}, [rsp + 8]",
            argc = out(reg) argc,
            argv = out(reg) argv,
        );

        crate::env::init_env();
        crate::filestream::init_stdio();
        crate::c_exports::init_environ();

        let ret = _main_wrapper(argc, argv);

        // Debug: About to exit
        let _ = crate::syscall::sys_write(2, b"[START_DEBUG] _start calling sys_exit\n");

        // Exit using the normal syscall wrapper
        crate::syscall::sys_exit(ret);
    }
}

fn _main_wrapper(argc: i32, argv: *const *const u8) -> i32 {
    unsafe extern "Rust" {
        fn main(argc: i32, argv: *const *const u8) -> i32;
    }
    let ret = unsafe { main(argc, argv) };
    // ... debug output ...
    ret
}
```

**Status:** ✅ This part works - we see all debug messages through "START_DEBUG".

### 2. Exit Syscall Wrapper (`userspace/libc/src/syscall.rs`)

```rust
pub fn sys_exit(status: i32) -> ! {
    let _ = sys_write(2, b"[LIBC_DEBUG] sys_exit called with status=");
    // ... print status ...
    let _ = sys_write(2, b"[LIBC_DEBUG] About to call syscall_exit\n");
    syscall_exit(status as usize);
}
```

**Status:** ✅ Partially working - debug messages print, but `syscall_exit` never completes.

### 3. Architecture-Specific Syscall (`userspace/libc/src/arch/x86_64/syscall.rs`)

**Current version (explicit mov instructions):**
```rust
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_exit(status: usize) -> ! {
    unsafe {
        asm!(
            "mov rax, 0",      // EXIT syscall number
            "mov rdi, {0}",    // Exit status
            "syscall",         // Execute syscall
            "ud2",             // Undefined instruction - should never reach
            in(reg) status,
            options(noreturn),
        );
    }
}
```

**Status:** ❌ FAILS - syscall instruction doesn't execute or doesn't reach kernel.

### 4. Regular Syscalls (for comparison)

```rust
pub fn syscall1(nr: u64, arg1: usize) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") nr,
            in("rdi") arg1,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}
```

**Status:** ✅ Works perfectly for all other syscalls (write, read, open, etc.).

## What We've Tried

### Attempt 1: Direct inline asm in _start (naked_asm)
**Code:**
```rust
core::arch::asm!(
    "syscall",
    in("rax") 0u64,
    in("rdi") ret,
    options(noreturn, nostack)
);
```
**Result:** ❌ Syscall instruction never executes. Possibly due to naked_asm stack corruption.

### Attempt 2: Normal function with inline asm + noreturn
**Code:**
```rust
core::arch::asm!(
    "syscall",
    in("rax") 0u64,
    in("rdi") ret,
    lateout("rax") _exit_result,
    lateout("rcx") _,
    lateout("r11") _,
    options(noreturn)
);
```
**Result:** ❌ Compilation error - can't use lateout with noreturn.

### Attempt 3: Without options(noreturn)
**Code:**
```rust
core::arch::asm!(
    "syscall",
    in("rax") 0u64,
    in("rdi") ret,
    lateout("rax") _exit_result,
    lateout("rcx") _,
    lateout("r11") _,
    options(nostack)
);
```
**Result:** ❌ System hangs earlier (at service manager).

### Attempt 4: Call sys_exit wrapper
**Code:**
```rust
crate::syscall::sys_exit(ret);
```
**Result:** ❌ Same issue - syscall_exit doesn't execute.

### Attempt 5: Dedicated syscall_exit with noreturn
**Code:**
```rust
#[inline(always)]
pub fn syscall_exit(status: usize) -> ! {
    unsafe {
        asm!(
            "syscall",
            in("rax") 0u64,
            in("rdi") status,
            options(noreturn, nostack),
        );
    }
}
```
**Result:** ❌ Same issue.

### Attempt 6: inline(never) + volatile write
**Code:**
```rust
#[inline(never)]
pub fn syscall_exit(status: usize) -> ! {
    unsafe {
        core::ptr::write_volatile(&status as *const usize as *mut usize, status);
        asm!(
            "syscall",
            in("rax") 0u64,
            in("rdi") status,
            options(noreturn),
        );
    }
}
```
**Result:** ❌ Same issue.

### Attempt 7: Explicit mov instructions + no_mangle
**Code:**
```rust
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_exit(status: usize) -> ! {
    unsafe {
        asm!(
            "mov rax, 0",
            "mov rdi, {0}",
            "syscall",
            "ud2",
            in(reg) status,
            options(noreturn),
        );
    }
}
```
**Result:** ❌ Same issue (latest attempt).

## Key Observations

### What Works
1. ✅ All other syscalls (write, read, open, fork, etc.) work perfectly
2. ✅ The same inline asm pattern works for syscall1-6
3. ✅ Debug output via sys_write executes immediately before syscall_exit
4. ✅ Kernel receives and processes all other syscalls

### What Doesn't Work
1. ❌ EXIT syscall never reaches kernel (no kernel log entry)
2. ❌ Process hangs after "About to call syscall_exit" message
3. ❌ Control never returns to calling code (no "ERROR: returned" message)
4. ❌ waitpid in parent process never completes

### Critical Difference
The ONLY difference between working syscalls and EXIT:
- Working syscalls: expect to RETURN and have `lateout` for return value
- EXIT syscall: should NEVER return, uses `options(noreturn)`

## Technical Analysis

### x86_64 Syscall Mechanism
1. Setup: `rax` = syscall number, `rdi` = arg1, etc.
2. Execute: `syscall` instruction
3. CPU switches to kernel mode (CPL 0)
4. Kernel SYSCALL handler runs (defined by MSR)
5. For EXIT: kernel should terminate process (never return)
6. For others: kernel returns via `sysretq`

### Compiler Behavior with options(noreturn)
The `options(noreturn)` tells the compiler:
- This asm block never returns control
- Don't generate function epilogue code
- Don't expect stack cleanup after this point

**Hypothesis:** Compiler might be:
1. Optimizing away the asm block entirely
2. Generating code that skips the syscall instruction
3. Placing the syscall in a branch that's never taken
4. Some other code generation issue specific to noreturn

### Stack and Register State
When we reach syscall_exit:
- Stack should be valid (we just did sys_write successfully)
- Registers should be clean
- No interrupts disabled
- User mode (CPL 3)

## Potential Root Causes

### 1. Compiler Code Generation Bug
**Evidence:**
- Same asm works for other syscalls
- Multiple variations all fail the same way
- Even explicit mov instructions fail

**Test:** Examine generated assembly with `objdump` or `cargo asm`.

### 2. Kernel SYSCALL Handler Issue with EXIT
**Evidence:**
- No kernel log entry for EXIT
- Other syscalls work fine

**Test:** Add debug output at START of kernel syscall handler, before dispatch.

### 3. CPU/SYSCALL Mechanism Issue
**Evidence:**
- syscall instruction not executing at all
- No transition to kernel mode

**Test:** Add INT 0x80 fallback or test if syscall instruction is even reached.

### 4. Linker/Relocation Issue
**Evidence:**
- Release builds with optimization
- Complex inline asm with noreturn

**Test:** Build with debug symbols, check disassembly.

### 5. ABI Mismatch
**Evidence:**
- Using extern "C" for syscall_exit
- Mixing Rust and C calling conventions

**Test:** Try pure Rust fn or pure assembly function.

## Next Steps

### Immediate Actions
1. **Disassemble syscall_exit:** Check what assembly is actually generated
   ```bash
   objdump -d target/x86_64-unknown-none/release/liboxide_libc.a | grep -A20 syscall_exit
   ```

2. **Add kernel-side debug:** Put debug output at ENTRY of syscall handler
   ```rust
   pub extern "C" fn syscall_handler() {
       serial_println!("[KERNEL] Syscall handler entered!");
       // ... existing code
   }
   ```

3. **Test if syscall instruction is reached:** Add debug BEFORE syscall
   ```rust
   asm!("int3");  // Breakpoint
   asm!("syscall");
   ```

4. **Try alternative exit mechanism:** Use INT 0x80 or different syscall
   ```rust
   asm!("int 0x80", in("rax") 60u64, in("rdi") status);
   ```

### Long-term Investigation
1. Compare working syscall (e.g., write) vs EXIT at assembly level
2. Test with minimal reproducer outside OS context
3. Check Rust compiler version/flags for known issues
4. Verify SYSCALL MSR configuration in kernel

## Files to Review
- `userspace/libc/src/arch/x86_64/start.rs` - Entry point
- `userspace/libc/src/arch/x86_64/syscall.rs` - Syscall wrappers
- `userspace/libc/src/syscall.rs` - sys_exit wrapper
- `crates/arch/arch-x86_64/src/syscall.rs` - Kernel syscall handler
- `crates/syscall/syscall/src/lib.rs` - Syscall dispatch

## Related Issues
- Original bug report: "commands hang after arch refactoring"
- Keyboard input issue (FIXED)
- Debug output policy (wrap in features, never delete)

---
**Status:** BLOCKED - EXIT syscall fundamentally broken, cause unknown.
**Priority:** CRITICAL - No userspace process can exit, system unusable for shell commands.
