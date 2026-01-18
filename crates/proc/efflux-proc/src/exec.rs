//! Exec implementation
//!
//! Implements the exec() system call, replacing the current process image
//! with a new executable.

use efflux_core::{PhysAddr, VirtAddr};
use efflux_elf::{ElfExecutable, ElfLoader};
use efflux_mm_paging::{phys_to_virt, flush_tlb_all};
use efflux_mm_traits::FrameAllocator;
use efflux_proc_traits::MemoryFlags;

use crate::{UserAddressSpace, process_table};

/// Error during exec
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecError {
    /// Invalid ELF format
    InvalidElf,
    /// Out of memory
    OutOfMemory,
    /// Process not found
    ProcessNotFound,
    /// Invalid address
    InvalidAddress,
}

/// User stack size (1MB)
const USER_STACK_SIZE: usize = 1024 * 1024;

/// User stack top address (just below kernel space)
const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;

/// Execute a new program in the current process
///
/// Replaces the current process's address space and context with the new
/// program loaded from the ELF binary.
///
/// # Arguments
/// * `pid` - Process ID to exec
/// * `elf_data` - ELF binary data
/// * `allocator` - Frame allocator for memory allocation
/// * `kernel_pml4` - Kernel PML4 for copying kernel mappings
///
/// # Returns
/// Entry point and stack pointer on success.
pub fn do_exec<A: FrameAllocator>(
    pid: efflux_proc_traits::Pid,
    elf_data: &[u8],
    allocator: &A,
    kernel_pml4: PhysAddr,
) -> Result<(VirtAddr, VirtAddr), ExecError> {
    // Parse ELF
    let elf = ElfExecutable::parse(elf_data).map_err(|_| ExecError::InvalidElf)?;

    // Get process
    let table = process_table();
    let proc_arc = table.get(pid).ok_or(ExecError::ProcessNotFound)?;
    let mut proc = proc_arc.lock();

    // Create new address space
    let mut new_address_space = unsafe {
        UserAddressSpace::new_with_kernel(allocator, kernel_pml4)
            .ok_or(ExecError::OutOfMemory)?
    };

    // Load segments
    for segment in elf.segments() {
        let (page_start, total_size) = ElfLoader::segment_pages(segment);
        let page_offset = ElfLoader::segment_page_offset(segment);
        let num_pages = total_size / 4096;

        // Get segment data
        let seg_data = elf.segment_data(segment);

        // Allocate and map each page
        for i in 0..num_pages {
            let page_virt = VirtAddr::new(page_start.as_u64() + (i as u64 * 4096));
            let frame = allocator.alloc_frame().ok_or(ExecError::OutOfMemory)?;

            // Zero the frame
            let frame_virt = phys_to_virt(frame);
            unsafe {
                core::ptr::write_bytes(frame_virt.as_mut_ptr::<u8>(), 0, 4096);
            }

            // Copy data from segment
            let page_start_in_segment = i * 4096;
            let data_start_in_page = if i == 0 { page_offset } else { 0 };

            // Calculate how much data to copy for this page
            if page_start_in_segment < segment.file_size + page_offset {
                let seg_data_start = if page_start_in_segment > page_offset {
                    page_start_in_segment - page_offset
                } else {
                    0
                };

                let copy_len = core::cmp::min(
                    4096 - data_start_in_page,
                    segment.file_size.saturating_sub(seg_data_start),
                );

                if copy_len > 0 && seg_data_start < seg_data.len() {
                    let src_end = core::cmp::min(seg_data_start + copy_len, seg_data.len());
                    let actual_len = src_end - seg_data_start;

                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            seg_data[seg_data_start..].as_ptr(),
                            frame_virt.as_mut_ptr::<u8>().add(data_start_in_page),
                            actual_len,
                        );
                    }
                }
            }

            // Map the page
            unsafe {
                new_address_space
                    .map_user_page(page_virt, frame, segment.flags, allocator)
                    .map_err(|_| ExecError::OutOfMemory)?;
            }
        }
    }

    // Set up user stack
    let stack_pages = USER_STACK_SIZE / 4096;
    let stack_bottom = VirtAddr::new(USER_STACK_TOP - USER_STACK_SIZE as u64);

    new_address_space
        .allocate_pages(
            stack_bottom,
            stack_pages,
            MemoryFlags::READ.union(MemoryFlags::WRITE).union(MemoryFlags::USER),
            allocator,
        )
        .map_err(|_| ExecError::OutOfMemory)?;

    let user_stack_top = VirtAddr::new(USER_STACK_TOP);
    let entry_point = elf.entry_point();

    // Replace the process's address space
    // TODO: Free old address space frames
    let old_as = core::mem::replace(proc.address_space_mut(), new_address_space);
    drop(old_as); // Will be cleaned up when dropped

    // Update process entry point and stack
    proc.set_entry_point(entry_point);
    proc.set_user_stack_top(user_stack_top);

    // Clear context for fresh start
    let ctx = proc.context_mut();
    *ctx = crate::ProcessContext::default();
    ctx.rip = entry_point.as_u64();
    ctx.rsp = user_stack_top.as_u64();
    ctx.rflags = 0x202; // IF set

    // Close cloexec file descriptors
    proc.fd_table_mut().close_cloexec();

    // Flush TLB
    flush_tlb_all();

    Ok((entry_point, user_stack_top))
}
