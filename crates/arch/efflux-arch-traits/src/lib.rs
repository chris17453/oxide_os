//! EFFLUX Architecture Traits
//!
//! Defines the interface that all architecture implementations must provide.

#![no_std]

use efflux_core::{PhysAddr, VirtAddr};

/// Boot information passed from bootloader to kernel
pub trait BootInfo {
    /// Get the memory map
    fn memory_map(&self) -> &[MemoryRegion];

    /// Get the framebuffer info (if available)
    fn framebuffer(&self) -> Option<FramebufferInfo>;

    /// Get the kernel command line
    fn cmdline(&self) -> Option<&str>;

    /// Physical address where kernel is loaded
    fn kernel_phys_addr(&self) -> PhysAddr;

    /// Virtual address where kernel is mapped
    fn kernel_virt_addr(&self) -> VirtAddr;

    /// Size of the kernel image in bytes
    fn kernel_size(&self) -> usize;
}

/// Memory region descriptor
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base: PhysAddr,
    pub size: u64,
    pub region_type: MemoryType,
}

/// Type of memory region
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    /// Usable RAM
    Usable,
    /// Reserved by firmware
    Reserved,
    /// ACPI reclaimable
    AcpiReclaimable,
    /// ACPI NVS
    AcpiNvs,
    /// Unusable/defective
    Unusable,
    /// Kernel code and data
    Kernel,
    /// Bootloader data
    Bootloader,
    /// Framebuffer
    Framebuffer,
}

/// Framebuffer information
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    pub addr: PhysAddr,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u8,
}

/// Core architecture trait
pub trait Arch: Send + Sync {
    /// Architecture name
    fn name() -> &'static str;

    /// Page size in bytes
    fn page_size() -> usize;

    /// Kernel virtual base address
    fn kernel_base() -> VirtAddr;

    /// Halt the CPU
    fn halt() -> !;

    /// Disable interrupts
    fn disable_interrupts();

    /// Enable interrupts
    fn enable_interrupts();

    /// Are interrupts enabled?
    fn interrupts_enabled() -> bool;
}

/// Serial port trait for early console
pub trait Serial: Send {
    /// Initialize the serial port
    fn init(&mut self);

    /// Write a single byte
    fn write_byte(&mut self, byte: u8);

    /// Read a byte (non-blocking)
    fn read_byte(&mut self) -> Option<u8>;

    /// Write a string
    fn write_str(&mut self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
    }
}
