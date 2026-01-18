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
use core::sync::atomic::{AtomicU64, Ordering};

use efflux_arch_traits::Arch;
use efflux_arch_x86_64 as arch;
use efflux_arch_x86_64::context::X86_64Context;
use efflux_arch_x86_64::serial;
use efflux_boot_proto::{BootInfo, MemoryType as BootMemoryType};
use efflux_core::VirtAddr;
use efflux_mm_frame::{BitmapFrameAllocator, MemoryRegion};
use efflux_mm_heap::LockedHeap;
use efflux_sched::{KernelThread, RoundRobinScheduler, Scheduler, Thread};

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

/// Global scheduler
static SCHEDULER: Mutex<Option<RoundRobinScheduler<X86_64Context>>> = Mutex::new(None);

/// Thread stack size (16 KB per thread)
const THREAD_STACK_SIZE: usize = 16 * 1024;

/// Thread 1 counter
static THREAD1_COUNT: AtomicU64 = AtomicU64::new(0);

/// Thread 2 counter
static THREAD2_COUNT: AtomicU64 = AtomicU64::new(0);

/// Flag to track if we've started running threads
static SCHEDULER_STARTED: AtomicU64 = AtomicU64::new(0);

/// Scheduler callback for preemptive context switching
///
/// Called by the timer interrupt handler with the current RSP.
/// Returns the RSP to restore from (same if no switch, different if switching threads).
fn scheduler_callback(current_rsp: u64) -> u64 {
    let mut sched = SCHEDULER.lock();

    if let Some(scheduler) = sched.as_mut() {
        // On first call, don't save RSP (we're still in kernel_main)
        // Just switch to the first thread
        if SCHEDULER_STARTED.load(Ordering::Relaxed) == 0 {
            SCHEDULER_STARTED.store(1, Ordering::Relaxed);
            // Get first thread
            if let Some(next) = scheduler.next() {
                return next.context().rsp();
            }
            return current_rsp;
        }

        // Save current thread's RSP
        if let Some(current) = scheduler.current_mut() {
            current.context_mut().set_rsp(current_rsp);
        }

        // Tick the scheduler - this may preempt current and schedule next
        let _switched = scheduler.tick();

        // Get the thread to run (current or newly scheduled)
        if let Some(thread) = scheduler.current() {
            return thread.context().rsp();
        } else if let Some(next) = scheduler.next() {
            return next.context().rsp();
        }
    }

    // No scheduler or no threads, return same RSP
    current_rsp
}


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
    // TODO: Use proper memory regions from boot info
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
    let _ = writeln!(writer, "[INFO] Free frames: {}", FRAME_ALLOCATOR.free_frames());
    let _ = writeln!(writer, "[INFO] Used frames: {}", FRAME_ALLOCATOR.used_frames());

    // Initialize architecture components (GDT, IDT, APIC)
    let _ = writeln!(writer, "[INFO] Initializing x86_64 architecture...");
    unsafe {
        arch::init();
    }

    // Start timer at 100Hz for preemptive scheduling
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

    let mut vec: Vec<u32> = Vec::new();
    vec.push(1);
    vec.push(2);
    vec.push(3);
    let _ = writeln!(writer, "[INFO] Vec: {:?}", vec.as_slice());

    // Report heap stats
    let _ = writeln!(writer, "[INFO] Heap used: {} bytes", HEAP_ALLOCATOR.used());
    let _ = writeln!(writer, "[INFO] Heap free: {} bytes", HEAP_ALLOCATOR.free());

    // Print framebuffer info if available
    if let Some(ref fb) = boot_info.framebuffer {
        let _ = writeln!(writer, "[INFO] Framebuffer: {}x{} @ {:#x}", fb.width, fb.height, fb.base);
    }

    let _ = writeln!(writer);
    let _ = writeln!(writer, "EFFLUX kernel initialized successfully!");
    let _ = writeln!(writer);

    // Wait for some timer ticks to verify timer is working
    let _ = writeln!(writer, "[INFO] Waiting for timer ticks...");
    let start_ticks = arch::timer_ticks();
    while arch::timer_ticks() < start_ticks + 10 {
        core::hint::spin_loop();
    }
    let elapsed = arch::timer_ticks() - start_ticks;
    let _ = writeln!(writer, "[INFO] Timer working: {} ticks elapsed", elapsed);

    // Initialize scheduler
    let _ = writeln!(writer, "[SCHED] Initializing scheduler...");
    {
        let mut sched = SCHEDULER.lock();
        *sched = Some(RoundRobinScheduler::new());
    }

    // Allocate stacks for test threads
    let _ = writeln!(writer, "[SCHED] Creating test threads...");

    // Thread 1 stack
    let stack1: Box<[u8; THREAD_STACK_SIZE]> = Box::new([0u8; THREAD_STACK_SIZE]);
    let stack1_ptr = Box::into_raw(stack1);
    let stack1_top = unsafe { (stack1_ptr as *const u8).add(THREAD_STACK_SIZE) as usize };

    // Thread 2 stack
    let stack2: Box<[u8; THREAD_STACK_SIZE]> = Box::new([0u8; THREAD_STACK_SIZE]);
    let stack2_ptr = Box::into_raw(stack2);
    let stack2_top = unsafe { (stack2_ptr as *const u8).add(THREAD_STACK_SIZE) as usize };

    // Create threads
    {
        let mut sched = SCHEDULER.lock();
        let scheduler = sched.as_mut().unwrap();

        let tid1 = scheduler.alloc_tid();
        let thread1 = KernelThread::<X86_64Context>::new(
            tid1,
            thread1_entry,
            VirtAddr::new(stack1_top as u64),
            THREAD_STACK_SIZE,
            1, // argument
        );
        scheduler.add(thread1);
        let _ = writeln!(writer, "[SCHED] Created thread 1 (tid={})", tid1);

        let tid2 = scheduler.alloc_tid();
        let thread2 = KernelThread::<X86_64Context>::new(
            tid2,
            thread2_entry,
            VirtAddr::new(stack2_top as u64),
            THREAD_STACK_SIZE,
            2, // argument
        );
        scheduler.add(thread2);
        let _ = writeln!(writer, "[SCHED] Created thread 2 (tid={})", tid2);
    }

    // Register scheduler callback with timer interrupt
    let _ = writeln!(writer, "[SCHED] Registering scheduler callback...");
    unsafe {
        arch::set_scheduler_callback(scheduler_callback);
    }

    let _ = writeln!(writer, "[SCHED] Starting preemptive multitasking...");
    let _ = writeln!(writer, "[SCHED] Threads will report progress and halt when done.");
    let _ = writeln!(writer);

    // Schedule the first thread
    {
        let mut sched = SCHEDULER.lock();
        if let Some(scheduler) = sched.as_mut() {
            // Mark first thread as running
            scheduler.next();
        }
    }

    // Wait for timer interrupt to switch us to a thread
    // Once that happens, this code path is abandoned
    loop {
        core::hint::spin_loop();
    }
}

/// Maximum iterations before thread reports and halts
const MAX_ITERATIONS: u64 = 500;

/// Thread 1 entry point
fn thread1_entry(_arg: usize) -> ! {
    let mut writer = serial::SerialWriter;
    loop {
        let count = THREAD1_COUNT.fetch_add(1, Ordering::Relaxed) + 1;

        // Report every 100 iterations
        if count % 100 == 0 {
            let _ = writeln!(writer, "[T1] count = {}", count);
        }

        // After enough iterations, report final and halt
        if count >= MAX_ITERATIONS {
            let t2 = THREAD2_COUNT.load(Ordering::Relaxed);
            let _ = writeln!(writer);
            let _ = writeln!(writer, "=== SCHEDULER TEST COMPLETE ===");
            let _ = writeln!(writer, "Thread 1: {} iterations", count);
            let _ = writeln!(writer, "Thread 2: {} iterations", t2);
            let _ = writeln!(writer);
            if count > 0 && t2 > 0 {
                let _ = writeln!(writer, "SUCCESS: Both threads executed!");
            } else {
                let _ = writeln!(writer, "FAILURE: A thread never ran!");
            }
            arch::X86_64::halt();
        }

        // Small delay
        for _ in 0..5000 {
            core::hint::spin_loop();
        }
    }
}

/// Thread 2 entry point
fn thread2_entry(_arg: usize) -> ! {
    let mut writer = serial::SerialWriter;
    loop {
        let count = THREAD2_COUNT.fetch_add(1, Ordering::Relaxed) + 1;

        // Report every 100 iterations
        if count % 100 == 0 {
            let _ = writeln!(writer, "[T2] count = {}", count);
        }

        // After enough iterations, report final and halt
        if count >= MAX_ITERATIONS {
            let t1 = THREAD1_COUNT.load(Ordering::Relaxed);
            let _ = writeln!(writer);
            let _ = writeln!(writer, "=== SCHEDULER TEST COMPLETE ===");
            let _ = writeln!(writer, "Thread 1: {} iterations", t1);
            let _ = writeln!(writer, "Thread 2: {} iterations", count);
            let _ = writeln!(writer);
            if count > 0 && t1 > 0 {
                let _ = writeln!(writer, "SUCCESS: Both threads executed!");
            } else {
                let _ = writeln!(writer, "FAILURE: A thread never ran!");
            }
            arch::X86_64::halt();
        }

        // Small delay
        for _ in 0..5000 {
            core::hint::spin_loop();
        }
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
