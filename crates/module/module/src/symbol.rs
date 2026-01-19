//! Kernel symbol table and exports
//!
//! Manages exported kernel symbols that modules can resolve.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::RwLock;

/// Kernel symbol entry
#[derive(Debug, Clone, Copy)]
pub struct KernelSymbol {
    /// Symbol name
    pub name: &'static str,
    /// Symbol address
    pub addr: usize,
    /// Is this a GPL-only symbol?
    pub gpl_only: bool,
}

impl KernelSymbol {
    /// Create a new kernel symbol
    pub const fn new(name: &'static str, addr: usize) -> Self {
        KernelSymbol {
            name,
            addr,
            gpl_only: false,
        }
    }

    /// Create a GPL-only kernel symbol
    pub const fn gpl(name: &'static str, addr: usize) -> Self {
        KernelSymbol {
            name,
            addr,
            gpl_only: true,
        }
    }
}

/// Global kernel symbol table
static KERNEL_SYMBOLS: RwLock<SymbolTable> = RwLock::new(SymbolTable::new());

/// Symbol table for kernel and module symbols
pub struct SymbolTable {
    /// Kernel symbols (static, never removed)
    kernel_symbols: Vec<KernelSymbol>,
    /// Module symbols (dynamic, can be added/removed)
    module_symbols: BTreeMap<String, usize>,
}

impl SymbolTable {
    /// Create a new empty symbol table
    pub const fn new() -> Self {
        SymbolTable {
            kernel_symbols: Vec::new(),
            module_symbols: BTreeMap::new(),
        }
    }

    /// Register a kernel symbol
    pub fn register_kernel_symbol(&mut self, sym: KernelSymbol) {
        self.kernel_symbols.push(sym);
    }

    /// Register a module symbol
    pub fn register_module_symbol(&mut self, name: String, addr: usize) {
        self.module_symbols.insert(name, addr);
    }

    /// Unregister module symbols by address range
    pub fn unregister_module_symbols(&mut self, start: usize, end: usize) {
        self.module_symbols.retain(|_, &mut addr| {
            addr < start || addr >= end
        });
    }

    /// Look up a symbol by name
    pub fn lookup(&self, name: &str) -> Option<usize> {
        // First check kernel symbols
        for sym in &self.kernel_symbols {
            if sym.name == name {
                return Some(sym.addr);
            }
        }

        // Then check module symbols
        self.module_symbols.get(name).copied()
    }

    /// Look up a GPL symbol by name
    pub fn lookup_gpl(&self, name: &str, has_gpl_license: bool) -> Option<usize> {
        for sym in &self.kernel_symbols {
            if sym.name == name {
                if sym.gpl_only && !has_gpl_license {
                    return None;
                }
                return Some(sym.addr);
            }
        }
        self.module_symbols.get(name).copied()
    }

    /// Get the number of kernel symbols
    pub fn kernel_symbol_count(&self) -> usize {
        self.kernel_symbols.len()
    }

    /// Get the number of module symbols
    pub fn module_symbol_count(&self) -> usize {
        self.module_symbols.len()
    }

    /// Iterate over all kernel symbols
    pub fn kernel_symbols(&self) -> impl Iterator<Item = &KernelSymbol> {
        self.kernel_symbols.iter()
    }
}

/// Register a kernel symbol in the global table
pub fn register_kernel_symbol(sym: KernelSymbol) {
    KERNEL_SYMBOLS.write().register_kernel_symbol(sym);
}

/// Register a module symbol in the global table
pub fn register_module_symbol(name: String, addr: usize) {
    KERNEL_SYMBOLS.write().register_module_symbol(name, addr);
}

/// Look up a symbol in the global table
pub fn lookup_symbol(name: &str) -> Option<usize> {
    KERNEL_SYMBOLS.read().lookup(name)
}

/// Look up a symbol with GPL check
pub fn lookup_symbol_gpl(name: &str, has_gpl_license: bool) -> Option<usize> {
    KERNEL_SYMBOLS.read().lookup_gpl(name, has_gpl_license)
}

/// Macro to export a kernel symbol
///
/// Usage: `EXPORT_SYMBOL!(my_function);`
///
/// This registers the symbol for use by loadable modules.
#[macro_export]
macro_rules! EXPORT_SYMBOL {
    ($sym:ident) => {
        const _: () = {
            #[used]
            #[link_section = ".ksymtab"]
            static KSYM: $crate::symbol::KernelSymbol = $crate::symbol::KernelSymbol::new(
                stringify!($sym),
                $sym as *const () as usize,
            );
        };
    };
}

/// Macro to export a GPL-only kernel symbol
///
/// Usage: `EXPORT_SYMBOL_GPL!(my_gpl_function);`
///
/// This registers the symbol for use only by GPL-licensed modules.
#[macro_export]
macro_rules! EXPORT_SYMBOL_GPL {
    ($sym:ident) => {
        const _: () = {
            #[used]
            #[link_section = ".ksymtab.gpl"]
            static KSYM_GPL: $crate::symbol::KernelSymbol = $crate::symbol::KernelSymbol::gpl(
                stringify!($sym),
                $sym as *const () as usize,
            );
        };
    };
}
