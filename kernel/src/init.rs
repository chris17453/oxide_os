//! Kernel initialization for the OXIDE kernel.
//!
//! Contains the kernel entry point and boot sequence.

extern crate alloc;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Write;
use core::ptr::addr_of_mut;

use arch_traits::Arch;
use arch_x86_64 as arch;
use arch_x86_64::serial;
use boot_proto::{BootInfo, MemoryType as BootMemoryType};
use devfs::DevFs;
use elf::ElfExecutable;
use mm_frame::MemoryRegion;
use mm_paging::{phys_to_virt, read_cr3};
use mm_traits::FrameAllocator as _;
use net::NetworkDevice;
use os_core::VirtAddr;
use proc::{Process, alloc_pid, process_table};
use proc_traits::MemoryFlags;
use procfs::ProcFs;
use pty::{PtsDir, PtyManager};
use syscall::SyscallContext;
use tmpfs::TmpDir;
use vfs::{File, FileFlags, MountFlags, VnodeOps, mount::GLOBAL_VFS};

use crate::console;
use crate::fault;
use crate::globals::{FRAME_ALLOCATOR, HEAP_ALLOCATOR, HEAP_SIZE, HEAP_STORAGE, KERNEL_PML4};
use crate::memory::{self, FrameAllocatorWrapper};
use crate::process::{kernel_exec, kernel_fork, kernel_wait, user_exit};
use crate::scheduler;
use crate::smp_init;

/// Kernel entry point
///
/// Called by the bootloader after setting up page tables and jumping to higher half.
pub fn kernel_main(boot_info: &'static BootInfo) -> ! {
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
    let _ = writeln!(
        writer,
        "[INFO] Kernel physical base: {:#x}",
        boot_info.kernel_phys_base
    );
    let _ = writeln!(
        writer,
        "[INFO] Kernel virtual base: {:#x}",
        boot_info.kernel_virt_base
    );
    let _ = writeln!(
        writer,
        "[INFO] Kernel size: {} bytes",
        boot_info.kernel_size
    );
    let _ = writeln!(
        writer,
        "[INFO] Physical map base: {:#x}",
        boot_info.phys_map_base
    );
    let _ = writeln!(writer, "[INFO] PML4 physical: {:#x}", boot_info.pml4_phys);

    // Print memory regions
    let _ = writeln!(
        writer,
        "[INFO] Memory regions: {}",
        boot_info.memory_region_count
    );
    let mut total_usable = 0u64;
    for region in boot_info.memory_regions() {
        if matches!(
            region.ty,
            BootMemoryType::Usable | BootMemoryType::BootServices
        ) {
            total_usable += region.len;
        }
    }
    let _ = writeln!(
        writer,
        "[INFO] Total usable memory: {} MB",
        total_usable / (1024 * 1024)
    );

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
        let usable = matches!(
            boot_region.ty,
            BootMemoryType::Usable | BootMemoryType::BootServices
        );
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
    let _ = writeln!(
        writer,
        "[INFO] Total frames: {}",
        FRAME_ALLOCATOR.total_frames()
    );
    let _ = writeln!(
        writer,
        "[INFO] Free frames: {}",
        FRAME_ALLOCATOR.free_frame_count()
    );

    // Initialize framebuffer if available
    if let Some(ref fb_info) = boot_info.framebuffer {
        let _ = writeln!(writer, "[INFO] Initializing framebuffer...");
        let _ = writeln!(
            writer,
            "[INFO] Framebuffer: {}x{} @ {:#x}",
            fb_info.width, fb_info.height, fb_info.base
        );
        let _ = writeln!(
            writer,
            "[INFO] Stride: {} pixels, BPP: {}",
            fb_info.stride, fb_info.bpp
        );

        // Initialize with video modes if available
        fb::init_from_boot(
            fb_info,
            boot_info.phys_map_base,
            boot_info.video_modes.as_ref(),
        );
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

    // Initialize SMP subsystem for Bootstrap Processor
    let _ = writeln!(writer, "[INFO] Initializing SMP subsystem...");
    let bsp_apic_id = arch::apic::id();
    unsafe {
        smp::cpu::init_bsp(bsp_apic_id as u32);
        // Note: init_bsp() already calls set_cpu_online(0)
    }
    let _ = writeln!(
        writer,
        "[SMP] Bootstrap Processor initialized (CPU 0, APIC ID {})",
        bsp_apic_id
    );

    // TEMPORARY: Manually register CPU 1 for testing
    // TODO: Replace with proper ACPI MADT enumeration and AP boot
    let _ = writeln!(writer, "[SMP] Detecting additional CPUs...");
    unsafe {
        // Register CPU 1 as present but not online (needs proper AP boot)
        smp::cpu::register_cpu(1, 1, false); // CPU 1, APIC ID 1, not BSP
        // Note: NOT calling set_cpu_online(1) - CPU is registered but offline
        // Need to implement INIT-SIPI-SIPI sequence to actually boot it
    }
    let _ = writeln!(
        writer,
        "[SMP] CPU 1 detected (APIC ID 1) - offline, needs AP boot"
    );

    let _ = writeln!(
        writer,
        "[SMP] CPUs detected: {}, CPUs online: {}",
        smp::cpu::cpu_count(),
        smp::cpu::cpus_online()
    );

    if smp::cpu::cpus_online() > 1 {
        let _ = writeln!(writer, "[SMP] Multi-CPU mode: TLB shootdown will use IPIs");
    } else {
        let _ = writeln!(
            writer,
            "[SMP] Single-CPU mode: TLB shootdown uses local flush only"
        );
    }

    // Register TLB shootdown IPI callback
    unsafe {
        arch::set_tlb_shootdown_callback(smp::tlb::handle_tlb_shootdown);
    }

    // Boot Application Processors if detected
    if smp::cpu::cpu_count() > 1 {
        let _ = writeln!(writer, "[SMP] Booting Application Processors...");

        // Allocate AP kernel stack (16KB)
        let ap_stack_phys = mm_frame::frame_allocator()
            .alloc_frames(4)
            .expect("Failed to allocate AP stack");
        let ap_stack_virt = mm_paging::phys_to_virt(ap_stack_phys).as_u64() + (4 * 4096);

        // Get CR3 for APs to use (current page table)
        let cr3 = <arch::X86_64 as arch_traits::TlbControl>::read_root();

        // Set up trampoline code at 0x8000
        unsafe {
            arch::ap_boot::setup_trampoline(
                cr3,
                ap_stack_virt,
                arch::ap_boot::ap_entry_rust as u64,
            );
        }
        let _ = writeln!(
            writer,
            "[SMP] AP trampoline set up at 0x{:x}",
            arch::ap_boot::TRAMPOLINE_PHYS
        );

        // Register AP initialization callback
        unsafe {
            arch::ap_boot::register_ap_init_callback(smp_init::ap_init_callback);
        }

        // Boot CPU 1
        let _ = writeln!(writer, "[SMP] Sending INIT-SIPI-SIPI to CPU 1...");
        match smp::cpu::boot_ap(1, arch::ap_boot::TRAMPOLINE_PAGE) {
            Ok(()) => {
                let _ = writeln!(writer, "[SMP] CPU 1 is now online!");
                let _ = writeln!(
                    writer,
                    "[SMP] CPUs online: {}/{}",
                    smp::cpu::cpus_online(),
                    smp::cpu::cpu_count()
                );
            }
            Err(e) => {
                let _ = writeln!(writer, "[SMP] Failed to boot CPU 1: {}", e);
            }
        }
    }

    // Register page fault callback for COW handling
    unsafe {
        arch::exceptions::set_page_fault_callback(fault::page_fault_handler);
    }

    // Register terminal tick callback for 30 FPS rendering
    if terminal::is_initialized() {
        unsafe {
            arch::set_terminal_tick_callback(console::terminal_tick);
        }
        let _ = writeln!(writer, "[INFO] Terminal tick callback registered (30 FPS)");
    }

    // Keyboard is handled by WATOS-style interrupt handler - no initialization needed
    let _ = writeln!(writer, "[INFO] Keyboard ready (WATOS-style)");

    // Register preemptive scheduler
    unsafe {
        arch::set_scheduler_callback(scheduler::scheduler_tick);
    }
    let _ = writeln!(writer, "[INFO] Preemptive scheduler registered");

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
        console_write: Some(console::console_write),
        console_read: Some(console::console_read),
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
        devfs::devices::set_serial_write(console::serial_write_bytes);
    }

    // Set up legacy console write function for devfs (fallback for early boot)
    unsafe {
        devfs::devices::set_console_write(console::console_write_bytes);
    }

    // Set up framebuffer info callback for /dev/fb0
    unsafe {
        devfs::devices::set_fb_info_callback(memory::get_fb_device_info);
        devfs::devices::set_fb_mode_count_callback(memory::get_fb_mode_count);
        devfs::devices::set_fb_mode_info_callback(memory::get_fb_mode_info);
        devfs::devices::set_fb_mode_set_callback(memory::set_fb_mode);
    }

    // Create /proc directory
    if let Err(e) = root_fs.mkdir("proc", vfs::Mode::DEFAULT_DIR) {
        let _ = writeln!(writer, "[VFS] Failed to create /proc: {:?}", e);
        arch::X86_64::halt();
    }

    // Set memory stats callback for procfs
    unsafe {
        procfs::set_memory_stats_callback(memory::get_memory_stats);
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
    if let Ok(_devfs_vnode) = GLOBAL_VFS.lookup("/dev") {
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
    let _ = writeln!(
        writer,
        "[NET] Found {} VirtIO network devices",
        virtio_net_devices.len()
    );

    // Initialize the first VirtIO network device found
    let net_initialized = if let Some(pci_dev) = virtio_net_devices.first() {
        let _ = writeln!(
            writer,
            "[NET] Initializing VirtIO network device at {:02x}:{:02x}.{}",
            pci_dev.address.bus, pci_dev.address.device, pci_dev.address.function
        );

        match unsafe { virtio_net::VirtioNet::from_pci(pci_dev) } {
            Some(virtio_net) => {
                let mac = virtio_net.mac_address();
                let _ = writeln!(writer, "[NET] VirtIO network device initialized");
                let _ = writeln!(
                    writer,
                    "[NET] MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    mac.0[0], mac.0[1], mac.0[2], mac.0[3], mac.0[4], mac.0[5]
                );

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
        lo_interface
            .set_ipv4_addr(
                net::Ipv4Addr::new(127, 0, 0, 1),
                net::Ipv4Addr::new(255, 0, 0, 0),
            )
            .ok();
        net::interface::add_interface(lo_interface);
    }

    let _ = writeln!(writer, "[NET] Network initialization complete");

    // Load and mount the initramfs (loaded from disk by bootloader)
    let initramfs_data = match boot_info.initramfs() {
        Some(data) => {
            let _ = writeln!(
                writer,
                "[INITRAMFS] Initramfs at phys {:#x}, {} bytes",
                boot_info.initramfs_phys, boot_info.initramfs_size
            );
            data
        }
        None => {
            let _ = writeln!(
                writer,
                "[INITRAMFS] ERROR: No initramfs loaded by bootloader!"
            );
            arch::X86_64::halt();
        }
    };

    let _ = writeln!(
        writer,
        "[INITRAMFS] Loading initramfs ({} bytes)...",
        initramfs_data.len()
    );
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

    let _ = writeln!(
        writer,
        "[USER] ELF entry point: {:#x}",
        elf.entry_point().as_u64()
    );

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

    let mut user_space =
        match unsafe { proc::UserAddressSpace::new_with_kernel(&alloc_wrapper, kernel_pml4) } {
            Some(s) => s,
            None => {
                let _ = writeln!(writer, "[USER] Failed to create user address space!");
                arch::X86_64::halt();
            }
        };
    let _ = writeln!(
        writer,
        "[USER] User PML4: {:#x}",
        user_space.pml4_phys().as_u64()
    );

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
                    let _ = writeln!(
                        writer,
                        "[USER] Failed to allocate page at {:#x}: {:?}",
                        page_addr.as_u64(),
                        e
                    );
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
                    let _ = writeln!(
                        writer,
                        "[USER] translate({:#x}) failed!",
                        page_vaddr.as_u64()
                    );
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

            if mem_offset + bytes_remaining_in_page > bss_start_in_seg
                && mem_offset < bss_end_in_seg
            {
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
    let stack_flags = MemoryFlags::READ
        .union(MemoryFlags::WRITE)
        .union(MemoryFlags::USER);

    if let Err(e) = user_space.allocate_pages(
        user_stack_base,
        user_stack_pages,
        stack_flags,
        &alloc_wrapper,
    ) {
        let _ = writeln!(writer, "[USER] Failed to allocate user stack: {:?}", e);
        arch::X86_64::halt();
    }

    let user_stack_end = VirtAddr::new(user_stack_base.as_u64() + (user_stack_pages * 4096) as u64);
    let _ = writeln!(
        writer,
        "[USER] User stack: {:#x} - {:#x}",
        user_stack_base.as_u64(),
        user_stack_end.as_u64()
    );

    // Set up argc/argv for init on the stack
    // Stack layout (growing down from top):
    //   [string]     - "/bin/init\0"
    //   [padding]
    //   [NULL]       - envp terminator
    //   [NULL]       - argv terminator
    //   [argv[0]]    - pointer to "/bin/init"
    //   [argc]       <- RSP points here
    let init_path_bytes = b"/bin/init\0";
    let mut stack_ptr = user_stack_end.as_u64();

    // Write string "/bin/init\0" near top of stack
    stack_ptr -= init_path_bytes.len() as u64;
    stack_ptr &= !0xF; // 16-byte align
    let string_addr = stack_ptr;

    // Write string to stack
    {
        let page_vaddr = VirtAddr::new(string_addr & !0xFFF);
        let page_offset = (string_addr & 0xFFF) as usize;
        let phys = user_space
            .translate(page_vaddr)
            .expect("Stack page not mapped");
        let dest_virt = phys_to_virt(phys);
        unsafe {
            let dest = dest_virt.as_mut_ptr::<u8>().add(page_offset);
            core::ptr::copy_nonoverlapping(init_path_bytes.as_ptr(), dest, init_path_bytes.len());
        }
    }

    // Space for: argc, argv[0], argv[1]=NULL, envp[0]=NULL (4 * 8 = 32 bytes)
    stack_ptr -= 32;
    stack_ptr &= !0xF;
    let final_rsp = stack_ptr;

    // Helper to write a u64 to the user stack
    let write_u64 = |addr: u64, value: u64| {
        let page_vaddr = VirtAddr::new(addr & !0xFFF);
        let page_offset = (addr & 0xFFF) as usize;
        let phys = user_space
            .translate(page_vaddr)
            .expect("Stack page not mapped");
        let dest_virt = phys_to_virt(phys);
        unsafe {
            let dest = dest_virt.as_mut_ptr::<u8>().add(page_offset) as *mut u64;
            *dest = value;
        }
    };

    // Write argc = 1
    write_u64(stack_ptr, 1);
    stack_ptr += 8;
    // Write argv[0] = pointer to string
    write_u64(stack_ptr, string_addr);
    stack_ptr += 8;
    // Write argv[1] = NULL (terminator)
    write_u64(stack_ptr, 0);
    stack_ptr += 8;
    // Write envp[0] = NULL (terminator)
    write_u64(stack_ptr, 0);

    let user_stack_top = VirtAddr::new(final_rsp);
    let _ = writeln!(
        writer,
        "[USER] Init stack set up: argc=1 argv={:#x} rsp={:#x}",
        string_addr, final_rsp
    );

    // Allocate kernel stack for syscalls and interrupts
    let _ = writeln!(writer, "[USER] Allocating kernel stack...");
    // Allocate 128KB kernel stack - fork+COW uses ~67KB during deep recursion
    const KERNEL_STACK_SIZE: usize = 128 * 1024;
    let kernel_stack_pages = KERNEL_STACK_SIZE / 4096;
    let kernel_stack_phys = FRAME_ALLOCATOR
        .alloc_frames(kernel_stack_pages)
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
    arch::gdt::set_kernel_stack(kernel_stack_top); // TSS.RSP0 for interrupts

    // Allocate a stack for double fault handling (IST1)
    let df_stack: Box<[u8; 8192]> = Box::new([0u8; 8192]);
    let df_stack_ptr = Box::into_raw(df_stack);
    let df_stack_top = unsafe { (df_stack_ptr as *const u8).add(8192) as u64 };
    arch::gdt::set_ist(0, df_stack_top); // IST1 = ist[0]

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
        0, // ppid = 0 (kernel)
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

    // Handle sched_yield specially - it needs to context switch
    const SCHED_YIELD: u64 = 130;
    if number == SCHED_YIELD {
        return scheduler::kernel_yield();
    }

    syscall::dispatch(number, arg1, arg2, arg3, arg4, arg5, arg6)
}
