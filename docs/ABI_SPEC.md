# EFFLUX Application Binary Interface (ABI) Specification

**Version:** 1.0
**Status:** Draft
**License:** MIT

---

## 0) Overview

This specification defines the ABI for EFFLUX OS across all supported architectures:

- Calling conventions (function calls)
- Syscall conventions (user → kernel)
- Data type sizes and alignment
- Stack layout
- Register usage
- ELF binary format specifics

Architecture-specific details are in `docs/arch/<arch>/ABI.md`.

---

## 1) Supported ABIs

| Architecture | ABI Name | Pointer Size | Endianness |
|--------------|----------|--------------|------------|
| x86_64 | System V AMD64 | 64-bit | Little |
| i686 | System V i386 | 32-bit | Little |
| AArch64 | AAPCS64 | 64-bit | Little |
| ARM32 | AAPCS | 32-bit | Little |
| MIPS64 | n64 | 64-bit | Big (configurable) |
| MIPS32 | o32 | 32-bit | Big (configurable) |
| RISC-V 64 | LP64D | 64-bit | Little |
| RISC-V 32 | ILP32D | 32-bit | Little |

---

## 2) Common Data Types

### 2.1 Fundamental Types

| Type | 32-bit Size | 64-bit Size | Alignment |
|------|-------------|-------------|-----------|
| char | 1 | 1 | 1 |
| short | 2 | 2 | 2 |
| int | 4 | 4 | 4 |
| long | 4 | 8 | 4/8 |
| long long | 8 | 8 | 8 |
| pointer | 4 | 8 | 4/8 |
| size_t | 4 | 8 | 4/8 |
| float | 4 | 4 | 4 |
| double | 8 | 8 | 8 |

### 2.2 Rust Types

```rust
/// Architecture-independent type definitions

// Pointer-width types
#[cfg(target_pointer_width = "64")]
pub type usize = u64;
#[cfg(target_pointer_width = "32")]
pub type usize = u32;

#[cfg(target_pointer_width = "64")]
pub type isize = i64;
#[cfg(target_pointer_width = "32")]
pub type isize = i32;

// Fixed-width types (always same)
pub type u8 = core::primitive::u8;
pub type u16 = core::primitive::u16;
pub type u32 = core::primitive::u32;
pub type u64 = core::primitive::u64;
pub type u128 = core::primitive::u128;

pub type i8 = core::primitive::i8;
pub type i16 = core::primitive::i16;
pub type i32 = core::primitive::i32;
pub type i64 = core::primitive::i64;
pub type i128 = core::primitive::i128;

// Kernel types
pub type pid_t = i32;
pub type uid_t = u32;
pub type gid_t = u32;
pub type mode_t = u32;
pub type off_t = i64;  // Always 64-bit for large file support
pub type dev_t = u64;
pub type ino_t = u64;
pub type nlink_t = u64;
pub type blksize_t = i64;
pub type blkcnt_t = i64;
pub type time_t = i64;
pub type clockid_t = i32;
```

---

## 3) Calling Convention Abstraction

### 3.1 Generic Trait

```rust
/// Calling convention trait - architecture implements this
pub trait CallingConvention {
    /// Number of registers for integer arguments
    const INT_ARG_REGS: usize;

    /// Number of registers for floating point arguments
    const FP_ARG_REGS: usize;

    /// Stack alignment requirement
    const STACK_ALIGN: usize;

    /// Red zone size (stack space callee can use without adjusting SP)
    const RED_ZONE: usize;

    /// Does this ABI pass structs by value in registers?
    const STRUCT_IN_REGS: bool;

    /// Maximum struct size passable in registers
    const MAX_STRUCT_REG_SIZE: usize;
}
```

### 3.2 Summary by Architecture

| Architecture | Int Args | FP Args | Stack Align | Red Zone |
|--------------|----------|---------|-------------|----------|
| x86_64 | 6 | 8 | 16 | 128 |
| i686 | 0 (stack) | 0 (stack) | 4/16 | 0 |
| AArch64 | 8 | 8 | 16 | 0 |
| ARM32 | 4 | 16 (VFP) | 8 | 0 |
| MIPS64 n64 | 8 | 8 | 16 | 0 |
| MIPS32 o32 | 4 | 2 | 8 | 0 |
| RISC-V 64 | 8 | 8 | 16 | 0 |
| RISC-V 32 | 8 | 8 | 16 | 0 |

---

## 4) Syscall ABI

### 4.1 Generic Syscall Interface

```rust
/// Syscall numbers are architecture-specific but follow a pattern
/// See arch/<arch>/ABI.md for the full syscall table

/// Generic syscall trait
pub trait SyscallAbi {
    /// Register for syscall number
    const SYSCALL_NUM_REG: &'static str;

    /// Registers for arguments (in order)
    const ARG_REGS: &'static [&'static str];

    /// Register(s) for return value
    const RET_REGS: &'static [&'static str];

    /// Instruction to invoke syscall
    const SYSCALL_INSN: &'static str;
}
```

### 4.2 Syscall Convention Summary

| Architecture | Number | Arg1 | Arg2 | Arg3 | Arg4 | Arg5 | Arg6 | Return | Instruction |
|--------------|--------|------|------|------|------|------|------|--------|-------------|
| x86_64 | rax | rdi | rsi | rdx | r10 | r8 | r9 | rax | syscall |
| i686 | eax | ebx | ecx | edx | esi | edi | ebp | eax | int 0x80 |
| AArch64 | x8 | x0 | x1 | x2 | x3 | x4 | x5 | x0 | svc #0 |
| ARM32 | r7 | r0 | r1 | r2 | r3 | r4 | r5 | r0 | svc #0 |
| MIPS64 | v0 | a0 | a1 | a2 | a3 | a4 | a5 | v0 | syscall |
| MIPS32 | v0 | a0 | a1 | a2 | a3 | (stack) | (stack) | v0 | syscall |
| RISC-V | a7 | a0 | a1 | a2 | a3 | a4 | a5 | a0 | ecall |

### 4.3 Syscall Numbers

EFFLUX uses its own syscall numbers (not Linux-compatible numbers):

```rust
/// Syscall number definitions (same across all architectures)
pub mod syscall_num {
    // Process management
    pub const SYS_EXIT: usize = 0;
    pub const SYS_FORK: usize = 1;
    pub const SYS_EXEC: usize = 2;
    pub const SYS_WAIT: usize = 3;
    pub const SYS_GETPID: usize = 4;
    pub const SYS_GETPPID: usize = 5;

    // File operations
    pub const SYS_OPEN: usize = 10;
    pub const SYS_CLOSE: usize = 11;
    pub const SYS_READ: usize = 12;
    pub const SYS_WRITE: usize = 13;
    pub const SYS_LSEEK: usize = 14;
    pub const SYS_STAT: usize = 15;
    pub const SYS_FSTAT: usize = 16;

    // Memory management
    pub const SYS_MMAP: usize = 20;
    pub const SYS_MUNMAP: usize = 21;
    pub const SYS_MPROTECT: usize = 22;
    pub const SYS_BRK: usize = 23;

    // Time
    pub const SYS_CLOCK_GETTIME: usize = 30;
    pub const SYS_NANOSLEEP: usize = 31;

    // ... continued in full syscall table
}
```

---

## 5) Stack Layout

### 5.1 Generic Stack Frame

```
High addresses
┌─────────────────────────────────────┐
│  Caller's stack frame               │
├─────────────────────────────────────┤
│  Return address (some arches)       │
├─────────────────────────────────────┤
│  Saved frame pointer (if used)      │
├─────────────────────────────────────┤
│  Callee-saved registers             │
├─────────────────────────────────────┤
│  Local variables                    │
├─────────────────────────────────────┤
│  Outgoing arguments (stack args)    │
├─────────────────────────────────────┤ ← Stack pointer
│  Red zone (if applicable)           │
└─────────────────────────────────────┘
Low addresses
```

### 5.2 Process Initial Stack

```
Top of stack (high address)
┌─────────────────────────────────────┐
│  Null terminator                    │
├─────────────────────────────────────┤
│  Environment strings                │
├─────────────────────────────────────┤
│  Argument strings                   │
├─────────────────────────────────────┤
│  Padding for alignment              │
├─────────────────────────────────────┤
│  Null auxiliary vector entry        │
├─────────────────────────────────────┤
│  Auxiliary vector entries           │
│  (AT_PHDR, AT_ENTRY, AT_UID, etc.)  │
├─────────────────────────────────────┤
│  NULL (envp terminator)             │
├─────────────────────────────────────┤
│  envp[n-1]                          │
│  ...                                │
│  envp[0]                            │
├─────────────────────────────────────┤
│  NULL (argv terminator)             │
├─────────────────────────────────────┤
│  argv[argc-1]                       │
│  ...                                │
│  argv[0]                            │
├─────────────────────────────────────┤
│  argc                               │
└─────────────────────────────────────┘ ← Initial stack pointer
```

### 5.3 Auxiliary Vector

```rust
/// Auxiliary vector entry types
pub const AT_NULL: usize = 0;      // End of vector
pub const AT_PHDR: usize = 3;      // Program headers address
pub const AT_PHENT: usize = 4;     // Size of program header entry
pub const AT_PHNUM: usize = 5;     // Number of program headers
pub const AT_PAGESZ: usize = 6;    // Page size
pub const AT_BASE: usize = 7;      // Interpreter base address
pub const AT_FLAGS: usize = 8;     // Flags
pub const AT_ENTRY: usize = 9;     // Program entry point
pub const AT_UID: usize = 11;      // Real user ID
pub const AT_EUID: usize = 12;     // Effective user ID
pub const AT_GID: usize = 13;      // Real group ID
pub const AT_EGID: usize = 14;     // Effective group ID
pub const AT_PLATFORM: usize = 15; // Platform string
pub const AT_HWCAP: usize = 16;    // Hardware capabilities
pub const AT_CLKTCK: usize = 17;   // Clock ticks per second
pub const AT_RANDOM: usize = 25;   // Random bytes for stack canary
pub const AT_EXECFN: usize = 31;   // Executable filename

#[repr(C)]
pub struct AuxvEntry {
    pub a_type: usize,
    pub a_val: usize,
}
```

---

## 6) ELF Binary Format

### 6.1 Supported ELF Types

| Architecture | Class | Data | Machine |
|--------------|-------|------|---------|
| x86_64 | ELFCLASS64 | ELFDATA2LSB | EM_X86_64 (62) |
| i686 | ELFCLASS32 | ELFDATA2LSB | EM_386 (3) |
| AArch64 | ELFCLASS64 | ELFDATA2LSB | EM_AARCH64 (183) |
| ARM32 | ELFCLASS32 | ELFDATA2LSB | EM_ARM (40) |
| MIPS64 | ELFCLASS64 | ELFDATA2MSB | EM_MIPS (8) |
| MIPS32 | ELFCLASS32 | ELFDATA2MSB | EM_MIPS (8) |
| RISC-V 64 | ELFCLASS64 | ELFDATA2LSB | EM_RISCV (243) |
| RISC-V 32 | ELFCLASS32 | ELFDATA2LSB | EM_RISCV (243) |

### 6.2 Program Headers

```rust
/// Required program header types
pub const PT_NULL: u32 = 0;    // Unused
pub const PT_LOAD: u32 = 1;    // Loadable segment
pub const PT_DYNAMIC: u32 = 2; // Dynamic linking info
pub const PT_INTERP: u32 = 3;  // Interpreter path
pub const PT_NOTE: u32 = 4;    // Auxiliary info
pub const PT_PHDR: u32 = 6;    // Program header table
pub const PT_TLS: u32 = 7;     // Thread-local storage

/// EFFLUX-specific
pub const PT_EFFLUX_TRUST: u32 = 0x60000001; // Trust/signature info
```

### 6.3 Relocation Types

Each architecture has its own relocation types. See `docs/arch/<arch>/ABI.md`.

---

## 7) Thread-Local Storage (TLS)

### 7.1 TLS Model

EFFLUX uses the ELF TLS model with architecture-specific implementations:

```rust
/// TLS descriptor
#[repr(C)]
pub struct TlsDescriptor {
    /// Pointer to TLS data for current thread
    pub tls_base: *mut u8,

    /// Pointer to dynamic thread vector (DTV)
    pub dtv: *mut *mut u8,

    /// Thread pointer value
    pub thread_pointer: usize,
}
```

### 7.2 Thread Pointer Register

| Architecture | Thread Pointer |
|--------------|----------------|
| x86_64 | fs segment base |
| i686 | gs segment base |
| AArch64 | TPIDR_EL0 |
| ARM32 | CP15 c13 |
| MIPS | $k0 or userlocal |
| RISC-V | tp (x4) |

---

## 8) Signal Handling

### 8.1 Signal Frame

When a signal is delivered, the kernel pushes a signal frame:

```rust
/// Generic signal frame structure
#[repr(C)]
pub struct SigFrame {
    /// Saved register context
    pub context: arch::SignalContext,

    /// Signal information
    pub siginfo: SigInfo,

    /// Return trampoline address
    pub retcode: usize,

    /// Signal mask to restore
    pub oldmask: SigSet,
}
```

### 8.2 Signal Context (Architecture-Specific)

Each architecture defines its own `SignalContext` containing all registers.

---

## 9) Kernel-User Interface Structures

### 9.1 struct stat

```rust
/// File status structure (same layout across arches, 64-bit fields)
#[repr(C)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub __pad1: u64,
    pub st_size: i64,
    pub st_blksize: i32,
    pub __pad2: i32,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_atime_nsec: i64,
    pub st_mtime: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime: i64,
    pub st_ctime_nsec: i64,
    pub __unused: [i32; 2],
}
```

### 9.2 struct timespec

```rust
#[repr(C)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,  // i64 for alignment, actual range 0-999999999
}
```

---

## 10) vDSO Interface

### 10.1 vDSO Functions

Every process has these functions mapped:

```c
// Time functions (no syscall needed)
int __vdso_clock_gettime(clockid_t clock_id, struct timespec *tp);
int __vdso_gettimeofday(struct timeval *tv, struct timezone *tz);
time_t __vdso_time(time_t *tloc);

// CPU identification
int __vdso_getcpu(unsigned *cpu, unsigned *node, void *unused);
```

### 10.2 vDSO Symbol Versioning

```
EFFLUX_1.0 {
    global:
        __vdso_clock_gettime;
        __vdso_gettimeofday;
        __vdso_time;
        __vdso_getcpu;
    local:
        *;
};
```

---

## 11) Architecture Implementation Files

Each architecture provides detailed ABI documentation in:

```
docs/arch/<arch>/ABI.md
```

Contents:
- Register usage and calling convention
- Syscall convention details
- Relocation types
- Signal context layout
- TLS implementation
- Platform-specific extensions

---

## 12) Exit Criteria

- [ ] All syscall ABIs documented and implemented
- [ ] Calling conventions match standard ABI documents
- [ ] ELF loading works for all architectures
- [ ] TLS works on all architectures
- [ ] Signal delivery works on all architectures
- [ ] vDSO works on all architectures

---

*End of EFFLUX ABI Specification*

*See `docs/arch/<arch>/ABI.md` for architecture-specific details.*
