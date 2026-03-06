//! OXIDE AArch64 (ARM64) Architecture Implementation
//!
//! Provides ARM64-specific implementations of architecture traits.
//! — NeonRoot

#![no_std]
#![allow(unused)]

use arch_traits::{
    Arch, AtomicOps, CacheOps, ControlRegisters, DmaOps, Endianness, ExceptionHandler,
    InterruptContext as ArchInterruptContext, PortIo, SmpOps, SyscallInterface, SystemRegisters,
    TlbControl,
};
use os_core::{PhysAddr, VirtAddr};

pub mod context;
pub mod exceptions;
pub mod serial;
pub mod syscall;

/// ARM64 architecture implementation
pub struct AArch64;

// ============================================================================
// Arch Trait Implementation
// — NeonRoot
// ============================================================================

impl Arch for AArch64 {
    const ELF_MACHINE: u16 = 0xB7; // EM_AARCH64

    fn name() -> &'static str {
        "aarch64"
    }

    fn page_size() -> usize {
        4096
    }

    fn kernel_base() -> VirtAddr {
        // ARM64 kernel typically at high address
        VirtAddr::new(0xFFFF_8000_0000_0000)
    }

    fn halt() -> ! {
        loop {
            unsafe {
                // WFI - Wait For Interrupt
                core::arch::asm!("wfi", options(nomem, nostack));
            }
        }
    }

    fn disable_interrupts() {
        unsafe {
            // MSR DAIFSet, #2 - Set I bit (IRQ mask)
            core::arch::asm!("msr daifset, #2", options(nomem, nostack));
        }
    }

    fn enable_interrupts() {
        unsafe {
            // MSR DAIFClr, #2 - Clear I bit (IRQ mask)
            core::arch::asm!("msr daifclr, #2", options(nomem, nostack));
        }
    }

    fn interrupts_enabled() -> bool {
        let daif: u64;
        unsafe {
            core::arch::asm!("mrs {}, daif", out(reg) daif, options(nomem, nostack));
        }
        // I bit is bit 7 of DAIF
        (daif & (1 << 7)) == 0
    }

    #[inline]
    fn wait_for_interrupt() {
        unsafe {
            // — NeonRoot: WFI — Wait For Interrupt. ARM equivalent of x86 sti+hlt.
            core::arch::asm!("msr daifclr, #2", options(nomem, nostack)); // enable IRQs
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn user_access_begin() {
        unsafe {
            // — NeonRoot: Clear PAN (Privileged Access Never) to allow
            // supervisor access to user pages. ARM equivalent of x86 STAC.
            core::arch::asm!("msr pan, #0", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn user_access_end() {
        unsafe {
            // — NeonRoot: Set PAN to re-enable user page protection.
            // ARM equivalent of x86 CLAC.
            core::arch::asm!("msr pan, #1", options(nomem, nostack));
        }
    }

    #[inline]
    fn read_page_table_root() -> PhysAddr {
        let ttbr0: u64;
        unsafe {
            core::arch::asm!("mrs {}, ttbr0_el1", out(reg) ttbr0, options(nomem, nostack));
        }
        PhysAddr::new(ttbr0)
    }

    #[inline]
    unsafe fn switch_page_table(root: PhysAddr) {
        unsafe {
            core::arch::asm!(
                "msr ttbr0_el1, {}",
                "isb",
                in(reg) root.as_u64(),
                options(nostack)
            );
        }
    }

    #[inline]
    fn read_stack_pointer() -> u64 {
        let sp: u64;
        unsafe {
            core::arch::asm!("mov {}, sp", out(reg) sp, options(nomem, nostack));
        }
        sp
    }

    #[inline]
    fn read_tsc() -> u64 {
        // — WireSaint: TODO — read CNTPCT_EL0 when ARM target is active
        0
    }

    #[inline]
    fn cpuid(_leaf: u32, _subleaf: u32) -> (u32, u32, u32, u32) {
        // — WireSaint: TODO — read MIDR_EL1 / feature regs when ARM target is active
        (0, 0, 0, 0)
    }

    #[inline]
    fn memory_fence() {
        // — WireSaint: TODO — dmb ish
    }

    #[inline]
    fn read_fence() {
        // — WireSaint: TODO — dmb ishld
    }

    #[inline]
    fn write_fence() {
        // — WireSaint: TODO — dmb ishst
    }
}

// ============================================================================
// TLB Control Implementation
// — NeonRoot
// ============================================================================

impl TlbControl for AArch64 {
    #[inline]
    fn flush(addr: VirtAddr) {
        unsafe {
            // TLBI VAE1, Xn - TLB Invalidate by VA, EL1
            core::arch::asm!(
                "tlbi vaae1is, {}",
                in(reg) addr.as_u64() >> 12,
                options(nostack)
            );
            // DSB - Data Synchronization Barrier
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
    }

    #[inline]
    fn flush_all() {
        unsafe {
            // TLBI VMALLE1 - TLB Invalidate all, EL1
            core::arch::asm!("tlbi vmalle1", options(nomem, nostack));
            // DSB - Ensure completion
            core::arch::asm!("dsb sy", options(nomem, nostack));
            // ISB - Instruction Synchronization Barrier
            core::arch::asm!("isb", options(nomem, nostack));
        }
    }

    #[inline]
    fn read_root() -> PhysAddr {
        let ttbr1: u64;
        unsafe {
            // Read TTBR1_EL1 (kernel page table)
            core::arch::asm!("mrs {}, ttbr1_el1", out(reg) ttbr1, options(nomem, nostack));
        }
        // Mask to get physical address (bits 47:1)
        PhysAddr::new(ttbr1 & 0x0000_FFFF_FFFF_FFFE)
    }

    #[inline]
    unsafe fn write_root(root: PhysAddr) {
        unsafe {
            // Write TTBR1_EL1 (kernel page table)
            core::arch::asm!(
                "msr ttbr1_el1, {}",
                in(reg) root.as_u64(),
                options(nostack)
            );
            // ISB to ensure visibility
            core::arch::asm!("isb", options(nomem, nostack));
        }
    }
}

// ============================================================================
// Port I/O (Not Applicable on ARM64)
// — TorqueJax
// ============================================================================

impl PortIo for AArch64 {
    // ARM64 does not have port I/O - all I/O is memory-mapped
    // These are stubs that panic if called

    #[inline]
    unsafe fn inb(_port: u16) -> u8 {
        panic!("Port I/O not supported on AArch64");
    }

    #[inline]
    unsafe fn outb(_port: u16, _value: u8) {
        panic!("Port I/O not supported on AArch64");
    }

    #[inline]
    unsafe fn inw(_port: u16) -> u16 {
        panic!("Port I/O not supported on AArch64");
    }

    #[inline]
    unsafe fn outw(_port: u16, _value: u16) {
        panic!("Port I/O not supported on AArch64");
    }

    #[inline]
    unsafe fn inl(_port: u16) -> u32 {
        panic!("Port I/O not supported on AArch64");
    }

    #[inline]
    unsafe fn outl(_port: u16, _value: u32) {
        panic!("Port I/O not supported on AArch64");
    }
}

// ============================================================================
// Control Registers Implementation
// — GraveShift
// ============================================================================

impl ControlRegisters for AArch64 {
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
        // Read PC register (approximate - can't read PC directly)
        let pc: u64;
        unsafe {
            core::arch::asm!(
                "adr {}, .",
                out(reg) pc,
                options(nomem, nostack)
            );
        }
        pc
    }

    #[inline]
    fn read_stack_pointer() -> u64 {
        let sp: u64;
        unsafe {
            core::arch::asm!(
                "mov {}, sp",
                out(reg) sp,
                options(nomem, nostack)
            );
        }
        sp
    }
}

// ============================================================================
// System Registers Implementation
// — GraveShift
// ============================================================================

impl SystemRegisters for AArch64 {
    #[inline]
    unsafe fn read_sys_reg(id: u32) -> u64 {
        // ARM64 system registers are accessed by name, not ID
        // This is a simplified implementation
        // Real implementation would use a match on id to select register
        panic!("Generic system register access not implemented for AArch64");
    }

    #[inline]
    unsafe fn write_sys_reg(id: u32, _value: u64) {
        // ARM64 system registers are accessed by name, not ID
        panic!("Generic system register access not implemented for AArch64");
    }
}

// ============================================================================
// Endianness Implementation (AArch64 is little-endian)
// — NeonRoot
// ============================================================================

impl Endianness for AArch64 {
    #[inline]
    fn is_big_endian() -> bool {
        false
    }

    #[inline]
    fn is_little_endian() -> bool {
        true
    }

    // TO little-endian (no-op on AArch64)
    #[inline]
    fn to_le16(val: u16) -> u16 {
        val
    }

    #[inline]
    fn to_le32(val: u32) -> u32 {
        val
    }

    #[inline]
    fn to_le64(val: u64) -> u64 {
        val
    }

    // FROM little-endian (no-op on AArch64)
    #[inline]
    fn from_le16(val: u16) -> u16 {
        val
    }

    #[inline]
    fn from_le32(val: u32) -> u32 {
        val
    }

    #[inline]
    fn from_le64(val: u64) -> u64 {
        val
    }

    // TO big-endian (swap on AArch64)
    #[inline]
    fn to_be16(val: u16) -> u16 {
        val.swap_bytes()
    }

    #[inline]
    fn to_be32(val: u32) -> u32 {
        val.swap_bytes()
    }

    #[inline]
    fn to_be64(val: u64) -> u64 {
        val.swap_bytes()
    }

    // FROM big-endian (swap on AArch64)
    #[inline]
    fn from_be16(val: u16) -> u16 {
        val.swap_bytes()
    }

    #[inline]
    fn from_be32(val: u32) -> u32 {
        val.swap_bytes()
    }

    #[inline]
    fn from_be64(val: u64) -> u64 {
        val.swap_bytes()
    }
}

// ============================================================================
// Cache Operations (ARM64 has explicit cache instructions)
// — WireSaint
// ============================================================================

impl CacheOps for AArch64 {
    #[inline]
    unsafe fn flush_cache() {
        // Clean and invalidate all data caches
        unsafe {
            // This is simplified - real implementation would walk cache levels
            core::arch::asm!("dc cisw, xzr", options(nomem, nostack));
            core::arch::asm!("dsb sy", options(nomem, nostack));
            core::arch::asm!("isb", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn flush_cache_range(start: VirtAddr, len: usize) {
        // DC CVAC - Data Cache Clean by VA to PoC
        let cache_line_size = 64; // Typical ARM64 cache line
        let mut addr = start.as_u64() & !(cache_line_size - 1);
        let end = (start.as_u64() + len as u64 + cache_line_size - 1) & !(cache_line_size - 1);

        while addr < end {
            unsafe {
                core::arch::asm!("dc cvac, {}", in(reg) addr, options(nostack));
            }
            addr += cache_line_size;
        }
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn invalidate_cache_range(start: VirtAddr, len: usize) {
        // DC IVAC - Data Cache Invalidate by VA to PoC
        let cache_line_size = 64;
        let mut addr = start.as_u64() & !(cache_line_size - 1);
        let end = (start.as_u64() + len as u64 + cache_line_size - 1) & !(cache_line_size - 1);

        while addr < end {
            unsafe {
                core::arch::asm!("dc ivac, {}", in(reg) addr, options(nostack));
            }
            addr += cache_line_size;
        }
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn invalidate_icache() {
        // IC IALLUIS - Instruction Cache Invalidate All to PoU, Inner Shareable
        unsafe {
            core::arch::asm!("ic ialluis", options(nomem, nostack));
            core::arch::asm!("dsb sy", options(nomem, nostack));
            core::arch::asm!("isb", options(nomem, nostack));
        }
    }

    #[inline]
    fn is_cache_coherent() -> bool {
        // Most ARM64 systems have coherent caches
        true
    }
}

// ============================================================================
// DMA Operations (ARM64 is typically coherent)
// — WireSaint
// ============================================================================

impl DmaOps for AArch64 {
    #[inline]
    fn is_dma_coherent() -> bool {
        // Most modern ARM64 systems have coherent DMA
        true
    }

    #[inline]
    unsafe fn dma_sync_for_device(_addr: PhysAddr, _len: usize) {
        // On coherent systems, just ensure memory ordering
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn dma_sync_for_cpu(_addr: PhysAddr, _len: usize) {
        // On coherent systems, just ensure memory ordering
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn dma_map(addr: VirtAddr, _len: usize) -> PhysAddr {
        // Simple identity mapping for now
        PhysAddr::new(addr.as_u64())
    }

    #[inline]
    unsafe fn dma_unmap(_addr: PhysAddr, _len: usize) {
        // Nothing to do on coherent systems
    }
}

// ============================================================================
// Atomic Operations (ARM64 load-exclusive/store-exclusive)
// — RustViper
// ============================================================================

impl AtomicOps for AArch64 {
    #[inline]
    unsafe fn atomic_compare_exchange_64(ptr: *mut u64, old: u64, new: u64) -> u64 {
        let prev: u64;
        unsafe {
            core::arch::asm!(
                "2:",
                "ldxr {prev}, [{ptr}]",       // Load Exclusive
                "cmp {prev}, {old}",          // Compare
                "b.ne 3f",                    // If not equal, exit
                "stxr w9, {new}, [{ptr}]",    // Store Exclusive (using w9 directly)
                "cbnz w9, 2b",                // Retry if failed
                "3:",
                ptr = in(reg) ptr,
                old = in(reg) old,
                new = in(reg) new,
                prev = out(reg) prev,
                out("x9") _,                  // Clobber x9
                options(nostack)
            );
        }
        prev
    }

    #[inline]
    unsafe fn memory_barrier() {
        // DMB SY - Data Memory Barrier, full system
        unsafe {
            core::arch::asm!("dmb sy", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn read_barrier() {
        // DMB LD - Data Memory Barrier, loads only
        unsafe {
            core::arch::asm!("dmb ld", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn write_barrier() {
        // DMB ST - Data Memory Barrier, stores only
        unsafe {
            core::arch::asm!("dmb st", options(nomem, nostack));
        }
    }
}

// ============================================================================
// Exception Handling Implementation (Skeleton)
// — BlackLatch
// ============================================================================

impl ExceptionHandler for AArch64 {
    type ExceptionFrame = exceptions::ExceptionFrame;
    type ExceptionVector = u8;

    #[inline]
    unsafe fn register_exception(_vector: Self::ExceptionVector, _handler: usize) {
        // TODO: Implement exception vector table setup
        panic!("Exception registration not yet implemented for AArch64");
    }

    #[inline]
    unsafe fn init_exceptions() {
        // TODO: Set up exception vector table (VBAR_EL1)
        panic!("Exception initialization not yet implemented for AArch64");
    }

    fn exception_context_from_frame(frame: &Self::ExceptionFrame) -> ArchInterruptContext {
        // Convert ARM64 exception frame to architecture-agnostic context
        let mut general_purpose = [0u64; 32];
        // ARM64 has 31 general purpose registers (x0-x30)
        general_purpose[0..31].copy_from_slice(&frame.x[0..31]);

        ArchInterruptContext {
            general_purpose,
            instruction_pointer: frame.elr,
            stack_pointer: frame.sp,
            flags: frame.spsr,
            arch_specific: [0; 8],
        }
    }
}

// ============================================================================
// Syscall Interface Implementation (Skeleton)
// — ThreadRogue
// ============================================================================

impl SyscallInterface for AArch64 {
    type SyscallFrame = syscall::SyscallFrame;

    #[inline]
    unsafe fn init_syscall_mechanism() {
        // TODO: Set up SVC handler
        panic!("Syscall initialization not yet implemented for AArch64");
    }

    #[inline]
    fn syscall_entry_point() -> usize {
        // TODO: Return address of SVC handler
        0
    }

    fn syscall_number(frame: &Self::SyscallFrame) -> usize {
        // Syscall number is in x8 on ARM64
        frame.x8 as usize
    }

    fn syscall_args(frame: &Self::SyscallFrame) -> [usize; 6] {
        // ARM64 syscall ABI: x0-x5
        [
            frame.x0 as usize,
            frame.x1 as usize,
            frame.x2 as usize,
            frame.x3 as usize,
            frame.x4 as usize,
            frame.x5 as usize,
        ]
    }

    fn set_syscall_return(frame: &mut Self::SyscallFrame, value: usize) {
        // Return value goes in x0
        frame.x0 = value as u64;
    }
}

// ============================================================================
// SMP Operations — NeonRoot: ARM uses PSCI for AP boot, GIC for IPIs
// ============================================================================

impl SmpOps for AArch64 {
    fn cpu_id() -> Option<u32> {
        let mpidr: u64;
        unsafe {
            core::arch::asm!("mrs {}, mpidr_el1", out(reg) mpidr, options(nomem, nostack));
        }
        Some((mpidr & 0xFF) as u32)
    }

    fn boot_ap_sequence(_hw_id: u32, _trampoline_page: u8) {
        // TODO: PSCI CPU_ON call
    }

    fn send_ipi_to(_hw_id: u32, _vector: u8) {
        // TODO: GIC SGI to specific PE
    }

    fn send_ipi_broadcast(_vector: u8, _include_self: bool) {
        // TODO: GIC SGI broadcast
    }

    fn send_ipi_self(_vector: u8) {
        // TODO: GIC SGI to self
    }

    fn delay_ms(_ms: u64) {
        // TODO: generic timer delay
    }

    fn delay_us(_us: u64) {
        // TODO: generic timer delay
    }

    fn monotonic_counter() -> u64 {
        let cnt: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) cnt, options(nomem, nostack));
        }
        cnt
    }

    fn monotonic_frequency() -> u64 {
        let freq: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq, options(nomem, nostack));
        }
        freq
    }
}
