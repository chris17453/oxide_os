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

use efflux_arch_traits::Arch;
use efflux_arch_x86_64 as arch;
use efflux_arch_x86_64::serial;
use efflux_mm_heap::LockedHeap;

/// Global kernel heap allocator
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Heap size: 16 MB
const HEAP_SIZE: usize = 16 * 1024 * 1024;

/// Static heap storage (will be replaced with proper memory map later)
static mut HEAP_STORAGE: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

/// Kernel entry point
///
/// Called by the bootloader after setting up basic environment.
#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    // Initialize serial port
    serial::init();

    // Print boot banner
    let mut writer = serial::SerialWriter;
    let _ = writeln!(writer);
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "  EFFLUX Operating System");
    let _ = writeln!(writer, "  Version 0.1.0");
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer);

    let _ = writeln!(writer, "[INFO] Kernel started on x86_64");
    let _ = writeln!(writer, "[INFO] Serial output initialized");

    // Initialize heap
    let _ = writeln!(writer, "[INFO] Initializing heap allocator...");
    unsafe {
        let heap_start = addr_of_mut!(HEAP_STORAGE) as usize;
        HEAP_ALLOCATOR.init(heap_start, HEAP_SIZE);
    }
    let _ = writeln!(writer, "[INFO] Heap initialized: {} KB", HEAP_SIZE / 1024);

    // Test heap allocation
    let _ = writeln!(writer, "[INFO] Testing heap allocation...");

    // Test Box
    let boxed_value = Box::new(42u32);
    let _ = writeln!(writer, "[INFO] Box::new(42) = {}", *boxed_value);

    // Test Vec
    let mut vec: Vec<u32> = Vec::new();
    vec.push(1);
    vec.push(2);
    vec.push(3);
    let _ = writeln!(writer, "[INFO] Vec: {:?}", vec.as_slice());

    // Report heap stats
    let _ = writeln!(writer, "[INFO] Heap used: {} bytes", HEAP_ALLOCATOR.used());
    let _ = writeln!(writer, "[INFO] Heap free: {} bytes", HEAP_ALLOCATOR.free());

    let _ = writeln!(writer);
    let _ = writeln!(writer, "Memory subsystem initialized successfully!");
    let _ = writeln!(writer);

    // Halt
    let _ = writeln!(writer, "[INFO] Halting...");
    arch::X86_64::halt()
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
