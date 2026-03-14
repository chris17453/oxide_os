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
mod exports;
mod module_driver_bridge;

// Kernel modules
mod arch;
mod cmdline;
mod console;
mod fault;
mod globals;
mod init;
mod kstack_guard;
mod memory;
mod memtest;
mod mount;
mod oom;
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
/// — GraveShift: Panic output goes to BOTH console AND serial. If the screen is
/// dead or we triple-faulted into a reboot loop, serial is the only witness.
/// ISR-safe path — no locks, no allocations, just raw bytes to the wire.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // — GraveShift: Serial first. If the console write deadlocks (because we panicked
    // while holding the terminal lock), at least the serial port has the evidence.
    unsafe {
        os_log::write_str_raw("\n========================================\n");
        os_log::write_str_raw("  KERNEL PANIC!\n");
        os_log::write_str_raw("========================================\n");
    }

    if let Some(location) = info.location() {
        unsafe {
            os_log::write_str_raw("Location: ");
            os_log::write_str_raw(location.file());
            os_log::write_str_raw(":");
            os_log::write_u32_raw(location.line());
            os_log::write_str_raw(":");
            os_log::write_u32_raw(location.column());
            os_log::write_str_raw("\n");
        }
    }

    // — GraveShift: format_args! for the message — it might contain useful context
    // but we can't use write! on the ISR path. Print what we can.
    unsafe {
        os_log::write_str_raw("Message: ");
        // Use the lock-free formatted path for the full panic message
        os_log::_print_unsafe(format_args!("{}\n", info.message()));
        os_log::write_str_raw("\nSystem halted.\n");
    }

    // — GraveShift: Now try the console too. If we're lucky, the user sees it on screen.
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
