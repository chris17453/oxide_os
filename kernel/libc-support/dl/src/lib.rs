//! Dynamic Linker/Loader Implementation
//!
//! Provides dlopen/dlsym API for self-hosting support.

#![no_std]
#![allow(unused)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types)]
#![allow(unsafe_attr_outside_unsafe)]

extern crate alloc;

pub mod elf;
pub mod reloc;
pub mod symbol;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

pub use elf::{ElfFile, ElfHeader, ProgramHeader, SectionHeader, Symbol as ElfSymbol};
pub use reloc::{apply_relocation, Relocation, RelocationType};
pub use symbol::{SymbolInfo, SymbolTable};

/// dlopen flags
pub mod flags {
    /// Perform lazy binding
    pub const RTLD_LAZY: i32 = 0x0001;
    /// Perform immediate binding
    pub const RTLD_NOW: i32 = 0x0002;
    /// Symbols defined are not made available to other objects
    pub const RTLD_LOCAL: i32 = 0x0000;
    /// Symbols defined are made available to other objects
    pub const RTLD_GLOBAL: i32 = 0x0100;
    /// Don't load the library, just check if it exists
    pub const RTLD_NOLOAD: i32 = 0x0004;
    /// Don't delete library on close
    pub const RTLD_NODELETE: i32 = 0x1000;
    /// Place library at start of search order
    pub const RTLD_DEEPBIND: i32 = 0x0008;
}

/// Special dlsym handles
pub const RTLD_DEFAULT: *mut c_void = 0 as *mut c_void;
pub const RTLD_NEXT: *mut c_void = (-1isize) as *mut c_void;

/// Error codes
pub const ESUCCESS: c_int = 0;
pub const EINVAL: c_int = 22;
pub const ENOENT: c_int = 2;
pub const ENOMEM: c_int = 12;
pub const ENOEXEC: c_int = 8;

/// Library handle
pub type DlHandle = u64;

/// Loaded library
struct LoadedLibrary {
    /// Handle ID
    handle: DlHandle,
    /// Library name/path
    name: String,
    /// Base load address
    base_addr: usize,
    /// Size of loaded image
    size: usize,
    /// Symbol table
    symbols: SymbolTable,
    /// Reference count
    refcount: u64,
    /// Flags used when loading
    flags: i32,
    /// Dependencies (other library handles)
    dependencies: Vec<DlHandle>,
    /// Initialization function address
    init_func: Option<usize>,
    /// Finalization function address
    fini_func: Option<usize>,
    /// Init array
    init_array: Vec<usize>,
    /// Fini array
    fini_array: Vec<usize>,
}

/// Global library registry
static LIBRARIES: Mutex<Option<BTreeMap<DlHandle, LoadedLibrary>>> = Mutex::new(None);

/// Next handle ID
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Last error message
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn get_libraries() -> spin::MutexGuard<'static, Option<BTreeMap<DlHandle, LoadedLibrary>>> {
    let mut libs = LIBRARIES.lock();
    if libs.is_none() {
        *libs = Some(BTreeMap::new());
    }
    libs
}

fn set_error(msg: &str) {
    let mut err = LAST_ERROR.lock();
    *err = Some(String::from(msg));
}

/// — IronGhost: Callback type for reading ELF files from the VFS.
/// The kernel registers this at boot so the dl crate stays decoupled
/// from VFS internals. Returns the entire file as a Vec<u8>, or None.
type ElfLoaderFn = fn(&str) -> Option<Vec<u8>>;

/// — IronGhost: Callback for allocating virtual memory for LOAD segments.
/// Returns a base address for the region, or None on failure.
type AllocRegionFn = fn(usize) -> Option<usize>;

static ELF_LOADER: Mutex<Option<ElfLoaderFn>> = Mutex::new(None);
static ALLOC_REGION: Mutex<Option<AllocRegionFn>> = Mutex::new(None);

/// Register the ELF file loader callback
pub fn register_elf_loader(loader: ElfLoaderFn) {
    *ELF_LOADER.lock() = Some(loader);
}

/// Register the memory allocation callback
pub fn register_alloc_region(alloc: AllocRegionFn) {
    *ALLOC_REGION.lock() = Some(alloc);
}

fn load_elf_file(name: &str) -> Option<Vec<u8>> {
    let loader = (*ELF_LOADER.lock())?;
    loader(name)
}

fn allocate_load_region(size: usize) -> Option<usize> {
    let alloc = (*ALLOC_REGION.lock())?;
    alloc(size)
}

use elf::SectionType;

/// Open a dynamic library
///
/// # Safety
/// The filename must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void {
    // If filename is NULL, return handle to main program
    if filename.is_null() {
        return 1 as *mut c_void; // Special handle for main program
    }

    // Convert filename
    let name = {
        let mut len = 0;
        while *filename.add(len) != 0 {
            len += 1;
        }
        let slice = core::slice::from_raw_parts(filename as *const u8, len);
        match core::str::from_utf8(slice) {
            Ok(s) => String::from(s),
            Err(_) => {
                set_error("Invalid filename encoding");
                return core::ptr::null_mut();
            }
        }
    };

    // Check if already loaded
    let existing_handle: Option<DlHandle> = {
        let libs = get_libraries();
        if let Some(ref map) = *libs {
            map.iter()
                .find(|(_, lib)| lib.name == name)
                .map(|(&handle, _)| handle)
        } else {
            None
        }
    };

    if let Some(handle) = existing_handle {
        // Already loaded - increment refcount
        let mut libs = get_libraries();
        if let Some(ref mut map) = *libs {
            if let Some(lib) = map.get_mut(&handle) {
                lib.refcount += 1;
                return handle as *mut c_void;
            }
        }
    }

    // Just checking if exists
    if flag & flags::RTLD_NOLOAD != 0 {
        set_error("Library not loaded");
        return core::ptr::null_mut();
    }

    // — IronGhost: Read the ELF file from the VFS, map LOAD segments into
    // the current process address space, process relocations, and run
    // DT_INIT / DT_INIT_ARRAY constructors.
    //
    // Step 1: Read the ELF file into a buffer via the loader callback.
    // The dl crate is no_std and doesn't directly depend on VFS —
    // the kernel registers a loader callback at init time.
    let elf_data = match load_elf_file(&name) {
        Some(data) => data,
        None => {
            set_error("Failed to read library file");
            return core::ptr::null_mut();
        }
    };

    // Step 2: Parse the ELF
    let elf = match ElfFile::parse(&elf_data) {
        Some(e) => e,
        None => {
            set_error("Invalid ELF file");
            return core::ptr::null_mut();
        }
    };

    if !elf.header.is_shared_object() {
        set_error("Not a shared object");
        return core::ptr::null_mut();
    }

    // Step 3: Calculate load range and allocate virtual memory
    let load_size = elf.load_size();
    if load_size == 0 {
        set_error("No loadable segments");
        return core::ptr::null_mut();
    }

    let base_addr = match allocate_load_region(load_size) {
        Some(addr) => addr,
        None => {
            set_error("Failed to allocate memory for library");
            return core::ptr::null_mut();
        }
    };

    // Step 4: Map LOAD segments into memory
    let min_vaddr = elf.program_headers.iter()
        .filter(|ph| ph.is_loadable())
        .map(|ph| ph.p_vaddr)
        .min()
        .unwrap_or(0) as usize;

    for ph in &elf.program_headers {
        if !ph.is_loadable() {
            continue;
        }

        let seg_offset = (ph.p_vaddr as usize) - min_vaddr;
        let dest = base_addr + seg_offset;

        // — IronGhost: Copy file data into the mapped region
        let file_offset = ph.p_offset as usize;
        let file_size = ph.p_filesz as usize;
        let mem_size = ph.p_memsz as usize;

        if file_offset + file_size <= elf_data.len() {
            core::ptr::copy_nonoverlapping(
                elf_data[file_offset..].as_ptr(),
                dest as *mut u8,
                file_size,
            );
        }

        // Zero-fill BSS (.bss is p_memsz > p_filesz)
        if mem_size > file_size {
            core::ptr::write_bytes(
                (dest + file_size) as *mut u8,
                0,
                mem_size - file_size,
            );
        }
    }

    // Step 5: Build symbol table from .dynsym + .dynstr
    let mut symbols = SymbolTable::new();
    let mut init_func = None;
    let mut fini_func = None;
    let mut init_array = Vec::new();
    let mut fini_array = Vec::new();

    // — IronGhost: Walk section headers for DYNSYM, STRTAB, RELA, INIT_ARRAY
    for sh in &elf.section_headers {
        if sh.sh_type == SectionType::Dynsym as u32 {
            // Parse dynamic symbol table
            let str_sh = &elf.section_headers[sh.sh_link as usize];
            let strtab_off = str_sh.sh_offset as usize;
            let strtab_end = strtab_off + str_sh.sh_size as usize;

            let symtab_off = sh.sh_offset as usize;
            let entry_size = if sh.sh_entsize > 0 { sh.sh_entsize as usize } else { 24 };
            let count = sh.sh_size as usize / entry_size;

            for i in 0..count {
                let off = symtab_off + i * entry_size;
                if let Some(sym) = elf::Symbol::parse(&elf_data[off..]) {
                    if sym.st_name > 0 && sym.st_shndx != 0 {
                        // Extract symbol name from string table
                        let name_off = strtab_off + sym.st_name as usize;
                        if name_off < strtab_end {
                            let mut name_end = name_off;
                            while name_end < strtab_end && elf_data[name_end] != 0 {
                                name_end += 1;
                            }
                            if let Ok(sym_name) = core::str::from_utf8(&elf_data[name_off..name_end]) {
                                let binding = match sym.binding() {
                                    1 => symbol::SymbolBinding::Global,
                                    2 => symbol::SymbolBinding::Weak,
                                    _ => symbol::SymbolBinding::Local,
                                };
                                let sym_type = match sym.sym_type() {
                                    1 => symbol::SymbolType::Object,
                                    2 => symbol::SymbolType::Function,
                                    _ => symbol::SymbolType::NoType,
                                };
                                symbols.add(SymbolInfo::new(
                                    String::from(sym_name),
                                    sym.st_value as usize,
                                    sym.st_size as usize,
                                    binding,
                                    sym_type,
                                    sym.st_shndx,
                                ));
                            }
                        }
                    }
                }
            }
        } else if sh.sh_type == SectionType::Rela as u32 {
            // — IronGhost: Apply relocations
            let rela_off = sh.sh_offset as usize;
            let entry_size = if sh.sh_entsize > 0 { sh.sh_entsize as usize } else { 24 };
            let iter = reloc::RelaIterator::new(
                &elf_data[rela_off..rela_off + sh.sh_size as usize],
                entry_size,
            );

            for r in iter {
                // — IronGhost: For RELATIVE relocations, sym_value is 0 (base-relative).
                // For GLOB_DAT/JUMP_SLOT, we'd need to resolve the symbol.
                // For now, handle RELATIVE (the most common in position-independent code).
                let _ = reloc::apply_relocation(base_addr, &r, 0, 0);
            }
        } else if sh.sh_type == SectionType::InitArray as u32 {
            let off = sh.sh_offset as usize;
            let count = sh.sh_size as usize / 8; // 64-bit function pointers
            for i in 0..count {
                let ptr_off = off + i * 8;
                if ptr_off + 8 <= elf_data.len() {
                    let func_addr = u64::from_le_bytes([
                        elf_data[ptr_off], elf_data[ptr_off+1],
                        elf_data[ptr_off+2], elf_data[ptr_off+3],
                        elf_data[ptr_off+4], elf_data[ptr_off+5],
                        elf_data[ptr_off+6], elf_data[ptr_off+7],
                    ]);
                    if func_addr != 0 {
                        init_array.push(base_addr + func_addr as usize - min_vaddr);
                    }
                }
            }
        } else if sh.sh_type == SectionType::FiniArray as u32 {
            let off = sh.sh_offset as usize;
            let count = sh.sh_size as usize / 8;
            for i in 0..count {
                let ptr_off = off + i * 8;
                if ptr_off + 8 <= elf_data.len() {
                    let func_addr = u64::from_le_bytes([
                        elf_data[ptr_off], elf_data[ptr_off+1],
                        elf_data[ptr_off+2], elf_data[ptr_off+3],
                        elf_data[ptr_off+4], elf_data[ptr_off+5],
                        elf_data[ptr_off+6], elf_data[ptr_off+7],
                    ]);
                    if func_addr != 0 {
                        fini_array.push(base_addr + func_addr as usize - min_vaddr);
                    }
                }
            }
        }
    }

    symbols.rebuild_offset_index();

    // Step 6: Run constructors
    for &init_addr in &init_array {
        let f: extern "C" fn() = core::mem::transmute(init_addr);
        f();
    }

    // Step 7: Register the loaded library
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);

    let lib = LoadedLibrary {
        handle,
        name,
        base_addr,
        size: load_size,
        symbols,
        refcount: 1,
        flags: flag,
        dependencies: Vec::new(),
        init_func,
        fini_func,
        init_array,
        fini_array,
    };

    {
        let mut libs = get_libraries();
        if let Some(ref mut map) = *libs {
            map.insert(handle, lib);
        }
    }

    handle as *mut c_void
}

/// Close a dynamic library
#[no_mangle]
pub unsafe extern "C" fn dlclose(handle: *mut c_void) -> c_int {
    let handle_id = handle as DlHandle;

    let should_unload = {
        let mut libs = get_libraries();
        if let Some(ref mut map) = *libs {
            if let Some(lib) = map.get_mut(&handle_id) {
                lib.refcount -= 1;
                lib.refcount == 0 && (lib.flags & flags::RTLD_NODELETE == 0)
            } else {
                set_error("Invalid handle");
                return -1;
            }
        } else {
            set_error("Invalid handle");
            return -1;
        }
    };

    if should_unload {
        // Run finalizers
        {
            let libs = get_libraries();
            if let Some(ref map) = *libs {
                if let Some(lib) = map.get(&handle_id) {
                    // Call fini_array in reverse order
                    for &fini in lib.fini_array.iter().rev() {
                        let f: extern "C" fn() = core::mem::transmute(fini);
                        f();
                    }

                    // Call _fini
                    if let Some(fini) = lib.fini_func {
                        let f: extern "C" fn() = core::mem::transmute(fini);
                        f();
                    }
                }
            }
        }

        // Remove from registry
        let mut libs = get_libraries();
        if let Some(ref mut map) = *libs {
            map.remove(&handle_id);
        }
    }

    ESUCCESS
}

/// Look up a symbol
///
/// # Safety
/// The symbol must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void {
    if symbol.is_null() {
        set_error("NULL symbol name");
        return core::ptr::null_mut();
    }

    // Convert symbol name
    let sym_name = {
        let mut len = 0;
        while *symbol.add(len) != 0 {
            len += 1;
        }
        let slice = core::slice::from_raw_parts(symbol as *const u8, len);
        match core::str::from_utf8(slice) {
            Ok(s) => s,
            Err(_) => {
                set_error("Invalid symbol name encoding");
                return core::ptr::null_mut();
            }
        }
    };

    // Special handles
    if handle == RTLD_DEFAULT {
        // Search all loaded libraries
        let libs = get_libraries();
        if let Some(ref map) = *libs {
            for (_, lib) in map.iter() {
                if let Some(info) = lib.symbols.find(sym_name) {
                    return (lib.base_addr + info.offset) as *mut c_void;
                }
            }
        }
        set_error("Symbol not found");
        return core::ptr::null_mut();
    }

    if handle == RTLD_NEXT {
        // Search libraries loaded after caller
        // This requires knowing the caller's address - complex to implement
        set_error("RTLD_NEXT not supported");
        return core::ptr::null_mut();
    }

    // Specific handle
    let handle_id = handle as DlHandle;

    let libs = get_libraries();
    if let Some(ref map) = *libs {
        if let Some(lib) = map.get(&handle_id) {
            if let Some(info) = lib.symbols.find(sym_name) {
                return (lib.base_addr + info.offset) as *mut c_void;
            }
            set_error("Symbol not found");
            return core::ptr::null_mut();
        }
    }

    set_error("Invalid handle");
    core::ptr::null_mut()
}

/// Get last error message
#[no_mangle]
pub extern "C" fn dlerror() -> *const c_char {
    let mut err = LAST_ERROR.lock();
    if let Some(ref msg) = *err {
        // Return pointer to error message
        // Note: This leaks memory in a real implementation
        // A proper implementation would use a static buffer
        let ptr = msg.as_ptr();
        *err = None;
        ptr as *const c_char
    } else {
        core::ptr::null()
    }
}

/// Get information about a symbol
#[repr(C)]
pub struct Dl_info {
    /// Pathname of shared object
    pub dli_fname: *const c_char,
    /// Base address of shared object
    pub dli_fbase: *mut c_void,
    /// Name of nearest symbol
    pub dli_sname: *const c_char,
    /// Address of nearest symbol
    pub dli_saddr: *mut c_void,
}

/// Get information about an address
#[no_mangle]
pub unsafe extern "C" fn dladdr(addr: *const c_void, info: *mut Dl_info) -> c_int {
    if info.is_null() {
        return 0;
    }

    let addr_val = addr as usize;

    let libs = get_libraries();
    if let Some(ref map) = *libs {
        for (_, lib) in map.iter() {
            // Check if address is in this library
            if addr_val >= lib.base_addr && addr_val < lib.base_addr + lib.size {
                (*info).dli_fname = lib.name.as_ptr() as *const c_char;
                (*info).dli_fbase = lib.base_addr as *mut c_void;

                // Find nearest symbol
                if let Some(sym_info) = lib.symbols.find_nearest(addr_val - lib.base_addr) {
                    (*info).dli_sname = sym_info.name.as_ptr() as *const c_char;
                    (*info).dli_saddr = (lib.base_addr + sym_info.offset) as *mut c_void;
                } else {
                    (*info).dli_sname = core::ptr::null();
                    (*info).dli_saddr = core::ptr::null_mut();
                }

                return 1;
            }
        }
    }

    // Address not found in any loaded library
    (*info).dli_fname = core::ptr::null();
    (*info).dli_fbase = core::ptr::null_mut();
    (*info).dli_sname = core::ptr::null();
    (*info).dli_saddr = core::ptr::null_mut();

    0
}
