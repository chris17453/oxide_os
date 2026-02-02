# OXIDE Userspace

All programs that run in userspace on OXIDE OS.

## Directory Structure

| Directory | Contents |
|-----------|----------|
| `system/` | Core boot/session programs: init, getty, login, passwd, servicemgr |
| `shell/` | Oxide Shell (esh) — interactive shell with job control |
| `coreutils/` | 90+ standard CLI utilities (ls, cat, grep, ps, mount, ...) |
| `libs/` | Shared libraries: libc, oxide-std, compression |
| `services/` | System daemons: networkd, sshd, journald, journalctl |
| `network/` | Network client tools: ssh |
| `devtools/` | On-target dev tools: as, ld, ar, make, search, modutils |
| `apps/` | End-user applications: gwbasic |
| `tests/` | Test harnesses: argtest, evtest, syscall-tests |

## How Userspace Binaries Are Built

All userspace programs are:

1. **Compiled for** `x86_64-unknown-none` with custom RUSTFLAGS
2. **Linked with** `userspace.ld` — custom linker script setting entry point and sections
3. **Linked against** `oxide_libc` — OXIDE's custom libc (in `libs/libc/`)
4. **Statically linked** — no dynamic loader, all deps baked in
5. **Release-optimized** — `opt-level = "z"` + LTO for minimal binary size

### Build Commands

```bash
make userspace              # Build all packages (debug)
make userspace-release      # Build all packages (release, for rootfs)
make userspace-pkg PKG=ssh  # Build a single package
```

## Adding a New Program

1. Create a directory under the appropriate subcategory:
   - System plumbing → `system/`
   - Daemon/service → `services/`
   - CLI tool → `coreutils/` (single binary) or `devtools/` (dev-focused)
   - End-user application → `apps/`
   - Test harness → `tests/`

2. Create `Cargo.toml`:
   ```toml
   [package]
   name = "your-program"
   version.workspace = true
   edition.workspace = true

   [dependencies]
   oxide_libc = { path = "../libs/libc" }
   ```

3. Add the package path to the root `Cargo.toml` workspace members list.

4. Add the binary name to the Makefile's userspace package list.

5. Build and test:
   ```bash
   make userspace-pkg PKG=your-program
   make build-full && make run
   ```

## Key Libraries

- **`libs/libc/`** — Custom C-compatible libc with syscall wrappers, stdio,
  string ops, malloc, POSIX threads, signals, sockets, and DNS
- **`libs/oxide-std/`** — Higher-level Rust standard library abstraction
  (fs, io, net, process, thread, collections)
- **`libs/compression/`** — Deflate/tar implementation for gzip and tar utilities
