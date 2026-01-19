//! Exec implementation
//!
//! Implements the exec() system call, replacing the current process image
//! with a new executable.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use os_core::{PhysAddr, VirtAddr};
use elf::{ElfExecutable, ElfLoader};
use mm_paging::{phys_to_virt, flush_tlb_all};
use mm_traits::FrameAllocator;
use proc_traits::MemoryFlags;

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
    /// Invalid argument
    InvalidArgument,
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
/// * `argv` - Command-line arguments
/// * `envp` - Environment variables
/// * `allocator` - Frame allocator for memory allocation
/// * `kernel_pml4` - Kernel PML4 for copying kernel mappings
///
/// # Returns
/// Entry point and stack pointer on success.
pub fn do_exec<A: FrameAllocator>(
    pid: proc_traits::Pid,
    elf_data: &[u8],
    argv: &[String],
    envp: &[String],
    allocator: &A,
    kernel_pml4: PhysAddr,
) -> Result<(VirtAddr, VirtAddr), ExecError> {
    // Parse ELF
    let elf = ElfExecutable::parse(elf_data).map_err(|_e| ExecError::InvalidElf)?;

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

            // Check if this page is already mapped (overlapping segments)
            let frame_virt = if let Some(existing_phys) = new_address_space.translate(page_virt) {
                // Page already mapped, use existing frame
                // But we need to update permissions if the new segment needs write access
                if segment.flags.contains(MemoryFlags::WRITE) {
                    new_address_space.update_user_page_flags(page_virt, MemoryFlags::WRITE);
                }
                phys_to_virt(existing_phys)
            } else {
                // Allocate new frame
                let frame = allocator.alloc_frame().ok_or(ExecError::OutOfMemory)?;

                // Zero the frame
                let frame_virt = phys_to_virt(frame);
                unsafe {
                    core::ptr::write_bytes(frame_virt.as_mut_ptr::<u8>(), 0, 4096);
                }

                // Map the page
                unsafe {
                    new_address_space
                        .map_user_page(page_virt, frame, segment.flags, allocator)
                        .map_err(|_| ExecError::OutOfMemory)?;
                }

                frame_virt
            };

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

    let entry_point = elf.entry_point();

    // Set up argv and envp on the stack
    // Stack layout (growing down):
    // [strings data] <- null-terminated strings
    // [padding for alignment]
    // [NULL]         <- envp terminator
    // [envp[n-1]]    <- pointers to env strings
    // ...
    // [envp[0]]
    // [NULL]         <- argv terminator
    // [argv[n-1]]    <- pointers to arg strings
    // ...
    // [argv[0]]
    // [argc]         <- number of arguments
    // <- rsp points here

    let mut stack_ptr = USER_STACK_TOP;

    // Calculate total size needed for strings
    let mut string_data_size = 0usize;
    for arg in argv {
        string_data_size += arg.len() + 1; // +1 for null terminator
    }
    for env in envp {
        string_data_size += env.len() + 1;
    }

    // Align down to start strings at aligned boundary
    stack_ptr -= string_data_size as u64;
    stack_ptr &= !0xF; // 16-byte align

    // Track where strings will be placed
    let strings_base = stack_ptr;
    let mut string_offsets_argv: Vec<u64> = Vec::with_capacity(argv.len());
    let mut string_offsets_envp: Vec<u64> = Vec::with_capacity(envp.len());

    // Calculate string offsets
    let mut current_offset = 0u64;
    for arg in argv {
        string_offsets_argv.push(strings_base + current_offset);
        current_offset += (arg.len() + 1) as u64;
    }
    for env in envp {
        string_offsets_envp.push(strings_base + current_offset);
        current_offset += (env.len() + 1) as u64;
    }

    // Now calculate space for pointers
    // envp array: (envp.len() + 1) * 8 bytes (including NULL terminator)
    // argv array: (argv.len() + 1) * 8 bytes (including NULL terminator)
    // argc: 8 bytes
    let pointers_size = ((envp.len() + 1) + (argv.len() + 1) + 1) * 8;
    stack_ptr -= pointers_size as u64;
    stack_ptr &= !0xF; // 16-byte align

    let final_rsp = VirtAddr::new(stack_ptr);

    // Replace the process's address space first so we can write to it
    let old_as = core::mem::replace(proc.address_space_mut(), new_address_space);
    drop(old_as);

    // Get mutable reference to new address space
    let new_as = proc.address_space_mut();

    // Write strings to stack
    let mut string_ptr = strings_base;
    for arg in argv {
        write_to_user_stack(new_as, string_ptr, arg.as_bytes())?;
        // Write null terminator
        write_to_user_stack(new_as, string_ptr + arg.len() as u64, &[0u8])?;
        string_ptr += (arg.len() + 1) as u64;
    }
    for env in envp {
        write_to_user_stack(new_as, string_ptr, env.as_bytes())?;
        write_to_user_stack(new_as, string_ptr + env.len() as u64, &[0u8])?;
        string_ptr += (env.len() + 1) as u64;
    }

    // Write argc
    let mut ptr = stack_ptr;
    let argc_bytes = (argv.len() as u64).to_le_bytes();
    write_to_user_stack(new_as, ptr, &argc_bytes)?;
    ptr += 8;

    // Write argv pointers
    for &offset in &string_offsets_argv {
        write_to_user_stack(new_as, ptr, &offset.to_le_bytes())?;
        ptr += 8;
    }
    // NULL terminator for argv
    write_to_user_stack(new_as, ptr, &0u64.to_le_bytes())?;
    ptr += 8;

    // Write envp pointers
    for &offset in &string_offsets_envp {
        write_to_user_stack(new_as, ptr, &offset.to_le_bytes())?;
        ptr += 8;
    }
    // NULL terminator for envp
    write_to_user_stack(new_as, ptr, &0u64.to_le_bytes())?;

    // Update process entry point and stack
    proc.set_entry_point(entry_point);
    proc.set_user_stack_top(final_rsp);

    // Store argv and envp for /proc
    proc.set_cmdline(argv.iter().map(|s| s.clone()).collect());
    proc.set_environ(envp.iter().map(|s| s.clone()).collect());

    // Clear context for fresh start
    let ctx = proc.context_mut();
    *ctx = crate::ProcessContext::default();
    ctx.rip = entry_point.as_u64();
    ctx.rsp = final_rsp.as_u64();
    ctx.rflags = 0x202; // IF set

    // Set up arguments in registers per System V ABI:
    // rdi = argc
    // rsi = argv (pointer to argv array)
    // rdx = envp (pointer to envp array)
    ctx.rdi = argv.len() as u64;
    ctx.rsi = stack_ptr + 8; // argv starts after argc
    ctx.rdx = stack_ptr + 8 + ((argv.len() + 1) * 8) as u64; // envp starts after argv + NULL

    // Close cloexec file descriptors
    proc.fd_table_mut().close_cloexec();

    // Flush TLB
    flush_tlb_all();

    Ok((entry_point, final_rsp))
}

/// Write data to user stack at given virtual address
fn write_to_user_stack(
    address_space: &UserAddressSpace,
    vaddr: u64,
    data: &[u8],
) -> Result<(), ExecError> {
    // Translate virtual to physical
    let page_vaddr = VirtAddr::new(vaddr & !0xFFF);
    let page_offset = (vaddr & 0xFFF) as usize;

    let phys = address_space
        .translate(page_vaddr)
        .ok_or(ExecError::InvalidAddress)?;

    let dest_virt = phys_to_virt(phys);
    let dest = unsafe { dest_virt.as_mut_ptr::<u8>().add(page_offset) };

    // Handle page boundary crossing
    let remaining_in_page = 4096 - page_offset;
    if data.len() <= remaining_in_page {
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), dest, data.len());
        }
    } else {
        // Write first part
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), dest, remaining_in_page);
        }
        // Write remainder to next page
        write_to_user_stack(address_space, vaddr + remaining_in_page as u64, &data[remaining_in_page..])?;
    }

    Ok(())
}
