//! Scratch Arena — a single 2MB AllocatePages bump allocator that replaces
//! the uefi-rs global_allocator. All temporary data (file reads, directory
//! listings, memory map buffers) lives here.
//!
//! save_mark() / restore_mark() for nested temporary allocations.
//! No free() — the whole arena dies when we exit_boot_services.
//!
//! — SableWire: one allocation to rule them all — 2MB of firmware-owned RAM,
//! bump-allocated, no fragmentation, no hidden state, no surprises

use super::types::*;
use super::boot_services::*;

/// Arena size — 2MB should be enough for any pre-boot file loading
/// — SableWire: if you need more than 2MB in a bootloader, you're doing it wrong
const SCRATCH_SIZE: usize = 2 * 1024 * 1024;
const SCRATCH_PAGES: usize = SCRATCH_SIZE / 4096;

/// The global scratch arena — initialized once in efi::init()
static mut SCRATCH: ScratchArena = ScratchArena {
    base: core::ptr::null_mut(),
    size: 0,
    offset: 0,
};

/// Bump allocator arena
/// — SableWire: the simplest allocator that could possibly work
pub struct ScratchArena {
    base: *mut u8,
    size: usize,
    offset: usize,
}

/// A saved position in the arena for nested allocations
#[derive(Clone, Copy)]
pub struct ScratchMark {
    offset: usize,
}

impl ScratchArena {
    /// Initialize the arena by allocating pages from UEFI boot services
    /// — SableWire: the one and only LOADER_DATA allocation we'll ever make (for temp data)
    pub unsafe fn init(bs: *mut EfiBootServices) -> bool {
        unsafe {
            let mut phys_addr: EfiPhysicalAddress = 0;
            let status = ((*bs).allocate_pages)(
                ALLOCATE_ANY_PAGES,
                EFI_LOADER_DATA,
                SCRATCH_PAGES,
                &mut phys_addr,
            );
            if efi_error(status) {
                return false;
            }

            // Zero the arena — start clean
            core::ptr::write_bytes(phys_addr as *mut u8, 0, SCRATCH_SIZE);

            SCRATCH.base = phys_addr as *mut u8;
            SCRATCH.size = SCRATCH_SIZE;
            SCRATCH.offset = 0;
            true
        }
    }
}

/// Save the current arena position — call before temporary work
/// — SableWire: checkpoint before you trash the arena with temp data
pub fn save_mark() -> ScratchMark {
    unsafe { ScratchMark { offset: SCRATCH.offset } }
}

/// Restore the arena to a saved position — frees everything allocated since the mark
/// — SableWire: the poor man's scope-based deallocation
pub fn restore_mark(mark: ScratchMark) {
    unsafe {
        SCRATCH.offset = mark.offset;
    }
}

/// Allocate `size` bytes from the scratch arena, 8-byte aligned.
/// Returns a mutable pointer to the allocation, or null if out of space.
/// — SableWire: bump, align, return — the fastest allocator in the West
pub fn alloc(size: usize) -> *mut u8 {
    unsafe {
        if SCRATCH.base.is_null() {
            return core::ptr::null_mut();
        }

        // Align to 8 bytes
        let aligned_offset = (SCRATCH.offset + 7) & !7;
        if aligned_offset + size > SCRATCH.size {
            return core::ptr::null_mut();
        }

        let ptr = SCRATCH.base.add(aligned_offset);
        SCRATCH.offset = aligned_offset + size;
        ptr
    }
}

/// Allocate a slice from the scratch arena.
/// Returns None if out of space.
pub fn alloc_slice(size: usize) -> Option<&'static mut [u8]> {
    let ptr = alloc(size);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { core::slice::from_raw_parts_mut(ptr, size) })
    }
}

/// Get the remaining space in the arena
pub fn remaining() -> usize {
    unsafe {
        if SCRATCH.base.is_null() {
            0
        } else {
            SCRATCH.size - SCRATCH.offset
        }
    }
}
