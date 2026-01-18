//! Module tracking and management
//!
//! Kernel objects for tracking loaded modules.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Global list of loaded modules
pub static MODULES: Mutex<Vec<Module>> = Mutex::new(Vec::new());

/// Module state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleState {
    /// Module is loading
    Coming,
    /// Module is live and active
    Live,
    /// Module is being removed
    Going,
    /// Module is unloaded
    Unloaded,
}

/// Module metadata (embedded in .modinfo)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ModuleInfo {
    /// Module name
    pub name: &'static str,
    /// Module version
    pub version: &'static str,
    /// Module author
    pub author: &'static str,
    /// Module description
    pub description: &'static str,
    /// License (e.g., "GPL", "MIT")
    pub license: &'static str,
    /// Module dependencies
    pub depends: &'static [&'static str],
}

impl ModuleInfo {
    /// Check if module has GPL-compatible license
    pub fn is_gpl_compatible(&self) -> bool {
        matches!(
            self.license,
            "GPL" | "GPL v2" | "GPL and additional rights" | "Dual BSD/GPL"
                | "Dual MIT/GPL" | "Dual MPL/GPL"
        )
    }
}

impl Default for ModuleInfo {
    fn default() -> Self {
        ModuleInfo {
            name: "",
            version: "0.0.0",
            author: "Unknown",
            description: "",
            license: "Proprietary",
            depends: &[],
        }
    }
}

/// Loaded module tracking structure
pub struct Module {
    /// Module name
    pub name: String,
    /// Module version
    pub version: String,
    /// Current state
    pub state: ModuleState,
    /// Base address in memory
    pub base_addr: usize,
    /// Total size in memory
    pub size: usize,
    /// Init function address
    pub init_fn: Option<usize>,
    /// Cleanup function address
    pub cleanup_fn: Option<usize>,
    /// Reference count (usage count)
    pub ref_count: u32,
    /// Modules that depend on this one
    pub dependents: Vec<String>,
}

impl Module {
    /// Create a new module entry
    pub fn new(name: String, base_addr: usize, size: usize) -> Self {
        Module {
            name,
            version: String::from("0.0.0"),
            state: ModuleState::Coming,
            base_addr,
            size,
            init_fn: None,
            cleanup_fn: None,
            ref_count: 0,
            dependents: Vec::new(),
        }
    }

    /// Get the module's memory range
    pub fn memory_range(&self) -> core::ops::Range<usize> {
        self.base_addr..self.base_addr + self.size
    }

    /// Check if an address is within this module
    pub fn contains_addr(&self, addr: usize) -> bool {
        self.memory_range().contains(&addr)
    }

    /// Mark module as live
    pub fn set_live(&mut self) {
        self.state = ModuleState::Live;
    }

    /// Mark module as going (unloading)
    pub fn set_going(&mut self) {
        self.state = ModuleState::Going;
    }
}

/// Get a list of all loaded module names
pub fn list_modules() -> Vec<String> {
    MODULES.lock().iter().map(|m| m.name.clone()).collect()
}

/// Get module information by name
pub fn get_module_info(name: &str) -> Option<(String, ModuleState, usize, usize)> {
    let modules = MODULES.lock();
    modules.iter().find(|m| m.name == name).map(|m| {
        (m.version.clone(), m.state, m.base_addr, m.size)
    })
}

/// Find which module contains an address
pub fn find_module_by_addr(addr: usize) -> Option<String> {
    let modules = MODULES.lock();
    modules.iter().find(|m| m.contains_addr(addr)).map(|m| m.name.clone())
}

/// Get the number of loaded modules
pub fn module_count() -> usize {
    MODULES.lock().len()
}

/// Check if a module is loaded
pub fn is_module_loaded(name: &str) -> bool {
    let modules = MODULES.lock();
    modules.iter().any(|m| m.name == name && m.state == ModuleState::Live)
}
