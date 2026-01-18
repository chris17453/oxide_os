//! Dynamic Linker/Loader Implementation
//!
//! Provides dlopen/dlsym API for self-hosting support.

#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(unused_imports)]
#![allow(non_camel_case_types)]
#![allow(unsafe_attr_outside_unsafe)]

extern crate alloc;

pub mod elf;
pub mod symbol;
pub mod reloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::{c_int, c_void, c_char};
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

pub use elf::{ElfFile, ElfHeader, ProgramHeader, SectionHeader, Symbol as ElfSymbol};
pub use symbol::{SymbolTable, SymbolInfo};
pub use reloc::{Relocation, RelocationType, apply_relocation};

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

    // In a real implementation, this would:
    // 1. Read the ELF file from disk
    // 2. Map it into memory
    // 3. Perform relocations
    // 4. Run initializers

    // For now, create a placeholder
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);

    let lib = LoadedLibrary {
        handle,
        name,
        base_addr: 0,
        size: 0,
        symbols: SymbolTable::new(),
        refcount: 1,
        flags: flag,
        dependencies: Vec::new(),
        init_func: None,
        fini_func: None,
        init_array: Vec::new(),
        fini_array: Vec::new(),
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
