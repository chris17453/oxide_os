# Phase 10: Loadable Kernel Modules

**Stage:** 3 - Hardware
**Status:** Not Started
**Dependencies:** Phase 9 (SMP)

---

## Goal

Support runtime loading and unloading of kernel modules (drivers).

---

## Deliverables

| Item | Status |
|------|--------|
| Module binary format (relocatable ELF) | [ ] |
| Module loader with relocation | [ ] |
| Symbol resolution (kernel exports) | [ ] |
| init/exit module hooks | [ ] |
| Module dependencies | [ ] |
| insmod/rmmod/lsmod utilities | [ ] |

---

## Architecture Status

| Arch | Loader | Relocs | Symbols | Deps | Done |
|------|--------|--------|---------|------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Module Format

Modules are relocatable ELF objects (.ko files):
- ET_REL (relocatable) not ET_EXEC
- Contains .text, .data, .bss, .rodata
- Contains relocation sections (.rela.*)
- Contains symbol table (.symtab)
- Special sections: .modinfo, .init, .exit

---

## Module Structure

```rust
/// Module metadata embedded in .modinfo section
pub struct ModInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub author: &'static str,
    pub description: &'static str,
    pub license: &'static str,
    pub depends: &'static [&'static str],
}

/// Module definition macro
#[macro_export]
macro_rules! module {
    ($name:ident, $init:expr, $exit:expr) => {
        #[used]
        #[link_section = ".modinfo"]
        static MODULE_INFO: ModInfo = ModInfo { ... };

        #[no_mangle]
        pub extern "C" fn init_module() -> i32 { $init() }

        #[no_mangle]
        pub extern "C" fn cleanup_module() { $exit() }
    };
}
```

---

## Module Loading Process

```
1. Read .ko file
   │
   ▼
2. Parse ELF headers
   │
   ▼
3. Check dependencies
   ├── Load dependencies first
   │
   ▼
4. Allocate memory for sections
   ├── .text (executable)
   ├── .data (read-write)
   ├── .rodata (read-only)
   └── .bss (zero-initialized)
   │
   ▼
5. Copy section contents
   │
   ▼
6. Process relocations
   ├── R_X86_64_64 (absolute)
   ├── R_X86_64_PC32 (relative)
   └── R_X86_64_PLT32 (function call)
   │
   ▼
7. Resolve symbols
   ├── Kernel exports
   └── Other module exports
   │
   ▼
8. Call init_module()
   │
   ▼
9. Register in module list
```

---

## Relocation Types

| Arch | Type | Calculation |
|------|------|-------------|
| x86_64 | R_X86_64_64 | S + A |
| x86_64 | R_X86_64_PC32 | S + A - P |
| x86_64 | R_X86_64_PLT32 | L + A - P |
| aarch64 | R_AARCH64_ABS64 | S + A |
| aarch64 | R_AARCH64_CALL26 | S + A - P |
| riscv | R_RISCV_64 | S + A |
| riscv | R_RISCV_CALL | S + A - P |

Where: S=symbol value, A=addend, P=place, L=PLT entry

---

## Kernel Symbol Exports

```rust
/// Export a kernel symbol for modules
#[macro_export]
macro_rules! EXPORT_SYMBOL {
    ($sym:ident) => {
        #[used]
        #[link_section = ".ksymtab"]
        static KSYM_$sym: KernelSymbol = KernelSymbol {
            name: stringify!($sym),
            addr: $sym as *const () as usize,
        };
    };
}

// Usage in kernel:
EXPORT_SYMBOL!(printk);
EXPORT_SYMBOL!(kmalloc);
EXPORT_SYMBOL!(kfree);
EXPORT_SYMBOL!(register_driver);
```

---

## Key Files

```
crates/module/efflux-module/src/
├── lib.rs
├── loader.rs          # Module loading
├── reloc.rs           # Relocation processing
├── symbol.rs          # Symbol resolution
├── deps.rs            # Dependency management
└── kobject.rs         # Module tracking

userspace/modutils/
├── insmod.c           # Load module
├── rmmod.c            # Unload module
├── lsmod.c            # List modules
└── modinfo.c          # Show module info
```

---

## Module Syscalls

| Number | Name | Args | Return |
|--------|------|------|--------|
| 50 | sys_init_module | image, len, params | 0 or -errno |
| 51 | sys_delete_module | name, flags | 0 or -errno |
| 52 | sys_query_module | name, which, buf, size | 0 or -errno |

---

## Example Module

```rust
// drivers/hello/hello.rs
use efflux_module::module;

fn hello_init() -> i32 {
    printk!("Hello module loaded!\n");
    0
}

fn hello_exit() {
    printk!("Hello module unloaded!\n");
}

module! {
    name: "hello",
    init: hello_init,
    exit: hello_exit,
    author: "EFFLUX Team",
    description: "Hello World module",
    license: "MIT",
}
```

---

## Exit Criteria

- [ ] Module loads from .ko file
- [ ] Relocations processed correctly
- [ ] Kernel symbols resolved
- [ ] init_module() called on load
- [ ] cleanup_module() called on unload
- [ ] Dependencies loaded automatically
- [ ] insmod/rmmod/lsmod work
- [ ] Works on all 8 architectures

---

## Test

```bash
# Build module
$ make modules

# Load module
$ insmod hello.ko
Hello module loaded!

# List modules
$ lsmod
Module                  Size  Used by
hello                   4096  0

# Unload module
$ rmmod hello
Hello module unloaded!
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 10 of EFFLUX Implementation*
