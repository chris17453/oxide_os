//! EFFLUX Architecture Traits
//!
//! Defines the interface that all architecture implementations must provide.

#![no_std]

use os_core::{PhysAddr, VirtAddr};

// ============================================================================
// Interrupt Controller Trait
// ============================================================================

/// Interrupt controller interface
pub trait InterruptController {
    /// Initialize the interrupt controller
    fn init();

    /// Enable interrupts globally
    fn enable();

    /// Disable interrupts globally
    fn disable();

    /// Send end-of-interrupt signal for the given vector
    fn end_of_interrupt(vector: u8);

    /// Set handler for a specific interrupt vector
    fn set_handler(vector: u8, handler: InterruptHandler);

    /// Mask (disable) a specific interrupt
    fn mask(irq: u8);

    /// Unmask (enable) a specific interrupt
    fn unmask(irq: u8);
}

/// Interrupt handler function type
pub type InterruptHandler = fn(&InterruptFrame);

/// Interrupt stack frame (architecture-specific layout)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptFrame {
    /// Instruction pointer at time of interrupt
    pub instruction_pointer: u64,
    /// Code segment
    pub code_segment: u64,
    /// CPU flags
    pub cpu_flags: u64,
    /// Stack pointer at time of interrupt
    pub stack_pointer: u64,
    /// Stack segment
    pub stack_segment: u64,
}

// ============================================================================
// Timer Trait
// ============================================================================

/// Timer device interface
pub trait Timer {
    /// Initialize the timer with the given frequency in Hz
    fn init(frequency_hz: u32);

    /// Start the timer
    fn start();

    /// Stop the timer
    fn stop();

    /// Set the timer interrupt handler
    fn set_handler(handler: fn());

    /// Get current tick count
    fn ticks() -> u64;
}

// ============================================================================
// Context Switch Trait
// ============================================================================

/// CPU context for context switching
///
/// Each architecture defines its own context layout.
/// This trait provides the interface for creating and switching contexts.
pub trait ContextSwitch {
    /// Architecture-specific context type
    type Context: Clone + Default;

    /// Create a new context for a thread
    ///
    /// - `entry`: Function to execute when the thread starts
    /// - `stack_top`: Top of the thread's kernel stack
    /// - `arg`: Argument to pass to the entry function
    fn new_context(entry: fn(usize) -> !, stack_top: usize, arg: usize) -> Self::Context;

    /// Switch from the current context to a new context
    ///
    /// # Safety
    /// - `old` must point to valid memory for saving the current context
    /// - `new` must contain a valid context to switch to
    unsafe fn switch(old: *mut Self::Context, new: *const Self::Context);
}

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

// ============================================================================
// TLB and Page Table Operations Trait
// ============================================================================

/// TLB and page table control operations
///
/// These are architecture-specific operations for managing the MMU.
pub trait TlbControl {
    /// Flush the TLB entry for a specific virtual address
    fn flush(addr: VirtAddr);

    /// Flush the entire TLB
    fn flush_all();

    /// Read the current page table root (e.g., CR3 on x86_64)
    fn read_root() -> PhysAddr;

    /// Write a new page table root
    ///
    /// # Safety
    /// The new root must point to a valid page table structure.
    unsafe fn write_root(root: PhysAddr);
}

// ============================================================================
// Port I/O Trait (x86-specific but needed by generic drivers)
// ============================================================================

/// Port-based I/O operations (primarily for x86 architectures)
///
/// On non-x86 architectures, this may be unimplemented or memory-mapped.
pub trait PortIo {
    /// Read a byte from an I/O port
    ///
    /// # Safety
    /// Port access may have side effects on hardware.
    unsafe fn inb(port: u16) -> u8;

    /// Write a byte to an I/O port
    ///
    /// # Safety
    /// Port access may have side effects on hardware.
    unsafe fn outb(port: u16, value: u8);

    /// Read a word from an I/O port
    ///
    /// # Safety
    /// Port access may have side effects on hardware.
    unsafe fn inw(port: u16) -> u16;

    /// Write a word to an I/O port
    ///
    /// # Safety
    /// Port access may have side effects on hardware.
    unsafe fn outw(port: u16, value: u16);

    /// Read a dword from an I/O port
    ///
    /// # Safety
    /// Port access may have side effects on hardware.
    unsafe fn inl(port: u16) -> u32;

    /// Write a dword to an I/O port
    ///
    /// # Safety
    /// Port access may have side effects on hardware.
    unsafe fn outl(port: u16, value: u32);
}
