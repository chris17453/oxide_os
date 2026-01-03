//! EFFLUX Kernel
//!
//! Main kernel entry point.

#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

use efflux_arch_traits::Arch;
use efflux_arch_x86_64 as arch;
use efflux_arch_x86_64::serial;

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

    let _ = writeln!(writer);
    let _ = writeln!(writer, "Hello from EFFLUX!");
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
