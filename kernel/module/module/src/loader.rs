//! Module loader
//!
//! Handles loading and unloading of kernel modules.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::ptr;

use crate::deps::resolve_dependencies;
use crate::kobject::{MODULES, Module, ModuleInfo, ModuleState};
use crate::reloc::{Rela64, Sym64, apply_relocations_x86_64};
use crate::symbol::register_module_symbol;
use crate::{ModuleCleanupFn, ModuleError, ModuleFlags, ModuleInitFn, ModuleResult};

/// ELF header magic
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class: 64-bit
const ELFCLASS64: u8 = 2;

/// ELF type: relocatable
const ET_REL: u16 = 1;

/// ELF machine: x86_64
const EM_X86_64: u16 = 62;

/// Section type: symtab
const SHT_SYMTAB: u32 = 2;
/// Section type: strtab
const SHT_STRTAB: u32 = 3;
/// Section type: rela
const SHT_RELA: u32 = 4;
/// Section type: nobits (BSS)
const SHT_NOBITS: u32 = 8;

/// Section flags
const SHF_WRITE: u64 = 1;
const SHF_ALLOC: u64 = 2;
const SHF_EXECINSTR: u64 = 4;

/// ELF64 header
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

/// ELF64 section header
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct Elf64Shdr {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
}

/// Loaded section info
struct LoadedSection {
    /// Section name
    name: String,
    /// Virtual address in module memory
    addr: usize,
    /// Section size
    size: usize,
    /// Section flags
    flags: u64,
}

/// Load a kernel module from raw ELF data
///
/// # Arguments
/// * `data` - Raw module ELF data
/// * `params` - Module parameters string
/// * `flags` - Loading flags
///
/// # Safety
/// The module init function will be called, which may have side effects.
pub fn load_module(data: &[u8], _params: &str, flags: ModuleFlags) -> ModuleResult<()> {
    // Verify ELF header
    if data.len() < core::mem::size_of::<Elf64Ehdr>() {
        return Err(ModuleError::InvalidFormat);
    }

    let ehdr = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };

    // Check magic
    if ehdr.e_ident[0..4] != ELF_MAGIC {
        return Err(ModuleError::InvalidFormat);
    }

    // Check class (64-bit)
    if ehdr.e_ident[4] != ELFCLASS64 {
        return Err(ModuleError::InvalidFormat);
    }

    // Check type (relocatable)
    if ehdr.e_type != ET_REL {
        return Err(ModuleError::InvalidFormat);
    }

    // Check machine
    #[cfg(target_arch = "x86_64")]
    if ehdr.e_machine != EM_X86_64 {
        return Err(ModuleError::InvalidFormat);
    }

    // Parse section headers
    let shdr_base = unsafe { data.as_ptr().add(ehdr.e_shoff as usize) };
    let shnum = ehdr.e_shnum as usize;
    let shentsize = ehdr.e_shentsize as usize;

    let get_shdr = |idx: usize| -> &Elf64Shdr {
        unsafe { &*(shdr_base.add(idx * shentsize) as *const Elf64Shdr) }
    };

    // Get section name string table
    let shstrtab_shdr = get_shdr(ehdr.e_shstrndx as usize);
    let shstrtab = unsafe {
        core::slice::from_raw_parts(
            data.as_ptr().add(shstrtab_shdr.sh_offset as usize),
            shstrtab_shdr.sh_size as usize,
        )
    };

    // Find symtab and strtab
    let mut symtab_idx = None;
    let mut strtab_idx = None;
    let mut modinfo_idx = None;

    for i in 0..shnum {
        let shdr = get_shdr(i);
        let name = get_section_name(shstrtab, shdr.sh_name as usize);

        match shdr.sh_type {
            SHT_SYMTAB => symtab_idx = Some(i),
            SHT_STRTAB if name == ".strtab" => strtab_idx = Some(i),
            _ => {}
        }

        if name == ".modinfo" {
            modinfo_idx = Some(i);
        }
    }

    // Parse module info
    let modinfo = if let Some(idx) = modinfo_idx {
        let shdr = get_shdr(idx);
        let info_data = unsafe {
            core::slice::from_raw_parts(
                data.as_ptr().add(shdr.sh_offset as usize),
                shdr.sh_size as usize,
            )
        };
        parse_modinfo(info_data)?
    } else {
        return Err(ModuleError::MissingSection);
    };

    // Check if already loaded
    if MODULES.lock().iter().any(|m| m.name == modinfo.name) {
        return Err(ModuleError::AlreadyLoaded);
    }

    // Resolve dependencies
    resolve_dependencies(&modinfo, flags)?;

    // Calculate total allocation size
    let mut total_size = 0usize;
    let mut sections_to_load = Vec::new();

    for i in 0..shnum {
        let shdr = get_shdr(i);
        let name = get_section_name(shstrtab, shdr.sh_name as usize);

        // Only load allocatable sections
        if (shdr.sh_flags & SHF_ALLOC) == 0 {
            continue;
        }

        let aligned_offset =
            (total_size + shdr.sh_addralign as usize - 1) & !(shdr.sh_addralign as usize - 1);
        total_size = aligned_offset + shdr.sh_size as usize;

        sections_to_load.push((i, name.to_string(), aligned_offset, shdr.sh_size as usize));
    }

    // Allocate module memory
    let module_mem = allocate_module_memory(total_size)?;
    let base = module_mem as usize;

    // Copy sections to allocated memory
    let mut loaded_sections = Vec::new();

    for (idx, name, offset, size) in sections_to_load {
        let shdr = get_shdr(idx);
        let dest = base + offset;

        if shdr.sh_type == SHT_NOBITS {
            // BSS: zero-initialize
            unsafe {
                ptr::write_bytes(dest as *mut u8, 0, size);
            }
        } else {
            // Copy section data
            unsafe {
                ptr::copy_nonoverlapping(
                    data.as_ptr().add(shdr.sh_offset as usize),
                    dest as *mut u8,
                    size,
                );
            }
        }

        loaded_sections.push(LoadedSection {
            name,
            addr: dest,
            size,
            flags: shdr.sh_flags,
        });
    }

    // Load symbol and string tables
    let symtab_shdr = get_shdr(symtab_idx.ok_or(ModuleError::MissingSection)?);
    let symtab = unsafe {
        core::slice::from_raw_parts(
            data.as_ptr().add(symtab_shdr.sh_offset as usize) as *const Sym64,
            symtab_shdr.sh_size as usize / core::mem::size_of::<Sym64>(),
        )
    };

    let strtab_shdr = get_shdr(strtab_idx.ok_or(ModuleError::MissingSection)?);
    let strtab = unsafe {
        core::slice::from_raw_parts(
            data.as_ptr().add(strtab_shdr.sh_offset as usize),
            strtab_shdr.sh_size as usize,
        )
    };

    // Process relocations
    for i in 0..shnum {
        let shdr = get_shdr(i);

        if shdr.sh_type != SHT_RELA {
            continue;
        }

        let target_section = shdr.sh_info as usize;
        let target_shdr = get_shdr(target_section);

        // Skip non-allocatable sections
        if (target_shdr.sh_flags & SHF_ALLOC) == 0 {
            continue;
        }

        // Find loaded address of target section
        let section_name = get_section_name(shstrtab, target_shdr.sh_name as usize);
        let section_base = loaded_sections
            .iter()
            .find(|s| s.name == section_name)
            .map(|s| s.addr)
            .unwrap_or(base);

        let rela = unsafe {
            core::slice::from_raw_parts(
                data.as_ptr().add(shdr.sh_offset as usize) as *const Rela64,
                shdr.sh_size as usize / core::mem::size_of::<Rela64>(),
            )
        };

        #[cfg(target_arch = "x86_64")]
        unsafe {
            apply_relocations_x86_64(section_base, rela, symtab, strtab)?;
        }
    }

    // Register exported symbols
    for sym in symtab {
        if sym.binding() == 1 && sym.st_shndx != 0 {
            // Global, defined symbol
            let name = get_string(strtab, sym.st_name as usize);
            let addr = base + sym.st_value as usize;
            register_module_symbol(String::from(name), addr);
        }
    }

    // Find init and cleanup functions
    let init_fn = find_symbol_addr(symtab, strtab, "init_module", base);
    let cleanup_fn = find_symbol_addr(symtab, strtab, "cleanup_module", base);

    // Call init function
    if let Some(init_addr) = init_fn {
        let init: ModuleInitFn = unsafe { core::mem::transmute(init_addr) };
        let ret = init();
        if ret != 0 {
            // Init failed, clean up
            free_module_memory(module_mem, total_size);
            return Err(ModuleError::InitFailed);
        }
    }

    // Create module entry
    let module = Module {
        name: String::from(modinfo.name),
        version: String::from(modinfo.version),
        state: ModuleState::Live,
        base_addr: base,
        size: total_size,
        init_fn,
        cleanup_fn,
        ref_count: 0,
        dependents: Vec::new(),
    };

    MODULES.lock().push(module);

    Ok(())
}

/// Unload a kernel module by name
pub fn unload_module(name: &str, _flags: ModuleFlags) -> ModuleResult<()> {
    let mut modules = MODULES.lock();

    // Find the module
    let idx = modules
        .iter()
        .position(|m| m.name == name)
        .ok_or(ModuleError::NotFound)?;

    // Check if in use
    if modules[idx].ref_count > 0 {
        return Err(ModuleError::InUse);
    }

    // Check if any other modules depend on this one
    for m in modules.iter() {
        if m.dependents.contains(&String::from(name)) {
            return Err(ModuleError::InUse);
        }
    }

    // Get module info before removing
    let module = modules.remove(idx);

    // Call cleanup function
    if let Some(cleanup_addr) = module.cleanup_fn {
        let cleanup: ModuleCleanupFn = unsafe { core::mem::transmute(cleanup_addr) };
        cleanup();
    }

    // Free module memory
    free_module_memory(module.base_addr as *mut u8, module.size);

    Ok(())
}

/// Parse module info from .modinfo section
fn parse_modinfo(data: &[u8]) -> ModuleResult<ModuleInfo> {
    // Module info is typically stored as the ModuleInfo struct
    // For simplicity, we just return a default if parsing fails
    if data.len() >= core::mem::size_of::<ModuleInfo>() {
        let info = unsafe { &*(data.as_ptr() as *const ModuleInfo) };
        Ok(ModuleInfo {
            name: info.name,
            version: info.version,
            author: info.author,
            description: info.description,
            license: info.license,
            depends: info.depends,
        })
    } else {
        Err(ModuleError::MissingSection)
    }
}

/// Get section name from string table
fn get_section_name(strtab: &[u8], offset: usize) -> &str {
    get_string(strtab, offset)
}

/// Get null-terminated string from buffer
fn get_string(buf: &[u8], offset: usize) -> &str {
    let start = offset;
    let mut end = offset;
    while end < buf.len() && buf[end] != 0 {
        end += 1;
    }
    core::str::from_utf8(&buf[start..end]).unwrap_or("")
}

/// Find symbol address in loaded module
fn find_symbol_addr(symtab: &[Sym64], strtab: &[u8], name: &str, base: usize) -> Option<usize> {
    for sym in symtab {
        if sym.st_shndx != 0 {
            let sym_name = get_string(strtab, sym.st_name as usize);
            if sym_name == name {
                return Some(base + sym.st_value as usize);
            }
        }
    }
    None
}

/// Allocate memory for module (page-aligned, executable)
fn allocate_module_memory(size: usize) -> ModuleResult<*mut u8> {
    // In a real implementation, this would use the memory allocator
    // to get executable memory pages
    use alloc::alloc::{Layout, alloc};

    let layout = Layout::from_size_align(size, 4096).map_err(|_| ModuleError::OutOfMemory)?;

    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        return Err(ModuleError::OutOfMemory);
    }

    Ok(ptr)
}

/// Free module memory
fn free_module_memory(ptr: *mut u8, size: usize) {
    use alloc::alloc::{Layout, dealloc};

    if let Ok(layout) = Layout::from_size_align(size, 4096) {
        unsafe { dealloc(ptr, layout) };
    }
}
