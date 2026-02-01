# Plan: Port Vim to OXIDE OS

## Overview

Port vim text editor to OXIDE OS by implementing missing POSIX regex support and filling stdlib gaps, then cross-compiling vim using the existing oxide-cc toolchain.

**Status**: OXIDE OS has 99% of what vim needs. Main gaps:
- POSIX regex library (regcomp/regexec/regfree)
- getopt() for command-line parsing
- Minor stdlib functions

**Estimated effort**: 18-20 hours total

---

## Phase 1: Implement POSIX Regex (5 hours)

### Why musl regex?
- Small (~2000 lines), POSIX-compliant, MIT licensed
- Battle-tested in Alpine Linux and embedded systems
- Clean C code, easy to integrate with oxide-cc

### Files to create/modify:

**1. Create regex header** (`toolchain/sysroot/include/regex.h`)
```c
// Standard POSIX regex API
typedef struct { ... } regex_t;
typedef struct { ... } regmatch_t;
int regcomp(regex_t *preg, const char *pattern, int cflags);
int regexec(const regex_t *preg, const char *string, size_t nmatch,
            regmatch_t pmatch[], int eflags);
size_t regerror(int errcode, const regex_t *preg, char *errbuf, size_t errbuf_size);
void regfree(regex_t *preg);
```

**2. Extract musl regex source** (`external/musl-regex/`)
- Download musl libc 1.2.5
- Extract: `regcomp.c`, `regexec.c`, `regerror.c`, `tre-mem.c`, `tre.h`

**3. Create build script** (`external/musl-regex/Makefile`)
```makefile
CC = ../../toolchain/bin/oxide-cc
AR = llvm-ar
SYSROOT = ../../toolchain/sysroot

libregex.a: regcomp.o regexec.o regerror.o tre-mem.o
	$(AR) rcs $@ $^
	cp $@ $(SYSROOT)/lib/
```

**4. Test regex** (`toolchain/tests/test-regex.c`)
- Test literal matching, character classes, quantifiers, anchors, subexpressions

---

## Phase 2: Fill stdlib Gaps (4 hours)

### Files to create/modify:

**1. Implement getopt** (`userspace/libc/src/getopt.rs`)
```rust
// Global state for C compatibility
static mut OPTIND: i32 = 1;
static mut OPTARG: *mut u8 = core::ptr::null_mut();
static mut OPTOPT: i32 = 0;
static mut OPTERR: i32 = 1;

#[unsafe(no_mangle)]
pub extern "C" fn getopt(argc: i32, argv: *const *const u8, optstring: *const u8) -> i32 {
    // Standard getopt implementation (~80 lines)
}
```

**2. Export getopt** (`userspace/libc/src/c_exports.rs`)
- Add exports for `getopt()`, `optind`, `optarg`, `opterr`, `optopt`

**3. Add getopt module** (`userspace/libc/src/lib.rs`)
```rust
pub mod getopt;
```

**4. Verify existing functions**
- Confirm `qsort()`, `bsearch()`, `strtol()`, `strtod()` work (already in c_exports.rs)
- Test edge cases

**5. Implement mkstemp if needed** (`userspace/libc/src/c_exports.rs`)
```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkstemp(template: *mut u8) -> i32 {
    // Find XXXXXX, replace with random chars, open with O_CREAT|O_EXCL
}
```

---

## Phase 3: Configure and Build Vim (5 hours)

### Files to create:

**1. Download vim source**
```bash
cd external
git clone --depth 1 --branch v9.1.0 https://github.com/vim/vim.git
```

**2. Create build script** (`scripts/build-vim.sh`)

Follow `scripts/build-cpython.sh` pattern:
```bash
#!/usr/bin/env bash
set -e

OXIDE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VIM_SRC="$OXIDE_ROOT/external/vim"
TOOLCHAIN="$OXIDE_ROOT/toolchain"
SYSROOT="$TOOLCHAIN/sysroot"

# Check prerequisites
[ -d "$VIM_SRC" ] || { echo "Clone vim first"; exit 1; }
[ -f "$TOOLCHAIN/bin/oxide-cc" ] || make toolchain

export PATH="$TOOLCHAIN/bin:$PATH"

# Configure vim
cd "$VIM_SRC/src"
./configure \
    --host=x86_64-unknown-linux-elf \
    --with-features=small \
    --disable-gui \
    --disable-gtktest \
    --disable-xim \
    --disable-netbeans \
    --disable-pythoninterp \
    --disable-python3interp \
    --disable-rubyinterp \
    --disable-luainterp \
    --disable-perlinterp \
    --disable-tclinterp \
    --disable-cscope \
    --disable-gpm \
    --disable-sysmouse \
    --enable-multibyte \
    --with-tlib=oxide_libc \
    CC=oxide-cc \
    AR=llvm-ar \
    RANLIB=llvm-ranlib \
    CFLAGS="-O2" \
    LDFLAGS="-static -L$SYSROOT/lib -lregex"

# Build
make -j$(nproc)

# Copy to release directory
cp vim "$OXIDE_ROOT/target/x86_64-unknown-none/release/vim"
strip "$OXIDE_ROOT/target/x86_64-unknown-none/release/vim"
```

**3. Create config cache** (`external/vim-oxide-config.cache`)

Override autoconf detection for cross-compilation:
```bash
# Terminal capabilities
ac_cv_header_termios_h=yes
ac_cv_func_tcgetattr=yes
ac_cv_func_tcsetattr=yes

# File I/O
ac_cv_func_fseeko=yes
ac_cv_func_lstat=yes
ac_cv_func_readlink=yes

# Memory
ac_cv_func_mmap=yes
ac_cv_func_mprotect=yes

# Regex
ac_cv_header_regex_h=yes
ac_cv_func_regcomp=yes

# System
ac_cv_func_getpwnam=yes
ac_cv_func_getpwuid=yes
```

Load with: `CONFIG_SITE=../../vim-oxide-config.cache ./configure ...`

---

## Phase 4: Integrate with Build System (1 hour)

### Files to modify:

**1. Add vim to Makefile** (`Makefile`)

Add external-vim target:
```makefile
.PHONY: external-vim
external-vim:
	@echo "  [BUILD] vim"
	@./scripts/build-vim.sh
```

Add to dependencies:
```makefile
userspace-release: external-vim
```

**2. Include vim in initramfs** (`Makefile` - initramfs target)

Add around line 200 (in initramfs binary copy section):
```makefile
@if [ -f "$(OXIDE_ROOT)/target/x86_64-unknown-none/release/vim" ]; then \
    cp "$(OXIDE_ROOT)/target/x86_64-unknown-none/release/vim" $(TARGET_DIR)/initramfs/bin/vim; \
fi
```

**3. Create default vimrc** (`build/vimrc.default`)
```vim
" OXIDE OS minimal vimrc
set nocompatible
syntax on
set backspace=indent,eol,start
set ruler
set showcmd
set incsearch
set hlsearch
set autoindent
```

Copy to initramfs as `/etc/vimrc`

---

## Phase 5: Testing and Validation (6 hours)

### Test plan:

**1. Basic functionality**
- Launch vim: `vim test.txt`
- Insert mode: `i`, type text, `Esc`
- Save: `:w`
- Quit: `:q`
- Search: `/pattern`
- Replace: `:%s/old/new/g`
- Undo/redo: `u`, `Ctrl-R`
- Visual mode: `v`, select, `d`

**2. Regex functionality**
- Search with regex: `/\d\+` (numbers)
- Replace with groups: `:%s/\(foo\)\(bar\)/\2\1/g`
- Character classes: `/[a-zA-Z_]\w*`
- Anchors: `/^start/`, `/end$/`

**3. Terminal integration**
- Syntax highlighting works
- Arrow keys navigate correctly
- Ctrl-C, Ctrl-Z behavior
- Window resize (SIGWINCH) handling
- Color escape sequences

**4. File operations**
- Open multiple files: `vim file1 file2`
- Switch buffers: `:bn`, `:bp`
- Save as: `:w newfile`
- Large file handling (>1MB)

**5. Create test script** (`toolchain/tests/test-vim.sh`)

Automated vim command execution test:
```bash
#!/bin/bash
# Create test file
echo "line1" > /tmp/test.txt
echo "line2" >> /tmp/test.txt

# Test vim commands
vim -c ":%s/line/LINE/g" -c ":wq" /tmp/test.txt

# Verify result
grep -q "LINE1" /tmp/test.txt && echo "PASS: vim editing works"
```

---

## Critical Files Summary

| File Path | Purpose | Complexity |
|-----------|---------|------------|
| `toolchain/sysroot/include/regex.h` | POSIX regex API | Low |
| `external/musl-regex/regcomp.c` | Regex compiler | Medium |
| `external/musl-regex/regexec.c` | Regex executor | Medium |
| `userspace/libc/src/getopt.rs` | Command-line parsing | Medium |
| `userspace/libc/src/c_exports.rs` | Export getopt symbols | Low |
| `scripts/build-vim.sh` | Vim build orchestration | Medium |
| `external/vim-oxide-config.cache` | Autoconf overrides | Medium |
| `Makefile` | Build integration | Low |

---

## Dependencies

```
regex.h header
    ↓
libregex.a ←────┐
    ↓            │
getopt.rs        │
    ↓            │
vim configure ───┘
    ↓
vim build
    ↓
Makefile integration
    ↓
testing
```

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Regex portability issues | Medium | High | Use stable musl implementation; allocate debug time |
| Vim configure failures | High | Medium | Comprehensive config.cache; manual Makefile edits if needed |
| Missing libc functions | Medium | Medium | Iterative build-fix-rebuild; stub non-critical functions |
| Terminal escape handling | Low | Low | OXIDE has working termios; fall back to simpler features |

---

## Acceptance Criteria

- [ ] Vim launches without error
- [ ] Can create, edit, and save text files
- [ ] Search with regex patterns works
- [ ] Substitute with regex works
- [ ] Syntax highlighting functional
- [ ] Undo/redo works
- [ ] Terminal handling (resize, colors) works
- [ ] Can edit files >100KB
- [ ] Can exit cleanly from all modes

---

## Build Commands

```bash
# Phase 1: Build regex library
cd external/musl-regex
make
make install

# Test regex
cd ../../toolchain/tests
oxide-cc -o test-regex test-regex.c -lregex
./test-regex

# Phase 2: Rebuild libc with getopt
cd ../../
make toolchain

# Phase 3: Build vim
./scripts/build-vim.sh

# Phase 4: Create initramfs with vim
make create-rootfs

# Phase 5: Test in QEMU
make run
# In OXIDE shell:
vim /tmp/test.txt
```

---

## Timeline

- Phase 1 (Regex): 5 hours
- Phase 2 (stdlib): 4 hours
- Phase 3 (Vim config): 5 hours
- Phase 4 (Integration): 1 hour
- Phase 5 (Testing): 6 hours

**Total: 21 hours** (sequential)
**Optimized: 18-20 hours** (with parallelization of Phase 1 + 2)

---

## Success Metrics

1. **Functional**: Vim runs and handles basic editing
2. **Reliable**: No crashes during extended use
3. **Complete**: All core features work (insert, visual, ex mode)
4. **Performant**: Handles 10,000-line files smoothly
5. **Integrated**: Available in standard OXIDE initramfs

---

## Next Steps After Completion

1. Port vimtutor for learning
2. Add vim runtime files (syntax highlighting definitions)
3. Consider `--with-features=normal` for enhanced experience
4. Port nano as simpler alternative editor
5. Add ctags/cscope for code navigation
