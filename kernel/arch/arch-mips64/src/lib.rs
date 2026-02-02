//! OXIDE MIPS64 (SGI) Architecture Implementation
//!
//! Provides MIPS64-specific implementations for Silicon Graphics (SGI) workstations.
//!
//! # Critical Characteristics
//!
//! - **Big-Endian**: Unlike x86/ARM, MIPS64 SGI systems are big-endian
//! - **Non-Coherent Caches**: Manual cache writeback/invalidation required for DMA
//! - **VIVT Caches**: Virtually Indexed, Virtually Tagged (aliasing concerns)
//! - **Small TLB**: Only 48-64 entries vs 1536+ on x86_64
//! - **ARCS Firmware**: Not UEFI, not Device Tree - SGI's ARCS boot protocol
//! - **Memory Segments**: KSEG0 (cached), KSEG1 (uncached), XKPHYS
//!
//! ## Target Platforms
//!
//! - **IP22**: Indy, Indigo2 (R4000-R5000, INT2, Zilog SCC)
//! - **IP27**: Origin 200/2000, Onyx2 (R10000-R14000, INT3, NUMA)
//! - **IP30**: Octane, Octane2 (R10000-R14000, INT3, HEART)
//! - **IP32**: O2, O2+ (R5000-R12000, INT2/CRM)
//!
//! — NeonRoot

#![no_std]
#![allow(unused)]

use arch_traits::{
    Arch, AtomicOps, CacheOps, ControlRegisters, DmaOps, Endianness, ExceptionHandler,
    InterruptContext as ArchInterruptContext, PortIo, SyscallInterface, SystemRegisters,
    TlbControl,
};
use os_core::{PhysAddr, VirtAddr};

pub mod context;
pub mod exceptions;
pub mod syscall;

/// MIPS64 architecture implementation (SGI big-endian)
pub struct Mips64;

// ============================================================================
// Memory Segment Constants
// — NeonRoot
// ============================================================================

/// KSEG0 base: Unmapped, cached kernel segment (0xFFFF_FFFF_8000_0000)
pub const KSEG0_BASE: u64 = 0xFFFF_FFFF_8000_0000;

/// KSEG1 base: Unmapped, uncached (for device I/O) (0xFFFF_FFFF_A000_0000)
pub const KSEG1_BASE: u64 = 0xFFFF_FFFF_A000_0000;

/// XKPHYS base: Large physical address window, cache-controllable
pub const XKPHYS_CACHED: u64 = 0x8000_0000_0000_0000;
pub const XKPHYS_UNCACHED: u64 = 0x9000_0000_0000_0000;

// ============================================================================
// Arch Trait Implementation
// — NeonRoot
// ============================================================================

impl Arch for Mips64 {
    fn name() -> &'static str {
        "mips64-sgi"
    }

    fn page_size() -> usize {
        4096 // MIPS64 supports multiple page sizes, 4K is typical
    }

    fn kernel_base() -> VirtAddr {
        // KSEG0: cached, unmapped kernel segment
        VirtAddr::new(KSEG0_BASE)
    }

    fn halt() -> ! {
        loop {
            unsafe {
                // WAIT - Enter wait state (low power)
                core::arch::asm!("wait", options(nomem, nostack));
            }
        }
    }

    fn disable_interrupts() {
        unsafe {
            // Clear IE bit in CP0 Status register
            let status: u64;
            core::arch::asm!(
                "mfc0 {status}, $12",       // Read CP0 Status
                "ori {status}, {status}, 0xFFFE",  // Set all bits except IE
                "xori {status}, {status}, 0xFFFE", // Clear IE bit (bit 0)
                "mtc0 {status}, $12",       // Write CP0 Status
                status = out(reg) status,
                options(nomem, nostack)
            );
        }
    }

    fn enable_interrupts() {
        unsafe {
            // Set IE bit in CP0 Status register
            let status: u64;
            core::arch::asm!(
                "mfc0 {status}, $12",       // Read CP0 Status
                "ori {status}, {status}, 0x0001", // Set IE bit (bit 0)
                "mtc0 {status}, $12",       // Write CP0 Status
                status = out(reg) status,
                options(nomem, nostack)
            );
        }
    }

    fn interrupts_enabled() -> bool {
        let status: u64;
        unsafe {
            core::arch::asm!(
                "mfc0 {}, $12",      // Read CP0 Status
                out(reg) status,
                options(nomem, nostack)
            );
        }
        // IE bit is bit 0
        (status & 0x1) != 0
    }
}

// ============================================================================
// TLB Control Implementation
// — NeonRoot
// ============================================================================

impl TlbControl for Mips64 {
    #[inline]
    fn flush(addr: VirtAddr) {
        unsafe {
            // MIPS TLB is indexed, need to probe then invalidate
            // TLBP - Probe TLB for matching entry
            let index: u64;
            core::arch::asm!(
                "dmtc0 {addr}, $10",  // Write to EntryHi (VPN)
                "tlbp",               // Probe
                "mfc0 {index}, $0",   // Read Index register
                "bltz {index}, 2f",   // If Index < 0, not found, skip
                "nop",
                // Entry found, invalidate it
                "dmtc0 $zero, $2",    // Clear EntryLo0
                "dmtc0 $zero, $3",    // Clear EntryLo1
                "tlbwi",              // Write Indexed
                "2:",
                addr = in(reg) addr.as_u64(),
                index = out(reg) index,
                options(nostack)
            );
        }
    }

    #[inline]
    fn flush_all() {
        unsafe {
            // MIPS requires invalidating all TLB entries individually
            // TLB size is typically 48-64 entries
            const TLB_SIZE: u32 = 64;

            for i in 0..TLB_SIZE {
                core::arch::asm!(
                    "dmtc0 {idx}, $0",     // Write to Index register
                    "dmtc0 $zero, $10",    // Clear EntryHi
                    "dmtc0 $zero, $2",     // Clear EntryLo0
                    "dmtc0 $zero, $3",     // Clear EntryLo1
                    "tlbwi",               // Write Indexed
                    idx = in(reg) i,
                    options(nostack)
                );
            }
        }
    }

    #[inline]
    fn read_root() -> PhysAddr {
        // MIPS uses CP0 Context register for page table base
        let context: u64;
        unsafe {
            core::arch::asm!(
                "dmfc0 {}, $4",      // Read CP0 Context
                out(reg) context,
                options(nomem, nostack)
            );
        }
        // Context register bits 63:23 contain PTEBase
        PhysAddr::new(context & 0xFFFF_FFFF_FF80_0000)
    }

    #[inline]
    unsafe fn write_root(root: PhysAddr) {
        unsafe {
            // Write page table base to CP0 Context register
            core::arch::asm!(
                "dmtc0 {}, $4",      // Write CP0 Context
                in(reg) root.as_u64(),
                options(nostack)
            );
        }
    }
}

// ============================================================================
// Port I/O (Not Applicable on MIPS64)
// — TorqueJax
// ============================================================================

impl PortIo for Mips64 {
    // MIPS64 does not have port I/O - all I/O is memory-mapped
    // These are stubs that panic if called

    #[inline]
    unsafe fn inb(_port: u16) -> u8 {
        panic!("Port I/O not supported on MIPS64");
    }

    #[inline]
    unsafe fn outb(_port: u16, _value: u8) {
        panic!("Port I/O not supported on MIPS64");
    }

    #[inline]
    unsafe fn inw(_port: u16) -> u16 {
        panic!("Port I/O not supported on MIPS64");
    }

    #[inline]
    unsafe fn outw(_port: u16, _value: u16) {
        panic!("Port I/O not supported on MIPS64");
    }

    #[inline]
    unsafe fn inl(_port: u16) -> u32 {
        panic!("Port I/O not supported on MIPS64");
    }

    #[inline]
    unsafe fn outl(_port: u16, _value: u32) {
        panic!("Port I/O not supported on MIPS64");
    }
}

// ============================================================================
// Control Registers Implementation
// — GraveShift
// ============================================================================

impl ControlRegisters for Mips64 {
    type PageTableRoot = PhysAddr;

    #[inline]
    fn read_page_table_root() -> Self::PageTableRoot {
        Self::read_root()
    }

    #[inline]
    unsafe fn write_page_table_root(root: Self::PageTableRoot) {
        unsafe {
            Self::write_root(root);
        }
    }

    #[inline]
    fn read_instruction_pointer() -> u64 {
        // Read CP0 EPC (Exception Program Counter)
        let epc: u64;
        unsafe {
            core::arch::asm!(
                "dmfc0 {}, $14",     // Read CP0 EPC
                out(reg) epc,
                options(nomem, nostack)
            );
        }
        epc
    }

    #[inline]
    fn read_stack_pointer() -> u64 {
        let sp: u64;
        unsafe {
            core::arch::asm!(
                "move {}, $sp",      // Move SP to output register
                out(reg) sp,
                options(nomem, nostack)
            );
        }
        sp
    }
}

// ============================================================================
// System Registers (CP0) Implementation
// — GraveShift
// ============================================================================

impl SystemRegisters for Mips64 {
    #[inline]
    unsafe fn read_sys_reg(id: u32) -> u64 {
        // MIPS CP0 registers are accessed by register number
        // This is a simplified implementation
        let value: u64;
        unsafe {
            match id {
                0 => core::arch::asm!("dmfc0 {}, $0", out(reg) value),  // Index
                4 => core::arch::asm!("dmfc0 {}, $4", out(reg) value),  // Context
                12 => core::arch::asm!("dmfc0 {}, $12", out(reg) value), // Status
                13 => core::arch::asm!("dmfc0 {}, $13", out(reg) value), // Cause
                14 => core::arch::asm!("dmfc0 {}, $14", out(reg) value), // EPC
                _ => panic!("Unsupported CP0 register: {}", id),
            }
        }
        value
    }

    #[inline]
    unsafe fn write_sys_reg(id: u32, value: u64) {
        unsafe {
            match id {
                0 => core::arch::asm!("dmtc0 {}, $0", in(reg) value),   // Index
                4 => core::arch::asm!("dmtc0 {}, $4", in(reg) value),   // Context
                12 => core::arch::asm!("dmtc0 {}, $12", in(reg) value), // Status
                14 => core::arch::asm!("dmtc0 {}, $14", in(reg) value), // EPC
                _ => panic!("Unsupported CP0 register: {}", id),
            }
        }
    }
}

// ============================================================================
// Endianness Implementation (MIPS64 SGI is BIG-ENDIAN) ⚠️
// — NeonRoot
// ============================================================================

impl Endianness for Mips64 {
    #[inline]
    fn is_big_endian() -> bool {
        true  // ⚠️ SGI MIPS64 is big-endian
    }

    #[inline]
    fn is_little_endian() -> bool {
        false
    }

    // TO little-endian (SWAP on big-endian MIPS64)
    #[inline]
    fn to_le16(val: u16) -> u16 {
        val.swap_bytes()  // Must swap on big-endian
    }

    #[inline]
    fn to_le32(val: u32) -> u32 {
        val.swap_bytes()  // Must swap on big-endian
    }

    #[inline]
    fn to_le64(val: u64) -> u64 {
        val.swap_bytes()  // Must swap on big-endian
    }

    // FROM little-endian (SWAP on big-endian MIPS64)
    #[inline]
    fn from_le16(val: u16) -> u16 {
        val.swap_bytes()  // Must swap on big-endian
    }

    #[inline]
    fn from_le32(val: u32) -> u32 {
        val.swap_bytes()  // Must swap on big-endian
    }

    #[inline]
    fn from_le64(val: u64) -> u64 {
        val.swap_bytes()  // Must swap on big-endian
    }

    // TO big-endian (NO-OP on big-endian MIPS64)
    #[inline]
    fn to_be16(val: u16) -> u16 {
        val  // No-op on big-endian
    }

    #[inline]
    fn to_be32(val: u32) -> u32 {
        val  // No-op on big-endian
    }

    #[inline]
    fn to_be64(val: u64) -> u64 {
        val  // No-op on big-endian
    }

    // FROM big-endian (NO-OP on big-endian MIPS64)
    #[inline]
    fn from_be16(val: u16) -> u16 {
        val  // No-op on big-endian
    }

    #[inline]
    fn from_be32(val: u32) -> u32 {
        val  // No-op on big-endian
    }

    #[inline]
    fn from_be64(val: u64) -> u64 {
        val  // No-op on big-endian
    }
}

// ============================================================================
// Cache Operations (MIPS64 SGI has NON-COHERENT VIVT caches) ⚠️
// — WireSaint
// ============================================================================

impl CacheOps for Mips64 {
    #[inline]
    unsafe fn flush_cache() {
        // Flush all data caches
        // This is a simplified implementation - real code would walk cache levels
        unsafe {
            core::arch::asm!(
                "sync",              // Synchronize
                ".set push",
                ".set noreorder",
                // Would need to iterate over cache lines here
                ".set pop",
                options(nomem, nostack)
            );
        }
    }

    #[inline]
    unsafe fn flush_cache_range(start: VirtAddr, len: usize) {
        // CACHE instruction: Hit Writeback Invalidate D-cache
        // Op code 0x15 = Hit Writeback Inv, cache 0 = D-cache
        const CACHE_LINE_SIZE: u64 = 32; // Typical for SGI MIPS

        let mut addr = start.as_u64() & !(CACHE_LINE_SIZE - 1);
        let end = (start.as_u64() + len as u64 + CACHE_LINE_SIZE - 1) & !(CACHE_LINE_SIZE - 1);

        unsafe {
            while addr < end {
                core::arch::asm!(
                    "cache 0x15, 0({})",  // Hit WB Inv D-cache
                    in(reg) addr,
                    options(nostack)
                );
                addr += CACHE_LINE_SIZE;
            }
            // SYNC to ensure completion
            core::arch::asm!("sync", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn invalidate_cache_range(start: VirtAddr, len: usize) {
        // CACHE instruction: Hit Invalidate D-cache
        const CACHE_LINE_SIZE: u64 = 32;

        let mut addr = start.as_u64() & !(CACHE_LINE_SIZE - 1);
        let end = (start.as_u64() + len as u64 + CACHE_LINE_SIZE - 1) & !(CACHE_LINE_SIZE - 1);

        unsafe {
            while addr < end {
                core::arch::asm!(
                    "cache 0x11, 0({})",  // Hit Inv D-cache
                    in(reg) addr,
                    options(nostack)
                );
                addr += CACHE_LINE_SIZE;
            }
            core::arch::asm!("sync", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn invalidate_icache() {
        // Invalidate instruction cache
        // CACHE instruction: Hit Invalidate I-cache
        unsafe {
            core::arch::asm!(
                "cache 0x10, 0($zero)", // Hit Inv I-cache
                "sync",
                options(nomem, nostack)
            );
        }
    }

    #[inline]
    fn is_cache_coherent() -> bool {
        false  // ⚠️ SGI MIPS64 has NON-coherent caches
    }
}

// ============================================================================
// DMA Operations (MIPS64 SGI requires MANUAL cache sync) ⚠️
// — WireSaint
// ============================================================================

impl DmaOps for Mips64 {
    #[inline]
    fn is_dma_coherent() -> bool {
        false  // ⚠️ SGI MIPS64 has non-coherent DMA
    }

    #[inline]
    unsafe fn dma_sync_for_device(addr: PhysAddr, len: usize) {
        // Before DMA write: flush cache to ensure data is in memory
        // Convert physical to KSEG0 virtual address for cache ops
        let virt = VirtAddr::new(KSEG0_BASE | addr.as_u64());
        unsafe {
            Self::flush_cache_range(virt, len);
        }
    }

    #[inline]
    unsafe fn dma_sync_for_cpu(addr: PhysAddr, len: usize) {
        // After DMA read: invalidate cache to read fresh data
        let virt = VirtAddr::new(KSEG0_BASE | addr.as_u64());
        unsafe {
            Self::invalidate_cache_range(virt, len);
        }
    }

    #[inline]
    unsafe fn dma_map(addr: VirtAddr, _len: usize) -> PhysAddr {
        // Convert KSEG0 virtual address to physical
        if addr.as_u64() >= KSEG0_BASE && addr.as_u64() < KSEG1_BASE {
            PhysAddr::new(addr.as_u64() & 0x1FFF_FFFF)
        } else {
            // Would need page table walk for mapped addresses
            PhysAddr::new(addr.as_u64())
        }
    }

    #[inline]
    unsafe fn dma_unmap(_addr: PhysAddr, _len: usize) {
        // Nothing to do
    }
}

// ============================================================================
// Atomic Operations (MIPS64 LL/SC - Load Linked/Store Conditional)
// — RustViper
// ============================================================================

impl AtomicOps for Mips64 {
    #[inline]
    unsafe fn atomic_compare_exchange_64(ptr: *mut u64, old: u64, new: u64) -> u64 {
        let prev: u64;
        let result: u64;
        unsafe {
            core::arch::asm!(
                "2:",
                "lld {prev}, 0({ptr})",     // Load Linked Doubleword
                "bne {prev}, {old}, 3f",    // If not equal, exit
                "nop",                       // Branch delay slot
                "move {result}, {new}",     // Prepare new value
                "scd {result}, 0({ptr})",   // Store Conditional
                "beqz {result}, 2b",        // Retry if failed
                "nop",                       // Branch delay slot
                "3:",
                ptr = in(reg) ptr,
                old = in(reg) old,
                new = in(reg) new,
                prev = out(reg) prev,
                result = out(reg) result,
                options(nostack)
            );
        }
        prev
    }

    #[inline]
    unsafe fn memory_barrier() {
        // SYNC - Full memory barrier
        unsafe {
            core::arch::asm!("sync", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn read_barrier() {
        // SYNC (MIPS doesn't have separate read/write barriers)
        unsafe {
            core::arch::asm!("sync", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn write_barrier() {
        // SYNC (MIPS doesn't have separate read/write barriers)
        unsafe {
            core::arch::asm!("sync", options(nomem, nostack));
        }
    }
}

// ============================================================================
// Exception Handling Implementation (Skeleton)
// — BlackLatch
// ============================================================================

impl ExceptionHandler for Mips64 {
    type ExceptionFrame = exceptions::ExceptionFrame;
    type ExceptionVector = u32;

    #[inline]
    unsafe fn register_exception(_vector: Self::ExceptionVector, _handler: usize) {
        // TODO: Set up exception handler in exception vector
        panic!("Exception registration not yet implemented for MIPS64");
    }

    #[inline]
    unsafe fn init_exceptions() {
        // TODO: Set up exception base (CP0 EBase register)
        panic!("Exception initialization not yet implemented for MIPS64");
    }

    fn exception_context_from_frame(frame: &Self::ExceptionFrame) -> ArchInterruptContext {
        // Convert MIPS64 exception frame to architecture-agnostic context
        ArchInterruptContext {
            general_purpose: frame.regs,
            instruction_pointer: frame.epc,
            stack_pointer: frame.sp,
            flags: frame.status,
            arch_specific: [
                frame.cause,
                frame.badvaddr,
                0, 0, 0, 0, 0, 0,
            ],
        }
    }
}

// ============================================================================
// Syscall Interface Implementation (Skeleton)
// — ThreadRogue
// ============================================================================

impl SyscallInterface for Mips64 {
    type SyscallFrame = syscall::SyscallFrame;

    #[inline]
    unsafe fn init_syscall_mechanism() {
        // TODO: Set up syscall handler
        panic!("Syscall initialization not yet implemented for MIPS64");
    }

    #[inline]
    fn syscall_entry_point() -> usize {
        // TODO: Return address of syscall handler
        0
    }

    fn syscall_number(frame: &Self::SyscallFrame) -> usize {
        // Syscall number is in v0 (register $2)
        frame.v0 as usize
    }

    fn syscall_args(frame: &Self::SyscallFrame) -> [usize; 6] {
        // MIPS64 syscall ABI: a0-a3 (registers $4-$7), plus stack args
        [
            frame.a0 as usize,
            frame.a1 as usize,
            frame.a2 as usize,
            frame.a3 as usize,
            frame.a4 as usize,
            frame.a5 as usize,
        ]
    }

    fn set_syscall_return(frame: &mut Self::SyscallFrame, value: usize) {
        // Return value goes in v0 (register $2)
        frame.v0 = value as u64;
    }
}
