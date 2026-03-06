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
// — SableWire: one feature per arch, mutually exclusive. Makefile passes
// --features arch-$(ARCH) so exactly one of these fires. No triple-negative
// cfg gates, no target_arch checks — the feature IS the source of truth.
// ============================================================================

#[cfg(feature = "arch-x86_64")]
pub use arch_x86_64 as imp;

#[cfg(feature = "arch-aarch64")]
pub use arch_aarch64 as imp;

#[cfg(feature = "arch-mips64")]
pub use arch_mips64 as imp;

// Re-export everything from the selected arch crate
pub use imp::*;

// Concrete arch type for trait dispatch
#[cfg(feature = "arch-x86_64")]
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

/// Get the ELF e_machine value for this architecture
pub const fn elf_machine() -> u16 {
    use arch_traits::Arch as ArchTrait;
    Arch::ELF_MACHINE
}

// ============================================================================
// Serial Console Abstraction
// ============================================================================

/// Initialize serial console
/// — NeonRoot: delegates to whichever arch crate `imp` resolved to.
/// Each arch crate exports serial::init() with the same signature.
pub fn serial_init() {
    imp::serial::init();
}

/// Write a single byte to serial console
pub fn serial_write_byte(byte: u8) {
    imp::serial::write_byte(byte);
}

/// Serial writer for debug output
///
/// — NeonRoot: thin wrapper around the arch crate's SerialWriter.
/// All arch crates export serial::SerialWriter implementing fmt::Write.
pub struct SerialWriter;

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        use core::fmt::Write;
        imp::serial::SerialWriter.write_str(s)
    }
}

/// Get a serial writer instance
pub fn serial_writer() -> SerialWriter {
    SerialWriter
}

// ============================================================================
// CPU Operations — wrappers around Arch trait methods
// — NeonRoot: the kernel calls these, never inline asm
// ============================================================================

/// Enable interrupts and wait for the next interrupt (idle/sleep pattern)
///
/// x86 = sti+hlt (atomic), ARM = wfi, MIPS = wait
#[inline]
pub fn wait_for_interrupt() {
    use arch_traits::Arch as ArchTrait;
    Arch::wait_for_interrupt();
}

/// Begin user-memory access region (disable SMAP/PAN)
///
/// # Safety
/// Must pair with `user_access_end()`. Leaving this open is a security hole.
#[inline]
pub unsafe fn user_access_begin() {
    use arch_traits::Arch as ArchTrait;
    unsafe { Arch::user_access_begin(); }
}

/// End user-memory access region (re-enable SMAP/PAN)
#[inline]
pub unsafe fn user_access_end() {
    use arch_traits::Arch as ArchTrait;
    unsafe { Arch::user_access_end(); }
}

/// Read the current page table root (CR3 on x86, TTBR on ARM)
#[inline]
pub fn read_page_table_root() -> os_core::PhysAddr {
    use arch_traits::Arch as ArchTrait;
    Arch::read_page_table_root()
}

/// Switch to a different page table
///
/// # Safety
/// `root` must point to a valid page table structure.
#[inline]
pub unsafe fn switch_page_table(root: os_core::PhysAddr) {
    use arch_traits::Arch as ArchTrait;
    unsafe { Arch::switch_page_table(root); }
}

/// Read page table root as raw u64 — thin wrapper for os_core hook registration.
/// — NeonRoot: os_core hooks use u64 to stay type-agnostic; this bridges the gap.
#[inline]
pub fn read_page_table_root_raw() -> u64 {
    read_page_table_root().as_u64()
}

/// Write page table root from raw u64 — thin wrapper for os_core hook registration.
/// — NeonRoot: the unsafe contract passes through unchanged.
#[inline]
pub unsafe fn write_page_table_root_raw(root: u64) {
    unsafe { switch_page_table(os_core::PhysAddr::new(root)); }
}

/// Read the current stack pointer
#[inline]
pub fn read_stack_pointer() -> u64 {
    use arch_traits::Arch as ArchTrait;
    Arch::read_stack_pointer()
}

/// Enable interrupts
#[inline]
pub fn enable_interrupts() {
    use arch_traits::Arch as ArchTrait;
    Arch::enable_interrupts();
}

/// Disable interrupts
#[inline]
pub fn disable_interrupts() {
    use arch_traits::Arch as ArchTrait;
    Arch::disable_interrupts();
}

// ============================================================================
// System Registers, TSC, CPUID, Memory Fences
// — GraveShift: MSR/TSC/cpuid/fence wrappers — generic kernel code calls
// these, never inline asm. Subsystem crates use os_core hooks instead.
// ============================================================================

/// Read a model-specific register (MSR on x86, sys reg on ARM)
///
/// # Safety
/// Reading system registers may have side effects.
#[inline]
pub unsafe fn read_msr(id: u32) -> u64 {
    use arch_traits::SystemRegisters;
    unsafe { Arch::read_sys_reg(id) }
}

/// Write a model-specific register
///
/// # Safety
/// Writing system registers can change CPU behavior.
#[inline]
pub unsafe fn write_msr(id: u32, value: u64) {
    use arch_traits::SystemRegisters;
    unsafe { Arch::write_sys_reg(id, value) }
}

/// Read the timestamp counter (TSC on x86, CNTPCT on ARM)
#[inline]
pub fn read_tsc() -> u64 {
    use arch_traits::Arch as ArchTrait;
    Arch::read_tsc()
}

/// Execute CPUID query. Returns (eax, ebx, ecx, edx).
#[inline]
pub fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    use arch_traits::Arch as ArchTrait;
    Arch::cpuid(leaf, subleaf)
}

/// Full memory fence (mfence on x86, dmb on ARM)
#[inline]
pub fn memory_fence() {
    use arch_traits::Arch as ArchTrait;
    Arch::memory_fence();
}

/// Read memory fence (lfence on x86, dmb ld on ARM)
#[inline]
pub fn read_fence() {
    use arch_traits::Arch as ArchTrait;
    Arch::read_fence();
}

/// Write memory fence (sfence on x86, dmb st on ARM)
#[inline]
pub fn write_fence() {
    use arch_traits::Arch as ArchTrait;
    Arch::write_fence();
}

// ============================================================================
// Port I/O Wrappers
// — NeonRoot: x86 in/out instructions wrapped for kernel binary code.
// Subsystem crates use os_core::inb/outb etc. instead.
// ============================================================================

/// Read a byte from I/O port
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    use arch_traits::PortIo;
    unsafe { Arch::inb(port) }
}

/// Write a byte to I/O port
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    use arch_traits::PortIo;
    unsafe { Arch::outb(port, value) }
}

/// Read a word from I/O port
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    use arch_traits::PortIo;
    unsafe { Arch::inw(port) }
}

/// Write a word to I/O port
#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    use arch_traits::PortIo;
    unsafe { Arch::outw(port, value) }
}

/// Read a dword from I/O port
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    use arch_traits::PortIo;
    unsafe { Arch::inl(port) }
}

/// Write a dword to I/O port
#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    use arch_traits::PortIo;
    unsafe { Arch::outl(port, value) }
}

// ============================================================================
// TLB Operations
// — NeonRoot: generic TLB wrappers for os_core hook registration
// ============================================================================

/// Flush TLB entry for a virtual address
#[inline]
pub fn tlb_flush(addr: u64) {
    use arch_traits::TlbControl;
    Arch::flush(os_core::VirtAddr::new(addr));
}

/// Flush entire TLB
#[inline]
pub fn tlb_flush_all() {
    use arch_traits::TlbControl;
    Arch::flush_all();
}

// ============================================================================
// SMP Operations
// — NeonRoot: arch-specific SMP wrappers for os_core hook registration.
// These delegate to SmpOps trait methods on the arch type.
// ============================================================================

/// Send IPI to specific CPU
#[inline]
pub fn smp_send_ipi_to(hw_id: u32, vector: u8) {
    use arch_traits::SmpOps;
    Arch::send_ipi_to(hw_id, vector);
}

/// Broadcast IPI
#[inline]
pub fn smp_send_ipi_broadcast(vector: u8, include_self: bool) {
    use arch_traits::SmpOps;
    Arch::send_ipi_broadcast(vector, include_self);
}

/// Send IPI to self
#[inline]
pub fn smp_send_ipi_self(vector: u8) {
    use arch_traits::SmpOps;
    Arch::send_ipi_self(vector);
}

/// Boot an AP (application processor)
#[inline]
pub fn smp_boot_ap(hw_id: u32, trampoline_page: u8) {
    use arch_traits::SmpOps;
    Arch::boot_ap_sequence(hw_id, trampoline_page);
}

/// Get current CPU ID from hardware
#[inline]
pub fn smp_cpu_id() -> Option<u32> {
    use arch_traits::SmpOps;
    Arch::cpu_id()
}

/// Busy-wait for N microseconds
#[inline]
pub fn smp_delay_us(us: u64) {
    use arch_traits::SmpOps;
    Arch::delay_us(us);
}

/// Read monotonic hardware counter
#[inline]
pub fn smp_monotonic_counter() -> u64 {
    use arch_traits::SmpOps;
    Arch::monotonic_counter()
}

/// Get monotonic counter frequency
#[inline]
pub fn smp_monotonic_freq() -> u64 {
    use arch_traits::SmpOps;
    Arch::monotonic_frequency()
}

// ============================================================================
// PS/2 Keyboard Interrupt Handling
// — GraveShift: Fixed missing arch wrappers that broke keyboard input
// ============================================================================

/// Keyboard callback type
pub type KeyboardCallback = fn();

/// Initialize PS/2 keyboard hardware
/// — GraveShift: delegates to arch crate. No-op on ARM/MIPS (different input hw).
pub fn init_ps2_keyboard() {
    imp::exceptions::init_ps2_keyboard();
}

/// Register keyboard IRQ callback
///
/// # Safety
/// Must be called during single-threaded initialization before interrupts
/// are fully enabled. The callback must be async-signal-safe and not block.
pub unsafe fn set_keyboard_callback(callback: KeyboardCallback) {
    unsafe { imp::exceptions::set_keyboard_callback(callback); }
}

/// Get keyboard IRQ count (for debugging)
pub fn keyboard_irq_count() -> u64 {
    imp::exceptions::keyboard_irq_count()
}
