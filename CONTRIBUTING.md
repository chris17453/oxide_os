# Contributing to OXIDE OS

## Prerequisites

- **Rust nightly** — pinned in `rust-toolchain.toml` (auto-installed by rustup)
- **QEMU** — `qemu-system-x86_64` for running the OS
- **OVMF** — UEFI firmware for QEMU (`edk2-ovmf` on Fedora, `ovmf` on Debian/Ubuntu)
- **ld.lld** — LLVM linker (`lld` package)
- **Standard build tools** — `make`, `cpio`, `mtools`

### Fedora / RHEL

```bash
sudo dnf install qemu-system-x86 edk2-ovmf lld make cpio mtools
```

### Debian / Ubuntu

```bash
sudo apt install qemu-system-x86 ovmf lld make cpio mtools
```

## Building

```bash
make build          # Kernel + bootloader only
make build-full     # Full system with userspace and rootfs
make userspace      # Rebuild all userspace packages
make toolchain      # Build the cross-compiler (needed for C programs)
```

## Running

```bash
make run            # Auto-detects QEMU binary (Fedora vs RHEL)
make run-fedora     # Explicitly use qemu-system-x86_64
```

## Testing

```bash
make test           # Headless QEMU boot test — checks serial log for OXIDE banner
cargo test -p <crate>  # Unit tests for individual crates
make check          # cargo check across workspace
make clippy         # Lint check
make fmt            # Format check
```

## Adding a New Userspace Program

1. Create a directory under the appropriate `userspace/` subcategory
2. Add a `Cargo.toml` with `oxide_libc` as a dependency
3. Add the package to the workspace `members` list in the root `Cargo.toml`
4. Add the binary name to the Makefile's userspace package list
5. Build with `make userspace-pkg PKG=your-package`

See `userspace/README.md` for details on the linker script, target, and RUSTFLAGS.

## Code Style

- Run `cargo fmt --all` and `cargo clippy --all-targets -D warnings`
- Four-space indentation, `snake_case` functions, `CamelCase` types
- Document every `unsafe` block
- Keep modules under ~500 lines — split when they grow

## Commits

- Short, imperative subject line: `Fix readline prompt`, `Add ioctl for ConsoleDevice`
- One logical change per commit
- Include `make build` / `make test` output in PR descriptions

## Debug Output

Debug output is gated via Cargo feature flags — never delete debug macros.
Use `debug_*!` macros from `kernel/src/debug.rs`. Available flags:

```bash
make run RUN_KERNEL_FEATURES="debug-syscall debug-sched"  # Enable specific channels
make run RUN_KERNEL_FEATURES="debug-all"                  # Enable everything
```
