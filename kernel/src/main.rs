//! EFFLUX Kernel
//!
//! Main kernel entry point.

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

use efflux_arch_traits::Arch;
use efflux_arch_x86_64 as arch;
use efflux_arch_x86_64::serial;
use efflux_arch_x86_64::get_user_context;
use efflux_boot_proto::{BootInfo, MemoryType as BootMemoryType};
use efflux_core::{PhysAddr, VirtAddr};
use efflux_elf::ElfExecutable;
use efflux_mm_frame::{BitmapFrameAllocator, MemoryRegion};
use efflux_mm_heap::LockedHeap;
use efflux_mm_paging::{phys_to_virt, read_cr3};
use efflux_proc::{
    UserAddressSpace, Process, ProcessContext, alloc_pid, process_table,
    do_fork, do_waitpid, WaitOptions, handle_cow_fault,
};
use efflux_proc_traits::{MemoryFlags, Pid};
use efflux_syscall::SyscallContext;
use efflux_vfs::{File, FileFlags, mount::GLOBAL_VFS, MountFlags, VnodeOps};
use efflux_devfs::DevFs;
use efflux_tmpfs::TmpDir;
use efflux_procfs::ProcFs;
use efflux_pty::{PtyManager, PtsDir};
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

/// User program (init.elf) embedded in kernel
static INIT_ELF: &[u8] = include_bytes!("../../userspace/init/init.elf");

/// Flag to track if user process has exited
static USER_EXITED: AtomicBool = AtomicBool::new(false);

/// Exit status from user process
static mut USER_EXIT_STATUS: i32 = 0;

/// Kernel PML4 physical address (for creating new address spaces)
static mut KERNEL_PML4: u64 = 0;

/// Child processes waiting to be run
static PENDING_CHILDREN: Mutex<Vec<Pid>> = Mutex::new(Vec::new());

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

    // Register page fault callback for COW handling
    unsafe {
        arch::exceptions::set_page_fault_callback(page_fault_handler);
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
        fork: Some(kernel_fork),
        exec: None,   // exec not needed for fork-wait test
        wait: Some(kernel_wait),
    };
    unsafe {
        efflux_syscall::init(syscall_ctx);
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
    if let Err(e) = root_fs.mkdir("dev", efflux_vfs::Mode::DEFAULT_DIR) {
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

    // Set up console write function for devfs
    unsafe {
        efflux_devfs::devices::set_console_write(console_write_bytes);
    }

    // Create /proc directory
    if let Err(e) = root_fs.mkdir("proc", efflux_vfs::Mode::DEFAULT_DIR) {
        let _ = writeln!(writer, "[VFS] Failed to create /proc: {:?}", e);
        arch::X86_64::halt();
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
    if let Err(e) = root_fs.mkdir("pts", efflux_vfs::Mode::DEFAULT_DIR) {
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
    // Allocate 128KB kernel stack - fork+COW uses ~67KB during deep recursion
    const KERNEL_STACK_SIZE: usize = 128 * 1024;
    let kernel_stack: Box<[u8; KERNEL_STACK_SIZE]> = Box::new([0u8; KERNEL_STACK_SIZE]);
    let kernel_stack_ptr = Box::into_raw(kernel_stack);
    let kernel_stack_top = unsafe { (kernel_stack_ptr as *const u8).add(KERNEL_STACK_SIZE) as u64 };

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
        PhysAddr::new(kernel_stack_ptr as u64),
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
    let user_pml4_virt = efflux_mm_paging::phys_to_virt(user_pml4_phys);
    unsafe {
        let pml4 = &*(user_pml4_virt.as_ptr::<efflux_mm_paging::PageTable>());
        let _ = writeln!(writer, "[USER] User PML4[256] = {:#x}", pml4[256].raw());
        let _ = writeln!(writer, "[USER] User PML4[511] = {:#x}", pml4[511].raw());
    }

    // Check user code mapping at 0x400000
    let _ = writeln!(writer, "[USER] User code mapping test:");
    if let Some(phys) = init_arc.lock().address_space().translate(VirtAddr::new(0x400000)) {
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
    efflux_syscall::dispatch(number, arg1, arg2, arg3, arg4, arg5, arg6)
}

/// Page fault handler callback (for COW and other page faults)
fn page_fault_handler(fault_addr: u64, error_code: u64, rip: u64) -> bool {
    let mut writer = serial::SerialWriter;

    // Check if this is a write fault on a present page (potential COW)
    let is_present = error_code & 1 != 0;
    let is_write = error_code & 2 != 0;
    let is_user = error_code & 4 != 0;

    let _ = writeln!(writer, "[COW] Page fault at {:#x}, error={:#x}, rip={:#x}", fault_addr, error_code, rip);
    let _ = writeln!(writer, "[COW] present={}, write={}, user={}", is_present, is_write, is_user);

    // COW faults are: present + write + user mode
    if is_present && is_write && is_user {
        // Get current process's PML4
        let table = process_table();
        let current_pid = table.current_pid();

        let _ = writeln!(writer, "[COW] Current PID: {}", current_pid);

        if let Some(proc) = table.get(current_pid) {
            let pml4 = proc.lock().address_space().pml4_phys();
            let alloc = FrameAllocatorWrapper;

            let _ = writeln!(writer, "[COW] PML4: {:#x}, attempting COW handling...", pml4.as_u64());

            // Try to handle as COW fault
            if handle_cow_fault(VirtAddr::new(fault_addr), pml4, &alloc) {
                let _ = writeln!(writer, "[COW] COW fault handled successfully!");
                return true; // Fault handled
            } else {
                let _ = writeln!(writer, "[COW] COW handler returned false");
            }
        } else {
            let _ = writeln!(writer, "[COW] Process not found!");
        }
    }

    let _ = writeln!(writer, "[COW] Fault not handled");
    false // Fault not handled - will panic
}

/// Console write function for syscalls
fn console_write(data: &[u8]) {
    let mut writer = serial::SerialWriter;
    for &byte in data {
        let _ = writer.write_char(byte as char);
    }
}

/// Console write function for devfs (same as console_write but typed for devfs)
fn console_write_bytes(data: &[u8]) {
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

    // Get current process and mark as zombie
    let table = process_table();
    let current_pid = table.current_pid();

    if let Some(proc) = table.get(current_pid) {
        proc.lock().exit(status);
    }

    unsafe {
        USER_EXIT_STATUS = status;
    }
    USER_EXITED.store(true, Ordering::SeqCst);

    let _ = writeln!(writer);
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "  User Process {} Exited", current_pid);
    let _ = writeln!(writer, "  Exit Status: {}", status);
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer);

    // Check if there's a parent waiting
    if let Some(proc) = table.get(current_pid) {
        let ppid = proc.lock().ppid();
        if ppid > 0 {
            // Parent exists, don't halt - let parent's wait() handle it
            // For now, just loop and let the parent detect the zombie
            let _ = writeln!(writer, "[INFO] Child {} exited, parent {} can reap", current_pid, ppid);

            // Simple approach: loop forever, parent's wait will detect zombie state
            loop {
                core::hint::spin_loop();
            }
        }
    }

    // No parent or init process exiting
    if status == 0 {
        let _ = writeln!(writer, "SUCCESS: User process completed successfully!");
    } else {
        let _ = writeln!(writer, "User process exited with non-zero status");
    }

    let _ = writeln!(writer);
    let _ = writeln!(writer, "[INFO] Phase 4 test complete. Halting.");

    arch::X86_64::halt();
}

/// Fork callback for syscalls
///
/// Creates a child process and returns child PID to parent, 0 to child.
fn kernel_fork() -> i64 {
    let mut writer = serial::SerialWriter;
    let _ = writeln!(writer, "[FORK] Fork called");

    let table = process_table();
    let parent_pid = table.current_pid();
    let _ = writeln!(writer, "[FORK] Parent PID: {}", parent_pid);

    // Get current process context from syscall user context
    let user_ctx = get_user_context();
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

    let _ = writeln!(writer, "[FORK] Parent context: rip={:#x} rsp={:#x}", parent_context.rip, parent_context.rsp);

    // Create wrapper for frame allocator
    let alloc_wrapper = FrameAllocatorWrapper;

    // Print RSP before call to validate stack
    let rsp_before: u64;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp_before, options(nomem, nostack)) };
    let _ = writeln!(writer, "[FORK] RSP before do_fork: {:#x}", rsp_before);
    // Print value at RSP to verify return address slot
    let ret_slot = unsafe { *(rsp_before as *const u64) };
    let _ = writeln!(writer, "[FORK] Value at RSP: {:#x}", ret_slot);

    let _ = writeln!(writer, "[FORK] Calling do_fork...");

    // Call do_fork
    let result = do_fork(parent_pid, &parent_context, &alloc_wrapper);

    // Print RSP after return to verify stack restored
    let rsp_after: u64;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp_after, options(nomem, nostack)) };
    let _ = writeln!(writer, "[FORK] RSP after do_fork: {:#x}", rsp_after);
    let _ = writeln!(writer, "[FORK] do_fork returned");

    match result {
        Ok(child_pid) => {
            let _ = writeln!(writer, "[FORK] Created child process {}", child_pid);

            // Add child to pending list for later execution
            PENDING_CHILDREN.lock().push(child_pid);

            // Return child PID to parent
            child_pid as i64
        }
        Err(e) => {
            let _ = writeln!(writer, "[FORK] Fork failed: {:?}", e);
            -1 // EAGAIN
        }
    }
}

/// Wait callback for syscalls
///
/// Waits for child process and returns (pid << 32) | status.
fn kernel_wait(pid: i32, options: i32) -> i64 {
    let mut writer = serial::SerialWriter;
    let _ = writeln!(writer, "[WAIT] Wait called for pid={}", pid);

    let table = process_table();
    let parent_pid = table.current_pid();
    let wait_opts = WaitOptions::from(options);

    // Check if we have a pending child to run first
    {
        let mut pending = PENDING_CHILDREN.lock();
        if let Some(child_pid) = pending.pop() {
            drop(pending); // Release lock before running child

            let _ = writeln!(writer, "[WAIT] Running pending child {}", child_pid);

            // Run the child process
            run_child_process(child_pid);

            let _ = writeln!(writer, "[WAIT] Child {} finished", child_pid);
        }
    }

    // Now wait for zombie children
    match do_waitpid(parent_pid, pid, wait_opts) {
        Ok(result) => {
            let _ = writeln!(writer, "[WAIT] Reaped child {} with status {}", result.pid, result.status);
            // Pack pid and status into result
            ((result.pid as i64) << 32) | ((result.status as i64) & 0xFFFFFFFF)
        }
        Err(e) => {
            let _ = writeln!(writer, "[WAIT] Wait failed: {:?}", e);
            match e {
                efflux_proc::WaitError::NoChildren => -10,  // ECHILD
                efflux_proc::WaitError::WouldBlock => -11,  // EAGAIN
                efflux_proc::WaitError::InvalidPid => -3,   // ESRCH
                efflux_proc::WaitError::Interrupted => -4,  // EINTR
            }
        }
    }
}

/// Run a child process to completion
fn run_child_process(child_pid: Pid) {
    let mut writer = serial::SerialWriter;

    let table = process_table();

    // Get child process info
    let (child_pml4, child_entry, child_stack, kernel_stack) = {
        let child = match table.get(child_pid) {
            Some(c) => c,
            None => {
                let _ = writeln!(writer, "[RUN] Child {} not found!", child_pid);
                return;
            }
        };

        let c = child.lock();
        (
            c.address_space().pml4_phys(),
            c.entry_point(),
            c.user_stack_top(),
            c.kernel_stack(),
        )
    };

    // Set current process to child
    let old_pid = table.current_pid();
    table.set_current_pid(child_pid);

    let _ = writeln!(writer, "[RUN] Switching to child {} at {:#x}", child_pid, child_entry.as_u64());

    // Allocate a new kernel stack for the child (the one in Process is physical addr)
    // Allocate 128KB kernel stack for child - matches parent stack size
    const CHILD_KERNEL_STACK_SIZE: usize = 128 * 1024;
    let child_kernel_stack: Box<[u8; CHILD_KERNEL_STACK_SIZE]> = Box::new([0u8; CHILD_KERNEL_STACK_SIZE]);
    let child_kernel_stack_ptr = Box::into_raw(child_kernel_stack);
    let child_kernel_stack_top = unsafe { (child_kernel_stack_ptr as *const u8).add(CHILD_KERNEL_STACK_SIZE) as u64 };

    // Set kernel stack for child's syscalls/interrupts
    unsafe {
        arch::syscall::set_kernel_stack(child_kernel_stack_top);
    }
    arch::gdt::set_kernel_stack(child_kernel_stack_top);

    // The child's context has rax=0 (fork return value)
    // We need to set up the child to return from the "fork syscall" with 0

    // For the child, we need to "return" to user mode at the point after fork syscall
    // The child's context was saved in do_fork with rax=0

    // Get child's saved context
    let child_ctx = {
        let child = table.get(child_pid).unwrap();
        child.lock().context().clone()
    };

    // Set up return to user mode
    // Child should return with rax=0 (fork return value is already set in context)
    let _ = writeln!(writer, "[RUN] Child context: rip={:#x} rsp={:#x} rax={}",
        child_ctx.rip, child_ctx.rsp, child_ctx.rax);

    // Enter user mode for child
    // Use return_to_usermode since we're returning from a "syscall"
    unsafe {
        arch::usermode::enter_usermode(
            child_kernel_stack_top,
            child_pml4.as_u64(),
            child_ctx.rip,
            child_ctx.rsp,
        );
    }

    // Note: We never get here because enter_usermode doesn't return
    // The child will exit via user_exit which will mark it as zombie
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
