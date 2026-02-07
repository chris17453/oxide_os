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
mod debug_buffer;

// Kernel modules
mod arch;
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

use arch_traits::Arch as _;
use core::fmt::Write;
use core::panic::PanicInfo;

/// Get a console writer for debug output
/// — PatchBay: Serial is DEAD. Returns console writer now.
pub fn console_writer() -> ConsoleWriter {
    ConsoleWriter
}

/// Console writer for panic/debug output
pub struct ConsoleWriter;

impl core::fmt::Write for ConsoleWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        console::console_write(s.as_bytes());
        Ok(())
    }
}

/// Kernel entry point - delegates to init module
#[unsafe(no_mangle)]
pub extern "C" fn kernel_main(boot_info: &'static boot_proto::BootInfo) -> ! {
    init::kernel_main(boot_info)
}

/// Panic handler
/// — PatchBay: Panic output goes to console (stderr), not serial
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut writer = ConsoleWriter;

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

    arch::Arch::halt()
}
