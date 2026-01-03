//! EFFLUX x86_64 Architecture Implementation
//!
//! Provides x86_64-specific implementations of architecture traits.

#![no_std]

use efflux_arch_traits::Arch;
use efflux_core::VirtAddr;

pub mod serial;

/// x86_64 architecture implementation
pub struct X86_64;

impl Arch for X86_64 {
    fn name() -> &'static str {
        "x86_64"
    }

    fn page_size() -> usize {
        4096
    }

    fn kernel_base() -> VirtAddr {
        VirtAddr::new(0xFFFF_8000_0000_0000)
    }

    fn halt() -> ! {
        loop {
            unsafe {
                core::arch::asm!("hlt");
            }
        }
    }

    fn disable_interrupts() {
        unsafe {
            core::arch::asm!("cli", options(nomem, nostack));
        }
    }

    fn enable_interrupts() {
        unsafe {
            core::arch::asm!("sti", options(nomem, nostack));
        }
    }

    fn interrupts_enabled() -> bool {
        let flags: u64;
        unsafe {
            core::arch::asm!(
                "pushfq",
                "pop {}",
                out(reg) flags,
                options(nomem)
            );
        }
        // IF flag is bit 9
        (flags & (1 << 9)) != 0
    }
}

/// Write a byte to an I/O port
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Read a byte from an I/O port
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}
