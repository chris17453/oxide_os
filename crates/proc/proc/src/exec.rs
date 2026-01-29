//! Exec implementation
//!
//! Implements the exec() system call, replacing the current process image
//! with a new executable.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use elf::{ElfExecutable, ElfLoader};
use mm_paging::{flush_tlb_all, phys_to_virt};
use mm_traits::FrameAllocator;
use os_core::{PhysAddr, VirtAddr};
use proc_traits::MemoryFlags;

use crate::{ProcessContext, UserAddressSpace};

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

/// Result of a successful exec operation
///
/// Contains all the data needed to update the Task and ProcessMeta.
pub struct ExecResult {
    /// New address space for the process
    pub address_space: UserAddressSpace,
    /// Entry point of the new program
    pub entry_point: VirtAddr,
    /// Initial stack pointer
    pub stack_pointer: VirtAddr,
    /// New context for the process
    pub context: ProcessContext,
    /// Command line arguments (for /proc/[pid]/cmdline)
    pub cmdline: Vec<String>,
    /// Environment variables (for /proc/[pid]/environ)
    pub environ: Vec<String>,
}

/// User stack size (1MB)
const USER_STACK_SIZE: usize = 1024 * 1024;

/// User stack top address (just below kernel space)
const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;

/// Execute a new program
///
/// Creates a new address space and loads the ELF binary into it.
/// Returns ExecResult with all data needed to update the process.
/// The caller is responsible for updating Task and ProcessMeta.
///
/// # Arguments
/// * `elf_data` - ELF binary data
/// * `argv` - Command-line arguments
/// * `envp` - Environment variables
/// * `allocator` - Frame allocator for memory allocation
/// * `kernel_pml4` - Kernel PML4 for copying kernel mappings
pub fn do_exec<A: FrameAllocator>(
    elf_data: &[u8],
    argv: &[String],
    envp: &[String],
    allocator: &A,
    kernel_pml4: PhysAddr,
) -> Result<ExecResult, ExecError> {
    // Parse ELF
    let elf = ElfExecutable::parse(elf_data).map_err(|_e| ExecError::InvalidElf)?;

    // TEMP DEBUG: Manually check for PT_TLS in raw ELF data
    #[cfg(debug_assertions)]
    {
        // Read ELF header to get phoff, phnum
        if elf_data.len() >= 64 {
            #[repr(C)]
            struct ElfHeader {
                e_ident: [u8; 16],
                e_type: u16,
                e_machine: u16,
                e_version: u32,
                e_entry: u64,
                e_phoff: u64,
                e_shoff: u64,
                e_flags: u32,
                e_ehsize: u16,
                e_phentsize: u16,
                e_phnum: u16,
                e_shentsize: u16,
                e_shnum: u16,
                e_shstrndx: u16,
            }
            let header = unsafe { &*(elf_data.as_ptr() as *const ElfHeader) };
            let ph_offset = header.e_phoff as usize;
            let ph_size = header.e_phentsize as usize;
            let ph_count = header.e_phnum as usize;

            // Check each program header for PT_TLS
            for i in 0..ph_count {
                let ph_start = ph_offset + i * ph_size;
                if ph_start + 4 <= elf_data.len() {
                    let p_type = unsafe { *(elf_data.as_ptr().add(ph_start) as *const u32) };
                    // PT_TLS = 7
                    if p_type == 7 {
                        // Found PT_TLS!
                        // Set a flag or break - we know TLS is in the file
                        break;
                    }
                }
            }
        }
    }

    // Create new address space
    let mut new_address_space = unsafe {
        UserAddressSpace::new_with_kernel(allocator, kernel_pml4).ok_or(ExecError::OutOfMemory)?
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

    // Set up TLS (Thread-Local Storage) if needed
    // TEMP HACK: Force TLS setup for testing
    // ALWAYS search manually since ELF parser seems broken
    let forced_tls = {
        // Manually search for PT_TLS in the ELF
        #[repr(C)]
        struct ElfHeader {
            e_ident: [u8; 16],
            e_type: u16,
            e_machine: u16,
            e_version: u32,
            e_entry: u64,
            e_phoff: u64,
            e_shoff: u64,
            e_flags: u32,
            e_ehsize: u16,
            e_phentsize: u16,
            e_phnum: u16,
            e_shentsize: u16,
            e_shnum: u16,
            e_shstrndx: u16,
        }
        #[repr(C)]
        struct ProgHeader {
            p_type: u32,
            p_flags: u32,
            p_offset: u64,
            p_vaddr: u64,
            p_paddr: u64,
            p_filesz: u64,
            p_memsz: u64,
            p_align: u64,
        }

        if elf_data.len() >= 64 {
            let header = unsafe { &*(elf_data.as_ptr() as *const ElfHeader) };
            let ph_offset = header.e_phoff as usize;
            let ph_size = header.e_phentsize as usize;
            let ph_count = header.e_phnum as usize;

            let mut found_tls = None;
            for i in 0..ph_count {
                let ph_start = ph_offset + i * ph_size;
                if ph_start + core::mem::size_of::<ProgHeader>() <= elf_data.len() {
                    let ph = unsafe { &*(elf_data.as_ptr().add(ph_start) as *const ProgHeader) };
                    if ph.p_type == 7 { // PT_TLS
                        // Found it! Create TlsTemplate manually
                        found_tls = Some(elf::TlsTemplate {
                            file_offset: ph.p_offset as usize,
                            file_size: ph.p_filesz as usize,
                            mem_size: ph.p_memsz as usize,
                            align: ph.p_align as usize,
                        });
                        break;
                    }
                }
            }
            found_tls
        } else {
            None
        }
    };

    let tls_template_to_use = forced_tls.as_ref().or(elf.tls_template());
    let tls_base = if let Some(tls_template) = tls_template_to_use {
        // Allocate TLS block
        // TLS block layout: [TLS data] [Thread Control Block (TCB)]
        // FS register points to TCB (end of TLS block)
        let tls_size = tls_template.mem_size;
        let tcb_size = 64; // Thread Control Block size (self-pointer + space)
        let total_size = tls_size + tcb_size;

        // Align to page boundary
        let pages_needed = (total_size + 4095) / 4096;
        let tls_vaddr = VirtAddr::new(0x0000_7000_0000_0000); // TLS region

        // Allocate TLS pages
        new_address_space
            .allocate_pages(
                tls_vaddr,
                pages_needed,
                MemoryFlags::READ
                    .union(MemoryFlags::WRITE)
                    .union(MemoryFlags::USER),
                allocator,
            )
            .map_err(|_| ExecError::OutOfMemory)?;

        // x86-64 TLS ABI Variant II layout: [TCB] [TLS data]
        // FS register points to TCB, TLS data is at positive offsets from there

        let tcb_addr = tls_vaddr.as_u64();

        // Write self-pointer to TCB (required by x86-64 TLS ABI)
        write_to_user_stack(&new_address_space, tcb_addr, &tcb_addr.to_le_bytes())?;

        // Copy TLS initialization data AFTER the TCB
        let tls_data = elf.tls_data();
        if !tls_data.is_empty() {
            let tls_data_addr = tcb_addr + tcb_size as u64;
            write_to_user_stack(&new_address_space, tls_data_addr, tls_data)?;
        }

        Some(tcb_addr)
    } else {
        None
    };

    // Set up user stack
    let stack_pages = USER_STACK_SIZE / 4096;
    let stack_bottom = VirtAddr::new(USER_STACK_TOP - USER_STACK_SIZE as u64);

    new_address_space
        .allocate_pages(
            stack_bottom,
            stack_pages,
            MemoryFlags::READ
                .union(MemoryFlags::WRITE)
                .union(MemoryFlags::USER),
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

    // Write strings to stack
    let mut string_ptr = strings_base;
    for arg in argv {
        write_to_user_stack(&new_address_space, string_ptr, arg.as_bytes())?;
        // Write null terminator
        write_to_user_stack(&new_address_space, string_ptr + arg.len() as u64, &[0u8])?;
        string_ptr += (arg.len() + 1) as u64;
    }
    for env in envp {
        write_to_user_stack(&new_address_space, string_ptr, env.as_bytes())?;
        write_to_user_stack(&new_address_space, string_ptr + env.len() as u64, &[0u8])?;
        string_ptr += (env.len() + 1) as u64;
    }

    // Write argc
    let mut ptr = stack_ptr;
    let argc_bytes = (argv.len() as u64).to_le_bytes();
    write_to_user_stack(&new_address_space, ptr, &argc_bytes)?;
    ptr += 8;

    // Write argv pointers
    for &offset in &string_offsets_argv {
        write_to_user_stack(&new_address_space, ptr, &offset.to_le_bytes())?;
        ptr += 8;
    }
    // NULL terminator for argv
    write_to_user_stack(&new_address_space, ptr, &0u64.to_le_bytes())?;
    ptr += 8;

    // Write envp pointers
    for &offset in &string_offsets_envp {
        write_to_user_stack(&new_address_space, ptr, &offset.to_le_bytes())?;
        ptr += 8;
    }
    // NULL terminator for envp
    write_to_user_stack(&new_address_space, ptr, &0u64.to_le_bytes())?;

    // Create context for fresh start
    let mut context = ProcessContext::default();
    context.rip = entry_point.as_u64();
    context.rsp = final_rsp.as_u64();
    context.rflags = 0x202; // IF set
    context.cs = 0x23; // User code segment
    context.ss = 0x1B; // User data segment
    context.fs_base = tls_base.unwrap_or(0); // Set FS base for TLS

    // Set up arguments in registers per System V ABI:
    // rdi = argc
    // rsi = argv (pointer to argv array)
    // rdx = envp (pointer to envp array)
    context.rdi = argv.len() as u64;
    context.rsi = stack_ptr + 8; // argv starts after argc
    context.rdx = stack_ptr + 8 + ((argv.len() + 1) * 8) as u64; // envp starts after argv + NULL

    // Flush TLB
    flush_tlb_all();

    Ok(ExecResult {
        address_space: new_address_space,
        entry_point,
        stack_pointer: final_rsp,
        context,
        cmdline: argv.iter().cloned().collect(),
        environ: envp.iter().cloned().collect(),
    })
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
        write_to_user_stack(
            address_space,
            vaddr + remaining_in_page as u64,
            &data[remaining_in_page..],
        )?;
    }

    Ok(())
}
