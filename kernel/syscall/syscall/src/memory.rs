//! Memory mapping syscall handlers
//!
//! Implements mmap, munmap, mprotect, mremap, and brk syscalls.

use crate::copy_to_user;
use crate::errno;
use crate::{get_current_meta, with_current_meta_mut};
use mm_cow::cow_tracker;
use mm_manager::mm;
use mm_traits::FrameAllocator;
use mm_vma::{VmArea, VmFlags, VmType};
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

/// ⚡ GraveShift: FIXED - Removed global NEXT_MMAP_ADDR, now per-process in ProcessMeta

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

    // Get the current process
    let meta = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    // — NeonRoot: Determine mapping address using VMA gap-finding instead of
    // the old blind bump allocator. Now we actually know what's already mapped.
    let map_addr = if map_flags & flags::MAP_FIXED != 0 {
        // Fixed address - must be page aligned
        if addr & 0xFFF != 0 {
            return errno::EINVAL;
        }
        if addr < MMAP_MIN_ADDR || addr + length > MMAP_MAX_ADDR {
            return errno::ENOMEM;
        }
        // — NeonRoot: MAP_FIXED — remove any existing VMAs in the target range.
        // Linux does this too: MAP_FIXED stomps whatever was there.
        {
            let mut m = meta.lock();
            let _ = m.address_space.remove_vma_range(addr, addr + length);
        }
        addr
    } else {
        // — NeonRoot: Use VMA-aware gap finder. Falls back to bump allocator
        // if VMA list is empty (e.g., kernel tasks, early boot).
        let mut m = meta.lock();
        let hint = if addr != 0 { (addr + 0xFFF) & !0xFFF } else { 0 };
        match m.address_space.vmas.find_free_region(length, hint, MMAP_MAX_ADDR) {
            Some(a) => a,
            None => {
                // — NeonRoot: Fallback to legacy bump allocator for compatibility.
                // This covers the case where VMAs aren't fully populated yet.
                drop(m);
                match allocate_mmap_addr(length) {
                    Ok(a) => a,
                    Err(e) => return e,
                }
            }
        }
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

        // — NeonRoot: Register VMA for the new mapping. Non-fatal on overlap —
        // page tables are the real authority, VMAs are metadata.
        let vm_type = if is_anonymous { VmType::Anon } else { VmType::FileBacked };
        let _ = m.address_space.add_vma(VmArea::new(
            map_addr,
            map_addr + length,
            prot_to_vm_flags(prot, map_flags),
            vm_type,
        ));
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
            os_core::user_access_begin();
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
                    os_core::user_access_end();
                }
                let _ = sys_munmap(map_addr, length);
                return errno::EIO;
            }
        }

        unsafe {
            os_core::user_access_end();
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
        let cow = cow_tracker();
        let allocator = mm();

        // — NeonRoot: Remove VMA metadata for the unmapped range. Handles
        // partial overlap via split/trim. Non-fatal — page tables are authority.
        let _ = m.address_space.remove_vma_range(addr, addr + length);

        // — GraveShift: Unmap each page AND free the physical frame. The old code
        // discarded the PhysAddr returned by unmap_user_page with `let _ = ...`,
        // which leaked every single unmapped frame permanently. Every munmap() call
        // was a slow death by a thousand frame leaks. COW-aware logic: decrement
        // the tracker first — only free to buddy if we're the last owner.
        mm_pagedb::set_free_context(mm_pagedb::CTX_MUNMAP);
        for i in 0..num_pages {
            let page_addr = VirtAddr::new(addr + (i as u64 * 0x1000));
            if let Ok(phys) = m.address_space.unmap_user_page(page_addr) {
                // — WireSaint: Guard against freeing frames that are already
                // back in the buddy free list. If the pagedb says FREE+rc=0,
                // the PTE was stale — the frame was freed by another path
                // (buddy coalescing reuse, exec cleanup, etc.). Calling
                // cow.decrement would create a stale BTreeMap entry, and
                // free_frame would corrupt the buddy free list.
                if let Some(db) = mm_pagedb::try_pagedb() {
                    if let Some(pf) = db.get(phys) {
                        if pf.flags() == mm_pagedb::PF_FREE && pf.refcount() == 0 {
                            continue;
                        }
                    }
                }
                let remaining = cow.decrement(phys);
                if remaining == 0 {
                    allocator.free_frame(phys);
                }
            }
        }
    }

    // — SableWire: TLB shootdown after unmapping. Without this, other CPUs
    // still have stale TLB entries pointing at the now-freed frames. That's
    // a use-after-free the CPU helpfully makes invisible until the frame gets
    // reused for something else and you get "impossible" data corruption.
    smp::tlb_shootdown(addr, addr + length, 0);

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
        os_core::user_access_begin();
        core::ptr::copy_nonoverlapping(
            old_addr as *const u8,
            new_addr as *mut u8,
            old_size as usize,
        );
        os_core::user_access_end();
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
        // — GraveShift: Shrinking heap — unmap pages AND free the physical frames.
        // Same bug as sys_munmap: the old code just discarded the PhysAddr.
        let num_pages = ((old_break - new_break) / page_size) as usize;
        let cow = cow_tracker();
        let allocator = mm();

        mm_pagedb::set_free_context(mm_pagedb::CTX_BRK_SHRINK);
        for i in 0..num_pages {
            let virt = VirtAddr::new(new_break + (i as u64 * page_size));
            if let Ok(phys) = m.address_space.unmap_user_page(virt) {
                let remaining = cow.decrement(phys);
                if remaining == 0 {
                    allocator.free_frame(phys);
                }
            }
        }
        // — SableWire: TLB shootdown for the unmapped range.
        smp::tlb_shootdown(new_break, old_break, 0);
    }

    // Update program break
    m.program_break = new_break;

    // — NeonRoot: Update the heap VMA to reflect the new break. Remove the old
    // heap VMA and insert a fresh one with the updated range. This is O(n) but
    // brk() is infrequent — typically called once at startup and then mmap takes over.
    let _ = m.address_space.remove_vma_range(HEAP_START, MMAP_MAX_ADDR.min(old_break.max(new_break)));
    if new_break > HEAP_START {
        let _ = m.address_space.add_vma(VmArea::new_named(
            HEAP_START,
            new_break,
            VmFlags::READ | VmFlags::WRITE,
            VmType::Heap,
            b"[heap]",
        ));
    }

    new_break as i64
}

/// Helper: Allocate an mmap address from the pool
fn allocate_mmap_addr(length: u64) -> Result<u64, i64> {
    // ⚡ GraveShift: Use per-process mmap hint instead of global
    crate::with_current_meta_mut(|meta| {
        let addr = meta.next_mmap_addr;
        if addr + length > MMAP_MAX_ADDR {
            // Wrap around
            meta.next_mmap_addr = MMAP_MIN_ADDR;
            if MMAP_MIN_ADDR + length > MMAP_MAX_ADDR {
                return Err(errno::ENOMEM);
            }
            meta.next_mmap_addr = MMAP_MIN_ADDR + length;
            return Ok(MMAP_MIN_ADDR);
        }

        meta.next_mmap_addr = addr + length;
        Ok(addr)
    })
    .unwrap_or(Err(errno::ESRCH))
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

/// — NeonRoot: Convert POSIX prot + map flags to VMA VmFlags.
fn prot_to_vm_flags(prot: i32, map_flags: i32) -> VmFlags {
    let mut vf = VmFlags::empty();
    if prot & prot::PROT_READ != 0 {
        vf |= VmFlags::READ;
    }
    if prot & prot::PROT_WRITE != 0 {
        vf |= VmFlags::WRITE;
    }
    if prot & prot::PROT_EXEC != 0 {
        vf |= VmFlags::EXEC;
    }
    if map_flags & flags::MAP_SHARED != 0 {
        vf |= VmFlags::SHARED;
    }
    if map_flags & flags::MAP_GROWSDOWN != 0 {
        vf |= VmFlags::GROWSDOWN;
    }
    if map_flags & flags::MAP_STACK != 0 {
        vf |= VmFlags::STACK;
    }
    vf
}

/// sys_madvise - Advise kernel about memory usage patterns
///
/// This is advisory only; we accept and ignore all advice values.
pub fn sys_madvise(_addr: u64, _length: u64, _advice: i32) -> i64 {
    0
}
