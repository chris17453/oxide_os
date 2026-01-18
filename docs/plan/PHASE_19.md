# Phase 19: Self-Hosting

**Stage:** 4 - Advanced
**Status:** Not Started
**Dependencies:** Phase 11 (Storage), Phase 12 (Networking)

---

## Goal

Enable EFFLUX to compile its own kernel (self-hosting).

---

## Deliverables

| Item | Status |
|------|--------|
| Port LLVM | [ ] |
| Port rustc | [ ] |
| Port cargo | [ ] |
| Full pthread support | [ ] |
| mmap/munmap | [ ] |
| Dynamic linking basics | [ ] |

---

## Architecture Status

| Arch | LLVM | rustc | cargo | pthreads | Done |
|------|------|-------|-------|----------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Self-Hosting Requirements

```
To compile rustc, we need:
├── LLVM (compiler backend)
│   ├── libLLVM.so
│   ├── clang (for C/C++ code)
│   └── lld (linker)
├── rustc (Rust compiler)
│   ├── Stage 0: Bootstrap from cross-compiled
│   ├── Stage 1: Compile with Stage 0
│   └── Stage 2: Compile with Stage 1 (final)
├── cargo (build system)
├── Supporting tools:
│   ├── cmake
│   ├── ninja
│   ├── python
│   └── git
└── System requirements:
    ├── pthreads
    ├── mmap
    ├── dlopen/dlsym
    └── ~16GB RAM (for LLVM)
```

---

## Pthread Implementation

```rust
// Thread management
pub fn pthread_create(
    thread: *mut pthread_t,
    attr: *const pthread_attr_t,
    start: fn(*mut c_void) -> *mut c_void,
    arg: *mut c_void,
) -> c_int;

pub fn pthread_join(thread: pthread_t, retval: *mut *mut c_void) -> c_int;

// Mutexes
pub fn pthread_mutex_init(mutex: *mut pthread_mutex_t, attr: *const pthread_mutexattr_t) -> c_int;
pub fn pthread_mutex_lock(mutex: *mut pthread_mutex_t) -> c_int;
pub fn pthread_mutex_unlock(mutex: *mut pthread_mutex_t) -> c_int;
pub fn pthread_mutex_destroy(mutex: *mut pthread_mutex_t) -> c_int;

// Condition variables
pub fn pthread_cond_init(cond: *mut pthread_cond_t, attr: *const pthread_condattr_t) -> c_int;
pub fn pthread_cond_wait(cond: *mut pthread_cond_t, mutex: *mut pthread_mutex_t) -> c_int;
pub fn pthread_cond_signal(cond: *mut pthread_cond_t) -> c_int;
pub fn pthread_cond_broadcast(cond: *mut pthread_cond_t) -> c_int;

// Thread-local storage
pub fn pthread_key_create(key: *mut pthread_key_t, destructor: Option<fn(*mut c_void)>) -> c_int;
pub fn pthread_getspecific(key: pthread_key_t) -> *mut c_void;
pub fn pthread_setspecific(key: pthread_key_t, value: *mut c_void) -> c_int;
```

---

## Memory Mapping Syscalls

| Number | Name | Args | Return |
|--------|------|------|--------|
| 90 | sys_mmap | addr, len, prot, flags, fd, offset | addr or -errno |
| 91 | sys_munmap | addr, len | 0 or -errno |
| 92 | sys_mprotect | addr, len, prot | 0 or -errno |
| 93 | sys_mremap | old_addr, old_size, new_size, flags | addr or -errno |
| 94 | sys_msync | addr, len, flags | 0 or -errno |
| 95 | sys_madvise | addr, len, advice | 0 or -errno |

```rust
// mmap flags
const MAP_SHARED: i32 = 0x01;
const MAP_PRIVATE: i32 = 0x02;
const MAP_FIXED: i32 = 0x10;
const MAP_ANONYMOUS: i32 = 0x20;

// mmap protection
const PROT_NONE: i32 = 0x0;
const PROT_READ: i32 = 0x1;
const PROT_WRITE: i32 = 0x2;
const PROT_EXEC: i32 = 0x4;
```

---

## LLVM Porting Steps

```
1. Cross-compile LLVM for EFFLUX
   $ cmake -G Ninja \
       -DCMAKE_SYSTEM_NAME=EFFLUX \
       -DCMAKE_C_COMPILER=efflux-cc \
       -DLLVM_HOST_TRIPLE=x86_64-unknown-efflux \
       -DLLVM_DEFAULT_TARGET_TRIPLE=x86_64-unknown-efflux \
       ../llvm

2. Add EFFLUX target support
   - llvm/lib/Target/X86/X86TargetMachine.cpp
   - Add efflux to Triple.cpp

3. Build minimal configuration
   - X86 target only (or host arch)
   - No debug info
   - Static linking initially

4. Test on EFFLUX
   $ llc --version
   $ clang --version
```

---

## Rust Porting Steps

```
1. Create EFFLUX target spec
   // efflux-x86_64.json
   {
     "llvm-target": "x86_64-unknown-efflux",
     "data-layout": "e-m:e-p270:32:32-...",
     "arch": "x86_64",
     "os": "efflux",
     "env": "",
     "linker": "efflux-ld",
     "executables": true,
     ...
   }

2. Bootstrap rustc
   - Use cross-compiled Stage 0
   - Build Stage 1 on EFFLUX
   - Build Stage 2 for final compiler

3. Build standard library
   - core (no_std)
   - alloc
   - std (with EFFLUX syscalls)

4. Test
   $ rustc --version
   $ cargo --version
   $ cargo build  # Simple project
```

---

## Key Files

```
userspace/toolchain/
├── llvm/
│   ├── patches/           # EFFLUX-specific patches
│   └── build.sh           # Build script
├── rust/
│   ├── src/               # Forked rust source
│   ├── library/std/       # EFFLUX std implementation
│   └── build.sh           # Bootstrap script
└── pthread/
    ├── pthread.c          # pthread implementation
    └── pthread.h          # pthread header

crates/libc-support/efflux-mmap/src/
├── lib.rs
├── anonymous.rs           # Anonymous mappings
├── file.rs                # File-backed mappings
└── vma.rs                 # Virtual memory areas
```

---

## Build Dependencies

| Package | Version | Notes |
|---------|---------|-------|
| LLVM | 17.x | Compiler backend |
| Rust | 1.75+ | Compiler |
| CMake | 3.20+ | LLVM build |
| Ninja | 1.10+ | Build system |
| Python | 3.10+ | LLVM scripts |
| Git | 2.x | Source management |

---

## Exit Criteria

- [ ] LLVM compiles and runs on EFFLUX
- [ ] clang can compile C code
- [ ] rustc compiles and runs on EFFLUX
- [ ] cargo can build projects
- [ ] pthread programs work
- [ ] mmap/munmap functional
- [ ] Kernel compiles on itself
- [ ] Works on all 8 architectures

---

## Test: Self-Compile

```bash
# On EFFLUX:

# Check toolchain
$ rustc --version
rustc 1.75.0 (efflux)

$ cargo --version
cargo 1.75.0

# Clone kernel source
$ git clone /mnt/efflux-source /tmp/efflux
$ cd /tmp/efflux

# Build kernel
$ cargo build --release

# Verify
$ file target/release/efflux-kernel
ELF 64-bit LSB executable, x86-64, ...

# Compare with running kernel
$ md5sum target/release/efflux-kernel /boot/efflux-kernel
abc123... target/release/efflux-kernel
abc123... /boot/efflux-kernel  # Should match!
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 19 of EFFLUX Implementation*
