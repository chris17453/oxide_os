//! Link map — tracking loaded shared objects
//!
//! — PatchBay: the link_map is the dynamic linker's inventory of every loaded
//! object (main executable + all .so files). It's a singly-linked list that
//! debuggers (GDB) use to enumerate loaded libraries, and the linker uses
//! for symbol resolution scope ordering.
//!
//! Linux's struct link_map is part of the ABI (r_debug/DT_DEBUG). We implement
//! a simplified version that's compatible enough for GDB to work.

use crate::elf::DynamicInfo;

/// Maximum number of loaded objects (main exe + shared libraries)
/// — PatchBay: 64 should be enough for anyone. If you need more than 64
/// shared libraries on a hobby OS, you're doing something deeply wrong.
pub const MAX_LOADED_OBJECTS: usize = 64;

/// A loaded ELF object in the link map
/// — PatchBay: tracks everything needed for symbol resolution and cleanup.
#[derive(Clone)]
pub struct LoadedObject {
    /// Name of the object (e.g., "libc.so" or "[main]")
    pub name: [u8; 128],
    pub name_len: usize,

    /// Load base address (actual load address - min vaddr)
    pub base: u64,

    /// Parsed dynamic section info (symbol tables, relocation tables, etc.)
    pub dyn_info: Option<DynamicInfo>,

    /// Is this the main executable? (affects symbol resolution priority)
    pub is_main: bool,

    /// Is this object initialized? (DT_INIT/DT_INIT_ARRAY has run)
    pub initialized: bool,
}

impl LoadedObject {
    pub fn new() -> Self {
        Self {
            name: [0; 128],
            name_len: 0,
            base: 0,
            dyn_info: None,
            is_main: false,
            initialized: false,
        }
    }

    pub fn set_name(&mut self, name: &[u8]) {
        let len = core::cmp::min(name.len(), 127);
        self.name[..len].copy_from_slice(&name[..len]);
        self.name_len = len;
    }
}

/// Global link map — fixed-capacity array of loaded objects
/// — PatchBay: no heap allocation. The dynamic linker runs before the allocator
/// is set up (for the main exe's perspective), so everything is static.
pub struct LinkMap {
    pub objects: [Option<LoadedObject>; MAX_LOADED_OBJECTS],
    pub count: usize,
}

impl LinkMap {
    pub const fn new() -> Self {
        // — PatchBay: const fn init with None array. Can't use [None; N] because
        // LoadedObject doesn't impl Copy (it has Option<DynamicInfo> with Vec-like fields).
        // Manual init it is.
        const NONE: Option<LoadedObject> = None;
        Self {
            objects: [NONE; MAX_LOADED_OBJECTS],
            count: 0,
        }
    }

    /// Add a loaded object to the link map
    pub fn add(&mut self, obj: LoadedObject) -> Option<usize> {
        if self.count >= MAX_LOADED_OBJECTS {
            return None;
        }
        let idx = self.count;
        self.objects[idx] = Some(obj);
        self.count += 1;
        Some(idx)
    }

    /// Iterate over loaded objects
    pub fn iter(&self) -> impl Iterator<Item = &LoadedObject> {
        self.objects[..self.count].iter().filter_map(|o| o.as_ref())
    }

    /// Get a loaded object by index
    pub fn get(&self, idx: usize) -> Option<&LoadedObject> {
        self.objects.get(idx)?.as_ref()
    }

    /// Get a mutable loaded object by index
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut LoadedObject> {
        self.objects.get_mut(idx)?.as_mut()
    }
}

/// Global link map instance
/// — PatchBay: static mutable because the dynamic linker is single-threaded
/// during startup. No concurrency concerns until we hand off to the main exe.
static mut LINK_MAP: LinkMap = LinkMap::new();

/// Get the global link map
/// — PatchBay: unsafe because it's a static mut. Caller must ensure single-threaded access.
pub unsafe fn link_map() -> &'static mut LinkMap {
    unsafe { &mut *(&raw mut LINK_MAP) }
}
