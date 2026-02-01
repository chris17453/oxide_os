//! OXIDE Kernel
//!
//! Main kernel entry point.
//!
//! This is a thin wrapper that imports all kernel modules and delegates
//! to the init module for the actual boot sequence.

#![no_std]
#![no_main]
#![allow(unused)]

extern crate alloc;

#[macro_use]
mod debug;

// Kernel modules
mod console;
mod fault;
mod globals;
mod init;
mod memory;
mod mount;
mod process;
mod scheduler;
mod smp_init;
mod vfs_sched_glue;

use arch_traits::Arch;
use arch_x86_64 as arch;
use arch_x86_64::serial;
use core::fmt::Write;
use core::panic::PanicInfo;

/// Get a serial writer for debug output
pub fn serial_writer() -> serial::SerialWriter {
    serial::SerialWriter
}

/// Kernel entry point - delegates to init module
#[unsafe(no_mangle)]
pub extern "C" fn kernel_main(boot_info: &'static boot_proto::BootInfo) -> ! {
    init::kernel_main(boot_info)
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
        let _ = writeln!(
            writer,
            "Location: {}:{}:{}",
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
