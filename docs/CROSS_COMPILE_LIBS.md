# OXIDE Cross-Compilation Library Requirements
**Date:** 2026-02-02
**Goal:** Enable cross-compilation of terminal software (htop → vim → Python)

---

## Current State

### Existing Libraries in OXIDE

| Library | Location | Status | Lines |
|---------|----------|--------|-------|
| libc | `userspace/libs/libc` | ~80% complete | 21K |
| oxide-std | `userspace/libs/oxide-std` | Basic | ~2K |
| compression | `userspace/libs/compression` | Minimal | ~1K |
| pthread | `kernel/libc-support/pthread` | Stubs | ~200 |

### libc Coverage

**Implemented:**
- stdio (printf, fopen, fread, fwrite, etc.)
- string (strlen, strcpy, memcpy, strcmp, etc.)
- stdlib (malloc, free, exit, atoi, etc.)
- unistd (read, write, open, close, fork, exec, etc.)
- fcntl (open flags, fcntl)
- dirent (opendir, readdir, closedir)
- time (time, gettimeofday, nanosleep)
- signal (signal, sigaction, kill)
- socket (socket, bind, connect, send, recv)
- termios (tcgetattr, tcsetattr)
- errno
- math (via libm crate)
- ctype (isalpha, isdigit, etc.)
- pwd (getpwuid, getpwnam - basic)
- poll (poll, select)

**Incomplete/Stubs:**
- zlib (CRC32 only, no compress/decompress)
- locale (minimal)
- wchar (partial)
- dlfcn (dlopen/dlsym stubs)

---

## Target Software Dependency Analysis

### Tier 1: Simple Terminal Tools

#### htop / top
**Dependencies:**
- ncurses (terminal UI)
- procfs (/proc filesystem) ✅ We have this
- libc ✅ Mostly covered

**Effort:** Medium - need ncurses

#### less
**Dependencies:**
- ncurses or termcap
- libc ✅
- regex (optional)

**Effort:** Medium - need ncurses/termcap

#### nano
**Dependencies:**
- ncurses
- libc ✅

**Effort:** Medium - need ncurses

---

### Tier 2: Medium Complexity

#### vim
**Dependencies:**
- ncurses (required)
- termcap/terminfo
- regex (built-in or libc)
- iconv (optional, for encoding)
- libc ✅

**Effort:** High - need ncurses + build system work

#### tmux
**Dependencies:**
- ncurses
- libevent (async I/O)
- utf8proc (Unicode)
- libc ✅

**Effort:** High - need ncurses + libevent

#### bash
**Dependencies:**
- readline
- ncurses
- termcap
- libc ✅

**Effort:** High - need readline + ncurses

---

### Tier 3: Complex Interpreters

#### Python 3.x
**Dependencies:**
- zlib (compression)
- libffi (ctypes, very complex)
- readline (interactive shell)
- ncurses (curses module)
- sqlite3 (optional but common)
- openssl (ssl module)
- bzip2 (optional)
- xz/lzma (optional)
- libexpat (xml parsing)
- libmpdec (decimal module)
- libc ✅

**Effort:** Very High - many dependencies

#### Node.js
**Dependencies:**
- libuv (event loop)
- openssl
- zlib
- icu (internationalization)
- libc ✅

**Effort:** Very High

---

## Required Libraries to Port/Implement

### Priority 1: ncurses (Enables Tier 1 & 2)

**Options:**

1. **Port ncurses** (~100K lines C)
   - Full compatibility
   - Complex build system
   - Many terminfo entries

2. **Port PDCurses** (~30K lines C)
   - Public domain
   - Simpler
   - Less terminal support

3. **Implement minimal curses in Rust** (~5K lines)
   - Just what's needed for htop/vim
   - Functions: initscr, endwin, mvprintw, getch, color_pair, etc.
   - Use ANSI escape codes directly
   - Fastest path to working software

**Recommendation:** Option 3 - Rust ncurses subset

### Priority 2: Full zlib (Enables Python)

**Current:** CRC32 only
**Needed:** deflate/inflate compression

**Options:**

1. **Port zlib** (~15K lines C)
   - Battle-tested
   - Well-documented

2. **Use miniz** (~10K lines C)
   - Single file
   - Public domain
   - zlib-compatible API

3. **Use Rust crate + C wrapper**
   - flate2 crate (Rust)
   - Write C API wrapper

**Recommendation:** Option 2 - miniz is simpler

### Priority 3: readline (Enables bash/Python REPL)

**Current:** Basic implementation in libc (readline.rs)
**Needed:** History, completion, vi/emacs modes

**Options:**

1. **Enhance existing Rust readline**
   - Add history file support
   - Add tab completion hooks
   - Simpler than porting GNU readline

2. **Port libedit** (~20K lines)
   - BSD licensed
   - Simpler than GNU readline

**Recommendation:** Option 1 - extend existing

### Priority 4: libffi (Enables Python ctypes)

**Complexity:** Very high - architecture-specific assembly

**Options:**

1. **Port libffi**
   - Complex, requires x86_64 assembly
   - Needed for full Python ctypes

2. **Disable ctypes in Python build**
   - Many Python packages won't work
   - But core Python works

**Recommendation:** Defer - disable ctypes initially

---

## Implementation Roadmap

### Phase 1: ncurses-lite (2-3 weeks)
```
userspace/libs/ncurses/
├── src/
│   ├── lib.rs          # Core types, initialization
│   ├── window.rs       # Window management
│   ├── input.rs        # Keyboard input, getch
│   ├── output.rs       # Screen output, mvprintw
│   ├── color.rs        # Color pairs, attributes
│   ├── terminfo.rs     # Basic terminal capabilities
│   └── c_api.rs        # C-compatible API exports
└── include/
    └── curses.h        # C header for linking
```

**Key functions to implement:**
- `initscr()`, `endwin()`
- `newwin()`, `delwin()`, `mvwin()`
- `wmove()`, `waddch()`, `waddstr()`, `wprintw()`
- `wgetch()`, `wgetnstr()`
- `wrefresh()`, `wnoutrefresh()`, `doupdate()`
- `start_color()`, `init_pair()`, `COLOR_PAIR()`
- `cbreak()`, `noecho()`, `keypad()`
- `getmaxy()`, `getmaxx()`
- `box()`, `wborder()`

### Phase 2: Full zlib (1-2 weeks)
- Port miniz.c
- Test with gzip/gunzip utility
- Verify Python zlib module works

### Phase 3: Enhanced readline (1 week)
- History file (~/.history)
- Basic tab completion API
- Emacs key bindings

### Phase 4: Cross-compile htop (1 week)
- First real test of ncurses
- Verify /proc integration
- Debug and fix issues

### Phase 5: Cross-compile vim-tiny (2-3 weeks)
- Minimal vim build
- No GUI, no Perl/Python/Ruby
- Text-mode only

### Phase 6: Python 3 minimal (4+ weeks)
- Disable: ctypes, ssl, tkinter, sqlite (initially)
- Enable: math, json, re, collections, itertools
- Test REPL, basic scripts

---

## Toolchain Requirements

### Current Toolchain
```
toolchain/bin/
├── oxide-as      # Assembler
├── oxide-ld      # Linker  
├── oxide-ar      # Archiver
└── oxide-cc      # C compiler wrapper (calls clang)
```

### Needed for Cross-Compilation
1. **Proper sysroot** with headers:
   ```
   toolchain/sysroot/
   ├── include/
   │   ├── stdio.h
   │   ├── stdlib.h
   │   ├── curses.h      # New
   │   ├── zlib.h        # New
   │   └── ...
   └── lib/
       ├── libc.a
       ├── libcurses.a   # New
       ├── libz.a        # New
       └── ...
   ```

2. **pkg-config support** (many builds use it)
   ```
   toolchain/lib/pkgconfig/
   ├── ncurses.pc
   ├── zlib.pc
   └── ...
   ```

3. **CMake toolchain file** for CMake-based projects

---

## Summary: Minimum Viable Libraries

| Library | Purpose | Effort | Enables |
|---------|---------|--------|---------|
| ncurses-lite | Terminal UI | 2-3 weeks | htop, less, nano, vim |
| miniz (zlib) | Compression | 1-2 weeks | Python, many tools |
| readline++ | Line editing | 1 week | bash, Python REPL |

**Total estimated effort for htop:** 3-4 weeks
**Total estimated effort for vim:** 6-8 weeks  
**Total estimated effort for Python:** 12+ weeks

---

## Quick Win: htop without ncurses?

An alternative approach: port/write a simpler process viewer that uses ANSI codes directly instead of ncurses. Like a custom `top` that:

1. Reads from /proc (already works)
2. Uses ANSI escape codes for colors/positioning
3. No ncurses dependency

This could be done in 1-2 days as proof of concept.
