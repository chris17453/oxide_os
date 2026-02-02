# OXIDE Userspace Multi-Architecture Support

**Last Updated:** 2026-02-02
**Status:** Implemented (x86_64 tested, ARM64/MIPS64 structure ready)

## Overview

OXIDE userspace libc now supports multiple architectures through conditional compilation. Each architecture provides its own syscall interface and entry point implementation.

## Supported Architectures

### 1. x86_64 (Intel/AMD 64-bit)

**Status:** ✅ Fully implemented and tested

#### Syscall ABI
- **Instruction:** `syscall`
- **Syscall number:** `rax`
- **Arguments:** `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`
- **Return value:** `rax`
- **Clobbered:** `rcx`, `r11`

#### Entry Point (_start)
```rust
// userspace/libc/src/arch/x86_64/start.rs
```

Stack layout on entry:
```
[rsp+0]  = argc
[rsp+8]  = argv[0]
[rsp+16] = argv[1]
...
```

Entry sequence:
1. Read argc from `[rsp]` into `r12`
2. Calculate argv as `rsp + 8` into `r13`
3. Call `init_env()` to set up environment
4. Call `init_stdio()` to initialize stdin/stdout/stderr
5. Call `init_environ()` for C compatibility
6. Call `main(argc, argv)`
7. Exit with return code

#### Linker Script
- **Base address:** `0x400000` (4MB)
- **File:** `userspace/userspace.ld`

---

### 2. ARM64 (aarch64)

**Status:** 🚧 Structure implemented, awaiting cross-compilation testing

#### Syscall ABI
- **Instruction:** `svc #0`
- **Syscall number:** `x8`
- **Arguments:** `x0`-`x5`
- **Return value:** `x0`

#### Entry Point (_start)
```rust
// userspace/libc/src/arch/aarch64/start.rs
```

Stack layout on entry (same as x86_64):
```
[sp+0]  = argc
[sp+8]  = argv[0]
[sp+16] = argv[1]
...
```

Entry sequence:
1. Read argc from `[sp]` into `x19` (callee-saved)
2. Calculate argv as `sp + 8` into `x20`
3. Call `init_env()` via `bl`
4. Call `init_stdio()` via `bl`
5. Call `init_environ()` via `bl`
6. Call `main(argc, argv)` with args in `w0`, `x1`
7. Exit with return code

#### Linker Script
- **Base address:** `0x400000` (4MB)
- **File:** `userspace/userspace-aarch64.ld`
- **Page alignment:** 4KB (4096 bytes)

---

### 3. MIPS64 (SGI)

**Status:** 🚧 Structure implemented, awaiting cross-compilation testing

**⚠️ BIG-ENDIAN:** SGI MIPS systems use big-endian byte order

#### Syscall ABI
- **Instruction:** `syscall`
- **Syscall number:** `$v0` (`$2`)
- **Arguments:** `$a0`-`$a5` (`$4`-`$9`)
- **Return value:** `$v0` (`$2`)

MIPS64 N64 calling convention:
- `$a0`-`$a7` (`$4`-`$11`): argument registers
- `$v0`-`$v1` (`$2`-`$3`): return values
- `$t0`-`$t9` (`$8`-`$15`, `$24`-`$25`): temporaries (caller-saved)
- `$s0`-`$s7` (`$16`-`$23`): saved registers (callee-saved)
- `$gp` (`$28`): global pointer
- `$sp` (`$29`): stack pointer
- `$fp` (`$30`): frame pointer
- `$ra` (`$31`): return address

#### Entry Point (_start)
```rust
// userspace/libc/src/arch/mips64/start.rs
```

Stack layout on entry (same as x86_64):
```
[sp+0]  = argc
[sp+8]  = argv[0]
[sp+16] = argv[1]
...
```

Entry sequence:
1. Read argc from `[$sp]` into `$s0` (callee-saved)
2. Calculate argv as `$sp + 8` into `$s1`
3. Call `init_env()` via `jal` (with `nop` delay slot)
4. Call `init_stdio()` via `jal`
5. Call `init_environ()` via `jal`
6. Call `main(argc, argv)` with args in `$a0`, `$a1`
7. Exit with return code

**Note:** All `jal` (jump and link) instructions require a delay slot (one instruction that executes before the branch takes effect). We use `nop` for simplicity.

#### Linker Script
- **Base address:** `0x400000` (4MB)
- **File:** `userspace/userspace-mips64.ld`
- **Page alignment:** 16KB (16384 bytes) for SGI compatibility
- **Output format:** `elf64-bigmips` (big-endian)
- **Global pointer:** `_gp` symbol at `.sdata + 0x8000`

**Key differences:**
1. **Big-endian output format** specified in linker script
2. **GP-relative data section** (`.sdata`) with global pointer
3. **Small BSS** (`.sbss`) for GP-relative uninitialized data

---

## File Structure

```
userspace/libc/src/
├── arch/
│   ├── mod.rs                 # Conditional compilation dispatch
│   ├── x86_64/
│   │   ├── mod.rs
│   │   ├── syscall.rs         # x86_64 syscall stubs
│   │   └── start.rs           # x86_64 _start entry point
│   ├── aarch64/
│   │   ├── mod.rs
│   │   ├── syscall.rs         # ARM64 syscall stubs
│   │   └── start.rs           # ARM64 _start entry point
│   └── mips64/
│       ├── mod.rs
│       ├── syscall.rs         # MIPS64 syscall stubs
│       └── start.rs           # MIPS64 _start entry point
├── lib.rs                     # Architecture-agnostic libc
├── syscall.rs                 # High-level syscall wrappers
└── ...                        # Other libc modules
```

## Conditional Compilation

The architecture is selected at compile time via Rust's `#[cfg(target_arch = "...")]`:

```rust
// In arch/mod.rs
#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::syscall;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::syscall;

#[cfg(target_arch = "mips64")]
pub mod mips64;
#[cfg(target_arch = "mips64")]
pub use mips64::syscall;
```

## Building for Different Architectures

### x86_64 (native or cross-compile)
```bash
cargo build -p libc --target x86_64-unknown-linux-gnu
```

### ARM64 (requires aarch64 toolchain)
```bash
cargo build -p libc --target aarch64-unknown-linux-gnu
```

### MIPS64 Big-Endian (requires mips64 toolchain)
```bash
cargo build -p libc --target mips64-unknown-linux-gnu
```

**Note:** Cross-compilation requires installing the appropriate target:
```bash
rustup target add aarch64-unknown-linux-gnu
rustup target add mips64-unknown-linux-gnu
```

You may also need to configure Cargo to use the correct linker in `.cargo/config.toml`:
```toml
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"

[target.mips64-unknown-linux-gnu]
linker = "mips64-linux-gnuabi64-gcc"
```

## Testing Status

| Architecture | Build | Runtime | Notes |
|--------------|-------|---------|-------|
| x86_64 | ✅ | ✅ | Tested in QEMU and native |
| aarch64 | 🚧 | ❌ | Structure ready, awaiting cross-compile |
| mips64 | 🚧 | ❌ | Structure ready, awaiting cross-compile |

---

## Implementation Details

### Syscall Interface

Each architecture provides `syscall0` through `syscall6` functions:

```rust
// Example: syscall3(nr, arg1, arg2, arg3)
pub fn syscall3(nr: u64, arg1: usize, arg2: usize, arg3: usize) -> i64;
```

These are used by high-level wrappers in `syscall.rs`:

```rust
pub fn sys_read(fd: i32, buf: &mut [u8]) -> isize {
    syscall3(nr::READ, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize
}
```

### Entry Point (_start)

Each architecture provides a `_start` function that:
1. Reads argc/argv from the stack
2. Initializes environment (`init_env`)
3. Initializes stdio streams (`init_stdio`)
4. Calls user's `main(argc, argv)`
5. Exits with return code

The entry point is architecture-specific due to:
- Different register names and calling conventions
- Different instruction encodings (naked assembly)
- Architecture-specific stack manipulation

---

## Future Work

1. **Test ARM64 cross-compilation** with aarch64 toolchain
2. **Test MIPS64 cross-compilation** with mips64 toolchain
3. **QEMU testing** for ARM64 and MIPS64 userspace
4. **Real hardware testing** on ARM64 boards and SGI MIPS workstations
5. **RISC-V support** (future architecture)

---

## References

- **x86_64 System V ABI:** https://gitlab.com/x86-psABIs/x86-64-ABI
- **ARM64 Procedure Call Standard:** ARM IHI 0055D
- **MIPS N64 ABI:** MIPS ABI Project, SGI documentation
- **ARCS Boot Specification:** SGI, 1990s

— NeonRoot, GraveShift (MIPS64 sections)
