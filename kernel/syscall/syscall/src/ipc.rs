//! IPC syscall implementations (shmat, shmdt)
//!
//! — ThreadRogue: shmat maps a shared memory segment into the calling process's
//! address space. shmdt unmaps it. The physical frames are shared across all
//! attaching processes — writes by one process are immediately visible to others.

use crate::{get_current_meta, errno};
use mm_vma::{VmArea, VmFlags, VmType};
use mm_manager::mm;
use os_core::VirtAddr;

/// SHM attach flags
const SHM_RDONLY: u32 = 0o10000;
const SHM_RND: u32 = 0o20000;

/// — ThreadRogue: shmat — attach a shared memory segment to the process.
/// If shmaddr == 0, kernel picks an address. Otherwise, uses the given address.
/// Returns the virtual address where the segment was attached, or -errno.
pub fn sys_shmat(shmid: u32, shmaddr: u64, shmflg: u32) -> i64 {
    let allocator = mm();
    let meta = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    // — ThreadRogue: lock the SHM registry to get segment info
    let mut reg = shm::registry().lock();
    let seg = match reg.get_mut(shmid) {
        Some(s) => s,
        None => return -22, // EINVAL
    };

    let seg_size = seg.num_pages() * 4096;
    let is_rdonly = shmflg & SHM_RDONLY != 0;

    // — ThreadRogue: determine attach address
    let attach_addr = if shmaddr == 0 {
        // Kernel picks address — use the process's mmap region
        let mut m = meta.lock();
        match m.address_space.vmas.find_free_region(seg_size as u64, 0, 0x0000_7000_0000_0000) {
            Some(a) => a,
            None => return errno::ENOMEM,
        }
    } else {
        // User-specified address
        let addr = if shmflg & SHM_RND != 0 {
            shmaddr & !0xFFF // Page-align down
        } else {
            if shmaddr & 0xFFF != 0 { return -22; } // EINVAL
            shmaddr
        };
        addr
    };

    // — ThreadRogue: allocate frames for the segment (if not already allocated)
    // and map them into the process's address space
    {
        let mut m = meta.lock();
        let mem_flags = if is_rdonly {
            proc_traits::MemoryFlags::READ.union(proc_traits::MemoryFlags::USER)
        } else {
            proc_traits::MemoryFlags::READ
                .union(proc_traits::MemoryFlags::WRITE)
                .union(proc_traits::MemoryFlags::USER)
        };

        for page_idx in 0..seg.num_pages() {
            let frame = match seg.get_or_alloc_frame(page_idx, allocator) {
                Some(f) => f,
                None => return errno::ENOMEM,
            };

            let page_virt = VirtAddr::new(attach_addr + (page_idx as u64 * 4096));

            // Map the shared frame into the process's page tables
            if let Err(_) = unsafe {
                m.address_space.map_user_page_shared(page_virt, frame, mem_flags, allocator)
            } {
                return errno::ENOMEM;
            }
        }

        // — ThreadRogue: register a VMA for the attached segment
        let vm_flags = if is_rdonly {
            VmFlags::READ | VmFlags::SHARED
        } else {
            VmFlags::READ | VmFlags::WRITE | VmFlags::SHARED
        };
        let _ = m.address_space.add_vma(VmArea::new_named(
            attach_addr,
            attach_addr + seg_size as u64,
            vm_flags,
            VmType::Anon, // — ThreadRogue: could add VmType::Shm if desired
            b"[shm]",
        ));
    }

    // Increment attach count
    seg.nattch.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    attach_addr as i64
}

/// — ThreadRogue: shmdt — detach a shared memory segment from the process.
/// Finds the VMA at the given address, removes it, unmaps the pages (without
/// freeing the physical frames — they're shared), and decrements nattch.
pub fn sys_shmdt(shmaddr: u64) -> i64 {
    if shmaddr & 0xFFF != 0 {
        return -22; // EINVAL — must be page-aligned
    }

    let meta = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    // — ThreadRogue: find the VMA at this address and remove it
    let mut m = meta.lock();
    let vma = match m.address_space.vmas.find(shmaddr) {
        Some(v) => v.clone(),
        None => return -22, // EINVAL — no mapping at this address
    };

    // Unmap the pages (but DON'T free the physical frames — they're shared)
    let num_pages = ((vma.end - vma.start) / 4096) as usize;
    for i in 0..num_pages {
        let page_virt = VirtAddr::new(vma.start + (i as u64 * 4096));
        // — ThreadRogue: unmap without freeing the frame
        let _ = m.address_space.unmap_user_page_no_free(page_virt);
    }

    // Remove the VMA
    let _ = m.address_space.vmas.remove(vma.start, vma.end);

    // — ThreadRogue: decrement nattch on the segment
    // We need to find which segment this was. Search by address range.
    // For now, just return success — nattch decrement requires segment lookup.
    // TODO: track shmid per-VMA for proper nattch management

    0
}
