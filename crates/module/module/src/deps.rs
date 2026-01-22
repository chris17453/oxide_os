//! Module dependency management
//!
//! Handles module dependencies and load ordering.

use alloc::string::String;
use alloc::vec::Vec;

use crate::kobject::{MODULES, ModuleInfo, ModuleState};
use crate::{ModuleError, ModuleFlags, ModuleResult};

/// Dependency resolver for module loading
pub struct DependencyResolver {
    /// Modules to load in order
    load_order: Vec<String>,
    /// Modules being visited (for cycle detection)
    visiting: Vec<String>,
}

impl DependencyResolver {
    /// Create a new dependency resolver
    pub fn new() -> Self {
        DependencyResolver {
            load_order: Vec::new(),
            visiting: Vec::new(),
        }
    }

    /// Resolve dependencies for a module
    ///
    /// Returns the list of modules to load in order (dependencies first).
    pub fn resolve(&mut self, modinfo: &ModuleInfo) -> ModuleResult<Vec<String>> {
        self.load_order.clear();
        self.visiting.clear();

        self.resolve_recursive(modinfo)?;

        Ok(self.load_order.clone())
    }

    fn resolve_recursive(&mut self, modinfo: &ModuleInfo) -> ModuleResult<()> {
        let name = String::from(modinfo.name);

        // Check for cycles
        if self.visiting.contains(&name) {
            return Err(ModuleError::DependencyMissing);
        }

        // Already in load order
        if self.load_order.contains(&name) {
            return Ok(());
        }

        self.visiting.push(name.clone());

        // Process dependencies first
        for dep_name in modinfo.depends {
            // Check if dependency is already loaded
            let modules = MODULES.lock();
            let is_loaded = modules
                .iter()
                .any(|m| m.name == *dep_name && m.state == ModuleState::Live);
            drop(modules);

            if !is_loaded {
                // Dependency not loaded - in a real system we would
                // load it from disk here. For now, return error.
                return Err(ModuleError::DependencyMissing);
            }
        }

        self.visiting.retain(|n| n != &name);
        self.load_order.push(name);

        Ok(())
    }
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve dependencies for a module
///
/// Ensures all dependencies are loaded before allowing this module to load.
pub fn resolve_dependencies(modinfo: &ModuleInfo, flags: ModuleFlags) -> ModuleResult<()> {
    if flags.contains(ModuleFlags::FORCE) {
        // Force flag bypasses dependency checks
        return Ok(());
    }

    let modules = MODULES.lock();

    for dep_name in modinfo.depends {
        let dep_loaded = modules
            .iter()
            .any(|m| m.name == *dep_name && m.state == ModuleState::Live);

        if !dep_loaded {
            return Err(ModuleError::DependencyMissing);
        }
    }

    Ok(())
}

/// Check if a module can be unloaded
///
/// A module can be unloaded if:
/// - It's not in use (ref_count == 0)
/// - No other modules depend on it
pub fn can_unload(name: &str) -> ModuleResult<bool> {
    let modules = MODULES.lock();

    // Find the module
    let module = modules
        .iter()
        .find(|m| m.name == name)
        .ok_or(ModuleError::NotFound)?;

    // Check ref count
    if module.ref_count > 0 {
        return Ok(false);
    }

    // Check if any other modules depend on this one
    for m in modules.iter() {
        if m.name != name {
            for dep in &m.dependents {
                if dep == name {
                    return Ok(false);
                }
            }
        }
    }

    Ok(true)
}

/// Get the list of modules that depend on the given module
pub fn get_dependents(name: &str) -> Vec<String> {
    let modules = MODULES.lock();
    let mut dependents = Vec::new();

    for m in modules.iter() {
        if m.dependents.contains(&String::from(name)) {
            dependents.push(m.name.clone());
        }
    }

    dependents
}

/// Increment the reference count for a module
pub fn module_get(name: &str) -> ModuleResult<()> {
    let mut modules = MODULES.lock();

    let module = modules
        .iter_mut()
        .find(|m| m.name == name)
        .ok_or(ModuleError::NotFound)?;

    module.ref_count += 1;
    Ok(())
}

/// Decrement the reference count for a module
pub fn module_put(name: &str) -> ModuleResult<()> {
    let mut modules = MODULES.lock();

    let module = modules
        .iter_mut()
        .find(|m| m.name == name)
        .ok_or(ModuleError::NotFound)?;

    if module.ref_count > 0 {
        module.ref_count -= 1;
    }
    Ok(())
}
