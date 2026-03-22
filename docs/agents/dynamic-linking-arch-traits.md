# Dynamic Linking with Arch-Abstracted Traits

## Status: WORKING END-TO-END

Dynamically-linked C programs run through the full chain:
kernel exec → PT_INTERP → ld-oxide.so.1 → DT_NEEDED → libc.so loaded → main exe runs.

## Architecture

### Kernel Side

**4 Arch Traits** (`kernel/arch/arch-traits/src/lib.rs`):
- `ExecConfig` — USER_STACK_TOP, MMAP_BASE_DEFAULT, TLS_BASE, USER_ADDR_LIMIT, ELF_MACHINE_EXEC
- `TlsLayout` — TLS Variant I/II, TCB_SIZE, thread_pointer(), tls_data_offset()
- `ProcessContextOps` — new_user_context(entry, sp, tls_base), get/set IP/SP/TLS
- `ElfRelocation` — apply_relocation(), is_none/relative/got_reloc()

**x86_64 implementations** in `kernel/arch/arch-x86_64/src/exec_config.rs`.
**Kernel wrappers** in `kernel/src/arch.rs`.

**ELF parser** (`kernel/exec/elf/src/lib.rs`):
- Accepts ET_DYN (PIE/.so) alongside ET_EXEC
- Parses PT_INTERP → InterpInfo (interpreter path)
- Parses PT_DYNAMIC → DynamicInfo, PT_PHDR → PhdrInfo
- AuxEntry/AuxType for auxiliary vector on stack

**exec.rs** (`kernel/proc/proc/src/exec.rs`):
- Uses arch traits for layout constants (no hardcoded x86_64 values)
- Builds auxiliary vector: AT_PAGESZ, AT_RANDOM, AT_ENTRY (always); AT_PHDR, AT_PHENT, AT_PHNUM, AT_BASE (for dynamic binaries)
- `do_exec()` accepts `interp_data: Option<&[u8]>` — interpreter loaded at its linked address
- ProcessContext created via ProcessContextOps trait dispatch

**kernel_exec** (`kernel/src/process.rs`):
- Scans program headers for PT_INTERP during initial phdr scan
- If found, reads interpreter path from ELF, looks up in VFS, reads entire file
- Passes interpreter data to do_exec()

**dl/reloc.rs** (`kernel/libc-support/dl/src/reloc.rs`):
- Relocation stores raw r_type_raw: u32, dispatches through registered callback
- Built-in x86_64 fallback for backward compatibility

### Userspace Side

**ld-oxide.so.1** (`userspace/system/ld-oxide/`):
- Self-contained: own _start (naked asm), syscall wrappers, panic handler. NO libc dependency.
- Linked at 0x200000 via `userspace/ld-oxide.ld` (avoids conflict with main exe at 0x400000)
- Startup: parse auxv → find PT_DYNAMIC → walk DT_NEEDED → open/read/mmap each .so → apply RELATIVE relocations → jump to AT_ENTRY
- Library search paths: /usr/lib/, /lib/
- **Critical**: DynamicInfo.relocate(base_offset) must be called after parse_dynamic for shared libs — raw vaddrs in .dynamic need base adjustment

**Shared Libraries** (`toolchain/sysroot/lib/*.so`):
- ALL sysroot .a archives automatically converted to .so during `make toolchain`
- Built by extracting PIC .o files from .a and relinking with `ld.lld --shared`
- Installed to /usr/lib/ on rootfs

| Library | Size | Source |
|---------|------|--------|
| libc.so | 904K | Our Rust libc (oxide-libc) |
| libncursesw.so | 437K | Fedora ncurses 6.5 SRPM |
| libreadline.so | 306K | Fedora readline 8.2 SRPM |
| libexpat.so | 178K | Fedora expat 2.7 SRPM |
| libpthread.so | 549K | Our Rust pthread |
| libformw.so | 66K | Fedora ncurses (forms) |
| libhistory.so | 32K | Fedora readline (history) |
| libmenuw.so | 27K | Fedora ncurses (menus) |
| libpanelw.so | 9.6K | Fedora ncurses (panels) |

**dyntest** (`userspace/system/dyntest/`):
- Rust binary with .interp section, no DT_NEEDED. Tests PT_INTERP → ld-oxide handoff only.
- Uses `userspace/userspace-dynamic.ld` linker script with PHDRS for PT_INTERP

**dynlink-test** (`userspace/tests/dynlink-test.c`):
- C program linked with `oxide-cc -dynamic` against libc.so
- Has PT_INTERP + PT_DYNAMIC with DT_NEEDED libc.so
- Uses direct syscalls internally (doesn't depend on libc function resolution yet)
- Tests the FULL chain: kernel → ld-oxide → libc.so loading → main exe

**oxide-cc** (`toolchain/bin/oxide-cc`):
- New `-dynamic` flag: uses ld-oxide.so.1 as interpreter, links against libc.so
- `--image-base=0x400000` to avoid collision with ld-oxide at 0x200000
- Default remains static linking

## Key Rules

1. **ld-oxide at 0x200000, executables at 0x400000** — no overlap
2. **DynamicInfo.relocate(base_offset) is MANDATORY** after parse_dynamic for loaded .so files — raw vaddrs cause SIGSEGV
3. **OXIDE sys_open takes (path_ptr, path_len, flags, mode)** — NOT null-terminated like Linux
4. **Shared libs loaded at 0x30000000+** by ld-oxide — below mmap region, above executables
5. **No arch-specific constants in proc crate** — all go through arch-traits

## Files

| Component | File |
|-----------|------|
| Arch traits | `kernel/arch/arch-traits/src/lib.rs` |
| x86_64 impl | `kernel/arch/arch-x86_64/src/exec_config.rs` |
| Kernel wrappers | `kernel/src/arch.rs` |
| ELF parser | `kernel/exec/elf/src/lib.rs` |
| exec | `kernel/proc/proc/src/exec.rs` |
| kernel_exec | `kernel/src/process.rs` |
| dl/reloc | `kernel/libc-support/dl/src/reloc.rs` |
| ld-oxide | `userspace/system/ld-oxide/src/main.rs` |
| ld-oxide syscalls | `userspace/system/ld-oxide/src/sys.rs` |
| ld-oxide ELF | `userspace/system/ld-oxide/src/elf.rs` |
| ld-oxide reloc | `userspace/system/ld-oxide/src/reloc.rs` |
| dyntest | `userspace/system/dyntest/src/main.rs` |
| dynlink-test | `userspace/tests/dynlink-test.c` |
| ld-oxide linker script | `userspace/ld-oxide.ld` |
| dynamic linker script | `userspace/userspace-dynamic.ld` |
| oxide-cc wrapper | `toolchain/bin/oxide-cc` |
| shared libc build | `mk/toolchain.mk` |

## What Works (verified on QEMU)

- [x] Kernel PT_INTERP detection and interpreter loading from VFS
- [x] ld-oxide parses aux vector, finds PT_DYNAMIC, walks DT_NEEDED
- [x] ld-oxide opens/reads/mmaps shared libraries from /usr/lib/
- [x] RELATIVE relocations applied (B+A)
- [x] **GLOB_DAT/JUMP_SLOT symbol resolution via ELF hash** — cross-library lookup works
- [x] Main executable's PLT/GOT resolved against loaded libraries
- [x] DT_INIT / DT_INIT_ARRAY execution for loaded libraries
- [x] `oxide-cc -dynamic` compiles C programs with PIC and links against libc.so
- [x] `dynlink-test` calls `strcmp()` from libc.so through PLT/GOT — **DYNLINK_PASS**
- [x] `dynlink-ncurses-test` loads libc.so + libncursesw.so, resolves strcmp + initscr — **NCURSES_DYNLINK_PASS**
- [x] Recursive DT_NEEDED: breadth-first loading with dedup (ncursesw.so → libc.so)
- [x] libc.so exports strlen/memcpy/memset/memmove/memcmp/wcwidth/tsearch/tfind/tdelete
- [x] All static binaries unaffected (zero regressions)

## FIXED: p_filesz vs p_memsz Bug (was blocking vim/python)

The crash was caused by `load_library_segments` reading p_filesz (ELF phdr offset 32-40) instead of p_memsz (offset 40-48) for the mmap size calculation. BSS segments (memsz >> filesz) were not included in the mapped range, causing SIGSEGV on any write to libc globals (allocator, locks, stdio).

Fix: `elf_data[off+32..off+40]` → `elf_data[off+40..off+48]` in two places.

## Verified Working

- [x] **Vim 9.2.45** dynamically linked (libc.so + libncursesw.so) — full ncurses UI, :q exits cleanly
- [x] **Python 3.13.12** dynamically linked (libc.so + libpthread.so) — `python -V` prints version
- [x] **dynlink-suite** — 18/18 tests pass (strcmp, strlen, memset, memcpy, malloc, free, setenv, getenv, function pointers, ncurses initscr resolved, 3 libraries loaded)
- [x] **mmap-write-test** — 5/5 kernel mmap tests pass

## Test Suite

**dynlink-suite** (userspace/tests/dynlink-suite.c) — 18 tests covering:
- Group 1: Pure function calls (strcmp, strlen, memset, memcpy)
- Group 2: Global state writes (malloc, free, multiple mallocs)
- Group 3: Function pointers (strcmp/malloc/initscr via GOT)
- Group 4: Nested libc calls (setenv→malloc, getenv→strcmp)
- Loads 3 libraries: libc.so + libncursesw.so + libreadline.so

**mmap-write-test** (userspace/tests/mmap-write-test.c) — 5 kernel mmap tests:
- Simple mmap + write + readback
- Large mmap (3MB) with BSS-range page writes
- Sequential page touch verification (808 pages)
- Cross-region integrity after second mmap
- Copy pattern + BSS area write

## All Items Complete

- [x] PLT lazy binding infrastructure (GOT[1]/GOT[2] set up, eager binding, ud2 safety trampoline in arch_x86_64.rs)
- [x] ld-oxide is a self-relocating PIE (ET_DYN, __ehdr_start base computation, RELATIVE reloc loop in _start)
- [x] WEAK symbol support (first-in-scope-order wins, matches Linux behavior)
- [x] All arch-specific asm isolated in arch_x86_64.rs (no asm in main.rs or kernel)
