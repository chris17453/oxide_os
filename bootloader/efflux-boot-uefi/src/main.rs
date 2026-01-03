//! EFFLUX UEFI Bootloader
//!
//! Minimal bootloader that prints a message and halts.
//! Full kernel loading will be added in a later phase.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    // Initialize UEFI services
    let _ = uefi::helpers::init();

    // Print banner
    uefi::helpers::system_table()
        .stdout()
        .output_string(cstr16!("\r\n"))
        .ok();
    uefi::helpers::system_table()
        .stdout()
        .output_string(cstr16!("========================================\r\n"))
        .ok();
    uefi::helpers::system_table()
        .stdout()
        .output_string(cstr16!("  EFFLUX UEFI Bootloader\r\n"))
        .ok();
    uefi::helpers::system_table()
        .stdout()
        .output_string(cstr16!("  Version 0.1.0\r\n"))
        .ok();
    uefi::helpers::system_table()
        .stdout()
        .output_string(cstr16!("========================================\r\n"))
        .ok();
    uefi::helpers::system_table()
        .stdout()
        .output_string(cstr16!("\r\n"))
        .ok();
    uefi::helpers::system_table()
        .stdout()
        .output_string(cstr16!("Bootloader running! Kernel loading not yet implemented.\r\n"))
        .ok();
    uefi::helpers::system_table()
        .stdout()
        .output_string(cstr16!("\r\n"))
        .ok();

    // Halt
    loop {
        unsafe { core::arch::asm!("hlt") };
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("hlt") };
    }
}
