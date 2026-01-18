# Phase 25: Full Libc

**Stage:** 5 - Polish
**Status:** Not Started
**Dependencies:** Phase 19 (Self-Hosting)

---

## Goal

Complete POSIX libc implementation for source compatibility with Linux applications.

---

## Deliverables

| Item | Status |
|------|--------|
| Complete POSIX coverage | [ ] |
| glibc compatibility shims | [ ] |
| Dynamic linking (ld.so) | [ ] |
| dlopen/dlsym | [ ] |
| Thread-local storage | [ ] |
| Locale support | [ ] |
| Wide character support | [ ] |

---

## Architecture Status

| Arch | POSIX | glibc | ld.so | dlopen | Done |
|------|-------|-------|-------|--------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Libc Categories

### Already Implemented (Phase 8)
- Basic string functions
- Basic stdio
- Basic stdlib
- Basic unistd
- Basic signal

### To Implement

| Category | Functions | Priority |
|----------|-----------|----------|
| stdio | popen, pclose, tmpfile, mkstemp, getline | High |
| stdlib | mktemp, realpath, system, getenv, setenv | High |
| string | strtok, strdup, strndup, strcasecmp | High |
| unistd | access, chown, link, symlink, readlink | High |
| fcntl | fcntl, flock | High |
| dirent | opendir, readdir, closedir, scandir | High |
| time | clock, times, clock_gettime, nanosleep | High |
| pthread | Full pthread API | High |
| dlfcn | dlopen, dlsym, dlclose, dlerror | High |
| netdb | gethostbyname, getaddrinfo, getnameinfo | Medium |
| regex | regcomp, regexec, regfree | Medium |
| locale | setlocale, localeconv | Medium |
| wchar | wprintf, fwprintf, wcslen, wcscpy | Medium |
| pwd/grp | getpwnam, getpwuid, getgrnam, getgrgid | Medium |
| termios | Full termios API | Medium |
| mmap | mmap, munmap, mprotect, msync | High |
| poll | poll, ppoll | High |
| select | select, pselect | High |
| iconv | iconv_open, iconv, iconv_close | Low |
| math | Full libm (sin, cos, exp, log, etc.) | Medium |

---

## Dynamic Linker (ld.so)

```
Program Start
      │
      ▼
┌─────────────────┐
│  Kernel loads   │
│  ld.so + binary │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ ld.so _start    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Parse ELF       │
│ - PT_DYNAMIC    │
│ - DT_NEEDED     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Load libraries  │
│ (recursive)     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Relocations     │
│ - R_*_GLOB_DAT  │
│ - R_*_JUMP_SLOT │
│ - R_*_RELATIVE  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Run .init       │
│ constructors    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Jump to main()  │
└─────────────────┘
```

---

## dlopen/dlsym

```rust
/// Open shared library
pub fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;

/// Look up symbol
pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;

/// Close library
pub fn dlclose(handle: *mut c_void) -> c_int;

/// Get error message
pub fn dlerror() -> *mut c_char;

// Flags
pub const RTLD_LAZY: c_int = 0x0001;    // Lazy binding
pub const RTLD_NOW: c_int = 0x0002;     // Immediate binding
pub const RTLD_GLOBAL: c_int = 0x0100;  // Symbols globally available
pub const RTLD_LOCAL: c_int = 0x0000;   // Symbols local (default)

// Special handles
pub const RTLD_DEFAULT: *mut c_void = 0 as _;  // Search default libs
pub const RTLD_NEXT: *mut c_void = -1isize as _; // Next occurrence
```

---

## Thread-Local Storage

```
┌─────────────────────────────────────────────────────┐
│                    Thread                            │
│                                                      │
│  ┌─────────────────────────────────────────────┐   │
│  │              TLS Block                       │   │
│  │                                              │   │
│  │  ┌─────────────────────────────────────────┐│   │
│  │  │ Static TLS (program + initial libs)    ││   │
│  │  │ - __thread variables                   ││   │
│  │  │ - errno                                ││   │
│  │  └─────────────────────────────────────────┘│   │
│  │                                              │   │
│  │  ┌─────────────────────────────────────────┐│   │
│  │  │ Dynamic TLS (dlopen'd libs)            ││   │
│  │  │ - Allocated on demand                  ││   │
│  │  └─────────────────────────────────────────┘│   │
│  │                                              │   │
│  │  TCB (Thread Control Block)                 │   │
│  │  - Points to TLS                            │   │
│  │  - Self pointer                             │   │
│  │  - Stack info                               │   │
│  └─────────────────────────────────────────────┘   │
│                                                      │
│  Access via:                                         │
│  - x86_64: FS segment                               │
│  - aarch64: TPIDR_EL0                               │
│  - riscv: TP register                               │
└─────────────────────────────────────────────────────┘
```

---

## glibc Compatibility Shims

```rust
// Version symbols
// libfoo.so may export:
//   foo@@GLIBC_2.17  (current version)
//   foo@GLIBC_2.0    (old version)

// We implement the latest version and provide
// compatibility for older symbol versions

// Example: realpath
// GLIBC_2.0: realpath(path, resolved) - resolved must be PATH_MAX
// GLIBC_2.3: realpath(path, NULL) - allocates result

pub fn realpath_glibc_2_0(path: *const c_char, resolved: *mut c_char) -> *mut c_char;
pub fn realpath_glibc_2_3(path: *const c_char, resolved: *mut c_char) -> *mut c_char;

// Symbol versioning in .gnu.version_d section
```

---

## Locale Support

```rust
pub struct Locale {
    /// LC_COLLATE: String collation
    pub collate: CollateData,

    /// LC_CTYPE: Character classification
    pub ctype: CtypeData,

    /// LC_MESSAGES: Message catalogs
    pub messages: MessagesData,

    /// LC_MONETARY: Currency formatting
    pub monetary: MonetaryData,

    /// LC_NUMERIC: Number formatting
    pub numeric: NumericData,

    /// LC_TIME: Date/time formatting
    pub time: TimeData,
}

// setlocale
pub fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;

// Categories
pub const LC_ALL: c_int = 0;
pub const LC_COLLATE: c_int = 1;
pub const LC_CTYPE: c_int = 2;
pub const LC_MESSAGES: c_int = 3;
pub const LC_MONETARY: c_int = 4;
pub const LC_NUMERIC: c_int = 5;
pub const LC_TIME: c_int = 6;
```

---

## Key Files

```
userspace/libc/
├── src/
│   ├── string/
│   │   ├── strdup.c
│   │   ├── strtok.c
│   │   └── ...
│   ├── stdio/
│   │   ├── popen.c
│   │   ├── getline.c
│   │   └── ...
│   ├── stdlib/
│   │   ├── realpath.c
│   │   ├── system.c
│   │   └── ...
│   ├── pthread/
│   │   ├── pthread_create.c
│   │   ├── pthread_mutex.c
│   │   └── ...
│   ├── dlfcn/
│   │   ├── dlopen.c
│   │   ├── dlsym.c
│   │   └── ...
│   ├── locale/
│   │   ├── setlocale.c
│   │   └── ...
│   └── math/
│       ├── sin.c
│       ├── cos.c
│       └── ...
├── include/
│   └── (all POSIX headers)
└── ld.so/
    ├── rtld.c             # Runtime linker
    ├── elf.c              # ELF parsing
    ├── reloc.c            # Relocations
    └── tls.c              # Thread-local storage
```

---

## Syscall Wrappers

```c
// All libc functions that need kernel interaction
// go through syscall wrappers

// Example: read
ssize_t read(int fd, void *buf, size_t count) {
    return syscall(SYS_read, fd, buf, count);
}

// Example: mmap
void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset) {
    return (void *)syscall(SYS_mmap, addr, length, prot, flags, fd, offset);
}

// Errno handling
// syscall returns -errno on error
// wrapper converts to -1 and sets errno
```

---

## Exit Criteria

- [ ] All POSIX.1-2017 functions implemented
- [ ] Dynamic linking works
- [ ] dlopen/dlsym functional
- [ ] Thread-local storage works
- [ ] Complex Linux apps recompile
- [ ] Binary compatibility where possible
- [ ] Works on all 8 architectures

---

## Test: Complex Application

```bash
# Build a complex app (e.g., git, vim, Python)
$ tar xf Python-3.12.tar.xz
$ cd Python-3.12
$ ./configure --prefix=/usr
$ make
$ make install

# Verify it works
$ python3 --version
Python 3.12.0

$ python3 -c "import sys; print(sys.platform)"
efflux

# Run test suite
$ python3 -m test
(tests pass)
```

---

## Compatibility Targets

| Application | Status | Notes |
|-------------|--------|-------|
| coreutils | [ ] | Basic system tools |
| bash | [ ] | Shell |
| vim | [ ] | Editor |
| git | [ ] | Version control |
| Python | [ ] | Interpreter |
| GCC | [ ] | Compiler |
| LLVM | [ ] | Compiler |
| nginx | [ ] | Web server |
| SQLite | [ ] | Database |
| curl | [ ] | HTTP client |

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 25 of EFFLUX Implementation*
