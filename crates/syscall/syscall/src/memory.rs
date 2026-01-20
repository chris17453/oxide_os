//! Memory mapping syscall handlers
//!
//! Implements mmap, munmap, mprotect, mremap, and brk syscalls.

use crate::errno;
use mm_frame::frame_allocator;
use os_core::VirtAddr;
use proc::process_table;
use proc_traits::MemoryFlags;

/// Protection flags (matching Linux/POSIX)
pub mod prot {
    pub const PROT_NONE: i32 = 0x0;
    pub const PROT_READ: i32 = 0x1;
    pub const PROT_WRITE: i32 = 0x2;
    pub const PROT_EXEC: i32 = 0x4;
}

/// Map flags (matching Linux)
pub mod flags {
    pub const MAP_SHARED: i32 = 0x01;
    pub const MAP_PRIVATE: i32 = 0x02;
    pub const MAP_FIXED: i32 = 0x10;
    pub const MAP_ANONYMOUS: i32 = 0x20;
    pub const MAP_ANON: i32 = MAP_ANONYMOUS;
    pub const MAP_GROWSDOWN: i32 = 0x0100;
    pub const MAP_STACK: i32 = 0x20000;
}

/// User space address limits
const MMAP_MIN_ADDR: u64 = 0x0000_1000;
const MMAP_MAX_ADDR: u64 = 0x0000_7FFF_FFFF_F000;

/// Next mmap hint address (simple bump allocator)
/// In a real implementation, this would be per-process
static NEXT_MMAP_ADDR: spin::Mutex<u64> = spin::Mutex::new(0x0000_7000_0000_0000);

/// sys_mmap - Map memory
///
/// # Arguments
/// * `addr` - Requested address (hint or fixed)
/// * `length` - Size of mapping
/// * `prot` - Protection flags (PROT_READ, PROT_WRITE, PROT_EXEC)
/// * `map_flags` - Mapping flags (MAP_ANONYMOUS, MAP_PRIVATE, etc.)
/// * `fd` - File descriptor (for file-backed mappings)
/// * `offset` - Offset in file
///
/// # Returns
/// Address of mapping, or negative errno
pub fn sys_mmap(addr: u64, length: u64, prot: i32, map_flags: i32, fd: i32, offset: i64) -> i64 {
    // Validate length
    if length == 0 {
        return errno::EINVAL;
    }

    // Page-align length
    let length = (length + 0xFFF) & !0xFFF;

    // For now, only support anonymous private mappings
    if map_flags & flags::MAP_ANONYMOUS == 0 {
        // File-backed mappings not yet implemented
        // TODO: Implement file-backed mmap
        return errno::ENOSYS;
    }

    // Determine mapping address
    let map_addr = if map_flags & flags::MAP_FIXED != 0 {
        // Fixed address - must be page aligned
        if addr & 0xFFF != 0 {
            return errno::EINVAL;
        }
        if addr < MMAP_MIN_ADDR || addr + length > MMAP_MAX_ADDR {
            return errno::ENOMEM;
        }
        addr
    } else if addr != 0 {
        // Hint address - try to use it, but fall back if not available
        let aligned = (addr + 0xFFF) & !0xFFF;
        if aligned >= MMAP_MIN_ADDR && aligned + length <= MMAP_MAX_ADDR {
            aligned
        } else {
            match allocate_mmap_addr(length) {
                Ok(a) => a,
                Err(e) => return e,
            }
        }
    } else {
        // No hint - allocate from our pool
        match allocate_mmap_addr(length) {
            Ok(a) => a,
            Err(e) => return e,
        }
    };

    // Get the current process
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    // Convert protection flags to MemoryFlags
    let mem_flags = prot_to_memory_flags(prot);

    // Allocate and map the pages
    let num_pages = (length / 0x1000) as usize;

    {
        let mut p = proc.lock();
        let address_space = p.address_space_mut();

        // Use the frame allocator to allocate and map pages
        let allocator = frame_allocator();

        match address_space.allocate_pages(
            VirtAddr::new(map_addr),
            num_pages,
            mem_flags,
            allocator,
        ) {
            Ok(()) => {}
            Err(_) => return errno::ENOMEM,
        }
    }

    map_addr as i64
}

/// sys_munmap - Unmap memory
///
/// # Arguments
/// * `addr` - Start address of mapping (must be page-aligned)
/// * `length` - Size to unmap
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_munmap(addr: u64, length: u64) -> i64 {
    // Validate address
    if addr & 0xFFF != 0 {
        return errno::EINVAL;
    }
    if length == 0 {
        return errno::EINVAL;
    }

    // Page-align length
    let length = (length + 0xFFF) & !0xFFF;

    // Get the current process
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let num_pages = (length / 0x1000) as usize;

    {
        let mut p = proc.lock();
        let address_space = p.address_space_mut();

        // Unmap each page
        for i in 0..num_pages {
            let page_addr = VirtAddr::new(addr + (i as u64 * 0x1000));
            // Ignore errors for pages that aren't mapped
            let _ = address_space.unmap_user_page(page_addr);
        }
    }

    0
}

/// sys_mprotect - Change memory protection
///
/// # Arguments
/// * `addr` - Start address (must be page-aligned)
/// * `length` - Size of region
/// * `prot` - New protection flags
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_mprotect(addr: u64, length: u64, prot: i32) -> i64 {
    // Validate address
    if addr & 0xFFF != 0 {
        return errno::EINVAL;
    }
    if length == 0 {
        return 0; // Nothing to do
    }

    // Page-align length
    let length = (length + 0xFFF) & !0xFFF;

    // Get the current process
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let mem_flags = prot_to_memory_flags(prot);
    let num_pages = (length / 0x1000) as usize;

    {
        let mut p = proc.lock();
        let address_space = p.address_space_mut();

        // Update protection for each page
        for i in 0..num_pages {
            let page_addr = VirtAddr::new(addr + (i as u64 * 0x1000));
            // Update flags - for now we can only add WRITABLE permission
            // A full implementation would need to handle all flag changes
            if mem_flags.writable() {
                address_space.update_user_page_flags(page_addr, mem_flags);
            }
        }
    }

    0
}

/// sys_mremap - Remap memory
///
/// # Arguments
/// * `old_addr` - Current address
/// * `old_size` - Current size
/// * `new_size` - New size
/// * `flags` - Remap flags (MREMAP_MAYMOVE, etc.)
/// * `new_addr` - New address (if MREMAP_FIXED)
///
/// # Returns
/// New address on success, negative errno on error
pub fn sys_mremap(old_addr: u64, old_size: u64, new_size: u64, mremap_flags: i32, _new_addr: u64) -> i64 {
    const MREMAP_MAYMOVE: i32 = 1;

    // Validate old address
    if old_addr & 0xFFF != 0 {
        return errno::EINVAL;
    }

    let old_size = (old_size + 0xFFF) & !0xFFF;
    let new_size = (new_size + 0xFFF) & !0xFFF;

    if new_size == 0 {
        return errno::EINVAL;
    }

    // If shrinking, just unmap the tail
    if new_size <= old_size {
        if new_size < old_size {
            let _ = sys_munmap(old_addr + new_size, old_size - new_size);
        }
        return old_addr as i64;
    }

    // Growing - try to extend in place first
    let extension = new_size - old_size;
    let extend_addr = old_addr + old_size;

    // Check if we can extend
    // For simplicity, just try to allocate at the extension address
    let result = sys_mmap(
        extend_addr,
        extension,
        prot::PROT_READ | prot::PROT_WRITE,
        flags::MAP_PRIVATE | flags::MAP_ANONYMOUS | flags::MAP_FIXED,
        -1,
        0,
    );

    if result >= 0 {
        return old_addr as i64;
    }

    // Can't extend in place - need to move
    if mremap_flags & MREMAP_MAYMOVE == 0 {
        return errno::ENOMEM;
    }

    // Allocate new region
    let new_addr = sys_mmap(
        0,
        new_size,
        prot::PROT_READ | prot::PROT_WRITE,
        flags::MAP_PRIVATE | flags::MAP_ANONYMOUS,
        -1,
        0,
    );

    if new_addr < 0 {
        return new_addr;
    }

    // Copy data from old to new
    // Note: This is a simple memcpy - in a real implementation we'd use
    // the kernel's safe copy mechanism
    unsafe {
        core::ptr::copy_nonoverlapping(
            old_addr as *const u8,
            new_addr as *mut u8,
            old_size as usize,
        );
    }

    // Unmap old region
    let _ = sys_munmap(old_addr, old_size);

    new_addr
}

/// sys_brk - Change data segment size
///
/// # Arguments
/// * `addr` - New end of data segment (0 to query current)
///
/// # Returns
/// Current/new end of data segment, or negative errno on error
pub fn sys_brk(addr: u64) -> i64 {
    // For now, brk is not implemented - programs should use mmap
    // In a full implementation, we'd track the program break and
    // allocate/deallocate pages as needed

    // Return current break (stub - returns 0 to indicate "use mmap instead")
    if addr == 0 {
        return 0;
    }

    errno::ENOMEM
}

/// Helper: Allocate an mmap address from the pool
fn allocate_mmap_addr(length: u64) -> Result<u64, i64> {
    let mut next = NEXT_MMAP_ADDR.lock();

    let addr = *next;
    if addr + length > MMAP_MAX_ADDR {
        // Wrap around
        *next = MMAP_MIN_ADDR;
        if MMAP_MIN_ADDR + length > MMAP_MAX_ADDR {
            return Err(errno::ENOMEM);
        }
        *next = MMAP_MIN_ADDR + length;
        return Ok(MMAP_MIN_ADDR);
    }

    *next = addr + length;
    Ok(addr)
}

/// Helper: Convert protection flags to MemoryFlags
fn prot_to_memory_flags(prot: i32) -> MemoryFlags {
    let mut flags = MemoryFlags::empty();

    if prot & prot::PROT_READ != 0 {
        flags = flags.union(MemoryFlags::READ);
    }
    if prot & prot::PROT_WRITE != 0 {
        flags = flags.union(MemoryFlags::WRITE);
    }
    if prot & prot::PROT_EXEC != 0 {
        flags = flags.union(MemoryFlags::EXECUTE);
    }

    // User pages should have USER flag
    flags = flags.union(MemoryFlags::USER);

    flags
}
