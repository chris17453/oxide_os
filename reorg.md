# OXIDE OS — Reorganization Plan

> Audit of project structure with actionable recommendations to improve
> professionalism, navigability, and contributor onboarding.

---

## 1. The Core Problem: Nobody Knows Where Anything Lives

The biggest issue isn't cosmetic — it's structural. The top-level directory
layout tells a confusing story about what this project is:

```
oxide_os/
├── kernel/      ← ~13 source files. A thin glue layer.
├── crates/      ← 30 DIRECTORIES of actual kernel subsystems (mm, vfs,
│                   drivers, net, sched...) hidden behind a Rust-jargon name
│                   that means nothing architecturally.
├── userspace/   ← 25 packages: init, shell, libc, coreutils, ssh, assembler,
│                   linker, journald, test harnesses — everything dumped flat.
├── apps/        ← ONE program (gwbasic). Lonely top-level dir for one app.
├── external/    ← vim, cpython, musl, zlib source drops that get cross-compiled
│                   into userspace apps. But they're not in apps/ or userspace/.
└── ...
```

**The questions a newcomer can't answer from the directory listing:**

1. "Where's the kernel code?" → You'd look in `kernel/` and find a handful of
   files. The actual kernel — memory management, drivers, filesystems, networking,
   scheduling — is in `crates/`. But nothing about the name `crates/` tells you that.
2. "Where do applications live?" → `apps/`? Only gwbasic. `userspace/`? That's
   90 coreutils + ssh + a linker + test harnesses all flat. `external/`? That's
   vim and cpython source code. Three places, no clarity.
3. "What's `crates/`?" → A Rust developer might guess "workspace member crates."
   Anyone else sees a meaningless directory of 30 subdirectories with no README.

**Comparison with real OS projects:**

| Project | Kernel subsystems | User programs |
|---------|------------------|---------------|
| Linux | Flat at root: `mm/`, `fs/`, `drivers/`, `net/` — each directory IS the subsystem | Out of tree |
| SerenityOS | `Kernel/` with subsystem subdirs inside | `Userland/Applications/`, `Userland/Utilities/`, `Userland/Services/` |
| Redox | `kernel/` (microkernel) | `programs/` with categories |
| **OXIDE** | `kernel/` (thin glue) + `crates/` (actual code, opaque name) | `userspace/` (flat dump) + `apps/` (1 program) + `external/` (3rd-party sources) |

---

## 2. Proposed Top-Level Layout

```
oxide_os/
├── README.md                ← NEW: project landing page
├── CONTRIBUTING.md          ← NEW: build/test/contribute guide
├── LICENSE                  ← NEW: MIT license text
├── CHANGELOG.md             ← NEW: release milestones
├── Makefile
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── .cargo/
├── .gitignore
├── AGENTS.md
├── CLAUDE.md
│
├── kernel/                  ← ALL kernel code lives here now
│   ├── src/                    entry point + glue (current kernel/src/)
│   ├── arch/                   architecture support (was crates/arch/)
│   ├── core/                   os_core, os_log (was crates/core/)
│   ├── mm/                     memory management (was crates/mm/)
│   ├── sched/                  scheduler (was crates/sched/)
│   ├── proc/                   process management (was crates/proc/)
│   ├── exec/                   ELF loader (was crates/exec/)
│   ├── syscall/                syscall dispatch (was crates/syscall/)
│   ├── signal/                 signals (was crates/signal/)
│   ├── smp/                    multiprocessor (was crates/smp/)
│   ├── vfs/                    VFS layer (was crates/vfs/)
│   ├── fs/                     filesystem impls (was crates/fs/)
│   ├── block/                  block device layer (was crates/block/)
│   ├── drivers/                ALL drivers (was crates/drivers/)
│   ├── net/                    networking stack (was crates/net/)
│   ├── tty/                    terminal subsystem (was crates/tty/)
│   ├── terminal/               terminal emulator (was crates/terminal/)
│   ├── input/                  input subsystem (was crates/input/)
│   ├── graphics/               framebuffer (was crates/graphics/)
│   ├── audio/                  audio subsystem (was crates/audio/)
│   ├── usb/                    USB core (was crates/usb/)
│   ├── boot/                   boot protocol (was crates/boot/)
│   ├── module/                 kernel modules (was crates/module/)
│   ├── async/                  epoll, io_uring (was crates/async/)
│   ├── security/               crypto, trust, seccomp, x509 (was crates/security/)
│   ├── container/              namespaces, cgroups (was crates/container/)
│   ├── hypervisor/             vmm, vmx, virtio-emu (was crates/hypervisor/)
│   ├── media/                  media, automount (was crates/media/)
│   ├── compat/                 v86, binfmt, syscall-compat (was crates/compat/)
│   ├── libc-support/           pthread, mmap, dl (was crates/libc-support/)
│   └── linker.ld
│
├── bootloader/              ← unchanged
│
├── userspace/               ← ALL user-facing code, properly categorized
│   ├── system/                 core OS programs (init, getty, login, passwd, servicemgr)
│   ├── shell/                  oxide shell (esh)
│   ├── coreutils/              90 standard utilities
│   ├── libs/                   shared libraries (libc, oxide-std, compression)
│   ├── devtools/               on-target dev tools (as, ld, ar, make, search, modutils)
│   ├── services/               daemons (networkd, sshd, ssh, journald, journalctl)
│   ├── apps/                   end-user applications (gwbasic — moved from top-level apps/)
│   └── tests/                  test harnesses (argtest, evtest, syscall-tests)
│
├── docs/                    ← restructured documentation
├── scripts/                 ← build/test scripts
├── targets/                 ← Rust target specs
├── toolchain/               ← cross-compiler toolchain
├── tools/                   ← host-side dev tools (qemu-mcp)
└── external/                ← third-party source drops for cross-compilation
```

**What this fixes:**

- `kernel/` actually contains the kernel. All of it. `mm/`, `drivers/`, `net/` —
  you can see the OS subsystems right there.
- `crates/` is gone. "Crates" was never a meaningful architectural concept — it
  was a Rust implementation detail leaking into the project layout.
- `apps/` is gone as a top-level directory. gwbasic moves to `userspace/apps/`.
  One program doesn't justify a root directory.
- `userspace/` is organized by role: system plumbing, daemons, dev tools, apps,
  tests. You can find anything in two clicks.
- `external/` stays exactly where it is — it's clearly "third-party sources we
  build from," not "where applications live."

---

## 3. The `crates/` → `kernel/` Migration In Detail

### 3.1 Why this is the right move

The current split is:

```
kernel/src/main.rs          ← calls into...
crates/mm/mm-manager/       ← ...the actual memory manager
crates/sched/sched/         ← ...the actual scheduler
crates/vfs/vfs/             ← ...the actual filesystem
crates/drivers/block/nvme/  ← ...the actual drivers
```

The kernel entry point and its subsystems are in **sibling directories** at
the repo root, connected only by Cargo dependency edges. This is like having
`linux/init/` and then `some_other_directory/mm/` — it fragments the kernel.

After the move:

```
kernel/src/main.rs           ← calls into...
kernel/mm/mm-manager/        ← ...same package, now co-located
kernel/sched/sched/          ← ...obvious relationship
kernel/vfs/vfs/              ← ...everything under one roof
kernel/drivers/block/nvme/   ← ...clear hierarchy
```

Each Rust crate keeps its own `Cargo.toml` and compiles independently — this is
just a directory move, not an architectural change. The workspace `Cargo.toml`
paths update from `crates/mm/mm-core` to `kernel/mm/mm-core`.

### 3.2 What about `ai/` crates?

`kernel/ai/` (hnsw, embed, indexd) stays in the kernel tree. These provide
kernel-level vector search and embedding infrastructure — designed for
in-kernel semantic indexing and inference capabilities. This is intentional
architecture, not misplacement. Add a `kernel/ai/README.md` documenting
the design rationale.

### 3.3 What about `compat/python-sandbox`?

A Python sandbox in the kernel tree is unusual. If this is a userspace container
that sandboxes CPython execution, it belongs in `userspace/`. If it's a
kernel-level execution environment (like eBPF but for Python), it should stay in
`kernel/compat/` but needs a README explaining why.

### 3.4 The `terminal` orphan

`crates/terminal/` sits alone at the top level of `crates/` while `tty/`, `pty/`,
and `vt/` are grouped under `crates/tty/`. It should join its family:

```
kernel/tty/
├── tty/
├── pty/
├── vt/
└── terminal/      ← moved from top-level
```

### 3.5 Single-crate subsystem flattening

Several subsystems have redundant nesting — a category directory containing a
single crate with the same name:

```
crates/signal/signal/    → just kernel/signal/
crates/smp/smp/          → just kernel/smp/
crates/module/module/    → just kernel/module/
crates/input/input/      → just kernel/input/
```

When a subsystem has only one crate, flatten it. If/when a `signal-traits` crate
is needed later, un-flatten at that point.

### 3.6 Migration steps

1. `mkdir kernel/` subdirectories matching `crates/` categories
2. `git mv crates/<subsystem> kernel/<subsystem>` for each
3. Update all `Cargo.toml` workspace member paths
4. Update all `[dependencies]` path references in workspace deps
5. Update `Makefile` if it references `crates/` paths directly
6. Update `.cargo/config.toml` if needed
7. Delete empty `crates/` directory
8. Run `make build` to verify

This is a large `git mv` operation but zero code changes — only `Cargo.toml`
path strings change. The compiler doesn't care about directory names.

---

## 4. Unifying Where Applications Live

### 4.1 The current three-way split

| Directory | Contains | Problem |
|-----------|----------|---------|
| `apps/gwbasic/` | GW-BASIC interpreter | One app gets a top-level dir |
| `userspace/shell/` | Oxide Shell | Is this an "app" or "system"? |
| `userspace/ssh/` | SSH client | Is this an "app" or "service"? |
| `userspace/as/` | Assembler | Dev tool mixed with system plumbing |
| `userspace/init/` | PID 1 | Core system, same level as test harnesses |
| `external/vim/` | Vim source | Compiled into userspace but lives in external/ |
| `external/cpython/` | Python source | Same — compiles to a userspace binary |

A contributor asking "where do I add a new app?" has no clear answer.

### 4.2 Proposed userspace hierarchy

```
userspace/
├── README.md               ← NEW: how userspace binaries are built, how to add one
├── userspace.ld             ← linker script (already exists)
│
├── system/                  ← boots the OS, manages sessions
│   ├── init/                   PID 1
│   ├── getty/                  TTY login manager
│   ├── login/                  login program
│   ├── passwd/                 user/password management
│   └── servicemgr/             service manager daemon
│
├── shell/                   ← the Oxide Shell (already exists)
│
├── coreutils/               ← 90 standard CLI utilities (already exists)
│
├── libs/                    ← shared userspace libraries
│   ├── libc/                   custom libc
│   ├── oxide-std/              oxide standard library
│   └── compression/            compression library
│
├── services/                ← long-running daemons
│   ├── networkd/               network daemon
│   ├── sshd/                   SSH server
│   ├── journald/               journal/syslog daemon
│   └── journalctl/             journal query tool
│
├── network/                 ← network client tools
│   └── ssh/                    SSH client
│
├── devtools/                ← development tools that run ON oxide
│   ├── as/                     assembler
│   ├── ld/                     linker
│   ├── ar/                     archiver
│   ├── make/                   build tool
│   ├── search/                 search utility
│   └── modutils/               kernel module utilities
│
├── apps/                    ← end-user applications
│   └── gwbasic/                GW-BASIC interpreter (moved from top-level apps/)
│
└── tests/                   ← test harnesses (not shipped in production images)
    ├── argtest/
    ├── evtest/
    └── syscall-tests/
```

### 4.3 What about `external/`?

`external/` is **not an application directory** — it's a build-time dependency
store. Vim source lives in `external/vim/`, gets cross-compiled by
`scripts/build-vim.sh`, and the resulting binary lands in the rootfs.

This is fine. But it needs to be documented clearly:

```
external/
├── README.md               ← NEW: explains what this is, how to add a dep
├── VERSIONS.md             ← NEW: version manifest
├── cpython/                   CPython source
├── musl-1.2.5/                MUSL libc source
├── musl-regex/                MUSL regex extraction
├── vim/                       Vim editor source
└── zlib-1.3.1/                zlib source
```

The README should say: "This directory contains third-party source code that gets
cross-compiled for the OXIDE target. These are **not** OXIDE-native programs —
they're ported software. The build scripts in `scripts/build-*.sh` handle
compilation. Built binaries are installed into the rootfs by the Makefile."

### 4.4 Delete `apps/` as a top-level directory

`apps/` exists for a single program. It should not be a peer of `kernel/` and
`userspace/` in the repo root. gwbasic is an application that runs in userspace —
it belongs in `userspace/apps/gwbasic/`.

If OXIDE grows more apps later (a text editor written in Rust, a file manager,
a calculator), they all go in `userspace/apps/`. One top-level directory for all
user-facing programs.

### 4.5 Kill `ssh_old/`

`userspace/ssh_old/` is a deprecated SSH client still listed in `Cargo.toml`.
Git history preserves it. Delete the directory and remove from workspace.

---

## 5. Root Directory Cleanup

### 5.1 Missing: README.md

No `README.md`. The single most important file for any project.

**Action:** Create `README.md` with:
- Project name, one-liner, logo/banner
- Quick-start (`make build-full && make run`)
- Architecture overview (ASCII diagram of kernel ↔ userspace ↔ bootloader)
- Directory map with one-line descriptions
- License badge

### 5.2 Stale / misplaced root files

| File | Issue | Action |
|------|-------|--------|
| `manifesto.md` | One-line file ("A Unix-like OS…") | Fold into `README.md` |
| `package.json` / `package-lock.json` | Node.js dev tooling | Move to `tools/` or `.gitignore` |
| `node_modules/` | NPM deps owned by root, committed | **Must not be committed.** Add to `.gitignore` |
| `.ahoy_creds` | Credentials in repo root | Verify `.gitignore`; if committed, rotate + purge |
| `build/` | Contains `scripts/` + `targets/` dupes | Delete — consolidate into existing dirs |

### 5.3 Missing project files

| File | Status | Action |
|------|--------|--------|
| `LICENSE` | Missing — `Cargo.toml` says MIT but no license text | Create with MIT text |
| `CONTRIBUTING.md` | Missing — `AGENTS.md` is AI-focused, not human-focused | Create for human contributors |
| `CHANGELOG.md` | No release history | Create with retroactive milestones |
| `.github/workflows/` | No CI | Add basic build + clippy + fmt checks |

---

## 6. Documentation Tree (`docs/`)

### 6.1 Current state (problems)

```
docs/
├── DRIVES.md                ← unclear name (it's about boot flow)
├── gwbasic_analysis.md      ← app-specific, underscore in name
├── analv1.md                ← what is "anal v1"? non-descriptive
└── arch/
    ├── MIGRATION_COMPLETE.md   ← 3 docs that should be 1
    ├── MIGRATION_PLAN.md
    ├── MIGRATION_SUMMARY.md
    └── ...5 other files
```

Flat dump. No index. No subsystem documentation. Inconsistent naming.

### 6.2 Proposed structure

```
docs/
├── INDEX.md                         ← table of contents
│
├── architecture/                    ← high-level design
│   ├── overview.md                     system architecture diagram
│   ├── boot-flow.md                    was DRIVES.md
│   ├── boot-protocols.md               UEFI boot protocol
│   ├── userspace.md                    userspace architecture
│   ├── assembly-inventory.md           x86_64 assembly modules
│   └── porting/
│       ├── guide.md                    how to port to new arch
│       ├── mips64-notes.md             SGI/MIPS64 notes
│       └── migration-log.md            merged from 3 migration docs
│
├── subsystems/                      ← kernel subsystem overviews
│   ├── memory.md
│   ├── scheduling.md
│   ├── filesystem.md
│   ├── networking.md
│   ├── drivers.md
│   ├── security.md
│   ├── terminal.md
│   └── containers.md
│
├── development/                     ← contributor docs
│   ├── building.md
│   ├── testing.md                      was TESTING_GUIDE.md
│   ├── debugging.md                    debug feature flags reference
│   └── toolchain.md
│
├── apps/
│   └── gwbasic.md                      was gwbasic_analysis.md
│
└── references/
    └── analysis-v1.md                  was analv1.md
```

### 6.3 Naming convention

- Root project files: `UPPERCASE.md` (`README.md`, `LICENSE`, `CONTRIBUTING.md`)
- Docs and everything else: `lowercase-hyphenated.md`
- Directories: `lowercase-hyphenated`

---

## 7. Cargo Workspace Hygiene

### 7.1 Bugs to fix now

```toml
# Cargo.toml lines 102-103 — duplicate entry
"userspace/passwd",
"userspace/passwd",    ← DELETE THIS LINE
```

```toml
# Deprecated package still in workspace
"userspace/ssh_old",   ← REMOVE (delete directory too)
```

### 7.2 Workspace dependency path updates

After the `crates/` → `kernel/` move, every workspace dependency path changes:

```toml
# Before
os_core = { path = "crates/core/os_core" }
mm-core = { path = "crates/mm/mm-core" }
vfs     = { path = "crates/vfs/vfs" }

# After
os_core = { path = "kernel/core/os_core" }
mm-core = { path = "kernel/mm/mm-core" }
vfs     = { path = "kernel/vfs/vfs" }
```

This is mechanical — find-and-replace `crates/` → `kernel/` in `Cargo.toml`.

### 7.3 Crate naming collisions

Several crates use bare names that collide with Rust concepts or popular crates:

| Name | Risk | Recommendation |
|------|------|----------------|
| `proc` | Rust `proc` keyword, `proc-macro` ecosystem | Rename to `oxide-proc` |
| `signal` | `signal` crate on crates.io | Fine for internal use; prefix if publishing |
| `block` | Common name | Fine for internal use |
| `module` | Common name | Fine for internal use |

Low priority unless you plan to publish to crates.io.

---

## 8. `external/` Cleanup

### 8.1 Version tracking

No manifest. Create `external/VERSIONS.md`:

```markdown
| Library | Version | Source | Purpose |
|---------|---------|--------|---------|
| musl    | 1.2.5   | musl.cc | regex library extraction |
| zlib    | 1.3.1   | zlib.net | compression for cpython/ssh |
| cpython | 3.x     | python.org | Python interpreter |
| vim     | 9.x     | vim.org | text editor |
```

### 8.2 Tarballs committed to git

`musl-1.2.5.tar.gz` and `zlib-1.3.1.tar.gz` are binary blobs in the repo.

**Action:** Add `external/*.tar.gz` to `.gitignore`. Have build scripts
download them or document manual fetch.

### 8.3 Build artifacts committed

`cpython-build/`, `cpython-build-native/` look like build output.

**Action:** Add to `.gitignore`: `external/cpython-build*/`

### 8.4 Add a README

`external/README.md` explaining: this is third-party source for cross-compilation,
not OXIDE-native code. Point to `scripts/build-*.sh` for how they're built.

---

## 9. Build System

### 9.1 Delete `build/` directory

```
build/
├── scripts/    ← duplicates top-level scripts/
└── targets/    ← duplicates top-level targets/
```

Merge anything unique into `scripts/` and `targets/`. Delete `build/`.

### 9.2 Makefile updates after reorg

After the `crates/` → `kernel/` and userspace regrouping, the Makefile needs
path updates. Key areas:

- Userspace package list (USERSPACE_PKGS or equivalent)
- Any `crates/` path references
- initramfs file collection paths

### 9.3 Script naming standardization

```
build-all-libs.sh        ← keep
build-cpython.sh         ← keep
qemu-x86_64.sh           → run-qemu-x86_64.sh
qemu-test.sh             → run-qemu-test.sh
validate-arch.sh         ← keep
validate-arch-simple.sh  → merge into validate-arch.sh --simple
```

---

## 10. `.gitignore` Additions

```gitignore
# Node.js dev tooling
node_modules/
package.json
package-lock.json

# External source tarballs and build artifacts
external/*.tar.gz
external/cpython-build/
external/cpython-build-native/

# Build output
build/
target/

# Credentials
.ahoy_creds
```

---

## 11. What NOT to Change

- **Workspace-per-crate model** — each subsystem as its own Cargo crate is good
  for compile-time isolation and testing. Keep it.
- **Makefile over cargo-make** — familiar, well-structured. Keep it.
- **`targets/` directory** — standard Rust pattern for custom targets.
- **`toolchain/` directory** — well-documented, has README + QUICKSTART + examples.
- **Debug feature flag system** — 30+ gated debug channels. Sophisticated. Keep.
- **`CLAUDE.md` personas** — unique to this project. Don't normalize away.
- **`AGENTS.md`** — complements a future `CONTRIBUTING.md` for AI workflows.
- **`external/` as a concept** — third-party source drops are fine here. Just
  needs a README and version tracking.
- **`bootloader/boot-uefi/` nesting** — room for `boot-bios/` later.

---

## 12. Priority Roadmap

### Phase 1 — Quick wins (no directory moves) ✅ COMPLETE

- [x] Create `README.md` at repo root
- [x] Create `LICENSE` file (MIT)
- [x] Fix duplicate `userspace/passwd` in `Cargo.toml`
- [x] Remove `userspace/ssh_old` from workspace, delete directory
- [x] Update `.gitignore` (node_modules, tarballs, build artifacts, creds)
- [x] Create `external/README.md` and `external/VERSIONS.md`
- [x] Delete `build/` directory
- [x] Fold `manifesto.md` into `README.md`

### Phase 2 — Documentation (no code changes) ✅ COMPLETE

- [x] Restructure `docs/` per Section 6.2
- [x] Create `docs/INDEX.md`
- [x] Create `CONTRIBUTING.md`
- [x] Add `userspace/README.md` (build process, how to add a program)
- [x] Add subsystem documentation (memory, scheduling, filesystem, networking, drivers, security, terminal, containers)
- [x] Merge three migration docs into one
- [x] Rename doc files to lowercase-hyphenated

### Phase 3 — The big move: `crates/` → `kernel/` ✅ COMPLETE

- [x] `git mv` all 30 crate category directories from `crates/` to `kernel/`
- [x] Move `crates/terminal/` into `kernel/tty/terminal/`
- [x] Update all `Cargo.toml` workspace member paths (`crates/` → `kernel/`)
- [x] Update all `[workspace.dependencies]` paths
- [x] Fix kernel/Cargo.toml relative arch paths
- [x] Update AGENTS.md, CLAUDE.md, source comments
- [x] Delete empty `crates/` directory

### Phase 4 — Userspace regrouping ✅ COMPLETE

- [x] Create subdirectories: `system/`, `libs/`, `services/`, `devtools/`, `apps/`, `network/`, `tests/`
- [x] `git mv` packages into their new homes
- [x] Move `apps/gwbasic/` → `userspace/apps/gwbasic/`
- [x] Delete top-level `apps/` directory
- [x] Move test harnesses to `userspace/tests/`
- [x] Update `Cargo.toml` workspace members
- [x] Fix 24 relative dependency paths across 22 Cargo.toml files
- [x] Fix gwbasic `.cargo/config.toml` linker path

### Phase 5 — Build/run script updates ✅ COMPLETE

- [x] Fix Makefile `find userspace/libc/src` → `userspace/libs/libc/src`
- [x] Fix Makefile TLS test path `apps/tls-test.c` → `userspace/tests/tls-test.c`
- [x] Verify `.cargo/config.toml` kernel linker path is correct
- [x] Verify `userspace/userspace.ld` path in RUSTFLAGS is correct

### Future work (not yet started)

- [ ] Set up GitHub Actions CI (build, clippy, fmt)
- [ ] Create `CHANGELOG.md` with retroactive milestones
- [ ] Flatten single-crate subsystem directories (`signal/signal/` → `signal/`)
- [ ] Add crate-level doc comments (`//!`) to all kernel crate `lib.rs` files

---

## 13. Before/After Comparison

### Before (current)

```
$ ls oxide_os/
AGENTS.md    Cargo.lock   Cargo.toml   CLAUDE.md    Makefile
apps/        bootloader/  build/       crates/      docs/
external/    kernel/      manifesto.md node_modules/ package.json
scripts/     target/      targets/     toolchain/   tools/
userspace/
```

15 directories and 8 files at root. `crates/` tells you nothing. `apps/` is
misleading. `build/` is confusing. `node_modules/` shouldn't be here.

### After (proposed)

```
$ ls oxide_os/
AGENTS.md        CHANGELOG.md     CONTRIBUTING.md  LICENSE
README.md        Cargo.lock       Cargo.toml       CLAUDE.md
Makefile         rust-toolchain.toml
bootloader/      docs/            external/        kernel/
scripts/         targets/         toolchain/       tools/
userspace/
```

9 directories and 10 files at root. Every directory name maps to an
architectural concept. A newcomer can orient in seconds:

- `kernel/` — the OS kernel and all subsystems
- `bootloader/` — UEFI boot stage
- `userspace/` — everything that runs in userspace
- `external/` — third-party source for cross-compilation
- `toolchain/` — cross-compiler
- `docs/` — documentation
- `scripts/` — build automation
- `targets/` — Rust target specs
- `tools/` — host development tools

No ambiguity. No jargon. No orphaned directories.

---

*Analysis performed on the OXIDE OS codebase as of commit `3682f55`.*
