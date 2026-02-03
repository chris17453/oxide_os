//! Architecture Abstraction Layer
//!
//! This module re-exports the current target architecture based on
//! conditional compilation. It provides a unified interface for
//! architecture-specific operations throughout the kernel.
//!
//! ## Supported Architectures
//!
//! - **x86_64**: Intel/AMD 64-bit (default)
//! - **aarch64**: ARM 64-bit
//! - **mips64**: SGI MIPS64 big-endian
//!
//! ## Usage
//!
//! ```rust
//! use crate::arch;
//!
//! // Use architecture-specific types
//! use arch::Arch;
//!
//! // Call architecture operations
//! arch::init();
//! arch::Arch::halt();
//! ```
//!
//! — NeonRoot

// ============================================================================
// Architecture Selection
// ============================================================================

#[cfg(all(
    target_arch = "x86_64",
    not(feature = "arch-aarch64"),
    not(feature = "arch-mips64")
))]
pub use arch_x86_64 as imp;

#[cfg(feature = "arch-aarch64")]
pub use arch_aarch64 as imp;

#[cfg(feature = "arch-mips64")]
pub use arch_mips64 as imp;

// Re-export the architecture type
pub use imp::*;

// Compile-time architecture validation
#[cfg(all(
    target_arch = "x86_64",
    not(feature = "arch-aarch64"),
    not(feature = "arch-mips64")
))]
pub type Arch = arch_x86_64::X86_64;

#[cfg(feature = "arch-aarch64")]
pub type Arch = arch_aarch64::AArch64;

#[cfg(feature = "arch-mips64")]
pub type Arch = arch_mips64::Mips64;

// ============================================================================
// Architecture Information
// ============================================================================

/// Get the current architecture name
pub fn arch_name() -> &'static str {
    use arch_traits::Arch as ArchTrait;
    Arch::name()
}

/// Get the page size for the current architecture
pub fn page_size() -> usize {
    use arch_traits::Arch as ArchTrait;
    Arch::page_size()
}

/// Check if interrupts are enabled
pub fn interrupts_enabled() -> bool {
    use arch_traits::Arch as ArchTrait;
    Arch::interrupts_enabled()
}

// ============================================================================
// Serial Console Abstraction
// ============================================================================

/// Initialize serial console
pub fn serial_init() {
    #[cfg(all(
        target_arch = "x86_64",
        not(feature = "arch-aarch64"),
        not(feature = "arch-mips64")
    ))]
    {
        arch_x86_64::serial::init();
    }

    #[cfg(feature = "arch-aarch64")]
    {
        // TODO: Initialize ARM64 serial
    }

    #[cfg(feature = "arch-mips64")]
    {
        // TODO: Initialize MIPS64 serial
    }
}

/// Write a single byte to serial console
pub fn serial_write_byte(byte: u8) {
    #[cfg(all(
        target_arch = "x86_64",
        not(feature = "arch-aarch64"),
        not(feature = "arch-mips64")
    ))]
    {
        arch_x86_64::serial::write_byte(byte);
    }

    #[cfg(feature = "arch-aarch64")]
    {
        // TODO: Implement ARM64 serial write
    }

    #[cfg(feature = "arch-mips64")]
    {
        // TODO: Implement MIPS64 serial write
    }
}

/// Serial writer for debug output
///
/// This wraps the architecture-specific serial implementation.
pub struct SerialWriter;

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        #[cfg(all(
            target_arch = "x86_64",
            not(feature = "arch-aarch64"),
            not(feature = "arch-mips64")
        ))]
        {
            use core::fmt::Write;
            arch_x86_64::serial::SerialWriter.write_str(s)
        }

        #[cfg(feature = "arch-aarch64")]
        {
            // TODO: Implement ARM64 serial
            Ok(())
        }

        #[cfg(feature = "arch-mips64")]
        {
            // TODO: Implement MIPS64 serial
            Ok(())
        }
    }
}

/// Get a serial writer instance
pub fn serial_writer() -> SerialWriter {
    SerialWriter
}

// ============================================================================
// PS/2 Keyboard Interrupt Handling
// — GraveShift: Fixed missing arch wrappers that broke keyboard input
// ============================================================================

/// Keyboard callback type
pub type KeyboardCallback = fn();

/// Initialize PS/2 keyboard hardware
///
/// Configures the 8042 controller and enables keyboard interrupts.
/// UEFI firmware may disable PS/2 after ExitBootServices, so this
/// re-enables it if needed.
pub fn init_ps2_keyboard() {
    #[cfg(all(
        target_arch = "x86_64",
        not(feature = "arch-aarch64"),
        not(feature = "arch-mips64")
    ))]
    {
        arch_x86_64::exceptions::init_ps2_keyboard();
    }

    #[cfg(feature = "arch-aarch64")]
    {
        // TODO: ARM64 keyboard init (if applicable)
    }

    #[cfg(feature = "arch-mips64")]
    {
        // TODO: MIPS64 keyboard init (SGI uses different input)
    }
}

/// Register keyboard IRQ callback
///
/// When keyboard IRQ (IRQ 1 / vector 33) fires, the registered callback
/// will be invoked to handle the scancode. This connects the low-level
/// interrupt handler to the PS/2 driver.
///
/// # Safety
/// Must be called during single-threaded initialization before interrupts
/// are fully enabled. The callback must be async-signal-safe and not block.
pub unsafe fn set_keyboard_callback(callback: KeyboardCallback) {
    #[cfg(all(
        target_arch = "x86_64",
        not(feature = "arch-aarch64"),
        not(feature = "arch-mips64")
    ))]
    {
        unsafe {
            arch_x86_64::exceptions::set_keyboard_callback(callback);
        }
    }

    #[cfg(feature = "arch-aarch64")]
    {
        // TODO: ARM64 keyboard callback registration
    }

    #[cfg(feature = "arch-mips64")]
    {
        // TODO: MIPS64 keyboard callback registration
    }
}

/// Get keyboard IRQ count (for debugging)
///
/// Returns the number of times the keyboard IRQ has fired since boot.
/// Useful for verifying that keyboard interrupts are working.
pub fn keyboard_irq_count() -> u64 {
    #[cfg(all(
        target_arch = "x86_64",
        not(feature = "arch-aarch64"),
        not(feature = "arch-mips64")
    ))]
    {
        arch_x86_64::exceptions::keyboard_irq_count()
    }

    #[cfg(feature = "arch-aarch64")]
    {
        0 // TODO: ARM64 keyboard IRQ counter
    }

    #[cfg(feature = "arch-mips64")]
    {
        0 // TODO: MIPS64 keyboard IRQ counter
    }
}
