//! EFFLUX Kernel
//!
//! Main kernel entry point.

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

use efflux_arch_traits::Arch;
use efflux_arch_x86_64 as arch;
use efflux_arch_x86_64::serial;
use efflux_boot_proto::{BootInfo, MemoryType as BootMemoryType};
use efflux_core::{PhysAddr, VirtAddr};
use efflux_elf::ElfExecutable;
use efflux_mm_frame::{BitmapFrameAllocator, MemoryRegion};
use efflux_mm_heap::LockedHeap;
use efflux_mm_paging::{phys_to_virt, read_cr3};
use efflux_proc::UserAddressSpace;
use efflux_proc_traits::MemoryFlags;
use efflux_syscall::SyscallContext;

/// Global kernel heap allocator
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Heap size: 16 MB
const HEAP_SIZE: usize = 16 * 1024 * 1024;

/// Static heap storage (temporary until we have proper MM)
static mut HEAP_STORAGE: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

/// Global frame allocator
static FRAME_ALLOCATOR: BitmapFrameAllocator = BitmapFrameAllocator::new();

/// User program (init.elf) embedded in kernel
static INIT_ELF: &[u8] = include_bytes!("../../userspace/init/init.elf");

/// Flag to track if user process has exited
static USER_EXITED: AtomicBool = AtomicBool::new(false);

/// Exit status from user process
static mut USER_EXIT_STATUS: i32 = 0;

/// Kernel entry point
///
/// Called by the bootloader after setting up page tables and jumping to higher half.
#[unsafe(no_mangle)]
pub extern "C" fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Initialize serial port first for early debugging
    serial::init();

    let mut writer = serial::SerialWriter;

    // Print boot banner
    let _ = writeln!(writer);
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "  EFFLUX Operating System");
    let _ = writeln!(writer, "  Version 0.1.0");
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer);

    let _ = writeln!(writer, "[INFO] Kernel started on x86_64");
    let _ = writeln!(writer, "[INFO] Serial output initialized");

    // Validate boot info
    if !boot_info.is_valid() {
        let _ = writeln!(writer, "[ERROR] Invalid boot info magic!");
        arch::X86_64::halt();
    }
    let _ = writeln!(writer, "[INFO] Boot info validated");

    // Print boot info
    let _ = writeln!(writer, "[INFO] Kernel physical base: {:#x}", boot_info.kernel_phys_base);
    let _ = writeln!(writer, "[INFO] Kernel virtual base: {:#x}", boot_info.kernel_virt_base);
    let _ = writeln!(writer, "[INFO] Kernel size: {} bytes", boot_info.kernel_size);
    let _ = writeln!(writer, "[INFO] Physical map base: {:#x}", boot_info.phys_map_base);
    let _ = writeln!(writer, "[INFO] PML4 physical: {:#x}", boot_info.pml4_phys);

    // Print memory regions
    let _ = writeln!(writer, "[INFO] Memory regions: {}", boot_info.memory_region_count);
    let mut total_usable = 0u64;
    for region in boot_info.memory_regions() {
        if matches!(region.ty, BootMemoryType::Usable | BootMemoryType::BootServices) {
            total_usable += region.len;
        }
    }
    let _ = writeln!(writer, "[INFO] Total usable memory: {} MB", total_usable / (1024 * 1024));

    // Initialize heap with static storage for now
    let _ = writeln!(writer, "[INFO] Initializing heap allocator...");
    unsafe {
        let heap_start = addr_of_mut!(HEAP_STORAGE) as usize;
        HEAP_ALLOCATOR.init(heap_start, HEAP_SIZE);
    }
    let _ = writeln!(writer, "[INFO] Heap initialized: {} KB", HEAP_SIZE / 1024);

    // Initialize frame allocator
    let _ = writeln!(writer, "[INFO] Initializing frame allocator...");
    let mut regions: Vec<MemoryRegion> = Vec::new();
    for boot_region in boot_info.memory_regions() {
        let usable = matches!(boot_region.ty, BootMemoryType::Usable | BootMemoryType::BootServices);
        regions.push(MemoryRegion::new(
            efflux_core::PhysAddr::new(boot_region.start),
            boot_region.len,
            usable,
        ));
    }
    FRAME_ALLOCATOR.init(&regions);

    // Mark kernel memory as used
    FRAME_ALLOCATOR.mark_used(
        efflux_core::PhysAddr::new(boot_info.kernel_phys_base),
        boot_info.kernel_size as usize,
    );

    let _ = writeln!(writer, "[INFO] Frame allocator initialized");
    let _ = writeln!(writer, "[INFO] Total frames: {}", FRAME_ALLOCATOR.total_frames());
    let _ = writeln!(writer, "[INFO] Free frames: {}", FRAME_ALLOCATOR.free_frame_count());

    // Initialize architecture components (GDT, IDT, APIC)
    let _ = writeln!(writer, "[INFO] Initializing x86_64 architecture...");
    unsafe {
        arch::init();
    }

    // Start timer at 100Hz
    let _ = writeln!(writer, "[INFO] Starting APIC timer at 100Hz...");
    arch::start_timer(100);

    // Enable interrupts
    let _ = writeln!(writer, "[INFO] Enabling interrupts...");
    arch::X86_64::enable_interrupts();
    let _ = writeln!(writer, "[INFO] Interrupts enabled");

    // Test heap allocation
    let _ = writeln!(writer, "[INFO] Testing heap allocation...");
    let boxed_value = Box::new(42u32);
    let _ = writeln!(writer, "[INFO] Box::new(42) = {}", *boxed_value);

    let _ = writeln!(writer);
    let _ = writeln!(writer, "EFFLUX kernel initialized successfully!");
    let _ = writeln!(writer);

    // ========================================
    // Phase 3: User Mode Test
    // ========================================

    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "  Phase 3: User Mode Test");
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer);

    // Initialize syscall mechanism
    let _ = writeln!(writer, "[USER] Initializing syscall mechanism...");
    unsafe {
        arch::syscall::init();
    }

    // Set up syscall handlers
    let _ = writeln!(writer, "[USER] Setting up syscall handlers...");
    let syscall_ctx = SyscallContext {
        console_write: Some(console_write),
        console_read: Some(console_read),
        exit: Some(user_exit),
    };
    unsafe {
        efflux_syscall::init(syscall_ctx);
    }

    // Register the syscall dispatch function
    unsafe {
        arch::syscall::set_syscall_handler(syscall_dispatch);
    }

    // Parse the embedded init.elf
    let _ = writeln!(writer, "[USER] Parsing init.elf ({} bytes)...", INIT_ELF.len());
    let elf = match ElfExecutable::parse(INIT_ELF) {
        Ok(e) => e,
        Err(err) => {
            let _ = writeln!(writer, "[USER] Failed to parse ELF: {:?}", err);
            arch::X86_64::halt();
        }
    };

    let _ = writeln!(writer, "[USER] ELF entry point: {:#x}", elf.entry_point().as_u64());
    for seg in elf.segments() {
        let _ = writeln!(writer, "[USER]   Segment: vaddr={:#x} memsz={:#x} flags={:?}",
            seg.vaddr.as_u64(), seg.mem_size, seg.flags.bits());
    }

    // Create user address space
    let _ = writeln!(writer, "[USER] Creating user address space...");
    let kernel_pml4 = read_cr3();
    let _ = writeln!(writer, "[USER] Kernel PML4: {:#x}", kernel_pml4.as_u64());

    // Create a wrapper that implements FrameAllocator
    let alloc_wrapper = FrameAllocatorWrapper;

    let mut user_space = match unsafe { UserAddressSpace::new_with_kernel(&alloc_wrapper, kernel_pml4) } {
        Some(s) => s,
        None => {
            let _ = writeln!(writer, "[USER] Failed to create user address space!");
            arch::X86_64::halt();
        }
    };
    let _ = writeln!(writer, "[USER] User PML4: {:#x}", user_space.pml4_phys().as_u64());

    // Load ELF segments into user address space
    let _ = writeln!(writer, "[USER] Loading ELF segments...");
    for seg in elf.segments() {
        // Allocate pages for this segment
        let (page_base, page_size) = efflux_elf::ElfLoader::segment_pages(seg);
        let num_pages = page_size / 4096;
        let page_offset = efflux_elf::ElfLoader::segment_page_offset(seg);

        let _ = writeln!(writer, "[USER]   Loading segment at {:#x} ({} pages)",
            page_base.as_u64(), num_pages);

        // Allocate pages
        if let Err(e) = user_space.allocate_pages(page_base, num_pages, seg.flags, &alloc_wrapper) {
            let _ = writeln!(writer, "[USER] Failed to allocate pages: {:?}", e);
            arch::X86_64::halt();
        }

        // Copy segment data
        let seg_data = elf.segment_data(seg);
        if !seg_data.is_empty() {
            // Get the physical address of the first page
            let phys = user_space.translate(page_base).unwrap();
            let dest_virt = phys_to_virt(phys);
            let dest = unsafe { dest_virt.as_mut_ptr::<u8>().add(page_offset) };

            unsafe {
                core::ptr::copy_nonoverlapping(seg_data.as_ptr(), dest, seg_data.len());
            }

            // Zero any remaining memory (BSS)
            if seg.mem_size > seg.file_size {
                let bss_start = unsafe { dest.add(seg.file_size) };
                let bss_size = seg.mem_size - seg.file_size;
                unsafe {
                    core::ptr::write_bytes(bss_start, 0, bss_size);
                }
            }
        }
    }

    // Allocate user stack
    let _ = writeln!(writer, "[USER] Allocating user stack...");
    let user_stack_base = VirtAddr::new(0x7FFF_F000_0000);
    let user_stack_pages = 4; // 16 KB stack
    let stack_flags = MemoryFlags::READ.union(MemoryFlags::WRITE).union(MemoryFlags::USER);

    if let Err(e) = user_space.allocate_pages(user_stack_base, user_stack_pages, stack_flags, &alloc_wrapper) {
        let _ = writeln!(writer, "[USER] Failed to allocate user stack: {:?}", e);
        arch::X86_64::halt();
    }

    let user_stack_top = VirtAddr::new(user_stack_base.as_u64() + (user_stack_pages * 4096) as u64);
    let _ = writeln!(writer, "[USER] User stack: {:#x} - {:#x}",
        user_stack_base.as_u64(), user_stack_top.as_u64());

    // Allocate kernel stack for syscalls and interrupts
    let _ = writeln!(writer, "[USER] Allocating kernel stack...");
    let kernel_stack: Box<[u8; 16384]> = Box::new([0u8; 16384]);
    let kernel_stack_ptr = Box::into_raw(kernel_stack);
    let kernel_stack_top = unsafe { (kernel_stack_ptr as *const u8).add(16384) as u64 };

    // Set kernel stack for:
    // 1. Syscalls (stored in GS base for syscall handler)
    // 2. Interrupts (TSS.RSP0 for privilege level changes)
    unsafe {
        arch::syscall::set_kernel_stack(kernel_stack_top);
    }
    arch::gdt::set_kernel_stack(kernel_stack_top);  // TSS.RSP0 for interrupts

    // Allocate a stack for double fault handling (IST1)
    let df_stack: Box<[u8; 8192]> = Box::new([0u8; 8192]);
    let df_stack_ptr = Box::into_raw(df_stack);
    let df_stack_top = unsafe { (df_stack_ptr as *const u8).add(8192) as u64 };
    arch::gdt::set_ist(0, df_stack_top);  // IST1 = ist[0]

    // Get the entry point before switching
    let entry_point = elf.entry_point().as_u64();
    let _ = writeln!(writer, "[USER] Kernel stack top: {:#x}", kernel_stack_top);

    let _ = writeln!(writer);
    let _ = writeln!(writer, "[USER] Jumping to user mode at {:#x}...", entry_point);
    let _ = writeln!(writer);

    // Debug: verify user page table entries before switching
    let _ = writeln!(writer, "[USER] Verifying page tables...");

    // Print kernel PML4 entries
    let kernel_pml4_virt = efflux_mm_paging::phys_to_virt(PhysAddr::new(boot_info.pml4_phys));
    unsafe {
        let pml4 = &*(kernel_pml4_virt.as_ptr::<efflux_mm_paging::PageTable>());
        let _ = writeln!(writer, "[USER] Kernel PML4[256] = {:#x}", pml4[256].raw());
        let _ = writeln!(writer, "[USER] Kernel PML4[511] = {:#x}", pml4[511].raw());
    }

    // Print user PML4 entries
    let user_pml4_virt = efflux_mm_paging::phys_to_virt(user_space.pml4_phys());
    unsafe {
        let pml4 = &*(user_pml4_virt.as_ptr::<efflux_mm_paging::PageTable>());
        let _ = writeln!(writer, "[USER] User PML4[256] = {:#x}", pml4[256].raw());
        let _ = writeln!(writer, "[USER] User PML4[511] = {:#x}", pml4[511].raw());
    }

    // Check user code mapping at 0x400000
    let _ = writeln!(writer, "[USER] User code mapping test:");
    if let Some(phys) = user_space.translate(VirtAddr::new(0x400000)) {
        let _ = writeln!(writer, "[USER]   0x400000 -> {:#x}", phys.as_u64());
    } else {
        let _ = writeln!(writer, "[USER]   0x400000 -> NOT MAPPED!");
    }

    // Use the combined enter_usermode function that:
    // 1. Switches to kernel stack (in higher half)
    // 2. Switches page tables
    // 3. Jumps to user mode
    let _ = writeln!(writer, "[USER] Entering user mode...");
    unsafe {
        arch::usermode::enter_usermode(
            kernel_stack_top,
            user_space.pml4_phys().as_u64(),
            entry_point,
            user_stack_top.as_u64(),
        );
    }
}

/// Syscall dispatch function
fn syscall_dispatch(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    efflux_syscall::dispatch(number, arg1, arg2, arg3, arg4, arg5, arg6)
}

/// Console write function for syscalls
fn console_write(data: &[u8]) {
    let mut writer = serial::SerialWriter;
    for &byte in data {
        let _ = writer.write_char(byte as char);
    }
}

/// Console read function for syscalls
fn console_read(_buf: &mut [u8]) -> usize {
    // For now, just return 0 (EOF)
    0
}

/// User exit function
fn user_exit(status: i32) -> ! {
    let mut writer = serial::SerialWriter;

    unsafe {
        USER_EXIT_STATUS = status;
    }
    USER_EXITED.store(true, Ordering::SeqCst);

    let _ = writeln!(writer);
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "  User Process Exited");
    let _ = writeln!(writer, "  Exit Status: {}", status);
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer);

    if status == 0 {
        let _ = writeln!(writer, "SUCCESS: User process completed successfully!");
    } else {
        let _ = writeln!(writer, "User process exited with non-zero status");
    }

    let _ = writeln!(writer);
    let _ = writeln!(writer, "[INFO] Phase 3 test complete. Halting.");

    arch::X86_64::halt();
}

/// Wrapper to use the global frame allocator
struct FrameAllocatorWrapper;

impl efflux_mm_traits::FrameAllocator for FrameAllocatorWrapper {
    fn alloc_frame(&self) -> Option<PhysAddr> {
        FRAME_ALLOCATOR.alloc_frame()
    }

    fn free_frame(&self, addr: PhysAddr) {
        FRAME_ALLOCATOR.free_frame(addr);
    }

    fn alloc_frames(&self, count: usize) -> Option<PhysAddr> {
        FRAME_ALLOCATOR.alloc_frames(count)
    }

    fn free_frames(&self, addr: PhysAddr, count: usize) {
        FRAME_ALLOCATOR.free_frames(addr, count);
    }
}

/// Panic handler
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut writer = serial::SerialWriter;

    let _ = writeln!(writer);
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "  KERNEL PANIC!");
    let _ = writeln!(writer, "========================================");

    if let Some(location) = info.location() {
        let _ = writeln!(writer, "Location: {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }

    let _ = writeln!(writer, "Message: {}", info.message());

    let _ = writeln!(writer);
    let _ = writeln!(writer, "System halted.");

    arch::X86_64::halt()
}

/// Allocation error handler
#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    let mut writer = serial::SerialWriter;
    let _ = writeln!(writer, "ALLOCATION ERROR: size={}, align={}",
        layout.size(), layout.align());
    arch::X86_64::halt()
}
