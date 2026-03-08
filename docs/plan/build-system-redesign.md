# Build System Redesign Plan

**Status:** PROPOSED
**Date:** 2026-03-07
**Trigger:** Stale vim binary (compiled against old syscall ABI) caused 2-day debugging session. Root cause: build system has no dependency tracking between kernel/libc changes and staged C packages.

---

## Problems with Current Build System

### P0: Stale Binaries (the vim incident)

The `pkgmgr-vim` target uses "skip if staged" logic:
```makefile
if [ -f "$(PKGMGR_STAGING)/bin/vim" ]; then
    echo "vim already staged, skipping..."
```

After commit `f791d007` remapped syscall numbers to Linux ABI, the staged vim binary still used old numbers. `nanosleep(63)` dispatched as `uname(63)` -- 390-byte struct written to 16-byte buffer -- stack corruption -- GOT zeroed -- crash at RIP=0x0.

**No dependency link exists between libc source changes and C package rebuilds.**

### P1: Missing Dependencies in `create-rootfs`

```
create-rootfs: kernel bootloader archive-kernel pkgmgr-binaries initramfs userspace-std
```

`userspace-release` is only pulled in indirectly through `initramfs` -> `$(INITRAMFS_PREREQ)`. If `RUN_BUILD_USERSPACE=0`, rootfs copies stale binaries from `$(USERSPACE_OUT_RELEASE)/`. The dependency should be explicit.

### P2: `build` Target is Incomplete

```makefile
build: increment-build kernel bootloader
```

This only builds kernel + bootloader. Running `make build && make go` boots stale userspace. The default target should produce a bootable system.

### P3: Debug Features are Ambient, Not Explicit

- `RUN_KERNEL_FEATURES` in config.mk (empty by default, easy to forget)
- `run-debug-*` targets work via target-specific variable overrides
- No way to say "debug build" vs "release build" as a single concept
- `PROFILE=debug` only affects kernel optimization level, not debug output

### P4: No Sysroot Staleness Tracking

The toolchain sysroot (`toolchain/sysroot/lib/liboxide_libc.a`) is built from `userspace/libs/libc`. If libc source changes, the sysroot should rebuild. If the sysroot rebuilds, all C packages (vim, python) must rebuild. Currently: zero tracking.

### P5: `make run` Rebuilds Everything Every Time

`run` -> `create-rootfs` recreates the entire disk image (dd, parted, mkfs, mount, copy, umount) even if nothing changed. This takes ~30-60 seconds with sudo prompts. There's no incremental rootfs update.

---

## Proposed Architecture

### Dependency DAG

```
libc/oxide-rt source (.rs files)
  |
  +---> userspace-release (Rust binaries -- cargo handles incremental)
  |       |
  |       +---> initramfs (cpio archive)
  |
  +---> toolchain sysroot (liboxide_libc.a)
          |
          +---> pkgmgr staged binaries (vim, python)

kernel source (.rs files)
  |
  +---> kernel ELF

bootloader source (.rs files)
  |
  +---> bootloader EFI

kernel + bootloader + initramfs + pkgmgr-binaries + userspace-std
  |
  +---> rootfs disk image
          |
          +---> run (QEMU)
```

### Staleness Sentinels

Use timestamp sentinel files to track what needs rebuilding:

```
target/.sentinel-libc          # touch after libc builds successfully
target/.sentinel-sysroot       # touch after sysroot libc.a is updated
target/.sentinel-userspace     # touch after userspace-release completes
target/.sentinel-pkgmgr        # touch after all staged C packages are current
target/.sentinel-rootfs        # touch after rootfs disk image is created
```

**Staleness rules:**
- If any `.rs` in `userspace/libs/libc/src/` is newer than `.sentinel-libc` -> rebuild userspace + sysroot
- If `.sentinel-libc` is newer than `toolchain/sysroot/lib/liboxide_libc.a` -> rebuild sysroot
- If `.sentinel-sysroot` is newer than `pkgmgr/staging/bin/vim` -> rebuild vim
- If `.sentinel-sysroot` is newer than `pkgmgr/staging/bin/python` -> rebuild python
- If any sentinel is newer than `.sentinel-rootfs` -> rebuild rootfs

### New Target Map

#### Primary Targets (what users type)

| Target | What it does | When to use |
|--------|-------------|-------------|
| `make run` | Build everything + create rootfs + boot QEMU | Daily development |
| `make go` | Boot existing image, no rebuild | Quick reboot after crash |
| `make build` | Build kernel + bootloader + userspace (no rootfs) | CI / quick check |
| `make test` | Build + headless boot test | Pre-commit validation |
| `make test-kernel` | Build + run oxide-test suite | Integration testing |

#### Profile & Debug Control

| Variable | Values | Effect |
|----------|--------|--------|
| `PROFILE` | `debug` (default), `release` | Kernel optimization level |
| `DEBUG` | `all`, `sched`, `fork`, `mouse`, `input`, `lock`, `tty-read`, `syscall-perf` | Kernel debug output features |

**New unified syntax:**
```bash
# Current (works, keep for compatibility)
make run KERNEL_FEATURES=debug-all

# New shorthand
make run DEBUG=all
make run DEBUG=sched,fork
make run PROFILE=release
make run PROFILE=release DEBUG=all    # optimized kernel with debug output
```

The `DEBUG` variable maps to `KERNEL_FEATURES=debug-<value>`:
```makefile
ifdef DEBUG
KERNEL_FEATURES := $(shell echo "$(DEBUG)" | sed 's/,/,debug-/g; s/^/debug-/')
endif
```

#### Convenience Aliases (keep existing, add new)

```makefile
# Keep these (backward compat)
run-debug-all:    DEBUG=all    -> run
run-debug-sched:  DEBUG=sched  -> run
run-debug-fork:   DEBUG=fork   -> run
run-debug-mouse:  DEBUG=mouse  -> run
run-debug-lock:   DEBUG=lock   -> run

# New
run-release:      PROFILE=release -> run
```

#### Build Chain Targets (internal, called by primary targets)

```
kernel              # cargo build kernel (respects PROFILE)
bootloader          # cargo build bootloader (respects PROFILE)
userspace-release   # cargo build all userspace packages (always release)
sysroot-check       # rebuild toolchain sysroot if libc changed
pkgmgr-check        # rebuild staged C packages if sysroot changed
initramfs           # create cpio from userspace-release outputs
create-rootfs       # assemble disk image from all components
```

### Revised `create-rootfs` Dependencies

```makefile
# EXPLICIT dependency chain -- no hidden paths
create-rootfs: kernel bootloader userspace-release sysroot-check pkgmgr-check initramfs userspace-std archive-kernel
```

### New `sysroot-check` Target

```makefile
# — Hexline: Rebuild sysroot libc if source changed.
# This is the firewall between "libc got new syscall numbers" and
# "vim is still calling the old ones."
LIBC_SOURCES := $(shell find userspace/libs/libc/src -name "*.rs" 2>/dev/null)
SYSROOT_LIBC := toolchain/sysroot/lib/liboxide_libc.a

sysroot-check: toolchain
    @if [ -f "$(SYSROOT_LIBC)" ]; then \
        STALE=$$(find userspace/libs/libc/src -name "*.rs" -newer "$(SYSROOT_LIBC)" 2>/dev/null | head -1); \
        if [ -n "$$STALE" ]; then \
            echo "  libc source changed — rebuilding sysroot..."; \
            $(MAKE) toolchain; \
        fi; \
    else \
        echo "  sysroot libc missing — building toolchain..."; \
        $(MAKE) toolchain; \
    fi
```

### New `pkgmgr-check` Target

```makefile
# — Hexline: Rebuild staged C packages if sysroot is newer.
# The sentinel that would have saved us two days on the vim incident.
pkgmgr-check: sysroot-check
    @NEED_REBUILD=0; \
    if [ -f "$(SYSROOT_LIBC)" ]; then \
        for bin in $(PKGMGR_STAGING)/bin/*; do \
            if [ -f "$$bin" ] && [ "$(SYSROOT_LIBC)" -nt "$$bin" ]; then \
                echo "  $$bin is older than sysroot — marking for rebuild"; \
                NEED_REBUILD=1; \
            fi; \
        done; \
    fi; \
    if [ "$$NEED_REBUILD" = "1" ]; then \
        echo "  Cleaning stale staged binaries..."; \
        rm -rf $(PKGMGR_STAGING)/bin $(PKGMGR_STAGING)/lib $(PKGMGR_STAGING)/share; \
    fi
```

This means `pkgmgr-vim` and `pkgmgr-python`'s "skip if exists" logic STILL works -- but `pkgmgr-check` removes the stale binaries BEFORE those targets run, forcing a rebuild.

### Incremental Rootfs (P5 fix -- optional/future)

Instead of recreating the entire disk image every time:

```makefile
# Quick-update: mount existing image, rsync changed files, unmount
update-rootfs: userspace-release pkgmgr-check
    @if [ ! -f $(ROOTFS_IMAGE) ]; then \
        $(MAKE) create-rootfs; \
    else \
        echo "Updating existing rootfs..."; \
        # mount, rsync changed binaries, unmount \
        ...
    fi
```

**Deferred.** The full rebuild is ~30s and correct. Incremental is faster but error-prone. Implement only if build times become a real bottleneck.

---

## Implementation Phases

### Phase 1: Fix Critical Dependencies (do now)

1. Add `userspace-release` as explicit dependency of `create-rootfs`
2. Add `sysroot-check` target
3. Add `pkgmgr-check` target
4. Wire: `create-rootfs` -> `sysroot-check` -> `pkgmgr-check`
5. Existing `userspace-release` already has libc change detection -- verify it works

**Result:** No more stale binaries. If you change a syscall number in libc, vim and python get rebuilt automatically.

### Phase 2: Unify Debug/Profile System (do now)

1. Add `DEBUG=` variable support in `mk/config.mk`
2. Map `DEBUG=all` -> `KERNEL_FEATURES=debug-all`
3. Add `run-release` target
4. Keep all existing `run-debug-*` targets for backward compatibility
5. Update `mk/help.mk` with new options

**Result:** `make run DEBUG=all` instead of `make run KERNEL_FEATURES=debug-all`.

### Phase 3: Fix `build` Target (do now)

```makefile
# Old
build: increment-build kernel bootloader

# New
build: increment-build kernel bootloader userspace-release sysroot-check pkgmgr-check initramfs userspace-std
```

**Result:** `make build` produces everything needed for a bootable system. `make build && make go` always works with fresh binaries.

### Phase 4: Documentation & Help (do now)

Update `mk/help.mk` to show:
- The dependency chain clearly
- DEBUG= shorthand
- PROFILE= options
- Which targets trigger rebuilds of what

---

## New Workflow Cheatsheet

```bash
# Daily development (most common)
make run                           # build everything, boot
make run DEBUG=all                 # same, with full debug output
make go                            # reboot without rebuild

# After changing libc/syscall numbers
make run                           # sysroot-check + pkgmgr-check auto-rebuild vim/python

# Release testing
make run PROFILE=release           # optimized kernel, no debug
make run PROFILE=release DEBUG=all # optimized but with debug output

# Quick kernel-only iteration
make kernel && make go             # rebuild kernel, boot existing rootfs
# NOTE: userspace is stale, but fine for kernel-only changes

# Force rebuild staged packages
make pkgmgr-rebuild-vim            # explicit single package rebuild
make clean-pkgmgr && make run     # nuke all staged, rebuild everything

# Full clean slate
make clean && make run             # cargo clean + full rebuild

# Testing
make test                          # headless boot test
make test-kernel                   # integration test suite

# CI
make build                         # everything except QEMU launch
make test                          # headless validation
```

---

## Files to Modify

| File | Changes |
|------|---------|
| `mk/config.mk` | Add `DEBUG` variable, map to `KERNEL_FEATURES` |
| `mk/rootfs.mk` | Add `userspace-release` to `create-rootfs` deps |
| `mk/toolchain.mk` | Add `sysroot-check` and `pkgmgr-check` targets |
| `mk/qemu.mk` | Add `run-release` target |
| `mk/help.mk` | Update help text with new workflow |
| `Makefile` | Update `build` target deps |

**Total: ~100 lines changed across 6 files. No new files needed.**

---

## Risk Assessment

- **Low risk:** All changes are additive. Existing targets keep working.
- **sysroot-check adds ~2s** to builds (find command), negligible.
- **pkgmgr-check adds ~1s** (stat comparisons), negligible.
- **Forced C package rebuilds** when sysroot changes: vim takes ~5min, python ~10min. This is correct -- you WANT this rebuild. The alternative is 2 days debugging a mystery crash.
- **`build` target gets slower** because it now builds userspace. But `build` should produce a bootable system, not half of one.
