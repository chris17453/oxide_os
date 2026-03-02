# Rust `std` for OXIDE OS — Implementation Results

## Status: COMPILING

`hello-std` builds successfully with full Rust standard library support via `-Zbuild-std`.

```
$ make userspace-std-pkg PKG=hello-std
   Compiling oxide-rt v0.1.0
   Compiling std v0.0.0 (rust-std/library/std)
   Compiling hello-std v0.1.0
    Finished `dev` profile target(s) in 1.89s
```

**Binary**: `target/x86_64-unknown-oxide-user/debug/hello-std`
- Format: ELF 64-bit LSB executable, x86-64, statically linked
- Size: 6.6MB (debug, unstripped)
- Kernel build (`make build`): No regressions

---

## What Was Built

### Phase 1: `oxide-rt` Runtime Crate
**Path**: `userspace/libs/oxide-rt/`

A `#![no_std]` crate with `rustc-dep-of-std` feature. Provides raw syscall wrappers that std's PAL calls into.

| Module | Purpose |
|--------|---------|
| `syscall.rs` | Raw x86_64 `syscall` instruction wrappers (syscall0–syscall6) |
| `nr.rs` | Syscall numbers (EXIT=0 through GETRANDOM=318) |
| `types.rs` | ABI structs: `Stat`, `Timespec`, `Timeval`, `Dirent`, `SockAddrIn`, `UtsName`, `PollFd`, `SigAction`, `Winsize`, etc. + constant submodules |
| `alloc.rs` | Two-tier bump allocator: 256KB BSS bootstrap + 2MB mmap arenas |
| `args.rs` | Static argc/argv storage via atomics, set by `_start` |
| `env.rs` | Array-backed env vars (MAX_ENVS=128), no HashMap |
| `io.rs` | `read`, `write`, `close`, `dup`, `dup2`, `ioctl`, `fcntl`, `fsync` |
| `fs.rs` | `open`, `stat`, `fstat`, `lstat`, `lseek`, `mkdir`, `rmdir`, `unlink`, `rename`, `getdents`, `readlink`, `symlink`, `link`, `ftruncate`, `chmod`, `fchmod`, `chown`, `fchown` |
| `os.rs` | `getcwd`, `chdir`, `getpid`, `getppid`, `getuid`, `getgid`, `exit`, `exit_group`, `uname`, `setsid`, `setpgid`, `gettid` |
| `process.rs` | `fork`, `execve`, `waitpid`, `kill`, `abort` + wait status macros |
| `thread.rs` | `nanosleep`, `sched_yield`, `clone`, `gettid`, `set_tid_address` |
| `time.rs` | `clock_gettime` (MONOTONIC + REALTIME) |
| `random.rs` | `getrandom` syscall wrapper |
| `pipe.rs` | `pipe`, `pipe2` |
| `net.rs` | `socket`, `bind`, `listen`, `accept`, `connect`, `send`, `recv`, `setsockopt`, `getsockopt`, `shutdown` |
| `signal.rs` | `sigaction`, `sigprocmask`, `kill`, `__oxide_sigreturn` trampoline |
| `start.rs` | Naked `_start` entry point — reads argc/argv/envp from stack, calls std's `lang_start` |
| `error.rs` | POSIX errno constants (EPERM=1 through ENOTRECOVERABLE=131) |
| `libc_compat.rs` | C type aliases (`c_int`, `c_char`, `RawFd`, etc.) + fd constants |
| `futex.rs` | `futex_wait`, `futex_wait_timeout`, `futex_wake`, `futex_wake_all` |
| `poll.rs` | `poll()` syscall wrapper |

### Phase 2: Userspace Target Spec
**Path**: `targets/x86_64-unknown-oxide-user.json`

| Field | Value | Notes |
|-------|-------|-------|
| `os` | `"oxide"` | Enables `cfg(target_os = "oxide")` |
| `code-model` | `"small"` | Userspace (kernel uses `"kernel"`) |
| `disable-redzone` | `false` | Userspace can use red zone |
| `has-thread-local` | `true` | Native ELF TLS support |
| `panic-strategy` | `"abort"` | No unwinding |
| `singlethread` | `false` | Multi-threaded support |
| `pre-link-args` | `userspace.ld`, `-e_start` | Userspace linker script + entry |

### Phase 3: Setup Script
**Path**: `scripts/setup-std-source.sh`

Copies nightly std source to `rust-std/library/`, creates custom sysroot at `target/oxide-sysroot/` with symlinks. Creates all OXIDE PAL files via heredocs and applies dispatch patches.

> **Note**: The sed-based dispatch patches in this script fail on multiline `cfg_select!` blocks. The patches were applied manually via direct file edits. Script needs updating for future runs.

### Phase 4: Patched `std` Source

#### New PAL Files (in `rust-std/library/std/src/`)

| File | What it does |
|------|-------------|
| `sys/pal/oxide/mod.rs` | PAL root: `init()`, `cleanup()`, `abort_internal()`, `cvt()`, `map_oxide_error()` |
| `sys/pal/oxide/os.rs` | `getcwd`, `chdir`, `current_exe`, `temp_dir`, `home_dir`, `exit`, `getpid` |
| `sys/pal/oxide/time.rs` | `Instant` (CLOCK_MONOTONIC), `SystemTime` (CLOCK_REALTIME) with MIN/MAX |
| `sys/pal/oxide/futex.rs` | Futex wrapper: adapts oxide_rt's raw API to std's `futex_wait(addr, expected, Option<Duration>) -> bool` / `futex_wake(addr) -> bool` signature |
| `sys/alloc/oxide.rs` | `GlobalAlloc for System` via mmap/munmap |
| `sys/args/oxide.rs` | `args() -> Args` from saved argc/argv |
| `sys/env/oxide.rs` | `env()`, `getenv()`, `setenv()`, `unsetenv()` via oxide_rt |
| `sys/stdio/oxide.rs` | `Stdin`/`Stdout`/`Stderr` wrapping fd 0/1/2 |
| `sys/random/oxide.rs` | `fill_bytes()` via getrandom syscall |
| `sys/fd/oxide.rs` | `FileDesc(OwnedFd)` with read/write/dup/set_nonblocking |
| `sys/fs/oxide.rs` | Full filesystem: `File`, `ReadDir`, `DirEntry`, `OpenOptions`, `FileAttr` via POSIX syscalls |
| `sys/thread/oxide.rs` | `Thread` stub (returns UNSUPPORTED), `yield_now`, `sleep` |
| `sys/process/oxide/mod.rs` | `Command`, `Process`, `ExitStatus` via fork/exec/waitpid |
| `sys/pipe/oxide.rs` | `pipe()` via oxide_rt |
| `sys/net/connection/oxide.rs` | Stubbed `TcpStream`/`TcpListener`/`UdpSocket` (UNSUPPORTED_PLATFORM) |
| `sys/io/is_terminal/oxide.rs` | `is_terminal()` via ioctl TIOCGWINSZ |
| `sys/io/error/oxide.rs` | `errno()`, `is_interrupted()`, `decode_error_kind()`, `error_string()` — POSIX errno mapping |
| `os/oxide/mod.rs` | OS-specific extensions module |
| `os/oxide/ffi.rs` | `OsStrExt`/`OsStringExt` for byte-oriented paths |

#### Dispatch Patches (~25 files modified)

Added `target_os = "oxide"` arms to `cfg_select!` macros in:

- `sys/pal/mod.rs`, `sys/alloc/mod.rs`, `sys/args/mod.rs`, `sys/env/mod.rs`
- `sys/stdio/mod.rs`, `sys/random/mod.rs`, `sys/fd/mod.rs`, `sys/fs/mod.rs`
- `sys/thread/mod.rs`, `sys/process/mod.rs`, `sys/pipe/mod.rs`
- `sys/io/mod.rs` (is_terminal), `sys/io/error/mod.rs`
- `sys/net/connection/mod.rs`
- `sys/sync/{mutex,condvar,rwlock,once,thread_parking}/mod.rs` — added oxide to futex `any()` lists
- `sys/personality/mod.rs` — added oxide to aborting stub
- `sys/thread_local/mod.rs` — oxide uses native ELF TLS (guard section)
- `sys/os_str/mod.rs` — oxide uses bytes backend
- `os/mod.rs` — added `pub mod oxide;` + oxide in fd cfg
- `os/fd/raw.rs` — oxide fd constants via inline module, `OwnedFd` import
- `os/fd/owned.rs` — oxide `try_clone_to_owned` via `oxide_rt::io::dup`, oxide `close` in Drop

### Phase 5: Build System

| File | Change |
|------|--------|
| `Cargo.toml` | Added `userspace/libs/oxide-rt` and `userspace/tests/hello-std` to workspace |
| `.cargo/config.toml` | Added `[target.x86_64-unknown-oxide-user]` section |
| `mk/config.mk` | Added `USERSPACE_STD_TARGET_JSON`, `OXIDE_SYSROOT` vars |
| `mk/userspace.mk` | Added `setup-std-source` and `userspace-std-pkg` targets |
| `mk/rootfs.mk` | Added hello-std binary copy to root partition |

### Phase 6: Test Program
**Path**: `userspace/tests/hello-std/src/main.rs`

```rust
// No #![no_std], no #![no_main] — just normal Rust!
fn main() {
    println!("Hello from OXIDE std!");
    for (i, arg) in std::env::args().enumerate() {
        println!("  arg[{}] = {}", i, arg);
    }
    if let Ok(entries) = std::fs::read_dir("/") {
        println!("Root directory:");
        for entry in entries {
            if let Ok(e) = entry {
                println!("  {}", e.file_name().to_string_lossy());
            }
        }
    }
}
```

---

## Issues Encountered & Resolved

### 1. `-Zbuild-std` ignores `--sysroot` in RUSTFLAGS
**Symptom**: Cargo compiled std from the nightly toolchain source, not our patched copy.
**Root cause**: `-Zbuild-std` finds source via `rustc --print sysroot`, not the RUSTFLAGS `--sysroot`.
**Fix**: Use `__CARGO_TESTS_ONLY_SRC_ROOT=/path/to/rust-std/library` env var. Cargo checks this first.

### 2. `compiler_builtins` native library conflict
**Symptom**: `package compiler_builtins links to the native library compiler-rt, but it conflicts with a previous package`
**Root cause**: oxide-rt declared `compiler_builtins = { version = "0.1" }` which pulled from crates.io, conflicting with std's local path dep at v0.1.160.
**Fix**: Removed `compiler_builtins` from oxide-rt entirely. The `rustc-dep-of-std` feature only needs `core`.

### 3. Malformed `cfg_select!` arms
**Symptom**: `expected '=>', found ','` in cfg_select macros.
**Root cause**: `target_os = "motor", target_os = "oxide" =>` is invalid — cfg_select expects a single predicate.
**Fix**: Wrap in `any()`: `any(target_os = "motor", target_os = "oxide") =>`. Or better: separate arms for separate modules.

### 4. Shared motor/oxide dispatch using `moto_rt`
**Symptom**: `unresolved module or unlinked crate moto_rt` when dispatching oxide to motor's module.
**Root cause**: Args, env, and is_terminal dispatched `any(motor, oxide)` to `mod motor` which imports `moto_rt`.
**Fix**: Separate dispatch arms — oxide gets its own module using `oxide_rt`.

### 5. Futex API signature mismatch
**Symptom**: `this function takes 2 arguments but 3 arguments were supplied`
**Root cause**: std expects `futex_wait(addr, expected, Option<Duration>) -> bool` and `futex_wake(addr) -> bool`. oxide-rt had `futex_wait(addr, expected) -> i32` and `futex_wake(addr, count) -> i32`.
**Fix**: Created `sys/pal/oxide/futex.rs` wrapper that converts Duration to Timespec and adapts return types.

### 6. `libc` crate shadows local module alias
**Symptom**: `cannot find value STDIN_FILENO in module libc`
**Root cause**: std depends on the `libc` crate (crates.io) which always exists in the namespace. `use oxide_rt::libc_compat as libc;` is shadowed by the external crate.
**Fix**: Inline `mod oxide_fd_consts { pub const STDIN_FILENO: i32 = 0; ... }` aliased as `libc`.

### 7. Private `SystemTime` fields
**Symptom**: `fields secs and nanos of struct SystemTime are private`
**Root cause**: fs/oxide.rs constructs SystemTime directly but fields were private.
**Fix**: Made fields `pub(crate)`.

### 8. Missing `PartialEq`/`Eq` on `Stat`
**Symptom**: `binary operation == cannot be applied to type Stat`
**Root cause**: `FileAttr` derives `PartialEq, Eq` but contains `oxide_rt::types::Stat` which lacked these derives.
**Fix**: Added `#[derive(PartialEq, Eq)]` to `Stat` in oxide-rt.

### 9. Setup script sed patterns fail on multiline blocks
**Symptom**: Dispatch patches not applied after running `scripts/setup-std-source.sh`.
**Root cause**: sed patterns assumed single-line `cfg_select!` entries but actual source uses multiline blocks.
**Fix**: Applied patches manually via direct file edits. Script needs updating for idempotent re-runs.

---

## Remaining Work

### Not Yet Tested at Runtime
- hello-std has not been booted on QEMU yet
- `oxide_rt::start::_start` needs to properly call `std::rt::lang_start`
- The allocator, args, env, and fs implementations need runtime validation

### Stubbed/Incomplete Modules
- **Threads** (`sys/thread/oxide.rs`): `Thread::new` returns UNSUPPORTED. Needs `clone()` implementation.
- **Networking** (`sys/net/connection/oxide.rs`): All connect/bind/listen return UNSUPPORTED_PLATFORM. Syscall wrappers exist in oxide-rt but PAL integration is stubbed.
- **File locking**: `lock`, `lock_shared`, `try_lock`, `try_lock_shared` all return unsupported.
- **File times**: `set_times` returns unsupported.
- **Process**: fork/exec/waitpid are implemented but untested.

### Build System Polish
- `scripts/setup-std-source.sh` sed patterns need fixing for multiline `cfg_select!` blocks
- Consider adding `hello-std` to the standard `make build` or `make create-rootfs` flow
- Release build + stripping would reduce binary from 6.6MB to ~500KB–1MB

---

## Architecture Diagram

```
hello-std (normal Rust, fn main())
    |
    v
std (patched, -Zbuild-std)
    |
    +-- sys/pal/oxide/    (PAL: init, os, time, futex)
    +-- sys/alloc/oxide   (GlobalAlloc via mmap)
    +-- sys/args/oxide    (argc/argv from _start)
    +-- sys/env/oxide     (array-backed env)
    +-- sys/stdio/oxide   (fd 0/1/2)
    +-- sys/fs/oxide      (POSIX file ops)
    +-- sys/fd/oxide      (file descriptor wrapper)
    +-- sys/process/oxide (fork/exec/waitpid)
    +-- sys/pipe/oxide    (pipe syscall)
    +-- sys/random/oxide  (getrandom syscall)
    +-- sys/io/error/oxide (errno mapping)
    +-- os/oxide/         (OsStr byte extensions)
    |
    v
oxide-rt (rustc-dep-of-std, #![no_std])
    |
    +-- syscall.rs  (raw syscall instruction)
    +-- nr.rs       (syscall numbers)
    +-- types.rs    (ABI structs)
    +-- alloc.rs    (mmap-based bump allocator)
    +-- args/env/io/fs/os/process/thread/time/...
    |
    v
OXIDE kernel (syscall interface)
```

## Build Command Reference

```bash
# Build hello-std with full Rust std
make userspace-std-pkg PKG=hello-std

# Build any std-enabled package
make userspace-std-pkg PKG=<package-name>

# Setup std source (first time only, or after rustup update)
./scripts/setup-std-source.sh

# Verify kernel still builds
make build
```
