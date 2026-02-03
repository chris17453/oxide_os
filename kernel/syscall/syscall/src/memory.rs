//! Memory mapping syscall handlers
//!
//! Implements mmap, munmap, mprotect, mremap, and brk syscalls.

use crate::copy_to_user;
use crate::errno;
use crate::{get_current_meta, with_current_meta_mut};
use mm_manager::mm;
use os_core::VirtAddr;
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

    // Check if this is file-backed or anonymous
    let is_anonymous = (map_flags & flags::MAP_ANONYMOUS) != 0;

    // If file-backed, validate fd and offset
    let file_opt = if !is_anonymous {
        // Validate offset is page-aligned
        if offset < 0 || (offset as u64 & 0xFFF) != 0 {
            return errno::EINVAL;
        }

        // Get the file from the file descriptor
        let file = match crate::with_current_meta(|meta| {
            meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone())
        }) {
            Some(Ok(f)) => f,
            Some(Err(_)) => return errno::EBADF,
            None => return errno::ESRCH,
        };

        Some(file)
    } else {
        None
    };

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
    let meta = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    // Convert protection flags to MemoryFlags
    let mem_flags = prot_to_memory_flags(prot);

    // Allocate and map the pages
    let num_pages = (length / 0x1000) as usize;

    {
        let mut m = meta.lock();

        // Use the memory manager to allocate and map pages
        let allocator = mm();

        match m.address_space.allocate_pages(
            VirtAddr::new(map_addr),
            num_pages,
            mem_flags,
            allocator,
        ) {
            Ok(()) => {}
            Err(_) => return errno::ENOMEM,
        }
    }

    // If file-backed, read file contents into the mapped pages
    if let Some(file) = file_opt {
        // Seek to the offset
        use vfs::SeekFrom;
        match file.seek(SeekFrom::Start(offset as u64)) {
            Ok(_) => {}
            Err(_) => {
                // Failed to seek - unmap and return error
                let _ = sys_munmap(map_addr, length);
                return errno::EIO;
            }
        }

        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
        }

        // Read file data into the mapped region
        let buffer =
            unsafe { core::slice::from_raw_parts_mut(map_addr as *mut u8, length as usize) };

        match file.read(buffer) {
            Ok(bytes_read) => {
                // Zero-fill the rest if file is smaller than mapping
                if bytes_read < length as usize {
                    buffer[bytes_read..].fill(0);
                }
            }
            Err(_) => {
                // Failed to read - unmap and return error
                unsafe {
                    core::arch::asm!("clac", options(nomem, nostack));
                }
                let _ = sys_munmap(map_addr, length);
                return errno::EIO;
            }
        }

        unsafe {
            core::arch::asm!("clac", options(nomem, nostack));
        }

        // Note: MAP_SHARED vs MAP_PRIVATE handling
        // For MAP_PRIVATE, we've already made a copy (COW not yet implemented)
        // For MAP_SHARED, writes will go to memory but won't be synced to file
        // Full msync() support would be needed for proper MAP_SHARED
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
    let meta = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    let num_pages = (length / 0x1000) as usize;

    {
        let mut m = meta.lock();

        // Unmap each page
        for i in 0..num_pages {
            let page_addr = VirtAddr::new(addr + (i as u64 * 0x1000));
            // Ignore errors for pages that aren't mapped
            let _ = m.address_space.unmap_user_page(page_addr);
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
    let meta = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    let mem_flags = prot_to_memory_flags(prot);
    let num_pages = (length / 0x1000) as usize;

    {
        let mut m = meta.lock();

        // Update protection for each page
        for i in 0..num_pages {
            let page_addr = VirtAddr::new(addr + (i as u64 * 0x1000));
            // Update flags - for now we can only add WRITABLE permission
            // A full implementation would need to handle all flag changes
            if mem_flags.writable() {
                m.address_space.update_user_page_flags(page_addr, mem_flags);
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
pub fn sys_mremap(
    old_addr: u64,
    old_size: u64,
    new_size: u64,
    mremap_flags: i32,
    _new_addr: u64,
) -> i64 {
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
    // Use STAC/CLAC to allow kernel access to user pages during the copy
    unsafe {
        #[cfg(target_arch = "x86_64")]
        {
            core::arch::asm!(
                "stac",                                      // Enable user page access
                "mov rcx, {len}",                           // Length in RCX
                "mov rsi, {src}",                           // Source in RSI
                "mov rdi, {dst}",                           // Destination in RDI
                "rep movsb",                                 // Copy bytes
                "clac",                                      // Disable user page access
                src = in(reg) old_addr,
                dst = in(reg) new_addr,
                len = in(reg) old_size,
                out("rcx") _,
                out("rsi") _,
                out("rdi") _,
                options(nostack)
            );
        }

        #[cfg(not(target_arch = "x86_64"))]
        {
            core::ptr::copy_nonoverlapping(
                old_addr as *const u8,
                new_addr as *mut u8,
                old_size as usize,
            );
        }
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
    // 🔥 GraveShift: The classic UNIX heap - brk/sbrk lives on 🔥

    let meta = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    let mut m = meta.lock();

    // Query current break
    if addr == 0 {
        return m.program_break as i64;
    }

    // Align requested address to page boundary
    let page_size = 4096u64;
    let new_break = (addr + page_size - 1) & !(page_size - 1);
    let old_break = m.program_break;

    // Can't shrink below initial heap start
    const HEAP_START: u64 = 0x600000;
    if new_break < HEAP_START {
        return old_break as i64;
    }

    if new_break > old_break {
        // Expanding heap - allocate and map new pages
        let num_pages = ((new_break - old_break) / page_size) as usize;

        let flags = MemoryFlags::READ
            .union(MemoryFlags::WRITE)
            .union(MemoryFlags::USER);

        let allocator = mm();

        match m
            .address_space
            .allocate_pages(VirtAddr::new(old_break), num_pages, flags, allocator)
        {
            Ok(()) => {}
            Err(_) => return errno::ENOMEM,
        }
    } else if new_break < old_break {
        // Shrinking heap - unmap and free pages
        let num_pages = ((old_break - new_break) / page_size) as usize;

        for i in 0..num_pages {
            let virt = VirtAddr::new(new_break + (i as u64 * page_size));
            // Ignore errors - page might not have been mapped
            let _ = m.address_space.unmap_user_page(virt);
        }
    }

    // Update program break
    m.program_break = new_break;
    new_break as i64
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

/// sys_madvise - Advise kernel about memory usage patterns
///
/// This is advisory only; we accept and ignore all advice values.
pub fn sys_madvise(_addr: u64, _length: u64, _advice: i32) -> i64 {
    0
}
