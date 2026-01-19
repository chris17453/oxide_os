# EFFLUX Loadable Kernel Modules Specification

**Version:** 1.0  
**Status:** Draft  
**License:** MIT  

---

## 0) Overview

EFFLUX supports runtime loading/unloading of kernel modules (drivers, filesystems, etc.) without kernel recompilation.

---

## 1) Module Format

Modules are ELF relocatable objects (.ko extension by convention).

```rust
#[repr(C)]
pub struct ModuleInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub author: &'static str,
    pub license: &'static str,
    pub description: &'static str,
    pub init: fn() -> Result<()>,
    pub exit: fn(),
    pub dependencies: &'static [&'static str],
}

#[macro_export]
macro_rules! module {
    ($name:ident, $init:expr, $exit:expr) => {
        #[no_mangle]
        #[link_section = ".modinfo"]
        pub static __MODULE_INFO: $crate::ModuleInfo = $crate::ModuleInfo {
            name: stringify!($name),
            version: env!("CARGO_PKG_VERSION"),
            author: env!("CARGO_PKG_AUTHORS"),
            license: "MIT",
            description: env!("CARGO_PKG_DESCRIPTION"),
            init: $init,
            exit: $exit,
            dependencies: &[],
        };
    };
}
```

---

## 2) Module Example

```rust
// drivers/example/src/lib.rs
#![no_std]

use kernel::prelude::*;

fn init() -> Result<()> {
    info!("Example module loaded");
    Ok(())
}

fn exit() {
    info!("Example module unloaded");
}

module!(example, init, exit);
```

---

## 3) Kernel Symbol Export

```rust
// Kernel exports symbols for modules
#[export_symbol]
pub fn register_driver(driver: &Driver) -> Result<()> { ... }

#[export_symbol]
pub fn unregister_driver(driver: &Driver) { ... }

#[export_symbol]
pub fn kmalloc(size: usize) -> *mut u8 { ... }

#[export_symbol]
pub fn kfree(ptr: *mut u8) { ... }
```

Symbol table built at kernel compile time.

---

## 4) Module Loader

```rust
pub struct ModuleLoader {
    loaded: RwLock<HashMap<String, LoadedModule>>,
    symbol_table: RwLock<HashMap<String, usize>>,
}

pub struct LoadedModule {
    pub info: &'static ModuleInfo,
    pub base_addr: VirtAddr,
    pub size: usize,
    pub state: ModuleState,
    pub ref_count: AtomicU32,
}

pub enum ModuleState {
    Loading,
    Live,
    Unloading,
}

impl ModuleLoader {
    pub fn load(&self, path: &Path) -> Result<()> {
        // 1. Read ELF file
        let elf = read_elf(path)?;
        
        // 2. Allocate kernel memory
        let base = vmalloc(elf.size())?;
        
        // 3. Load sections
        for section in elf.sections() {
            if section.is_alloc() {
                copy_section(base, &section);
            }
        }
        
        // 4. Resolve relocations
        for reloc in elf.relocations() {
            let symbol_addr = self.resolve_symbol(reloc.symbol())?;
            apply_relocation(base, &reloc, symbol_addr)?;
        }
        
        // 5. Find module info
        let info = find_module_info(base)?;
        
        // 6. Check dependencies
        for dep in info.dependencies {
            if !self.is_loaded(dep) {
                self.load_by_name(dep)?;
            }
        }
        
        // 7. Call init
        (info.init)()?;
        
        // 8. Register module
        self.loaded.write().insert(info.name.to_string(), LoadedModule {
            info,
            base_addr: base,
            size: elf.size(),
            state: ModuleState::Live,
            ref_count: AtomicU32::new(0),
        });
        
        Ok(())
    }
    
    pub fn unload(&self, name: &str) -> Result<()> {
        let mut loaded = self.loaded.write();
        let module = loaded.get(name).ok_or(Error::NotFound)?;
        
        // Check ref count
        if module.ref_count.load(Ordering::Acquire) > 0 {
            return Err(Error::Busy);
        }
        
        // Call exit
        (module.info.exit)();
        
        // Free memory
        vfree(module.base_addr, module.size);
        
        // Remove from table
        loaded.remove(name);
        
        Ok(())
    }
    
    fn resolve_symbol(&self, name: &str) -> Result<usize> {
        self.symbol_table.read()
            .get(name)
            .copied()
            .ok_or(Error::SymbolNotFound)
    }
}
```

---

## 5) Syscalls

```rust
pub fn sys_init_module(image: *const u8, len: usize, param: *const u8) -> Result<()>;
pub fn sys_finit_module(fd: i32, param: *const u8, flags: i32) -> Result<()>;
pub fn sys_delete_module(name: *const u8, flags: u32) -> Result<()>;
```

---

## 6) CLI Tools

```bash
# Load module
efflux insmod /lib/modules/example.ko

# Load module with dependencies
efflux modprobe example

# Unload module
efflux rmmod example

# List loaded modules
efflux lsmod

# Show module info
efflux modinfo /lib/modules/example.ko
```

---

## 7) Module Directory Structure

```
/lib/modules/
├── example.ko
├── virtio_blk.ko
├── virtio_net.ko
├── effluxfs.ko
├── fat32.ko
├── e1000.ko
└── modules.dep        # Dependency info
```

---

## 8) Security

- Modules must be signed (if secure boot enabled)
- Only root can load/unload modules
- Module signature verified against trusted keys

```rust
pub fn verify_module_signature(elf: &[u8]) -> Result<TrustLevel> {
    let sig = extract_signature(elf)?;
    let cert = extract_certificate(&sig)?;
    verify_certificate_chain(&cert)?;
    verify_signature(&sig, elf)?;
    Ok(cert.trust_level())
}
```

---

## 9) Exit Criteria

- [ ] Modules load at runtime
- [ ] Symbol resolution works
- [ ] Dependencies resolved automatically
- [ ] Modules unload cleanly
- [ ] Module signing works (optional)
- [ ] Works on both arches

---

*End of EFFLUX Module Loader Specification*
