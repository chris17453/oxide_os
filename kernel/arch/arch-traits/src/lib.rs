//! OXIDE Architecture Traits
//!
//! Defines the interface that all architecture implementations must provide.
//!
//! This crate provides trait-based abstractions for:
//! - x86_64 (little-endian)
//! - ARM64/aarch64 (little-endian)
//! - SGI MIPS64 (big-endian)
//!
//! All traits use zero-cost abstractions via inline methods and static dispatch.
//! — NeonRoot

#![no_std]

pub mod context;

pub use context::*;
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

    /// ELF machine type (e_machine) for this architecture
    /// — BlackLatch: x86_64 = 0x3E (EM_X86_64), AArch64 = 0xB7 (EM_AARCH64),
    /// MIPS = 0x08 (EM_MIPS). Module loader uses this to validate ELF binaries.
    const ELF_MACHINE: u16;

    /// Page size in bytes
    fn page_size() -> usize;

    /// Kernel virtual base address
    fn kernel_base() -> VirtAddr;

    /// Halt the CPU (diverging — never returns)
    fn halt() -> !;

    /// Disable interrupts
    fn disable_interrupts();

    /// Enable interrupts
    fn enable_interrupts();

    /// Are interrupts enabled?
    fn interrupts_enabled() -> bool;

    /// Enable interrupts and wait for the next interrupt, then return.
    ///
    /// — NeonRoot: x86 = `sti; hlt` (atomically enables + halts so no
    /// interrupt window race). ARM = `wfi`. MIPS = `wait`. Every idle loop
    /// and blocking syscall sleep calls this instead of inlining asm.
    fn wait_for_interrupt();

    /// Begin a user-memory access region (disable SMAP/PAN protection).
    ///
    /// — NeonRoot: x86 = `stac` (Set AC flag to allow supervisor access to
    /// user pages). ARM = clear PAN bit. MIPS = nop (no equivalent).
    ///
    /// # Safety
    /// Caller must call `user_access_end()` when done. Leaving user access
    /// enabled is a security hole — any kernel bug can read/write user memory.
    unsafe fn user_access_begin();

    /// End a user-memory access region (re-enable SMAP/PAN protection).
    ///
    /// — NeonRoot: x86 = `clac` (Clear AC flag). ARM = set PAN bit.
    unsafe fn user_access_end();

    /// Read the current page table root (CR3 on x86, TTBR on ARM)
    fn read_page_table_root() -> PhysAddr;

    /// Switch to a different page table
    ///
    /// # Safety
    /// `root` must point to a valid page table structure.
    unsafe fn switch_page_table(root: PhysAddr);

    /// Read the current stack pointer
    fn read_stack_pointer() -> u64;

    /// Read the timestamp counter (TSC on x86, CNTPCT on ARM, Count on MIPS)
    /// — WireSaint: raw counter value, no calibration. For calibrated ns use os_core::now_ns().
    fn read_tsc() -> u64;

    /// Execute CPUID (x86) or equivalent feature query
    /// Returns (eax, ebx, ecx, edx) on x86; arch-specific on others
    /// — WireSaint: leaf/subleaf map to EAX/ECX inputs on x86. Other arches
    /// can repurpose or ignore them.
    fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32);

    /// Full memory fence (mfence on x86, dmb ish on ARM, sync on MIPS)
    /// — WireSaint: serializes all loads and stores across the fence.
    fn memory_fence();

    /// Read memory fence (lfence on x86, dmb ishld on ARM)
    /// — WireSaint: serializes loads only. Stores may reorder past this.
    fn read_fence();

    /// Write memory fence (sfence on x86, dmb ishst on ARM)
    /// — WireSaint: serializes stores only. Loads may reorder past this.
    fn write_fence();
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

// ============================================================================
// Control Register Operations
// ============================================================================

/// Control register operations for MMU and system state
///
/// Abstracts CR0/CR3/CR4 (x86), TTBR (ARM), CP0 Context (MIPS), etc.
/// — GraveShift
pub trait ControlRegisters {
    /// Page table root type (e.g., PhysAddr)
    type PageTableRoot;

    /// Read the page table root register
    fn read_page_table_root() -> Self::PageTableRoot;

    /// Write the page table root register
    ///
    /// # Safety
    /// Must point to a valid page table. May flush TLB.
    unsafe fn write_page_table_root(root: Self::PageTableRoot);

    /// Read the instruction pointer
    fn read_instruction_pointer() -> u64;

    /// Read the stack pointer
    fn read_stack_pointer() -> u64;
}

// ============================================================================
// System Registers (MSR, Special Registers)
// ============================================================================

/// System/Model-Specific Register access
///
/// Abstracts MSRs (x86), System regs (ARM), CP0 (MIPS)
/// — GraveShift
pub trait SystemRegisters {
    /// Read a system register
    ///
    /// # Safety
    /// Reading system registers may have side effects
    unsafe fn read_sys_reg(id: u32) -> u64;

    /// Write a system register
    ///
    /// # Safety
    /// Writing system registers can change system behavior
    unsafe fn write_sys_reg(id: u32, value: u64);
}

// ============================================================================
// Syscall Interface
// ============================================================================

/// System call mechanism abstraction
///
/// Handles syscall/sysret (x86), svc/eret (ARM), syscall/eret (MIPS)
/// — ThreadRogue
pub trait SyscallInterface {
    /// Syscall frame type (architecture-specific)
    type SyscallFrame;

    /// Initialize the syscall mechanism
    ///
    /// # Safety
    /// Sets up MSRs, vectors, or other arch-specific state
    unsafe fn init_syscall_mechanism();

    /// Get the syscall entry point address
    fn syscall_entry_point() -> usize;

    /// Extract syscall number from frame
    fn syscall_number(frame: &Self::SyscallFrame) -> usize;

    /// Extract syscall arguments from frame
    fn syscall_args(frame: &Self::SyscallFrame) -> [usize; 6];

    /// Set syscall return value in frame
    fn set_syscall_return(frame: &mut Self::SyscallFrame, value: usize);
}

// ============================================================================
// Exception Handling
// ============================================================================

/// Exception and interrupt handling abstraction
///
/// Handles IDT (x86), exception vectors (ARM/MIPS)
/// — BlackLatch
pub trait ExceptionHandler {
    /// Exception frame type
    type ExceptionFrame;

    /// Exception vector identifier type
    type ExceptionVector: Copy;

    /// Register an exception handler
    ///
    /// # Safety
    /// Handler must be a valid function pointer with correct calling convention
    unsafe fn register_exception(vector: Self::ExceptionVector, handler: usize);

    /// Initialize exception handling (IDT, vector table, etc.)
    ///
    /// # Safety
    /// Sets up critical system state
    unsafe fn init_exceptions();

    /// Convert exception frame to architecture-agnostic context
    fn exception_context_from_frame(frame: &Self::ExceptionFrame) -> InterruptContext;
}

/// Architecture-agnostic interrupt context
/// — BlackLatch
#[derive(Debug, Clone)]
pub struct InterruptContext {
    /// General purpose registers (up to 32)
    pub general_purpose: [u64; 32],
    /// Instruction pointer
    pub instruction_pointer: u64,
    /// Stack pointer
    pub stack_pointer: u64,
    /// Flags/status register
    pub flags: u64,
    /// Architecture-specific data (segments, etc.)
    pub arch_specific: [u64; 8],
}

// ============================================================================
// Cache Operations
// ============================================================================

/// Cache management operations
///
/// Critical for SGI MIPS (non-coherent DMA), less so for x86/ARM
/// — WireSaint
pub trait CacheOps {
    /// Flush all caches
    ///
    /// # Safety
    /// May impact performance, required before shutdown
    unsafe fn flush_cache();

    /// Flush cache for a specific range
    ///
    /// # Safety
    /// Required before DMA on non-coherent systems
    unsafe fn flush_cache_range(start: VirtAddr, len: usize);

    /// Invalidate cache for a specific range
    ///
    /// # Safety
    /// Required after DMA on non-coherent systems
    unsafe fn invalidate_cache_range(start: VirtAddr, len: usize);

    /// Invalidate instruction cache
    ///
    /// # Safety
    /// Required after code modification
    unsafe fn invalidate_icache();

    /// Is cache coherent with DMA?
    ///
    /// Returns false for SGI MIPS, true for x86/most ARM
    fn is_cache_coherent() -> bool;
}

// ============================================================================
// Endianness Handling
// ============================================================================

/// Endianness abstraction for big-endian SGI MIPS support
///
/// All conversions are zero-cost on matching endianness
/// — NeonRoot
pub trait Endianness {
    /// Is this architecture big-endian?
    fn is_big_endian() -> bool;

    /// Is this architecture little-endian?
    fn is_little_endian() -> bool {
        !Self::is_big_endian()
    }

    // Convert TO little-endian (for writing to disk/network in LE format)
    fn to_le16(val: u16) -> u16;
    fn to_le32(val: u32) -> u32;
    fn to_le64(val: u64) -> u64;

    // Convert FROM little-endian (for reading from disk/network in LE format)
    fn from_le16(val: u16) -> u16;
    fn from_le32(val: u32) -> u32;
    fn from_le64(val: u64) -> u64;

    // Convert TO big-endian (for writing in network byte order)
    fn to_be16(val: u16) -> u16;
    fn to_be32(val: u32) -> u32;
    fn to_be64(val: u64) -> u64;

    // Convert FROM big-endian (for reading network byte order)
    fn from_be16(val: u16) -> u16;
    fn from_be32(val: u32) -> u32;
    fn from_be64(val: u64) -> u64;
}

// ============================================================================
// DMA Operations
// ============================================================================

/// DMA synchronization for non-coherent systems
///
/// Critical for SGI MIPS, no-op for x86/coherent ARM
/// — WireSaint
pub trait DmaOps {
    /// Is DMA coherent with CPU caches?
    ///
    /// Returns false for SGI MIPS, true for x86/most ARM
    fn is_dma_coherent() -> bool;

    /// Synchronize cache before device reads from memory
    ///
    /// Writes back dirty cache lines
    ///
    /// # Safety
    /// Must be called before DMA write operation on non-coherent systems
    unsafe fn dma_sync_for_device(addr: PhysAddr, len: usize);

    /// Synchronize cache after device writes to memory
    ///
    /// Invalidates cache lines so CPU reads fresh data
    ///
    /// # Safety
    /// Must be called after DMA read operation on non-coherent systems
    unsafe fn dma_sync_for_cpu(addr: PhysAddr, len: usize);

    /// Map virtual address for DMA
    ///
    /// # Safety
    /// Returns physical address suitable for DMA
    unsafe fn dma_map(addr: VirtAddr, len: usize) -> PhysAddr;

    /// Unmap DMA region
    ///
    /// # Safety
    /// Must be called after DMA complete
    unsafe fn dma_unmap(addr: PhysAddr, len: usize);
}

// ============================================================================
// Boot Protocol
// ============================================================================

/// Boot protocol abstraction
///
/// Handles UEFI (x86/ARM), ARCS (SGI MIPS), Device Tree, etc.
/// — NeonRoot
pub trait BootProtocol {
    /// Boot information type
    type BootInfo: BootInfo;

    /// Early architecture initialization
    ///
    /// # Safety
    /// Called very early in boot, before memory management
    unsafe fn early_init(boot_info: &Self::BootInfo);

    /// Parse boot information from bootloader
    fn parse_boot_info(raw: &[u8]) -> Self::BootInfo;
}

// ============================================================================
// Atomic Operations
// ============================================================================

/// Architecture-optimized atomic operations
///
/// Uses lock prefix (x86), ldrex/strex (ARM), ll/sc (MIPS)
/// — RustViper
pub trait AtomicOps {
    /// Atomic compare-and-exchange on 64-bit value
    ///
    /// # Safety
    /// Pointer must be valid and aligned
    unsafe fn atomic_compare_exchange_64(ptr: *mut u64, old: u64, new: u64) -> u64;

    /// Full memory barrier
    ///
    /// # Safety
    /// Ensures all memory operations complete
    unsafe fn memory_barrier();

    /// Read memory barrier
    ///
    /// # Safety
    /// Ensures all prior reads complete
    unsafe fn read_barrier();

    /// Write memory barrier
    ///
    /// # Safety
    /// Ensures all prior writes complete
    unsafe fn write_barrier();
}

// ============================================================================
// Virtualization Support
// ============================================================================

/// Virtualization extensions support
///
/// VMX (Intel), SVM (AMD), VHE (ARM), VZ (MIPS)
/// — ColdCipher
pub trait VirtualizationExt {
    /// VM control structure type
    type VmcsType;

    /// Check if virtualization is supported
    fn has_virtualization() -> bool;

    /// Enable virtualization extensions
    ///
    /// # Safety
    /// Modifies CPU state to enable hypervisor mode
    unsafe fn enable_virtualization();

    /// Create a new VM control structure
    ///
    /// # Safety
    /// Returns VMCS/VMCB or equivalent
    unsafe fn create_vmcs() -> Option<Self::VmcsType>;
}

// ============================================================================
// SMP Operations
// — NeonRoot: the INIT/SIPI sequence, APIC IPIs, TSC timing — all of that
// belongs in the arch crate. Generic SMP code calls through this trait.
// ARM uses PSCI/GIC, MIPS uses cop0/KSEG — none of them have APICs.
// ============================================================================

/// SMP hardware operations — everything the generic SMP layer needs from arch
pub trait SmpOps {
    /// Get the current logical CPU ID
    fn cpu_id() -> Option<u32>;

    /// Execute the arch-specific AP boot sequence (INIT/SIPI/SIPI on x86,
    /// PSCI on ARM, etc). Does NOT do state tracking or timeout — that's
    /// the generic SMP crate's job.
    ///
    /// `hw_id` — hardware CPU identifier (APIC ID on x86, MPIDR on ARM)
    /// `trampoline_page` — page number containing AP startup trampoline
    fn boot_ap_sequence(hw_id: u32, trampoline_page: u8);

    /// Send a fixed-delivery IPI to a specific CPU by hardware ID
    fn send_ipi_to(hw_id: u32, vector: u8);

    /// Send IPI to all CPUs (optionally including self)
    fn send_ipi_broadcast(vector: u8, include_self: bool);

    /// Send IPI to self only
    fn send_ipi_self(vector: u8);

    /// Busy-wait for `ms` milliseconds
    fn delay_ms(ms: u64);

    /// Busy-wait for `us` microseconds
    fn delay_us(us: u64);

    /// Read a monotonic hardware counter (TSC on x86, CNTPCT on ARM)
    fn monotonic_counter() -> u64;

    /// Get the frequency of the monotonic counter in Hz
    fn monotonic_frequency() -> u64;
}
