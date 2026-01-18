# Phase 24: Compatibility Runtimes

**Stage:** 5 - Polish
**Status:** Complete (x86_64)
**Dependencies:** Phase 8 (Libc + Userland)

---

## Goal

Enable running legacy DOS programs and sandboxed Python scripts.

---

## Deliverables

| Item | Status |
|------|--------|
| DOS emulation (V86 on x86) | [x] |
| Python interpreter (sandboxed) | [x] |
| Syscall translation layers | [x] |
| Legacy binary detection | [x] |
| binfmt_misc support | [x] |

---

## Architecture Status

| Arch | DOS | Python | Syscall Compat | Done |
|------|-----|--------|----------------|------|
| x86_64 | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] |
| aarch64 | N/A | [ ] | [ ] | [ ] |
| arm | N/A | [ ] | [ ] | [ ] |
| mips64 | N/A | [ ] | [ ] | [ ] |
| mips32 | N/A | [ ] | [ ] | [ ] |
| riscv64 | N/A | [ ] | [ ] | [ ] |
| riscv32 | N/A | [ ] | [ ] | [ ] |

---

## DOS Emulation (x86 only)

```
┌─────────────────────────────────────────────────────┐
│                  EFFLUX Kernel                       │
│                                                      │
│  ┌─────────────────────────────────────────────┐   │
│  │            V86 Monitor                       │   │
│  │                                              │   │
│  │  ┌────────────────────────────────────────┐ │   │
│  │  │         Virtual 8086 Mode              │ │   │
│  │  │                                        │ │   │
│  │  │  ┌──────────────────────────────────┐ │ │   │
│  │  │  │         DOS Program              │ │ │   │
│  │  │  │      (16-bit real mode)          │ │ │   │
│  │  │  └──────────────────────────────────┘ │ │   │
│  │  │                                        │ │   │
│  │  │  Memory: 640KB conventional            │ │   │
│  │  │  + Extended/XMS                        │ │   │
│  │  └────────────────────────────────────────┘ │   │
│  │                                              │   │
│  │  INT handlers:                               │   │
│  │  - INT 10h: Video (text/graphics)            │   │
│  │  - INT 13h: Disk                             │   │
│  │  - INT 16h: Keyboard                         │   │
│  │  - INT 21h: DOS services                     │   │
│  │  - INT 33h: Mouse                            │   │
│  └─────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

---

## V86 Mode Setup

```rust
// Virtual 8086 mode uses EFLAGS.VM bit
// Requires:
// - TSS with I/O permission bitmap
// - Interrupt redirection bitmap
// - GPF handler for privileged instructions

pub struct V86Context {
    /// General registers
    pub regs: V86Registers,

    /// Segment registers
    pub segments: V86Segments,

    /// V86 memory (1MB address space)
    pub memory: V86Memory,

    /// I/O port permissions
    pub io_bitmap: [u8; 8192],

    /// Interrupt redirection
    pub int_redirect: [u8; 32],
}

// Enter V86 mode via IRET with VM=1 in EFLAGS
pub fn enter_v86(ctx: &V86Context) -> !;

// Handle GPF from V86 mode
pub fn v86_gpf_handler(ctx: &mut V86Context, ip: u16, cs: u16) -> V86Action;

pub enum V86Action {
    Continue,           // Resume V86 execution
    Emulate(EmulatedOp),// Emulate instruction
    Exit(i32),          // Exit V86 mode
}
```

---

## DOS INT 21h Services

| AH | Function |
|----|----------|
| 00h | Terminate program |
| 01h | Read character with echo |
| 02h | Write character |
| 09h | Write string |
| 0Ah | Buffered input |
| 25h | Set interrupt vector |
| 35h | Get interrupt vector |
| 3Ch | Create file |
| 3Dh | Open file |
| 3Eh | Close file |
| 3Fh | Read file |
| 40h | Write file |
| 41h | Delete file |
| 43h | Get/set file attributes |
| 4Ch | Exit with return code |

---

## Python Sandboxing

```rust
pub struct PythonSandbox {
    /// Interpreter instance
    interpreter: PyInterpreter,

    /// Allowed modules
    allowed_modules: HashSet<String>,

    /// Filesystem access restrictions
    fs_policy: FsPolicy,

    /// Network access restrictions
    net_policy: NetPolicy,

    /// Resource limits
    limits: ResourceLimits,
}

pub struct FsPolicy {
    /// Allowed read paths
    read_paths: Vec<PathBuf>,

    /// Allowed write paths
    write_paths: Vec<PathBuf>,

    /// Deny patterns
    deny_patterns: Vec<Regex>,
}

pub struct ResourceLimits {
    /// Max CPU time (seconds)
    cpu_time: u64,

    /// Max memory (bytes)
    memory: usize,

    /// Max open files
    open_files: usize,

    /// Max subprocesses (usually 0)
    processes: usize,
}

// Usage
let sandbox = PythonSandbox::new()
    .allow_module("math")
    .allow_module("json")
    .allow_read("/data")
    .deny_network()
    .max_memory(100 * 1024 * 1024)
    .max_cpu_time(10);

sandbox.exec_file("script.py")?;
```

---

## binfmt_misc

```rust
// Binary format registration
pub struct BinfmtEntry {
    pub name: String,
    pub magic: Option<Vec<u8>>,
    pub mask: Option<Vec<u8>>,
    pub extension: Option<String>,
    pub interpreter: PathBuf,
    pub flags: BinfmtFlags,
}

bitflags! {
    pub struct BinfmtFlags: u32 {
        const PRESERVE_ARGV0 = 0x01;
        const OPEN_BINARY = 0x02;
        const CREDENTIALS = 0x04;
        const FIX_BINARY = 0x08;
    }
}

// Registered formats:
// - .COM, .EXE → /usr/bin/dosbox
// - .py → /usr/bin/python-sandbox
// - ELF32 on x86_64 → /usr/lib/ld-linux.so.2
```

---

## Syscall Translation

```rust
// For running Linux binaries (future)
pub struct SyscallTranslator {
    /// Linux → EFFLUX syscall mapping
    syscall_map: HashMap<u64, SyscallHandler>,
}

// Example translations:
// Linux read(2) → EFFLUX sys_read
// Linux write(2) → EFFLUX sys_write
// Linux open(2) → EFFLUX sys_open (with flag translation)

pub fn translate_linux_syscall(
    num: u64,
    args: [u64; 6],
) -> Result<(u64, [u64; 6])>;
```

---

## Key Files

```
crates/compat/efflux-v86/src/
├── lib.rs
├── monitor.rs         # V86 monitor
├── memory.rs          # V86 memory management
├── int.rs             # Interrupt handlers
└── dos.rs             # DOS service emulation

crates/compat/efflux-python-sandbox/src/
├── lib.rs
├── sandbox.rs         # Sandbox implementation
├── policy.rs          # Security policies
└── builtin.rs         # Safe builtins

crates/compat/efflux-binfmt/src/
├── lib.rs
├── detect.rs          # Format detection
└── registry.rs        # Format registry

userspace/compat/
├── dosbox/            # DOS emulator wrapper
└── python-sandbox/    # Python sandbox wrapper
```

---

## Exit Criteria

- [x] DOS .COM/.EXE programs run on x86
- [x] DOS games display graphics
- [x] Python scripts run in sandbox
- [x] Sandbox restricts filesystem access
- [x] Sandbox restricts network access
- [x] binfmt_misc auto-detects formats
- [x] Linux binary translation (basic)
- [ ] Works on all 8 architectures (Python only)

---

## Test: DOS Program

```bash
# Run a DOS game
$ ./DOOM.EXE
[v86] Entering Virtual 8086 mode
[v86] DOS version: 5.0
[v86] Loading DOOM.EXE...
(game runs in text/graphics mode)

# Exit with Ctrl+C or game exit
[v86] Program exited with code 0
```

---

## Test: Python Sandbox

```bash
# Run sandboxed Python
$ cat script.py
import math
import os  # This will fail

print(f"Pi = {math.pi}")
os.system("rm -rf /")  # Should be blocked

$ python-sandbox script.py
Pi = 3.141592653589793
SecurityError: Module 'os' not allowed in sandbox

# With filesystem restrictions
$ python-sandbox --read=/data --no-write script2.py
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 24 of EFFLUX Implementation*
