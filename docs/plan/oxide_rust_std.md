# Plan: Implement Rust `std` for OXIDE OS

## Context

OXIDE OS has a complete libc (`userspace/libs/libc/`) with 100+ syscall wrappers and an existing `oxide-std` wrapper crate. But userspace programs must use `#![no_std]` + `#![no_main]`. The goal: make real Rust `std` available so programs can use `println!()`, `std::fs::File`, `std::env::args()`, etc.

**Reference**: Motor OS (`target_os = "motor"`) — another custom Rust OS that already ships with std support in upstream nightly. We follow their exact pattern: a `oxide-rt` runtime crate + patched std source with an OXIDE PAL.

## Architecture

```
┌──────────────────────────────────────────┐
│  std programs        │  no_std programs  │  ← userspace
│  (use std::fs, etc.) │  (use libc::*)    │
├──────────────────────┤                   │
│  std (patched PAL)   │                   │
├──────────────────────┤                   │
│  oxide-rt            │  libc             │  ← userspace
│  (raw syscalls)      │  (raw syscalls)   │
├══════════════════ SYSCALL BOUNDARY ══════┤
│  Kernel (unchanged)                      │  ← kernel (ring 0)
└──────────────────────────────────────────┘
```

```
User program (uses std)
    └── std (patched, built from source via -Zbuild-std)
        └── oxide-rt (rustc-dep-of-std, #![no_std], raw syscalls)
            └── inline asm syscall instruction
```

- **Kernel**: unchanged
- **libc**: unchanged, still used by existing no_std programs
- **oxide-rt**: new small crate, duplicates ~200 lines of syscall asm from libc
- **std**: patched copy of Rust's std that calls oxide-rt instead of Linux/Windows
- **Programs**: can choose `no_std + libc` OR `std` (two separate targets)

## Step 1: Create `oxide-rt` Runtime Crate

**Location**: `userspace/libs/oxide-rt/`

A minimal `#![no_std]` crate providing raw kernel interface for std. NO alloc dependency. Uses `rustc-dep-of-std` feature so it compiles as part of std's dep graph.

Extract raw syscall wrappers from `userspace/libs/libc/src/arch/x86_64/syscall.rs` and syscall numbers from `userspace/libs/libc/src/syscall.rs`.

**Modules**:
| File | Purpose |
|------|---------|
| `lib.rs` | Crate root, feature gates, `pub mod` declarations |
| `syscall.rs` | Raw `syscall0`-`syscall6` inline asm (from libc) |
| `nr.rs` | Syscall number constants (from libc) |
| `types.rs` | `Stat`, `Timespec`, `Timeval`, `Dirent` (repr(C)) |
| `alloc.rs` | `mmap`/`munmap` based `GlobalAlloc` impl |
| `args.rs` | argc/argv storage (set by `_start`, read by std) |
| `env.rs` | In-process env var storage (HashMap-free, array-based) |
| `io.rs` | `read(fd, buf)`, `write(fd, buf)`, `close(fd)` |
| `fs.rs` | `open`, `stat`, `lseek`, `mkdir`, `unlink`, `readdir`, etc. |
| `os.rs` | `getcwd`, `chdir`, `getpid`, `exit` |
| `process.rs` | `fork`, `execve`, `waitpid`, `kill`, `abort` |
| `thread.rs` | `nanosleep`, `sched_yield`, `futex_wait`, `futex_wake` |
| `time.rs` | `clock_gettime` for both MONOTONIC and REALTIME |
| `random.rs` | `getrandom` syscall |
| `pipe.rs` | `pipe()` syscall |
| `net.rs` | `socket`, `bind`, `listen`, `accept`, `connect`, `send`, `recv`, `sendto`, `recvfrom`, `setsockopt`, `getsockopt`, `getsockname`, `getpeername`, `shutdown` |
| `start.rs` | `_start` naked fn entry point (parses argc/argv from stack) |
| `error.rs` | Errno-to-io::ErrorKind mapping |
| `libc.rs` | Compatibility shim (type aliases used by `os/fd/raw.rs` and `os/fd/owned.rs`) |

**Cargo.toml**:
```toml
[package]
name = "oxide-rt"
version = "0.1.0"
edition = "2024"

[features]
default = []
rustc-dep-of-std = ["core", "compiler_builtins"]

[dependencies]
core = { version = "1.0.0", optional = true, package = "rustc-std-workspace-core" }
compiler_builtins = { version = "0.1", optional = true }
```

### Why oxide-rt duplicates libc's syscall asm

oxide-rt can't depend on libc because of a circular dependency:
- std needs an allocator → provided by oxide-rt
- if oxide-rt depended on libc → libc has a GlobalAllocator → which comes from alloc → which is part of std
- std depends on itself = 💥

So oxide-rt carries its own copy of the ~200 lines of raw `syscall` inline asm.

## Step 2: Create Userspace Target Spec

**File**: `targets/x86_64-unknown-oxide-user.json`

Key differences from kernel target (`x86_64-unknown-oxide.json`):
- `"code-model": "small"` (not `kernel`)
- `"disable-redzone": false` (userspace can use red zone)
- No `rustc-abi` (default ABI, SSE2 enabled)
- `"pre-link-args"`: userspace linker script + `_start` entry

## Step 3: Copy & Patch Rust std Source

**Location**: `rust-std/library/` (copied from sysroot)

Script `scripts/setup-std-source.sh`:
1. Copy `$(rustc --print sysroot)/lib/rustlib/src/rust/library/` to `rust-std/library/`
2. Apply our patches (add oxide PAL files + edit dispatch `cfg_select!` macros)

### 3a: New PAL Files

| File | Content |
|------|---------|
| `std/src/sys/pal/oxide/mod.rs` | `init()`, `cleanup()`, `abort_internal()`, entry point |
| `std/src/sys/pal/oxide/os.rs` | `getcwd`, `chdir`, `exit`, `getpid`, error types |
| `std/src/sys/pal/oxide/time.rs` | `Instant`, `SystemTime` via `clock_gettime` |
| `std/src/sys/alloc/oxide.rs` | `GlobalAlloc` for `System` via oxide-rt mmap |
| `std/src/sys/args/oxide.rs` | `Args` iterator from saved argc/argv |
| `std/src/sys/env/oxide.rs` | `Env` iterator, `getenv`/`setenv` |
| `std/src/sys/stdio/oxide.rs` | `Stdin`/`Stdout`/`Stderr` wrapping fd 0/1/2 |
| `std/src/sys/random/oxide.rs` | `fill_bytes` via `getrandom` |
| `std/src/sys/fd/oxide.rs` | `FileDesc` wrapping i32 fd |
| `std/src/sys/io/oxide.rs` | `IoSlice`/`IoSliceMut`, `is_terminal` |
| `std/src/sys/io/error/oxide.rs` | errno decode |
| `std/src/sys/fs/oxide.rs` | `File`, `Dir`, `ReadDir`, `OpenOptions`, `FileAttr` |
| `std/src/sys/thread/oxide.rs` | `Thread`, `sleep`, `yield_now` |
| `std/src/sys/process/oxide/` | `Command`, `Process`, `ExitStatus` (fork/exec/wait) |
| `std/src/sys/pipe/oxide.rs` | `Pipe`, `pipe()` |
| `std/src/sys/net/connection/oxide.rs` | `TcpStream`, `TcpListener`, `UdpSocket` via socket/bind/listen/accept/connect/send/recv syscalls |
| `std/src/os/oxide/mod.rs` | OS-specific extensions |
| `std/src/os/oxide/ffi.rs` | `OsStrExt`/`OsStringExt` |

### 3b: Dispatch Patches (27 files)

Every file that has `target_os = "motor"` needs an equivalent `target_os = "oxide"` arm:

| Dispatch File | What to Add |
|---|---|
| `std/Cargo.toml` | `oxide-rt` path dependency with `rustc-dep-of-std` |
| `std/build.rs` | `\|\| target_os == "oxide"` in supported platforms |
| `std/src/sys/pal/mod.rs` | `target_os = "oxide" => { mod oxide; ... }` |
| `std/src/sys/alloc/mod.rs` | oxide dispatch arm |
| `std/src/sys/args/mod.rs` | oxide dispatch arm + `common` gate |
| `std/src/sys/env/mod.rs` | oxide dispatch arm + `common` gate |
| `std/src/sys/stdio/mod.rs` | oxide dispatch arm |
| `std/src/sys/random/mod.rs` | oxide dispatch arm |
| `std/src/sys/fs/mod.rs` | oxide dispatch arm |
| `std/src/sys/fd/mod.rs` | oxide dispatch arm |
| `std/src/sys/thread/mod.rs` | oxide dispatch arm |
| `std/src/sys/process/mod.rs` | oxide dispatch arm |
| `std/src/sys/pipe/mod.rs` | oxide dispatch arm |
| `std/src/sys/io/mod.rs` | oxide dispatch arm |
| `std/src/sys/io/error/mod.rs` | oxide dispatch arm |
| `std/src/sys/sync/mutex/mod.rs` | Add oxide to futex arm |
| `std/src/sys/sync/condvar/mod.rs` | Add oxide to futex arm |
| `std/src/sys/sync/rwlock/mod.rs` | Add oxide to futex arm |
| `std/src/sys/sync/once/mod.rs` | Add oxide to futex arm |
| `std/src/sys/sync/thread_parking/mod.rs` | Add oxide to futex arm |
| `std/src/sys/thread_local/mod.rs` | oxide dispatch (racy mode like motor) |
| `std/src/sys/os_str/mod.rs` | Add oxide to bytes arm (default `_` is fine) |
| `std/src/sys/path/unix.rs` | Add oxide to is_absolute check |
| `std/src/sys/personality/mod.rs` | Add oxide to no-personality arm |
| `std/src/sys/net/connection/mod.rs` | oxide dispatch arm (full socket implementation) |
| `std/src/os/mod.rs` | `pub mod oxide;` + add to fd module gate |
| `std/src/os/fd/raw.rs` | Add oxide alongside motor (RawFd = i32, use oxide_rt::libc) |
| `std/src/os/fd/owned.rs` | Add oxide alongside motor (try_clone_to_owned, dup) |
| `std/src/os/unix/io/mod.rs` | Add oxide to dup2 gate |

## Step 4: Build System Integration

### `mk/config.mk` additions:
```makefile
USERSPACE_STD_TARGET_JSON := targets/x86_64-unknown-oxide-user.json
USERSPACE_STD_OUT := $(TARGET_DIR)/x86_64-unknown-oxide-user/$(PROFILE)
STD_SOURCE := rust-std/library
```

### `mk/userspace.mk` additions:
New target `userspace-std-pkg` for building std-capable packages:
```makefile
userspace-std-pkg:
    RUSTFLAGS="-C linker=$(LINKER) -C relocation-model=static" \
    cargo build --package $(PKG) \
        --target $(USERSPACE_STD_TARGET_JSON) \
        -Zbuild-std=std,panic_abort \
        -Zbuild-std-features=compiler-builtins-mem \
        $(CARGO_USER_FLAGS)
```

### `.cargo/config.toml` addition:
```toml
[target.x86_64-unknown-oxide-user]
linker = "ld.lld"
rustflags = ["-C", "relocation-model=static"]
```

### `Cargo.toml` workspace:
Add `oxide-rt` and test program to workspace members.

## Step 5: Test Program

**File**: `userspace/tests/hello-std/src/main.rs`
```rust
fn main() {
    println!("Hello from OXIDE std!");
    for (i, arg) in std::env::args().enumerate() {
        println!("  arg[{}] = {}", i, arg);
    }
}
```

No `#![no_std]`, no `#![no_main]` — just a normal Rust program.

## Step 6: std Source Override Mechanism

`-Zbuild-std` compiles from `$(rustc --print sysroot)/lib/rustlib/src/rust/library/`. To use our patched source:

**Approach**: Create a custom sysroot with symlinks:
```bash
REAL=$(rustc +nightly --print sysroot)
CUSTOM=target/oxide-sysroot
# Symlink everything except library source
ln -s $REAL/bin $CUSTOM/bin
ln -s $REAL/lib/lib*.so $CUSTOM/lib/
ln -s $REAL/lib/rustlib/x86_64-* $CUSTOM/lib/rustlib/
# Point library source to our patched copy
ln -s $PWD/rust-std/library $CUSTOM/lib/rustlib/src/rust/library
```

Then build with `RUSTFLAGS="--sysroot target/oxide-sysroot"`.

Alternative: `scripts/setup-std-source.sh` creates the sysroot automatically.

## What Does NOT Change

- **Existing `no_std` programs** continue to build with `x86_64-unknown-none` unchanged
- **Kernel** stays on `x86_64-unknown-oxide` (kernel target)
- **Bootloader** stays on `x86_64-unknown-uefi`
- **libc** stays where it is, existing programs keep using it
- **Kernel arch layer** (`kernel/arch/`) is purely ring-0, untouched

## Verification

1. `scripts/setup-std-source.sh` — copies and patches std source
2. `make userspace-std-pkg PKG=hello-std` — compiles hello-std with real `std`
3. Add hello-std to rootfs, boot, run it — prints "Hello from OXIDE std!"
4. `make build` — existing kernel + bootloader still work
5. `make run` — full system boots as before
6. Existing `no_std` programs unaffected

## Risk: std Source Pinning

The patched std source must match the nightly compiler version. Store patches as a script rather than committing the entire library/ (4000+ files). `scripts/setup-std-source.sh` can be re-run after `rustup update nightly`.
