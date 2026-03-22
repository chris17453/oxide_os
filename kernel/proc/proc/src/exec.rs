//! Exec implementation
//!
//! Implements the exec() system call, replacing the current process image
//! with a new executable. Now uses arch-traits for address space layout,
//! TLS layout, and process context creation — no more hardcoded x86_64 constants.
//!
//! — BlackLatch: the function that ends one life and begins another. Every
//! userspace process passes through here at least once. Get it wrong and
//! nothing runs. Get it really wrong and the kernel dies too.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use elf::{AuxEntry, AuxType, ElfExecutable, ElfLoader, Elf64ProgramHeader};
use mm_paging::phys_to_virt;
use mm_traits::FrameAllocator;
use os_core::{PhysAddr, VirtAddr};
use proc_traits::MemoryFlags;
use smp;

use mm_vma::{VmArea, VmFlags, VmType};

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
    /// Interpreter not found on filesystem
    InterpreterNotFound,
    /// Interpreter ELF is invalid
    InvalidInterpreter,
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
    /// ASLR-randomized mmap base for this process image.
    ///
    /// — ColdCipher: Fresh exec, fresh layout. Caller must install this into
    /// ProcessMeta::next_mmap_addr so all subsequent mmap(NULL) calls start
    /// from the jittered base rather than the predictable default.
    pub mmap_base: u64,
}

/// User stack size (1MB)
const USER_STACK_SIZE: usize = 1024 * 1024;

/// Maximum random downward shift for the user stack top.
///
/// — ColdCipher: 4MB window, page-aligned mask. 1023 pages of entropy.
/// The 1MB stack still fits — worst-case bottom is ~5MB below the ceiling.
/// Nobody wins by guessing 1-in-1024. — ColdCipher
const ASLR_STACK_ENTROPY: u64 = 0x3F_F000; // 4MB - 4KB, page-aligned mask

/// Maximum random downward shift for the mmap base on exec.
///
/// — ColdCipher: 4MB page-aligned mask — same window as the stack entropy.
/// Combined with stack ASLR, the cost to brute-force the full layout doubles.
const ASLR_MMAP_ENTROPY: u64 = 0x3FF_F000; // 4MB - 4KB, page-aligned mask

/// Address space layout constants — pulled from arch traits at compile time.
/// — BlackLatch: no more hardcoded 0x7FFF_FFFF_0000. The arch layer decides
/// where things go, exec just uses the values.
mod layout {
    use arch_traits::ExecConfig;

    // — BlackLatch: type alias to avoid repeating the cfg dance everywhere
    #[cfg(feature = "arch-x86_64")]
    type A = arch_x86_64::X86_64;

    pub const USER_STACK_TOP: u64 = A::USER_STACK_TOP;
    pub const MMAP_BASE_DEFAULT: u64 = A::MMAP_BASE_DEFAULT;
    pub const TLS_BASE: u64 = A::TLS_BASE;
    pub const USER_ADDR_LIMIT: u64 = A::USER_ADDR_LIMIT;
    pub const ELF_MACHINE: u16 = A::ELF_MACHINE_EXEC;
}

/// TLS layout helpers — pulled from arch traits.
mod tls_arch {
    use arch_traits::TlsLayout;

    #[cfg(feature = "arch-x86_64")]
    type A = arch_x86_64::X86_64;

    pub const TCB_SIZE: usize = A::TCB_SIZE;

    #[inline]
    pub fn thread_pointer(alloc_base: u64, mem_size: usize) -> u64 {
        A::thread_pointer(alloc_base, mem_size)
    }

    #[inline]
    pub fn tls_data_offset() -> usize {
        A::tls_data_offset()
    }
}

/// Process context creation — dispatches through arch-traits ProcessContextOps.
/// — SableWire: the arch crate knows the right rflags/cs/ss values for user mode.
/// We call through the trait, which returns X86_64ProcessContext (or AArch64Context, etc),
/// then convert to proc's ProcessContext. No x86_64 constants in the proc crate.
fn new_user_context(entry: u64, sp: u64, tls_base: u64) -> ProcessContext {
    #[cfg(feature = "arch-x86_64")]
    {
        use arch_traits::ProcessContextOps;
        let arch_ctx = arch_x86_64::X86_64::new_user_context(entry, sp, tls_base);
        // — SableWire: map arch-specific context to proc's ProcessContext.
        // The field names match because ProcessContext was designed for x86_64.
        // When adding AArch64, ProcessContext needs to become generic or use
        // the arch trait's associated type directly.
        ProcessContext {
            rip: arch_ctx.rip,
            rsp: arch_ctx.rsp,
            rflags: arch_ctx.rflags,
            rax: arch_ctx.rax,
            rbx: arch_ctx.rbx,
            rcx: arch_ctx.rcx,
            rdx: arch_ctx.rdx,
            rsi: arch_ctx.rsi,
            rdi: arch_ctx.rdi,
            rbp: arch_ctx.rbp,
            r8: arch_ctx.r8,
            r9: arch_ctx.r9,
            r10: arch_ctx.r10,
            r11: arch_ctx.r11,
            r12: arch_ctx.r12,
            r13: arch_ctx.r13,
            r14: arch_ctx.r14,
            r15: arch_ctx.r15,
            cs: arch_ctx.cs,
            ss: arch_ctx.ss,
            fs_base: arch_ctx.fs_base,
            gs_base: arch_ctx.gs_base,
        }
    }
    #[cfg(not(feature = "arch-x86_64"))]
    {
        // — SableWire: other arches will implement their own mapping.
        // For now, default context with entry/sp/tls set.
        let mut ctx = ProcessContext::default();
        ctx.rip = entry;
        ctx.rsp = sp;
        ctx.fs_base = tls_base;
        ctx
    }
}

/// Execute a new program
///
/// Creates a new address space and loads the ELF binary into it.
/// Returns ExecResult with all data needed to update the process.
/// The caller is responsible for updating Task and ProcessMeta.
///
/// — BlackLatch: now supports dynamically-linked executables via PT_INTERP.
/// If the ELF has a PT_INTERP segment, exec loads both the main binary AND
/// the interpreter, sets up an auxiliary vector on the stack, and jumps to
/// the interpreter's entry point instead of the main binary's.
///
/// # Arguments
/// * `elf_data` - ELF binary data
/// * `argv` - Command-line arguments
/// * `envp` - Environment variables
/// * `allocator` - Frame allocator for memory allocation
/// * `kernel_pml4` - Kernel PML4 for copying kernel mappings
/// * `interp_data` - Optional interpreter ELF data (read by caller from VFS via PT_INTERP path)
pub fn do_exec<A: FrameAllocator>(
    elf_data: &[u8],
    argv: &[String],
    envp: &[String],
    allocator: &A,
    kernel_pml4: PhysAddr,
    interp_data: Option<&[u8]>,
) -> Result<ExecResult, ExecError> {
    // Parse ELF with arch-provided constants
    let elf = ElfExecutable::parse_with_arch(elf_data, layout::ELF_MACHINE, layout::USER_ADDR_LIMIT)
        .map_err(|_e| ExecError::InvalidElf)?;

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

    let entry_point = elf.entry_point();
    let entry_addr = entry_point.as_u64();
    let mut entry_in_exec_segment = false;

    // Load segments
    for segment in elf.segments() {
        let (page_start, total_size) = ElfLoader::segment_pages(segment);
        let page_offset = ElfLoader::segment_page_offset(segment);
        let num_pages = total_size / 4096;
        let seg_start = page_start.as_u64();
        let seg_end = seg_start.saturating_add(total_size as u64);

        if segment.flags.contains(MemoryFlags::EXECUTE)
            && entry_addr >= seg_start
            && entry_addr < seg_end
        {
            entry_in_exec_segment = true;
        }

        // Get segment data
        let seg_data = elf.segment_data(segment);

        // Allocate and map each page
        for i in 0..num_pages {
            let page_virt = VirtAddr::new(page_start.as_u64() + (i as u64 * 4096));

            // Check if this page is already mapped (overlapping segments)
            let frame_virt = if let Some(existing_phys) = new_address_space.translate(page_virt) {
                // — GraveShift: Page already mapped by a previous segment. Union
                // permissions — if EITHER segment wants WRITE or EXECUTE, the page
                // gets it. The old code only added WRITE, so a RO .rodata segment
                // overlapping an X .text segment would set NO_EXECUTE on the shared
                // page, nuking executable code mid-page. Classic #UD at 3 AM.
                new_address_space.update_user_page_flags(page_virt, segment.flags);
                phys_to_virt(existing_phys)
            } else {
                // Allocate new frame
                let frame = allocator.alloc_frame().ok_or(ExecError::OutOfMemory)?;

                // Zero the frame
                let frame_virt = phys_to_virt(frame);
                unsafe {
                    core::ptr::write_bytes(frame_virt.as_mut_ptr::<u8>(), 0, 4096);
                }

                // — ColdCipher: Map the page. If mapping fails (PT allocation OOM),
                // free the data frame we just allocated. Without this, the frame is
                // orphaned — not in the page tables (so Drop can't find it), not in
                // allocated_frames (by design). One leaked frame per failed exec
                // under memory pressure.
                if let Err(_) = unsafe {
                    new_address_space.map_user_page(page_virt, frame, segment.flags, allocator)
                } {
                    allocator.free_frame(frame);
                    return Err(ExecError::OutOfMemory);
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

        // — Hexline: Trace segment mapping so we can triage faults against
        // the actual page table layout. Costs nothing in release builds.
        #[cfg(debug_assertions)]
        {
            extern crate os_log;
            unsafe {
                os_log::write_str_raw("[EXEC-SEG] ");
                os_log::write_u64_hex_raw(page_start.as_u64());
                os_log::write_str_raw("-");
                os_log::write_u64_hex_raw(page_start.as_u64() + total_size as u64);
                if segment.flags.contains(MemoryFlags::WRITE) {
                    os_log::write_str_raw(" RW");
                } else {
                    os_log::write_str_raw(" RO");
                }
                if segment.flags.contains(MemoryFlags::EXECUTE) {
                    os_log::write_str_raw(" X");
                }
                os_log::write_str_raw("\n");
            }
        }

        // — NeonRoot: Register a VMA for this ELF segment. Now the kernel knows
        // "0x400000-0x402000 is .text" instead of guessing from page table flags.
        let seg_end = page_start.as_u64() + total_size as u64;
        let vm_flags = mem_flags_to_vm(segment.flags);
        let vm_type = if segment.flags.contains(MemoryFlags::EXECUTE) {
            VmType::Text
        } else if segment.flags.contains(MemoryFlags::WRITE) {
            VmType::Data
        } else {
            VmType::Data
        };
        let _ = new_address_space.add_vma(VmArea::new(page_start.as_u64(), seg_end, vm_flags, vm_type));
    }

    // Set up TLS (Thread-Local Storage) if needed
    // — Hexline: ELF parser handles PT_TLS correctly now. Manual fallback retained
    // for safety until we've validated across all userspace binaries.
    let parser_tls = elf.tls_template();

    // Fallback: manual PT_TLS scan if parser missed it
    // — Hexline: This catches edge cases where the parser's segment array fills up
    let manual_tls = if parser_tls.is_none() {
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
                    if ph.p_type == 7 {
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
    } else {
        None
    };

    // — Hexline: Parser-first, manual-fallback. Log when fallback catches something.
    #[cfg(debug_assertions)]
    if parser_tls.is_none() && manual_tls.is_some() {
        extern crate os_log;
        os_log::println!("[TLS] WARNING: ELF parser missed PT_TLS, manual fallback used");
    }

    let tls_template_to_use = parser_tls.or(manual_tls.as_ref());
    let tls_base = if let Some(tls_template) = tls_template_to_use {
        // — GraveShift: TLS setup now uses arch-traits for layout calculations.
        // No more hardcoded Variant II assumptions — the arch layer decides
        // where the thread pointer goes relative to the data.
        let tls_size = tls_template.mem_size;
        let total_size = tls_arch::TCB_SIZE + tls_size;

        // Align to page boundary
        let pages_needed = (total_size + 4095) / 4096;
        // — ColdCipher: TLS lives BELOW the mmap region so they can't collide.
        let tls_vaddr = VirtAddr::new(layout::TLS_BASE);

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

        // — GraveShift: Calculate thread pointer using arch trait.
        // x86_64 Variant II: TP = alloc_base + mem_size (points to TCB after data)
        // AArch64 Variant I: TP = alloc_base (points to TCB before data)
        let tp = tls_arch::thread_pointer(tls_vaddr.as_u64(), tls_size);

        // Write self-pointer to TCB (required by TLS ABI — TP:0 = tp)
        write_to_user_stack(&new_address_space, tp, &tp.to_le_bytes())?;

        // Copy TLS initialization data using arch-specific offset
        let data_dest = tls_vaddr.as_u64() + tls_arch::tls_data_offset() as u64;
        let tls_data = elf.tls_data();
        if !tls_data.is_empty() {
            write_to_user_stack(&new_address_space, data_dest, tls_data)?;
        }
        // BSS portion (mem_size - file_size) is already zero from page allocation

        // — NeonRoot: Register the TLS VMA so /proc and fault handlers know about it.
        let tls_end = tls_vaddr.as_u64() + (pages_needed * 4096) as u64;
        let _ = new_address_space.add_vma(VmArea::new_named(
            tls_vaddr.as_u64(),
            tls_end,
            VmFlags::READ | VmFlags::WRITE,
            VmType::Tls,
            b"[tls]",
        ));

        Some(tp)
    } else {
        None
    };

    // — ColdCipher: ASLR — randomize the stack top within a 4MB window.
    let stack_aslr_shift = crate::meta::aslr_random() & ASLR_STACK_ENTROPY;
    let randomized_stack_top = layout::USER_STACK_TOP - stack_aslr_shift;

    // — ColdCipher: mmap base ASLR — fresh jitter per exec.
    let mmap_aslr_shift = crate::meta::aslr_random() & ASLR_MMAP_ENTROPY;
    let randomized_mmap_base = layout::MMAP_BASE_DEFAULT - mmap_aslr_shift;

    // Set up user stack
    let stack_pages = USER_STACK_SIZE / 4096;
    let stack_bottom = VirtAddr::new(randomized_stack_top - USER_STACK_SIZE as u64);

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

    // — NeonRoot: Register the user stack VMA.
    let _ = new_address_space.add_vma(VmArea::new_named(
        stack_bottom.as_u64(),
        layout::USER_STACK_TOP,
        VmFlags::READ | VmFlags::WRITE | VmFlags::GROWSDOWN | VmFlags::STACK,
        VmType::Stack,
        b"[stack]",
    ));

    // — VeilAudit: Never transfer control to an address outside executable PT_LOAD
    // mappings. Corrupted/partial ELF reads can produce garbage e_entry values.
    if !entry_in_exec_segment {
        return Err(ExecError::InvalidElf);
    }

    // =========================================================================
    // PT_INTERP handling — dynamic linking support
    // — BlackLatch: if the ELF has a PT_INTERP segment AND the caller provided
    // interpreter data, load the interpreter into the address space above the
    // mmap base. The interpreter's entry point becomes the actual entry —
    // it reads AT_ENTRY from the aux vector to find the main executable later.
    // =========================================================================
    let has_interp = elf.interp().is_some();
    let mut interp_base: u64 = 0;
    let mut interp_entry: u64 = 0;

    if has_interp {
        if let Some(idata) = interp_data {
            // — BlackLatch: load the interpreter ELF into the address space.
            // The interpreter is loaded at its LINKED address (from its own LOAD
            // segments). It's built with a non-conflicting base address (e.g., 0x200000)
            // so it doesn't overlap with the main executable at 0x400000.
            // This avoids the need for self-relocation in the interpreter.
            let interp_elf = ElfExecutable::parse_with_arch(
                idata,
                layout::ELF_MACHINE,
                layout::USER_ADDR_LIMIT,
            ).map_err(|_| ExecError::InvalidInterpreter)?;

            // — BlackLatch: for PIE interpreters (ET_DYN, min_vaddr near 0), pick a
            // load address above the main executable. For ET_EXEC interpreters
            // (linked at a fixed address like 0x200000), load at their linked address.
            let interp_min_vaddr = interp_elf.min_vaddr();
            let base_offset: u64 = if interp_elf.is_pie() || interp_min_vaddr < 0x10000 {
                // — BlackLatch: PIE interpreter — load at 0x200000 (above NULL page, below exe)
                0x200000 - interp_min_vaddr
            } else {
                // — BlackLatch: fixed-address interpreter — load at linked address
                0
            };

            for segment in interp_elf.segments() {
                let adjusted_vaddr = segment.vaddr.as_u64() + base_offset;
                let adjusted_seg = elf::LoadSegment {
                    vaddr: VirtAddr::new(adjusted_vaddr),
                    mem_size: segment.mem_size,
                    file_offset: segment.file_offset,
                    file_size: segment.file_size,
                    flags: segment.flags,
                };

                let (page_start, total_size) = ElfLoader::segment_pages(&adjusted_seg);
                let page_offset = ElfLoader::segment_page_offset(&adjusted_seg);
                let num_pages = total_size / 4096;
                let seg_data = interp_elf.segment_data(segment);

                for i in 0..num_pages {
                    let page_virt = VirtAddr::new(page_start.as_u64() + (i as u64 * 4096));

                    let frame_virt = if let Some(existing_phys) = new_address_space.translate(page_virt) {
                        new_address_space.update_user_page_flags(page_virt, segment.flags);
                        phys_to_virt(existing_phys)
                    } else {
                        let frame = allocator.alloc_frame().ok_or(ExecError::OutOfMemory)?;
                        let frame_virt = phys_to_virt(frame);
                        unsafe { core::ptr::write_bytes(frame_virt.as_mut_ptr::<u8>(), 0, 4096); }
                        if let Err(_) = unsafe {
                            new_address_space.map_user_page(page_virt, frame, segment.flags, allocator)
                        } {
                            allocator.free_frame(frame);
                            return Err(ExecError::OutOfMemory);
                        }
                        frame_virt
                    };

                    // Copy data from interpreter segment
                    let page_start_in_segment = i * 4096;
                    let data_start_in_page = if i == 0 { page_offset } else { 0 };

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

                // — NeonRoot: register VMA for interpreter segment
                let seg_end = page_start.as_u64() + total_size as u64;
                let vm_flags = mem_flags_to_vm(segment.flags);
                let _ = new_address_space.add_vma(VmArea::new_named(
                    page_start.as_u64(),
                    seg_end,
                    vm_flags,
                    VmType::Text,
                    b"[interp]",
                ));
            }

            interp_base = interp_elf.min_vaddr();
            // — BlackLatch: interpreter entry point (no relocation needed)
            interp_entry = interp_elf.entry_point().as_u64() + base_offset;

            #[cfg(debug_assertions)]
            {
                extern crate os_log;
                unsafe {
                    os_log::write_str_raw("[EXEC] Loaded interpreter at ");
                    os_log::write_u64_hex_raw(interp_base);
                    os_log::write_str_raw(", entry=");
                    os_log::write_u64_hex_raw(interp_entry);
                    os_log::write_str_raw("\n");
                }
            }
        }
        // — BlackLatch: if has_interp but no interp_data, caller didn't provide
        // the interpreter. This is NOT an error for backward compat — static
        // binaries that somehow have a stale PT_INTERP will just run directly.
    }

    // — BlackLatch: if interpreter was loaded, jump to it. Otherwise, main exe.
    let actual_entry = if interp_entry != 0 {
        interp_entry
    } else {
        entry_point.as_u64()
    };

    // Set up argv and envp on the stack, with optional auxiliary vector
    // Stack layout (growing down):
    // [random bytes (16)]  <- AT_RANDOM points here
    // [AT_NULL entry]      <- aux vector terminator
    // [aux entries...]     <- auxiliary vector (only if PT_INTERP present)
    // [NULL]               <- envp terminator
    // [envp[n-1]]          <- pointers to env strings
    // ...
    // [envp[0]]
    // [NULL]               <- argv terminator
    // [argv[n-1]]          <- pointers to arg strings
    // ...
    // [argv[0]]
    // [argc]               <- number of arguments
    // <- rsp points here

    let mut stack_ptr = randomized_stack_top;

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

    // — WireSaint: Reserve space for random bytes (16 bytes for AT_RANDOM)
    stack_ptr -= 16;
    stack_ptr &= !0xF;
    let random_bytes_addr = stack_ptr;

    // Write 16 pseudo-random bytes for AT_RANDOM
    // — ColdCipher: stack canary seed, ASLR entropy source. Not cryptographic
    // but good enough to defeat non-adaptive attacks. Use TSC as entropy source.
    let rand_seed = crate::meta::aslr_random();
    let rand_bytes: [u8; 16] = {
        let a = rand_seed.to_le_bytes();
        let b = crate::meta::aslr_random().to_le_bytes();
        let mut buf = [0u8; 16];
        buf[..8].copy_from_slice(&a);
        buf[8..].copy_from_slice(&b);
        buf
    };
    write_to_user_stack(&new_address_space, random_bytes_addr, &rand_bytes)?;

    // Build auxiliary vector entries
    // — WireSaint: the aux vector tells the dynamic linker about the loaded binary.
    // Even for static binaries, AT_PAGESZ and AT_RANDOM are useful (musl reads them).
    let mut aux_entries: Vec<AuxEntry> = Vec::with_capacity(12);

    // Always provide basic entries
    aux_entries.push(AuxEntry::new(AuxType::AtPagesz, 4096));
    aux_entries.push(AuxEntry::new(AuxType::AtRandom, random_bytes_addr));
    aux_entries.push(AuxEntry::new(AuxType::AtEntry, entry_point.as_u64()));

    // Add dynamic linking entries if PT_INTERP is present
    if has_interp {
        if let Some(phdr_info) = elf.phdr() {
            aux_entries.push(AuxEntry::new(AuxType::AtPhdr, phdr_info.vaddr));
            aux_entries.push(AuxEntry::new(AuxType::AtPhent, phdr_info.entry_size as u64));
            aux_entries.push(AuxEntry::new(AuxType::AtPhnum, phdr_info.count as u64));
        } else {
            // — WireSaint: fallback — compute AT_PHDR from ELF header offset
            let header = elf.header();
            let phoff = header.e_phoff;
            // Find the first LOAD segment that contains the phdr table
            for seg in elf.segments() {
                let seg_start = seg.vaddr.as_u64();
                if phoff >= seg.file_offset as u64
                    && phoff < (seg.file_offset + seg.file_size) as u64
                {
                    let phdr_vaddr = seg_start + (phoff - seg.file_offset as u64);
                    aux_entries.push(AuxEntry::new(AuxType::AtPhdr, phdr_vaddr));
                    break;
                }
            }
            aux_entries.push(AuxEntry::new(AuxType::AtPhent, header.e_phentsize as u64));
            aux_entries.push(AuxEntry::new(AuxType::AtPhnum, header.e_phnum as u64));
        }
        aux_entries.push(AuxEntry::new(AuxType::AtBase, interp_base));
    }

    // Null terminator
    aux_entries.push(AuxEntry::null());

    // Calculate space for pointers + aux vector
    let aux_size = aux_entries.len() * 16; // Each AuxEntry is 16 bytes (u64 + u64)
    let pointers_size = ((envp.len() + 1) + (argv.len() + 1) + 1) * 8;
    stack_ptr -= (pointers_size + aux_size) as u64;
    stack_ptr &= !0xF; // 16-byte align

    let final_rsp = VirtAddr::new(stack_ptr);

    // DEBUG: print stack layout
    #[cfg(feature = "debug-fork")]
    {
        extern crate os_log;
        os_log::debug!("[EXEC] Stack layout:");
        os_log::debug!("[EXEC]   USER_STACK_TOP (ASLR) = {:#x}", randomized_stack_top);
        os_log::debug!("[EXEC]   strings_base = {:#x}", strings_base);
        os_log::debug!("[EXEC]   string_data_size = {}", string_data_size);
        os_log::debug!("[EXEC]   pointers_size = {}", pointers_size);
        os_log::debug!("[EXEC]   aux_entries = {} ({} bytes)", aux_entries.len(), aux_size);
        os_log::debug!("[EXEC]   final_rsp = {:#x}", stack_ptr);
        os_log::debug!("[EXEC]   argc will be at {:#x}", stack_ptr);
        os_log::debug!("[EXEC]   argv[0] ptr will be at {:#x}", stack_ptr + 8);
        if !string_offsets_argv.is_empty() {
            os_log::debug!(
                "[EXEC]   argv[0] will point to {:#x}",
                string_offsets_argv[0]
            );
            if string_offsets_argv.len() > 1 {
                os_log::debug!(
                    "[EXEC]   argv[1] will point to {:#x}",
                    string_offsets_argv[1]
                );
            }
        }
        if has_interp {
            os_log::debug!("[EXEC]   PT_INTERP present — aux vector on stack");
        }
    }

    // Write strings to stack
    let mut string_ptr = strings_base;
    for (i, arg) in argv.iter().enumerate() {
        #[cfg(feature = "debug-fork")]
        {
            extern crate os_log;
            os_log::debug!(
                "[EXEC] Writing argv[{}] = \"{}\" to vaddr {:#x}",
                i,
                arg,
                string_ptr
            );
        }
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
    let argc_val = argv.len() as u64;

    // GraveShift: Write argc at [final_rsp]. _start reads it via mov r12d, [rsp].
    let argc_bytes = argc_val.to_le_bytes();
    write_to_user_stack(&new_address_space, ptr, &argc_bytes)?;

    ptr += 8;

    // Write argv pointers
    for (i, &offset) in string_offsets_argv.iter().enumerate() {
        #[cfg(feature = "debug-fork")]
        {
            extern crate os_log;
            os_log::debug!(
                "[EXEC] Writing argv[{}] pointer = {:#x} at stack offset {:#x}",
                i,
                offset,
                ptr
            );
        }
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
    ptr += 8;

    // — WireSaint: Write auxiliary vector entries after envp NULL terminator
    for aux in &aux_entries {
        write_to_user_stack(&new_address_space, ptr, &aux.a_type.to_le_bytes())?;
        ptr += 8;
        write_to_user_stack(&new_address_space, ptr, &aux.a_val.to_le_bytes())?;
        ptr += 8;
    }

    // — SableWire: Create context using arch-aware helper. The arch layer knows
    // the right rflags/cs/ss values for user mode on this architecture.
    let context = new_user_context(
        actual_entry,
        final_rsp.as_u64(),
        tls_base.unwrap_or(0),
    );

    // — BlackLatch: flush_tlb_all() only reloads CR3 on THIS core. Shoot down
    // stale entries on all CPUs before handing control to the new image.
    smp::tlb_shootdown(0, u64::MAX, 0);

    Ok(ExecResult {
        address_space: new_address_space,
        entry_point: VirtAddr::new(actual_entry),
        stack_pointer: final_rsp,
        context,
        cmdline: argv.iter().cloned().collect(),
        environ: envp.iter().cloned().collect(),
        mmap_base: randomized_mmap_base,
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

/// — NeonRoot: Convert proc-traits MemoryFlags to VMA VmFlags.
/// The mapping is straightforward — both encode R/W/X. VmFlags adds
/// semantic bits (GROWSDOWN, STACK, DONTCOPY) that MemoryFlags doesn't have.
fn mem_flags_to_vm(mf: MemoryFlags) -> VmFlags {
    let mut vf = VmFlags::empty();
    if mf.readable() {
        vf |= VmFlags::READ;
    }
    if mf.writable() {
        vf |= VmFlags::WRITE;
    }
    if mf.executable() {
        vf |= VmFlags::EXEC;
    }
    vf
}
