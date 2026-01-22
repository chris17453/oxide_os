//! OXIDE Kernel
//!
//! Main kernel entry point.

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

#[macro_use]
mod debug;

/// Get a serial writer for debug output
pub fn serial_writer() -> arch_x86_64::serial::SerialWriter {
    arch_x86_64::serial::SerialWriter
}

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

use arch_traits::Arch;
use arch_x86_64 as arch;
use arch_x86_64::serial;
use arch_x86_64::get_user_context;
use boot_proto::{BootInfo, MemoryType as BootMemoryType};
use os_core::{PhysAddr, VirtAddr};
use elf::ElfExecutable;
use mm_frame::{BitmapFrameAllocator, MemoryRegion};
use mm_traits::FrameAllocator as _;
use mm_heap::LockedHeap;
use mm_paging::{phys_to_virt, read_cr3, write_cr3, flush_tlb_all};
use proc::{
    UserAddressSpace, Process, ProcessContext, alloc_pid, process_table,
    do_fork, do_exec, do_waitpid, WaitOptions, handle_cow_fault,
};
use proc_traits::{MemoryFlags, Pid};
use syscall::SyscallContext;
use vfs::{File, FileFlags, mount::GLOBAL_VFS, MountFlags, VnodeOps, VnodeType};
use devfs::DevFs;
use tmpfs::TmpDir;
use procfs::ProcFs;
use initramfs;
use pty::{PtyManager, PtsDir};
use net::{self, NetworkDevice};
use tcpip;
use virtio_net;
use pci;
use fb;
use terminal;
use input;
use spin::Mutex;

/// Global kernel heap allocator
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Heap size: 16 MB
const HEAP_SIZE: usize = 16 * 1024 * 1024;

/// Static heap storage (temporary until we have proper MM)
static mut HEAP_STORAGE: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

/// Global frame allocator
static FRAME_ALLOCATOR: BitmapFrameAllocator = BitmapFrameAllocator::new();

/// Flag to track if user process has exited
static USER_EXITED: AtomicBool = AtomicBool::new(false);

/// Exit status from user process
static mut USER_EXIT_STATUS: i32 = 0;

/// Kernel PML4 physical address (for creating new address spaces)
static mut KERNEL_PML4: u64 = 0;

/// Child processes waiting to be run
static PENDING_CHILDREN: Mutex<Vec<Pid>> = Mutex::new(Vec::new());

/// Full parent context for returning from child process
/// Stores all registers so parent can resume with correct state
#[derive(Clone)]
struct ParentContext {
    pid: u32,
    pml4: u64,
    rip: u64,
    rsp: u64,
    rflags: u64,
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

/// Saved parent context for returning from child process
static PARENT_CONTEXT: Mutex<Option<ParentContext>> = Mutex::new(None);

/// Flag indicating a child has exited and we should return to parent
static CHILD_DONE: AtomicBool = AtomicBool::new(false);

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
    let _ = writeln!(writer, "  OXIDE Operating System");
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
            os_core::PhysAddr::new(boot_region.start),
            boot_region.len,
            usable,
        ));
    }
    FRAME_ALLOCATOR.init(&regions);

    // Initialize global frame allocator reference for syscalls
    unsafe { mm_frame::init_global_allocator(&FRAME_ALLOCATOR) };

    // Mark kernel memory as used
    FRAME_ALLOCATOR.mark_used(
        os_core::PhysAddr::new(boot_info.kernel_phys_base),
        boot_info.kernel_size as usize,
    );

    let _ = writeln!(writer, "[INFO] Frame allocator initialized");
    let _ = writeln!(writer, "[INFO] Total frames: {}", FRAME_ALLOCATOR.total_frames());
    let _ = writeln!(writer, "[INFO] Free frames: {}", FRAME_ALLOCATOR.free_frame_count());

    // Initialize framebuffer if available
    if let Some(ref fb_info) = boot_info.framebuffer {
        let _ = writeln!(writer, "[INFO] Initializing framebuffer...");
        let _ = writeln!(writer, "[INFO] Framebuffer: {}x{} @ {:#x}",
            fb_info.width, fb_info.height, fb_info.base);
        let _ = writeln!(writer, "[INFO] Stride: {} pixels, BPP: {}", fb_info.stride, fb_info.bpp);

        // Initialize with video modes if available
        fb::init_from_boot(fb_info, boot_info.phys_map_base, boot_info.video_modes.as_ref());
        let _ = writeln!(writer, "[INFO] Framebuffer initialized");

        // Log video mode count
        let mode_count = fb::get_mode_count();
        let _ = writeln!(writer, "[INFO] Video modes available: {}", mode_count);

        // Initialize terminal emulator with framebuffer
        if let Some(framebuffer) = fb::framebuffer() {
            terminal::init(framebuffer);
            let _ = writeln!(writer, "[INFO] Terminal emulator initialized");
        }

        // Clear the screen with a dark background
        terminal::clear();
        let _ = writeln!(writer, "[INFO] Terminal ready");
    } else {
        let _ = writeln!(writer, "[INFO] No framebuffer available, serial-only mode");
    }

    // Initialize architecture components (GDT, IDT, APIC)
    let _ = writeln!(writer, "[INFO] Initializing x86_64 architecture...");
    unsafe {
        arch::init();
    }

    // Register page fault callback for COW handling
    unsafe {
        arch::exceptions::set_page_fault_callback(page_fault_handler);
    }

    // Register terminal tick callback for 30 FPS rendering
    if terminal::is_initialized() {
        unsafe {
            arch::set_terminal_tick_callback(terminal_tick);
        }
        let _ = writeln!(writer, "[INFO] Terminal tick callback registered (30 FPS)");
    }

    // Keyboard is handled by WATOS-style interrupt handler - no initialization needed
    let _ = writeln!(writer, "[INFO] Keyboard ready (WATOS-style)");

    // Start timer at 100Hz
    let _ = writeln!(writer, "[INFO] Starting APIC timer at 100Hz...");
    arch::start_timer(100);

    // Enable interrupts
    let _ = writeln!(writer, "[INFO] Enabling interrupts...");
    arch::X86_64::enable_interrupts();
    let _ = writeln!(writer, "[INFO] Interrupts enabled");

    let _ = writeln!(writer);

    // Test heap allocation
    let _ = writeln!(writer, "[INFO] Testing heap allocation...");
    let boxed_value = Box::new(42u32);
    let _ = writeln!(writer, "[INFO] Box::new(42) = {}", *boxed_value);

    let _ = writeln!(writer);
    let _ = writeln!(writer, "OXIDE kernel initialized successfully!");
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
        fork: Some(kernel_fork),
        exec: Some(kernel_exec),
        wait: Some(kernel_wait),
    };
    unsafe {
        syscall::init(syscall_ctx);
    }

    // Register the syscall dispatch function
    unsafe {
        arch::syscall::set_syscall_handler(syscall_dispatch);
    }

    // ========================================
    // VFS Initialization
    // ========================================
    let _ = writeln!(writer, "[VFS] Initializing virtual filesystem...");

    // Mount tmpfs as root filesystem
    let root_fs = TmpDir::new_root();
    if let Err(e) = GLOBAL_VFS.mount(root_fs.clone(), "/", MountFlags::empty(), "tmpfs") {
        let _ = writeln!(writer, "[VFS] Failed to mount root: {:?}", e);
        arch::X86_64::halt();
    }
    let _ = writeln!(writer, "[VFS] Mounted tmpfs at /");

    // Create /dev directory
    if let Err(e) = root_fs.mkdir("dev", vfs::Mode::DEFAULT_DIR) {
        let _ = writeln!(writer, "[VFS] Failed to create /dev: {:?}", e);
        arch::X86_64::halt();
    }

    // Mount devfs at /dev
    let dev_fs = DevFs::new();
    if let Err(e) = GLOBAL_VFS.mount(dev_fs, "/dev", MountFlags::empty(), "devfs") {
        let _ = writeln!(writer, "[VFS] Failed to mount devfs: {:?}", e);
        arch::X86_64::halt();
    }
    let _ = writeln!(writer, "[VFS] Mounted devfs at /dev");

    // Set up serial write function for devfs (raw debug output)
    unsafe {
        devfs::devices::set_serial_write(serial_write_bytes);
    }

    // Set up legacy console write function for devfs (fallback for early boot)
    unsafe {
        devfs::devices::set_console_write(console_write_bytes);
    }

    // Set up framebuffer info callback for /dev/fb0
    unsafe {
        devfs::devices::set_fb_info_callback(get_fb_device_info);
        devfs::devices::set_fb_mode_count_callback(get_fb_mode_count);
        devfs::devices::set_fb_mode_info_callback(get_fb_mode_info);
    }

    // Create /proc directory
    if let Err(e) = root_fs.mkdir("proc", vfs::Mode::DEFAULT_DIR) {
        let _ = writeln!(writer, "[VFS] Failed to create /proc: {:?}", e);
        arch::X86_64::halt();
    }

    // Set memory stats callback for procfs
    unsafe {
        procfs::set_memory_stats_callback(get_memory_stats);
    }

    // Mount procfs at /proc
    let proc_fs = ProcFs::new();
    if let Err(e) = GLOBAL_VFS.mount(proc_fs, "/proc", MountFlags::empty(), "procfs") {
        let _ = writeln!(writer, "[VFS] Failed to mount procfs: {:?}", e);
        arch::X86_64::halt();
    }
    let _ = writeln!(writer, "[VFS] Mounted procfs at /proc");

    // Initialize PTY subsystem
    let pty_manager = Arc::new(PtyManager::new());

    // Create /dev/pts directory
    if let Err(e) = root_fs.mkdir("pts", vfs::Mode::DEFAULT_DIR) {
        // Ignore error if directory already exists somehow
        let _ = writeln!(writer, "[VFS] Note: /dev/pts mkdir: {:?}", e);
    }

    // Get the devfs to register PTY devices
    if let Ok(devfs_vnode) = GLOBAL_VFS.lookup("/dev") {
        // We need to downcast and use DevFs's register method
        // For now, the PTY devices are accessible through the PtsDir
        let _ = writeln!(writer, "[VFS] PTY manager initialized");
    }

    // Mount pts filesystem at /dev/pts (using tmpfs for the mount point)
    let pts_dir = PtsDir::new(pty_manager.clone(), 100);
    if let Err(e) = GLOBAL_VFS.mount(pts_dir, "/dev/pts", MountFlags::empty(), "devpts") {
        let _ = writeln!(writer, "[VFS] Failed to mount devpts: {:?}", e);
        // Non-fatal - PTY support just won't work
    } else {
        let _ = writeln!(writer, "[VFS] Mounted devpts at /dev/pts");
    }

    let _ = writeln!(writer, "[VFS] VFS initialized");

    // ========================================
    // Network Initialization
    // ========================================
    let _ = writeln!(writer, "[NET] Initializing network stack...");

    // Enumerate PCI devices to find network cards
    pci::enumerate();
    let pci_devices = pci::devices();
    let _ = writeln!(writer, "[NET] Found {} PCI devices", pci_devices.len());

    // Look for VirtIO network devices
    let virtio_net_devices = pci::find_virtio_net();
    let _ = writeln!(writer, "[NET] Found {} VirtIO network devices", virtio_net_devices.len());

    // Initialize the first VirtIO network device found
    let net_initialized = if let Some(pci_dev) = virtio_net_devices.first() {
        let _ = writeln!(writer, "[NET] Initializing VirtIO network device at {:02x}:{:02x}.{}",
            pci_dev.address.bus, pci_dev.address.device, pci_dev.address.function);

        match unsafe { virtio_net::VirtioNet::from_pci(pci_dev) } {
            Some(virtio_net) => {
                let mac = virtio_net.mac_address();
                let _ = writeln!(writer, "[NET] VirtIO network device initialized");
                let _ = writeln!(writer, "[NET] MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    mac.0[0], mac.0[1], mac.0[2], mac.0[3], mac.0[4], mac.0[5]);

                // Create network interface
                let device = Arc::new(virtio_net);
                net::register_device(device.clone());

                let interface = Arc::new(net::NetworkInterface::new(device));
                net::interface::add_interface(interface.clone());

                // Initialize TCP/IP stack
                tcpip::init(interface);
                let _ = writeln!(writer, "[NET] TCP/IP stack initialized");

                true
            }
            None => {
                let _ = writeln!(writer, "[NET] Failed to initialize VirtIO network device");
                false
            }
        }
    } else {
        let _ = writeln!(writer, "[NET] No VirtIO network device found");
        false
    };

    // If no VirtIO, initialize loopback device at minimum
    if !net_initialized {
        let _ = writeln!(writer, "[NET] Initializing loopback device only");
        let loopback = Arc::new(net::LoopbackDevice::new());
        net::register_device(loopback.clone());

        let lo_interface = Arc::new(net::NetworkInterface::new(loopback));
        lo_interface.set_ipv4_addr(
            net::Ipv4Addr::new(127, 0, 0, 1),
            net::Ipv4Addr::new(255, 0, 0, 0),
        ).ok();
        net::interface::add_interface(lo_interface);
    }

    let _ = writeln!(writer, "[NET] Network initialization complete");

    // Load and mount the initramfs (loaded from disk by bootloader)
    let initramfs_data = match boot_info.initramfs() {
        Some(data) => {
            let _ = writeln!(writer, "[INITRAMFS] Initramfs at phys {:#x}, {} bytes",
                boot_info.initramfs_phys, boot_info.initramfs_size);
            data
        }
        None => {
            let _ = writeln!(writer, "[INITRAMFS] ERROR: No initramfs loaded by bootloader!");
            arch::X86_64::halt();
        }
    };

    let _ = writeln!(writer, "[INITRAMFS] Loading initramfs ({} bytes)...", initramfs_data.len());
    let initramfs_root = match initramfs::load(initramfs_data) {
        Ok(root) => root,
        Err(e) => {
            let _ = writeln!(writer, "[INITRAMFS] Failed to load initramfs: {:?}", e);
            arch::X86_64::halt();
        }
    };

    // Mount initramfs as root filesystem
    if let Err(e) = GLOBAL_VFS.mount(initramfs_root, "/", MountFlags::empty(), "initramfs") {
        let _ = writeln!(writer, "[INITRAMFS] Failed to mount initramfs: {:?}", e);
        arch::X86_64::halt();
    }
    let _ = writeln!(writer, "[INITRAMFS] Mounted as root filesystem at /");

    // Load /sbin/init
    let init_path = "/sbin/init";
    let _ = writeln!(writer, "[USER] Loading {}...", init_path);
    let init_vnode = match GLOBAL_VFS.lookup(init_path) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(writer, "[USER] Failed to find {}: {:?}", init_path, e);
            arch::X86_64::halt();
        }
    };

    // Read init binary
    let init_size = init_vnode.size() as usize;
    let mut init_data = alloc::vec![0u8; init_size];
    match init_vnode.read(0, &mut init_data) {
        Ok(n) if n == init_size => {}
        Ok(n) => {
            let _ = writeln!(writer, "[USER] Short read: {} of {} bytes", n, init_size);
            arch::X86_64::halt();
        }
        Err(e) => {
            let _ = writeln!(writer, "[USER] Failed to read init: {:?}", e);
            arch::X86_64::halt();
        }
    }
    let _ = writeln!(writer, "[USER] Read {} bytes from init", init_size);

    // Parse the ELF
    let elf = match ElfExecutable::parse(&init_data) {
        Ok(e) => e,
        Err(err) => {
            let _ = writeln!(writer, "[USER] Failed to parse init ELF: {:?}", err);
            arch::X86_64::halt();
        }
    };

    let _ = writeln!(writer, "[USER] ELF entry point: {:#x}", elf.entry_point().as_u64());

    // Create user address space
    let _ = writeln!(writer, "[USER] Creating user address space...");
    let kernel_pml4 = read_cr3();
    let _ = writeln!(writer, "[USER] Kernel PML4: {:#x}", kernel_pml4.as_u64());

    // Store kernel PML4 for fork/exec
    unsafe {
        KERNEL_PML4 = kernel_pml4.as_u64();
    }

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
    // Handle overlapping segments by checking if pages are already mapped
    for seg in elf.segments() {
        let (page_base, page_size) = elf::ElfLoader::segment_pages(seg);
        let num_pages = page_size / 4096;
        let _page_offset = elf::ElfLoader::segment_page_offset(seg);

        // Allocate pages that aren't already mapped, or update flags if already mapped
        for i in 0..num_pages {
            let page_addr = VirtAddr::new(page_base.as_u64() + (i as u64 * 4096));
            // Check if page is already mapped
            let existing = user_space.translate(page_addr);
            if existing.is_none() {
                // Allocate single page
                if let Err(e) = user_space.allocate_pages(page_addr, 1, seg.flags, &alloc_wrapper) {
                    let _ = writeln!(writer, "[USER] Failed to allocate page at {:#x}: {:?}",
                        page_addr.as_u64(), e);
                    arch::X86_64::halt();
                }
            } else {
                // Page already mapped - upgrade permissions if this segment needs more
                // (e.g., .data segment may need write permission on a page that .text only needed read)
                if seg.flags.writable() {
                    user_space.update_user_page_flags(page_addr, MemoryFlags::WRITE);
                }
            }
        }

        // Copy segment data page by page (physical pages may not be contiguous!)
        let seg_data = elf.segment_data(seg);
        let seg_vaddr_start = seg.vaddr.as_u64();
        let mut data_offset = 0usize;
        let mut mem_offset = 0usize;

        while data_offset < seg_data.len() || mem_offset < seg.mem_size {
            // Calculate which virtual page we're on
            let current_vaddr = seg_vaddr_start + mem_offset as u64;
            let page_vaddr = VirtAddr::new(current_vaddr & !0xFFF);
            let in_page_offset = (current_vaddr & 0xFFF) as usize;
            let bytes_remaining_in_page = 4096 - in_page_offset;

            // Get physical address for this page
            let phys = match user_space.translate(page_vaddr) {
                Some(p) => p,
                None => {
                    let _ = writeln!(writer, "[USER] translate({:#x}) failed!", page_vaddr.as_u64());
                    arch::X86_64::halt();
                }
            };

            let dest_virt = phys_to_virt(phys);
            let dest = unsafe { dest_virt.as_mut_ptr::<u8>().add(in_page_offset) };

            // Copy file data for this page
            let file_bytes_remaining = seg_data.len().saturating_sub(data_offset);
            let copy_len = bytes_remaining_in_page.min(file_bytes_remaining);

            if copy_len > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        seg_data.as_ptr().add(data_offset),
                        dest,
                        copy_len,
                    );
                }
                data_offset += copy_len;
            }

            // Zero BSS portion of this page (if any)
            let bss_start_in_seg = seg.file_size;
            let bss_end_in_seg = seg.mem_size;

            if mem_offset + bytes_remaining_in_page > bss_start_in_seg && mem_offset < bss_end_in_seg {
                // There's some BSS in this page
                let bss_start_in_page = if mem_offset < bss_start_in_seg {
                    (bss_start_in_seg - mem_offset).min(bytes_remaining_in_page)
                } else {
                    0
                };
                let bss_end_in_page = (bss_end_in_seg - mem_offset).min(bytes_remaining_in_page);
                let bss_len = bss_end_in_page - bss_start_in_page;

                if bss_len > 0 {
                    unsafe {
                        core::ptr::write_bytes(dest.add(bss_start_in_page), 0, bss_len);
                    }
                }
            }

            mem_offset += bytes_remaining_in_page;
        }
    }

    // Allocate user stack (larger to accommodate typical program needs)
    let _ = writeln!(writer, "[USER] Allocating user stack...");
    let user_stack_pages = 64; // 256 KB stack
    // Stack grows down, so allocate below the top address
    let user_stack_base = VirtAddr::new(0x7FFF_F000_0000 - (user_stack_pages * 4096) as u64);
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
    // Allocate 128KB kernel stack - fork+COW uses ~67KB during deep recursion
    const KERNEL_STACK_SIZE: usize = 128 * 1024;
    let kernel_stack_pages = KERNEL_STACK_SIZE / 4096;
    let kernel_stack_phys = FRAME_ALLOCATOR.alloc_frames(kernel_stack_pages)
        .expect("Failed to allocate kernel stack");
    // Convert physical to virtual for the kernel to use
    let kernel_stack_virt = phys_to_virt(kernel_stack_phys);
    let kernel_stack_top = kernel_stack_virt.as_u64() + KERNEL_STACK_SIZE as u64;

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

    // Create and register init process (PID 1)
    let _ = writeln!(writer, "[USER] Registering init process...");
    let init_pid = alloc_pid(); // Should be 1
    let _ = writeln!(writer, "[USER] Init PID: {}", init_pid);

    // Create the init Process struct
    let init_process = Process::new(
        init_pid,
        0,  // ppid = 0 (kernel)
        user_space,
        kernel_stack_phys,
        KERNEL_STACK_SIZE,
        elf.entry_point(),
        user_stack_top,
    );

    // Register in process table
    let init_arc = process_table().add(init_process);
    process_table().set_current_pid(init_pid);
    let _ = writeln!(writer, "[USER] Init process registered");

    // Set up standard file descriptors (stdin, stdout, stderr) for init
    let _ = writeln!(writer, "[USER] Setting up stdin/stdout/stderr...");
    {
        let mut proc = init_arc.lock();
        // Open /dev/console for stdin (fd 0), stdout (fd 1), stderr (fd 2)
        match GLOBAL_VFS.lookup("/dev/console") {
            Ok(console_vnode) => {
                // stdin (read-only)
                let stdin = Arc::new(File::new(console_vnode.clone(), FileFlags::O_RDONLY));
                if let Err(e) = proc.fd_table_mut().insert(0, stdin) {
                    let _ = writeln!(writer, "[USER] Failed to set up stdin: {:?}", e);
                }

                // stdout (write-only)
                let stdout = Arc::new(File::new(console_vnode.clone(), FileFlags::O_WRONLY));
                if let Err(e) = proc.fd_table_mut().insert(1, stdout) {
                    let _ = writeln!(writer, "[USER] Failed to set up stdout: {:?}", e);
                }

                // stderr (write-only)
                let stderr = Arc::new(File::new(console_vnode, FileFlags::O_WRONLY));
                if let Err(e) = proc.fd_table_mut().insert(2, stderr) {
                    let _ = writeln!(writer, "[USER] Failed to set up stderr: {:?}", e);
                }

                let _ = writeln!(writer, "[USER] Standard fds set up (0,1,2 -> /dev/console)");
            }
            Err(e) => {
                let _ = writeln!(writer, "[USER] Failed to open /dev/console: {:?}", e);
            }
        }
    }

    // Get the PML4 from the registered process
    let user_pml4_phys = init_arc.lock().address_space().pml4_phys();

    let _ = writeln!(writer);
    let _ = writeln!(writer, "[USER] Entering user mode at {:#x}...", entry_point);
    let _ = writeln!(writer);

    // Use the combined enter_usermode function that:
    // 1. Switches to kernel stack (in higher half)
    // 2. Switches page tables
    // 3. Jumps to user mode
    let _ = writeln!(writer, "[USER] Entering user mode...");
    unsafe {
        arch::usermode::enter_usermode(
            kernel_stack_top,
            user_pml4_phys.as_u64(),
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
    // Debug: log syscall 3 (fork) and 6 (waitpid) calls
    if number == 3 || number == 6 {
        use core::fmt::Write;
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[SYSCALL] number={} arg1={:#x}", number, arg1);
    }
    syscall::dispatch(number, arg1, arg2, arg3, arg4, arg5, arg6)
}

/// Page fault handler callback (for COW and other page faults)
fn page_fault_handler(fault_addr: u64, error_code: u64, rip: u64) -> bool {
    // Check if this is a write fault on a present page (potential COW)
    let is_present = error_code & 1 != 0;
    let is_write = error_code & 2 != 0;
    let is_user = error_code & 4 != 0;

    // Get actual CR3 to compare with what process_table says
    let actual_cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) actual_cr3);
    }

    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[PF] fault_addr={:#x} error={:#x} rip={:#x}", fault_addr, error_code, rip);
        let _ = writeln!(writer, "[PF] present={} write={} user={} actual_cr3={:#x}", is_present, is_write, is_user, actual_cr3);
    }

    // COW faults are: present + write + user mode
    if is_present && is_write && is_user {
        // Get current process's PML4
        let table = process_table();
        let current_pid = table.current_pid();

        {
            let mut writer = serial::SerialWriter;
            let _ = writeln!(writer, "[PF] COW check: current_pid={}", current_pid);
        }

        if let Some(proc) = table.get(current_pid) {
            let pml4 = proc.lock().address_space().pml4_phys();
            let alloc = FrameAllocatorWrapper;

            {
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[PF] PML4={:#x}, calling handle_cow_fault", pml4.as_u64());
            }

            // Try to handle as COW fault
            if handle_cow_fault(VirtAddr::new(fault_addr), pml4, &alloc) {
                {
                    let mut writer = serial::SerialWriter;
                    let _ = writeln!(writer, "[PF] COW handled OK");
                }
                return true; // Fault handled
            } else {
                {
                    let mut writer = serial::SerialWriter;
                    let _ = writeln!(writer, "[PF] COW handler failed");
                }
            }
        } else {
            {
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[PF] Process {} not found!", current_pid);
            }
        }
    }

    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[PF] Fault NOT handled - will panic");
    }
    false // Fault not handled - will panic
}

/// Console write function for syscalls
///
/// Writes to serial and terminal emulator (if initialized).
fn console_write(data: &[u8]) {
    // Write to serial for debugging
    let mut writer = serial::SerialWriter;
    for &byte in data {
        let _ = writer.write_char(byte as char);
    }

    // Write to terminal emulator for ANSI-processed framebuffer output
    if terminal::is_initialized() {
        terminal::write(data);
    } else if fb::is_initialized() {
        // Fallback to basic fb console before terminal is ready
        for &byte in data {
            fb::putchar(byte as char);
        }
    }
}

/// Terminal tick callback - called at 30 FPS from timer interrupt
fn terminal_tick() {
    // Process keyboard scancodes
    while let Some(scancode) = arch::get_scancode() {
        // Convert scancode to ASCII character (basic mapping)
        if let Some(ch) = scancode_to_char(scancode) {
            // Push character to console input buffer (handles both stdin and display)
            devfs::console_push_char(ch as u8);
        }
    }
    
    terminal::tick();
}

/// Convert PS/2 scancode to ASCII character (basic US keyboard layout)
fn scancode_to_char(scancode: u8) -> Option<char> {
    // Handle key release (high bit set) - ignore for now
    if scancode & 0x80 != 0 {
        return None;
    }
    
    // Basic scancode to ASCII conversion (subset from WATOS)
    let ascii = match scancode {
        // Numbers
        0x02 => '1', 0x03 => '2', 0x04 => '3', 0x05 => '4', 0x06 => '5',
        0x07 => '6', 0x08 => '7', 0x09 => '8', 0x0A => '9', 0x0B => '0',
        // Letters
        0x1E => 'a', 0x30 => 'b', 0x2E => 'c', 0x20 => 'd', 0x12 => 'e',
        0x21 => 'f', 0x22 => 'g', 0x23 => 'h', 0x17 => 'i', 0x24 => 'j',
        0x25 => 'k', 0x26 => 'l', 0x32 => 'm', 0x31 => 'n', 0x18 => 'o',
        0x19 => 'p', 0x10 => 'q', 0x13 => 'r', 0x1F => 's', 0x14 => 't',
        0x16 => 'u', 0x2F => 'v', 0x11 => 'w', 0x2D => 'x', 0x15 => 'y',
        0x2C => 'z',
        // Special keys
        0x39 => ' ',  // Space
        0x1C => '\n', // Enter
        0x0E => '\x08', // Backspace
        _ => return None,
    };
    
    Some(ascii)
}


/// Serial-only write function for devfs
///
/// Writes only to serial port for raw debug output.
fn serial_write_bytes(data: &[u8]) {
    let mut writer = serial::SerialWriter;
    for &byte in data {
        let _ = writer.write_char(byte as char);
    }
}

/// Console write function for devfs (legacy fallback)
///
/// Writes to both serial and framebuffer (if initialized).
/// Used before terminal emulator is initialized.
fn console_write_bytes(data: &[u8]) {
    // Write to serial
    let mut writer = serial::SerialWriter;
    for &byte in data {
        let _ = writer.write_char(byte as char);
    }

    // Write to framebuffer console if available (legacy path)
    if fb::is_initialized() && !terminal::is_initialized() {
        for &byte in data {
            fb::putchar(byte as char);
        }
    }
}

/// Get framebuffer device info for /dev/fb0
///
/// Returns information needed by the devfs framebuffer device.
fn get_fb_device_info() -> Option<devfs::devices::FramebufferDeviceInfo> {
    let info = fb::get_fb_info()?;

    Some(devfs::devices::FramebufferDeviceInfo {
        base: info.base,
        phys_base: info.phys_base,
        size: info.size,
        width: info.width,
        height: info.height,
        stride: info.stride,
        bpp: info.bpp,
        is_bgr: info.is_bgr,
    })
}

/// Get video mode count for /dev/fb0
fn get_fb_mode_count() -> u32 {
    fb::get_mode_count()
}

/// Get video mode info by index for /dev/fb0
fn get_fb_mode_info(index: u32) -> Option<devfs::devices::VideoModeDeviceInfo> {
    let mode = fb::get_mode_info(index)?;

    Some(devfs::devices::VideoModeDeviceInfo {
        mode_number: mode.mode_number,
        width: mode.width,
        height: mode.height,
        bpp: mode.bpp,
        stride: mode.stride,
        framebuffer_size: mode.framebuffer_size,
        is_bgr: mode.is_bgr,
        _pad: [0; 7],
    })
}

/// Get memory statistics for /proc/meminfo
fn get_memory_stats() -> procfs::MemoryStats {
    // Get frame allocator stats
    let total_frames = FRAME_ALLOCATOR.total_frames();
    let free_frames = FRAME_ALLOCATOR.free_frame_count();
    let page_size = 4096u64;

    // Get heap stats
    let heap_used = HEAP_ALLOCATOR.used() as u64;
    let heap_free = HEAP_ALLOCATOR.free() as u64;

    procfs::MemoryStats {
        total_mem: (total_frames as u64) * page_size,
        free_mem: (free_frames as u64) * page_size,
        total_swap: 0, // No swap
        free_swap: 0,
        heap_used,
        heap_free,
    }
}

/// Console read function for syscalls
fn console_read(buf: &mut [u8]) -> usize {
    // Read from keyboard buffer (PS/2) or serial port
    // NOTE: No echo here - the application (shell) handles echoing
    let mut count = 0;
    for byte in buf.iter_mut() {
        // Poll for input from either keyboard buffer or serial
        loop {
            // First check keyboard buffer (PS/2 console input)
            if devfs::console_has_input() {
                // Read from console (keyboard) buffer via VFS-like mechanism
                // The console_push_str pushed bytes, we need to pop them
                // But we can't directly pop - we use a temp buffer approach
                let mut temp = [0u8; 1];
                // Read one byte from console device
                if let Ok(vnode) = vfs::mount::GLOBAL_VFS.lookup("/dev/console") {
                    if let Ok(n) = vnode.read(0, &mut temp) {
                        if n > 0 {
                            let b = temp[0];
                            *byte = b;
                            count += 1;
                            // Return on newline for line-buffered input
                            if b == b'\n' || b == b'\r' {
                                if b == b'\r' {
                                    *byte = b'\n';
                                }
                                return count;
                            }
                            break;
                        }
                    }
                }
            }

            // Fallback to serial port (for serial console / QEMU)
            if let Some(b) = serial::read_byte() {
                *byte = b;
                count += 1;
                // Return on newline for line-buffered input
                if b == b'\n' || b == b'\r' {
                    if b == b'\r' {
                        // Convert CR to LF
                        *byte = b'\n';
                    }
                    return count;
                }
                break;
            }

            // Use HLT instruction to wait for interrupt instead of busy-spin
            // This saves CPU and wakes on timer/serial/keyboard interrupts
            unsafe {
                core::arch::asm!("hlt", options(nomem, nostack));
            }
        }
    }
    count
}

/// User exit function
fn user_exit(status: i32) -> ! {
    // Get current process and mark as zombie
    let table = process_table();
    let current_pid = table.current_pid();

    // Debug: Print framebuffer write stats on process exit
    {
        let (writes, bytes, base) = devfs::devices::get_fb_write_stats();
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[FB_DEBUG] Process {} exiting - FB writes={} bytes={} base={:#x}",
            current_pid, writes, bytes, base);
    }

    if let Some(proc) = table.get(current_pid) {
        proc.lock().exit(status);
    }

    unsafe {
        USER_EXIT_STATUS = status;
    }
    USER_EXITED.store(true, Ordering::SeqCst);

    // Check if there's a saved parent context to return to
    let parent_ctx = PARENT_CONTEXT.lock().take();

    if let Some(ctx) = parent_ctx {
        // Restore parent as current process
        table.set_current_pid(ctx.pid);

        // Get parent's kernel stack
        if let Some(parent) = table.get(ctx.pid) {
            let p = parent.lock();
            let parent_stack_phys = p.kernel_stack();
            let parent_stack_size = p.kernel_stack_size();
            drop(p);

            let parent_stack_virt = mm_paging::phys_to_virt(parent_stack_phys);
            let parent_stack_top = parent_stack_virt.as_u64() + parent_stack_size as u64;

            // Restore parent's kernel stack for syscalls
            unsafe {
                arch::syscall::set_kernel_stack(parent_stack_top);
            }
            arch::gdt::set_kernel_stack(parent_stack_top);

            // Calculate wait result: (child_pid << 32) | status
            let wait_result = ((current_pid as i64) << 32) | ((status as i64) & 0xFFFFFFFF);

            // Return to parent's user mode via sysretq
            // CRITICAL: Must restore ALL registers the parent had when making the waitpid syscall
            // sysretq clobbers RCX (uses for RIP) and R11 (uses for RFLAGS)
            // All other registers must be restored to parent's values

            // Copy context to static memory that survives the CR3 switch
            // We use a static because inline asm can't handle this many registers
            static mut RESTORE_CTX: ParentContext = ParentContext {
                pid: 0, pml4: 0, rip: 0, rsp: 0, rflags: 0,
                rax: 0, rbx: 0, rcx: 0, rdx: 0, rsi: 0, rdi: 0, rbp: 0,
                r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
            };
            static mut RESTORE_RESULT: i64 = 0;

            unsafe {
                use core::ptr::addr_of_mut;
                *addr_of_mut!(RESTORE_CTX) = ctx.clone();
                *addr_of_mut!(RESTORE_RESULT) = wait_result;

                // Switch page tables first
                core::arch::asm!(
                    "mov cr3, {}",
                    in(reg) ctx.pml4,
                    options(nostack)
                );

                // Now restore all registers from the static context and sysretq
                // The context is at a fixed virtual address (higher half)
                let ctx_ptr = addr_of_mut!(RESTORE_CTX) as u64;
                let result_ptr = addr_of_mut!(RESTORE_RESULT) as u64;

                // ParentContext layout:
                // pid: u32 (offset 0, padded to 8)
                // pml4: u64 (offset 8)
                // rip: u64 (offset 16)
                // rsp: u64 (offset 24)
                // rflags: u64 (offset 32)
                // rax: u64 (offset 40)
                // rbx: u64 (offset 48)
                // rcx: u64 (offset 56)
                // rdx: u64 (offset 64)
                // rsi: u64 (offset 72)
                // rdi: u64 (offset 80)
                // rbp: u64 (offset 88)
                // r8: u64 (offset 96)
                // r9: u64 (offset 104)
                // r10: u64 (offset 112)
                // r11: u64 (offset 120)
                // r12: u64 (offset 128)
                // r13: u64 (offset 136)
                // r14: u64 (offset 144)
                // r15: u64 (offset 152)
                // Store values in statics so we can access them after restoring all registers
                static mut SYSRET_USER_RSP: u64 = 0;
                static mut SYSRET_USER_RIP: u64 = 0;
                static mut SYSRET_USER_RFLAGS: u64 = 0;
                static mut SYSRET_RESULT: i64 = 0;
                static mut SYSRET_R14: u64 = 0;
                static mut SYSRET_R15: u64 = 0;

                unsafe {
                    use core::ptr::addr_of_mut;
                    *addr_of_mut!(SYSRET_USER_RSP) = ctx.rsp;
                    *addr_of_mut!(SYSRET_USER_RIP) = ctx.rip;
                    *addr_of_mut!(SYSRET_USER_RFLAGS) = ctx.rflags;
                    *addr_of_mut!(SYSRET_RESULT) = wait_result;
                    *addr_of_mut!(SYSRET_R14) = ctx.r14;
                    *addr_of_mut!(SYSRET_R15) = ctx.r15;
                }

                core::arch::asm!(
                    // r15 = context pointer (only used for loading registers, not for sysret values)
                    "mov r15, {ctx}",
                    // Restore callee-saved registers
                    "mov rbx, [r15 + 48]",    // rbx at offset 48
                    "mov rbp, [r15 + 88]",    // rbp at offset 88
                    "mov r12, [r15 + 128]",   // r12 at offset 128
                    "mov r13, [r15 + 136]",   // r13 at offset 136
                    // Restore caller-saved registers (that syscall should preserve)
                    "mov rdi, [r15 + 80]",    // rdi at offset 80
                    "mov rsi, [r15 + 72]",    // rsi at offset 72
                    "mov rdx, [r15 + 64]",    // rdx at offset 64
                    "mov r8, [r15 + 96]",     // r8 at offset 96
                    "mov r9, [r15 + 104]",    // r9 at offset 104
                    "mov r10, [r15 + 112]",   // r10 at offset 112
                    // Now load sysret values and r14/r15 from statics (using absolute addresses)
                    "mov rax, [{result}]",    // result value
                    "mov rcx, [{rip}]",       // user rip
                    "mov r11, [{rflags}]",    // user rflags
                    "mov r14, [{r14_val}]",   // restore r14
                    "mov r15, [{r15_val}]",   // restore r15
                    // Load user RSP last and sysretq
                    "mov rsp, [{rsp_val}]",
                    "sysretq",
                    ctx = in(reg) ctx_ptr,
                    result = sym SYSRET_RESULT,
                    rip = sym SYSRET_USER_RIP,
                    rflags = sym SYSRET_USER_RFLAGS,
                    r14_val = sym SYSRET_R14,
                    r15_val = sym SYSRET_R15,
                    rsp_val = sym SYSRET_USER_RSP,
                    options(noreturn)
                );
            }
        }
    }

    // Check if there's a parent waiting (for processes without saved context)
    if let Some(proc) = table.get(current_pid) {
        let ppid = proc.lock().ppid();
        if ppid > 0 {
            arch::X86_64::halt();
        }
    }

    // Init process or orphan exiting - halt the system
    arch::X86_64::halt();
}

/// Fork callback for syscalls
///
/// Creates a child process and returns child PID to parent, 0 to child.
fn kernel_fork() -> i64 {
    let table = process_table();
    let parent_pid = table.current_pid();

    // Debug: always print fork info for now
    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[FORK] Fork called from PID {}", parent_pid);
    }

    // Get current process context from syscall user context
    let user_ctx = get_user_context();

    // Debug: print user context
    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[FORK] user_ctx.rip={:#x} rsp={:#x}", user_ctx.rip, user_ctx.rsp);
    }
    let parent_context = ProcessContext {
        rip: user_ctx.rip,
        rsp: user_ctx.rsp,
        rflags: user_ctx.rflags,
        rax: user_ctx.rax,
        rbx: user_ctx.rbx,
        rcx: user_ctx.rcx,
        rdx: user_ctx.rdx,
        rsi: user_ctx.rsi,
        rdi: user_ctx.rdi,
        rbp: user_ctx.rbp,
        r8: user_ctx.r8,
        r9: user_ctx.r9,
        r10: user_ctx.r10,
        r11: user_ctx.r11,
        r12: user_ctx.r12,
        r13: user_ctx.r13,
        r14: user_ctx.r14,
        r15: user_ctx.r15,
    };

    debug_fork!("[FORK] Parent context: rip={:#x} rsp={:#x}", parent_context.rip, parent_context.rsp);

    // Create wrapper for frame allocator
    let alloc_wrapper = FrameAllocatorWrapper;

    // Call do_fork
    let result = do_fork(parent_pid, &parent_context, &alloc_wrapper);

    match result {
        Ok(child_pid) => {
            debug_fork!("[FORK] Created child process {}", child_pid);

            // Add child to pending list for later execution
            PENDING_CHILDREN.lock().push(child_pid);

            // Return child PID to parent
            child_pid as i64
        }
        Err(e) => {
            debug_fork!("[FORK] Fork failed: {:?}", e);
            -1 // EAGAIN
        }
    }
}

/// Wait callback for syscalls
///
/// Waits for child process and returns (pid << 32) | status.
fn kernel_wait(pid: i32, options: i32) -> i64 {
    let table = process_table();
    let parent_pid = table.current_pid();
    let wait_opts = WaitOptions::from(options);

    // Check if we have a pending child to run first
    {
        let mut pending = PENDING_CHILDREN.lock();
        if let Some(child_pid) = pending.pop() {
            drop(pending); // Release lock before running child

            // Run the child process
            run_child_process(child_pid);
        }
    }

    // Now wait for zombie children
    match do_waitpid(parent_pid, pid, wait_opts) {
        Ok(result) => {
            // Debug: print framebuffer write stats after each child exits
            {
                let (writes, bytes, base) = devfs::devices::get_fb_write_stats();
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[FB_DEBUG] Child {} exited - FB writes={} bytes={} base={:#x}",
                    result.pid, writes, bytes, base);
            }
            // Pack pid and status into result
            ((result.pid as i64) << 32) | ((result.status as i64) & 0xFFFFFFFF)
        }
        Err(e) => {
            match e {
                proc::WaitError::NoChildren => -10,  // ECHILD
                proc::WaitError::WouldBlock => -11,  // EAGAIN
                proc::WaitError::InvalidPid => -3,   // ESRCH
                proc::WaitError::Interrupted => -4,  // EINTR
            }
        }
    }
}

/// Run a child process to completion
///
/// This function saves the parent's context and enters the child.
/// When the child exits, control returns to parent via sysretq.
fn run_child_process(child_pid: Pid) {
    let table = process_table();
    let parent_pid = table.current_pid();

    // Get parent's PML4 for restoring later
    let parent_pml4 = if let Some(p) = table.get(parent_pid) {
        p.lock().address_space().pml4_phys().as_u64()
    } else {
        // Fallback to kernel PML4
        unsafe { KERNEL_PML4 }
    };

    // Get child process info
    let (child_pml4, _child_entry, _child_stack, kernel_stack_phys, kernel_stack_size) = {
        let child = match table.get(child_pid) {
            Some(c) => c,
            None => return,
        };

        let c = child.lock();
        (
            c.address_space().pml4_phys(),
            c.entry_point(),
            c.user_stack_top(),
            c.kernel_stack(),
            c.kernel_stack_size(),
        )
    };

    // Set current process to child
    table.set_current_pid(child_pid);
    {
        let mut writer = serial::SerialWriter;
        let verify_pid = table.current_pid();
        let _ = writeln!(writer, "[RUN_CHILD] set_current_pid({}) done, verify={}", child_pid, verify_pid);
    }

    // Use the kernel stack already allocated for this child (in fork)
    let kernel_stack_virt = mm_paging::phys_to_virt(kernel_stack_phys);
    let child_kernel_stack_top = kernel_stack_virt.as_u64() + kernel_stack_size as u64;

    // Set kernel stack for child's syscalls/interrupts
    unsafe {
        arch::syscall::set_kernel_stack(child_kernel_stack_top);
    }
    arch::gdt::set_kernel_stack(child_kernel_stack_top);

    // Get child's saved context
    let child_ctx = {
        let child = table.get(child_pid).unwrap();
        child.lock().context().clone()
    };

    // Debug: print child's context (all callee-saved registers)
    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[CHILD] PID {} entering usermode", child_pid);
        let _ = writeln!(writer, "[CHILD] rip={:#x} rsp={:#x} rbp={:#x}", child_ctx.rip, child_ctx.rsp, child_ctx.rbp);
        let _ = writeln!(writer, "[CHILD] rax={:#x} rbx={:#x} r12={:#x}", child_ctx.rax, child_ctx.rbx, child_ctx.r12);
        let _ = writeln!(writer, "[CHILD] r13={:#x} r14={:#x} r15={:#x}", child_ctx.r13, child_ctx.r14, child_ctx.r15);
    }

    // Save parent's FULL user context so we can restore ALL registers when child exits
    // This is critical because the parent's syscall handler saved registers to the
    // kernel stack, but we're going to bypass the normal epilogue via user_exit's sysretq
    let parent_user_ctx = get_user_context();
    {
        *PARENT_CONTEXT.lock() = Some(ParentContext {
            pid: parent_pid,
            pml4: parent_pml4,
            rip: parent_user_ctx.rip,
            rsp: parent_user_ctx.rsp,
            rflags: parent_user_ctx.rflags,
            rax: parent_user_ctx.rax,
            rbx: parent_user_ctx.rbx,
            rcx: parent_user_ctx.rcx,
            rdx: parent_user_ctx.rdx,
            rsi: parent_user_ctx.rsi,
            rdi: parent_user_ctx.rdi,
            rbp: parent_user_ctx.rbp,
            r8: parent_user_ctx.r8,
            r9: parent_user_ctx.r9,
            r10: parent_user_ctx.r10,
            r11: parent_user_ctx.r11,
            r12: parent_user_ctx.r12,
            r13: parent_user_ctx.r13,
            r14: parent_user_ctx.r14,
            r15: parent_user_ctx.r15,
        });
        CHILD_DONE.store(false, Ordering::SeqCst);
    }

    // Build UserContext for enter_usermode_with_context
    let user_ctx = arch::UserContext {
        rax: child_ctx.rax,
        rbx: child_ctx.rbx,
        rcx: child_ctx.rcx,
        rdx: child_ctx.rdx,
        rsi: child_ctx.rsi,
        rdi: child_ctx.rdi,
        rbp: child_ctx.rbp,
        rsp: child_ctx.rsp,
        r8: child_ctx.r8,
        r9: child_ctx.r9,
        r10: child_ctx.r10,
        r11: child_ctx.r11,
        r12: child_ctx.r12,
        r13: child_ctx.r13,
        r14: child_ctx.r14,
        r15: child_ctx.r15,
        rip: child_ctx.rip,
        rflags: child_ctx.rflags,
    };

    // Debug: verify UserContext before entering usermode
    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[CHILD] UserContext ptr: {:p}", &user_ctx);
        let _ = writeln!(writer, "[CHILD] UserContext.rip={:#x} rsp={:#x}", user_ctx.rip, user_ctx.rsp);
        let _ = writeln!(writer, "[CHILD] UserContext.rcx={:#x} rax={:#x}", user_ctx.rcx, user_ctx.rax);
        let _ = writeln!(writer, "[CHILD] kernel_stack={:#x} pml4={:#x}", child_kernel_stack_top, child_pml4.as_u64());

        // Verify by reading raw bytes at context address
        let ctx_ptr = &user_ctx as *const arch::UserContext as *const u64;
        unsafe {
            let _ = writeln!(writer, "[CHILD] Raw ctx[0]={:#x} (rax)", *ctx_ptr.add(0));
            let _ = writeln!(writer, "[CHILD] Raw ctx[2]={:#x} (rcx)", *ctx_ptr.add(2));
            let _ = writeln!(writer, "[CHILD] Raw ctx[16]={:#x} (rip)", *ctx_ptr.add(16));
        }

        // Test: copy context to child kernel stack and verify it's readable after CR3 switch
        // Use EXACT same address as enter_usermode_with_context: kernel_stack_top - 184
        let child_stack_base = child_kernel_stack_top - 184;
        let dest_ptr = child_stack_base as *mut u64;
        let _ = writeln!(writer, "[CHILD] Test dest_ptr={:#x}", dest_ptr as u64);
        let _ = writeln!(writer, "[CHILD] rcx will be at {:#x}", dest_ptr as u64 + 16);
        let src_ptr = &user_ctx as *const arch::UserContext as *const u64;

        // Copy context to child's kernel stack
        for i in 0..18 {
            unsafe {
                *dest_ptr.add(i) = *src_ptr.add(i);
            }
        }

        // Now switch to child's page tables and read back
        unsafe {
            // Read CR3 to verify current value
            let current_cr3: u64;
            core::arch::asm!("mov {}, cr3", out(reg) current_cr3);
            let _ = writeln!(writer, "[CHILD] Current CR3: {:#x}", current_cr3);
            let _ = writeln!(writer, "[CHILD] Child PML4: {:#x}", child_pml4.as_u64());

            // Switch to child's page tables
            core::arch::asm!("mov cr3, {}", in(reg) child_pml4.as_u64());

            // Read back from the copied context
            let read_rax = *dest_ptr.add(0);
            let read_rcx = *dest_ptr.add(2);
            let read_rip = *dest_ptr.add(16);

            // Switch back to original page tables
            core::arch::asm!("mov cr3, {}", in(reg) current_cr3);

            let _ = writeln!(writer, "[CHILD] After CR3 switch and back:");
            let _ = writeln!(writer, "[CHILD]   read_rax={:#x}", read_rax);
            let _ = writeln!(writer, "[CHILD]   read_rcx={:#x}", read_rcx);
            let _ = writeln!(writer, "[CHILD]   read_rip={:#x}", read_rip);
        }
    }

    // Enter user mode for child with full context restoration
    // When child calls exit(), user_exit will set CHILD_DONE and we'll detect it
    unsafe {
        arch::enter_usermode_with_context(
            child_kernel_stack_top,
            child_pml4.as_u64(),
            &user_ctx,
        );
    }

    // Note: We never reach here via normal flow.
    // But if we did somehow return, that would be the child exit path.
}

/// Exec callback for syscalls
///
/// Replaces the current process image with a new executable.
fn kernel_exec(path_ptr: *const u8, path_len: usize, argv_ptr: *const *const u8, envp_ptr: *const *const u8) -> i64 {
    let table = process_table();
    let current_pid = table.current_pid();

    // Read path from user space
    let path = unsafe {
        if path_ptr.is_null() || path_len == 0 {
            let mut writer = serial::SerialWriter;
            let _ = writeln!(writer, "[EXEC] Invalid path (null or zero len)");
            return -22; // EINVAL
        }
        let slice = core::slice::from_raw_parts(path_ptr, path_len);
        match core::str::from_utf8(slice) {
            Ok(s) => s,
            Err(_) => {
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[EXEC] Invalid UTF-8 in path");
                return -22; // EINVAL
            }
        }
    };

    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[EXEC] PID {} exec(\"{}\")", current_pid, path);
    }

    // Read argv from user space
    let mut argv: Vec<String> = Vec::new();
    if !argv_ptr.is_null() {
        unsafe {
            let mut i = 0;
            loop {
                let arg_ptr = *argv_ptr.add(i);
                if arg_ptr.is_null() {
                    break;
                }
                // Read null-terminated string
                let mut len = 0;
                while *arg_ptr.add(len) != 0 && len < 4096 {
                    len += 1;
                }
                let arg_slice = core::slice::from_raw_parts(arg_ptr, len);
                if let Ok(s) = core::str::from_utf8(arg_slice) {
                    argv.push(String::from(s));
                }
                i += 1;
                if i > 1024 {
                    break; // Safety limit
                }
            }
        }
    }
    // If no argv provided, use the path as argv[0]
    if argv.is_empty() {
        argv.push(String::from(path));
    }

    // Read envp from user space
    let mut envp: Vec<String> = Vec::new();
    if !envp_ptr.is_null() {
        unsafe {
            let mut i = 0;
            loop {
                let env_ptr = *envp_ptr.add(i);
                if env_ptr.is_null() {
                    break;
                }
                // Read null-terminated string
                let mut len = 0;
                while *env_ptr.add(len) != 0 && len < 4096 {
                    len += 1;
                }
                let env_slice = core::slice::from_raw_parts(env_ptr, len);
                if let Ok(s) = core::str::from_utf8(env_slice) {
                    envp.push(String::from(s));
                }
                i += 1;
                if i > 1024 {
                    break; // Safety limit
                }
            }
        }
    }

    // Look up the file in VFS
    let vnode = match GLOBAL_VFS.lookup(path) {
        Ok(v) => v,
        Err(e) => {
            let mut writer = serial::SerialWriter;
            let _ = writeln!(writer, "[EXEC] VFS lookup failed for '{}': {:?}", path, e);
            return -2; // ENOENT
        }
    };

    // Check it's a regular file
    if vnode.vtype() != VnodeType::File {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[EXEC] Not a regular file: {:?}", vnode.vtype());
        return -21; // EISDIR or not a file
    }

    // Read the file contents
    let size = vnode.size() as usize;
    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[EXEC] File size: {} bytes", size);
    }

    let mut elf_data = alloc::vec![0u8; size];
    let read_result = vnode.read(0, &mut elf_data);
    let bytes_read = match read_result {
        Ok(n) => n,
        Err(e) => {
            let mut writer = serial::SerialWriter;
            let _ = writeln!(writer, "[EXEC] Read failed: {:?}", e);
            return -5; // EIO
        }
    };

    if bytes_read != size {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[EXEC] Short read: {} of {} bytes", bytes_read, size);
        return -5; // EIO - short read
    }
    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[EXEC] Read {} bytes, calling do_exec", bytes_read);
    }

    // Get kernel PML4 for creating new address space
    let kernel_pml4 = PhysAddr::new(unsafe { KERNEL_PML4 });
    let alloc_wrapper = FrameAllocatorWrapper;

    // Call do_exec
    match do_exec(current_pid, &elf_data, &argv, &envp, &alloc_wrapper, kernel_pml4) {
        Ok((_entry_point, _stack_ptr)) => {
            // Get the new PML4 and switch to it
            if let Some(proc) = table.get(current_pid) {
                let p = proc.lock();
                let new_pml4 = p.address_space().pml4_phys();
                let ctx = p.context().clone();
                drop(p);

                // Debug: print exec return values
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[EXEC] Switching to PML4={:#x}", new_pml4.as_u64());
                let _ = writeln!(writer, "[EXEC] rip={:#x} rsp={:#x}", ctx.rip, ctx.rsp);
                let _ = writeln!(writer, "[EXEC] argc={} argv={:#x} envp={:#x}", ctx.rdi, ctx.rsi, ctx.rdx);

                // Switch to new address space and jump to entry point
                unsafe {
                    write_cr3(new_pml4);
                    flush_tlb_all();

                    // Return to user mode at new entry point
                    // We use sysretq which expects: rcx = rip, r11 = rflags
                    // Use explicit registers to prevent compiler from reusing registers
                    // that we overwrite before their values are consumed
                    core::arch::asm!(
                        // Set up rip for sysretq
                        "mov rcx, r8",
                        // Set up rflags for sysretq
                        "mov r11, r9",
                        // Set up user stack - do this AFTER loading values into rcx/r11
                        // to avoid any chance of compiler putting inputs in rsp
                        "mov rsp, r10",
                        // Set up argc, argv, envp in registers per System V ABI
                        // These are already loaded into r12, r13, r14 respectively
                        "mov rdi, r12",
                        "mov rsi, r13",
                        "mov rdx, r14",
                        // Load user data segment selectors for DS/ES/FS (0x1B = USER_DS | 3)
                        // NOTE: Do NOT load GS - swapgs will handle it
                        "mov ax, 0x1b",
                        "mov ds, ax",
                        "mov es, ax",
                        "mov fs, ax",
                        // Clear rax for return value
                        "xor rax, rax",
                        // Swap GS back to user mode (required before sysretq)
                        "swapgs",
                        // Return to user mode
                        "sysretq",
                        in("r8") ctx.rip,
                        in("r9") 0x202u64, // IF set
                        in("r10") ctx.rsp,
                        in("r12") ctx.rdi,
                        in("r13") ctx.rsi,
                        in("r14") ctx.rdx,
                        options(noreturn)
                    );
                }
            }
            0 // Never reached
        }
        Err(e) => {
            let mut writer = serial::SerialWriter;
            let code = match e {
                proc::ExecError::InvalidElf => {
                    let _ = writeln!(writer, "[EXEC] Error: InvalidElf");
                    -8 // ENOEXEC
                }
                proc::ExecError::OutOfMemory => {
                    let _ = writeln!(writer, "[EXEC] Error: OutOfMemory");
                    -12 // ENOMEM
                }
                proc::ExecError::ProcessNotFound => {
                    let _ = writeln!(writer, "[EXEC] Error: ProcessNotFound");
                    -3 // ESRCH
                }
                proc::ExecError::InvalidAddress => {
                    let _ = writeln!(writer, "[EXEC] Error: InvalidAddress");
                    -14 // EFAULT
                }
                proc::ExecError::InvalidArgument => {
                    let _ = writeln!(writer, "[EXEC] Error: InvalidArgument");
                    -22 // EINVAL
                }
            };
            code
        }
    }
}

/// Wrapper to use the global frame allocator
struct FrameAllocatorWrapper;

impl mm_traits::FrameAllocator for FrameAllocatorWrapper {
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
