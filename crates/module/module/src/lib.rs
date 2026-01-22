//! Loadable Kernel Module support for OXIDE OS
//!
//! This crate provides:
//! - Module binary format parsing (relocatable ELF)
//! - Module loading with relocation support
//! - Symbol resolution (kernel exports)
//! - Module dependency management
//! - Module lifecycle (init/exit hooks)

#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]

extern crate alloc;

pub mod deps;
pub mod kobject;
pub mod loader;
pub mod reloc;
pub mod symbol;

pub use deps::DependencyResolver;
pub use kobject::{Module, ModuleInfo, ModuleState};
pub use loader::{load_module, unload_module};
pub use symbol::{KernelSymbol, SymbolTable};

/// Module init function type
pub type ModuleInitFn = extern "C" fn() -> i32;

/// Module cleanup function type
pub type ModuleCleanupFn = extern "C" fn();

/// Module load flags
#[derive(Debug, Clone, Copy)]
pub struct ModuleFlags(u32);

impl ModuleFlags {
    pub const NONE: ModuleFlags = ModuleFlags(0);
    /// Force load even if version mismatch
    pub const FORCE: ModuleFlags = ModuleFlags(1 << 0);
    /// Allow loading on a tainted kernel
    pub const TAINT: ModuleFlags = ModuleFlags(1 << 1);
    /// Module should not be unloaded
    pub const PERMANENT: ModuleFlags = ModuleFlags(1 << 2);

    pub fn contains(&self, other: ModuleFlags) -> bool {
        (self.0 & other.0) != 0
    }
}

/// Module errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleError {
    /// Invalid ELF format
    InvalidFormat,
    /// Missing required section
    MissingSection,
    /// Unknown relocation type
    UnknownRelocation,
    /// Symbol not found
    SymbolNotFound,
    /// Module already loaded
    AlreadyLoaded,
    /// Module not found
    NotFound,
    /// Module is in use
    InUse,
    /// Dependency not satisfied
    DependencyMissing,
    /// Version mismatch
    VersionMismatch,
    /// Memory allocation failed
    OutOfMemory,
    /// Module init failed
    InitFailed,
    /// Permission denied
    PermissionDenied,
}

impl core::fmt::Display for ModuleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ModuleError::InvalidFormat => write!(f, "Invalid ELF format"),
            ModuleError::MissingSection => write!(f, "Missing required section"),
            ModuleError::UnknownRelocation => write!(f, "Unknown relocation type"),
            ModuleError::SymbolNotFound => write!(f, "Symbol not found"),
            ModuleError::AlreadyLoaded => write!(f, "Module already loaded"),
            ModuleError::NotFound => write!(f, "Module not found"),
            ModuleError::InUse => write!(f, "Module is in use"),
            ModuleError::DependencyMissing => write!(f, "Dependency not satisfied"),
            ModuleError::VersionMismatch => write!(f, "Version mismatch"),
            ModuleError::OutOfMemory => write!(f, "Out of memory"),
            ModuleError::InitFailed => write!(f, "Module init failed"),
            ModuleError::PermissionDenied => write!(f, "Permission denied"),
        }
    }
}

/// Result type for module operations
pub type ModuleResult<T> = Result<T, ModuleError>;

/// Macro to define a kernel module
#[macro_export]
macro_rules! module {
    (
        name: $name:expr,
        init: $init:expr,
        exit: $exit:expr,
        author: $author:expr,
        description: $desc:expr,
        license: $license:expr
        $(, depends: [$($dep:expr),* $(,)?])?
    ) => {
        #[used]
        #[link_section = ".modinfo"]
        static __MODULE_INFO: $crate::ModuleInfo = $crate::ModuleInfo {
            name: $name,
            version: env!("CARGO_PKG_VERSION"),
            author: $author,
            description: $desc,
            license: $license,
            depends: &[$($($dep,)*)?],
        };

        #[unsafe(no_mangle)]
        pub extern "C" fn init_module() -> i32 {
            $init()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn cleanup_module() {
            $exit()
        }
    };
}
