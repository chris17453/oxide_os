//! OXIDE x86_64 Architecture Implementation
//!
//! Provides x86_64-specific implementations of architecture traits.

#![no_std]
#![allow(unused)]

extern crate ps2;

use arch_traits::{Arch, PortIo, TlbControl};
use os_core::{PhysAddr, VirtAddr};

pub mod ap_boot;
pub mod apic;
pub mod context;
pub mod exceptions;
pub mod gdt;
pub mod idt;
pub mod serial;
pub mod syscall;
pub mod usermode;

/// Return the current hardware CPU identifier (APIC ID on x86_64)
pub fn cpu_id() -> Option<u32> {
    Some(apic::id() as u32)
}

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
        VirtAddr::new(0xFFFF_FFFF_8000_0000)
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

impl TlbControl for X86_64 {
    #[inline]
    fn flush(addr: VirtAddr) {
        unsafe {
            core::arch::asm!("invlpg [{}]", in(reg) addr.as_u64(), options(nostack, preserves_flags));
        }
    }

    #[inline]
    fn flush_all() {
        unsafe {
            let cr3: u64;
            core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
            core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack));
        }
    }

    #[inline]
    fn read_root() -> PhysAddr {
        let cr3: u64;
        unsafe {
            core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
        }
        PhysAddr::new(cr3 & 0x000F_FFFF_FFFF_F000)
    }

    #[inline]
    unsafe fn write_root(root: PhysAddr) {
        unsafe {
            core::arch::asm!("mov cr3, {}", in(reg) root.as_u64(), options(nostack));
        }
    }
}

impl PortIo for X86_64 {
    #[inline]
    unsafe fn inb(port: u16) -> u8 {
        unsafe { inb(port) }
    }

    #[inline]
    unsafe fn outb(port: u16, value: u8) {
        unsafe { outb(port, value) }
    }

    #[inline]
    unsafe fn inw(port: u16) -> u16 {
        unsafe { inw(port) }
    }

    #[inline]
    unsafe fn outw(port: u16, value: u16) {
        unsafe { outw(port, value) }
    }

    #[inline]
    unsafe fn inl(port: u16) -> u32 {
        unsafe { inl(port) }
    }

    #[inline]
    unsafe fn outl(port: u16, value: u32) {
        unsafe { outl(port, value) }
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

/// Write a word to an I/O port
#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Read a word from an I/O port
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        core::arch::asm!(
            "in ax, dx",
            in("dx") port,
            out("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

/// Write a dword to an I/O port
#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Read a dword from an I/O port
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {
        core::arch::asm!(
            "in eax, dx",
            in("dx") port,
            out("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

/// Print to serial port (for use in arch crate)
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::serial::SerialWriter, $($arg)*);
    }};
}

/// Print to serial port with newline (for use in arch crate)
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => {{
        $crate::serial_print!($($arg)*);
        $crate::serial_print!("\n");
    }};
}

/// Initialize x86_64 architecture components
///
/// This sets up:
/// - GDT with TSS
/// - IDT with exception handlers
/// - Local APIC
///
/// # Safety
/// Must only be called once during kernel initialization.
pub unsafe fn init() {
    use core::ptr::addr_of_mut;

    unsafe {
        // Initialize GDT first (needed for IDT)
        gdt::init();
        serial_println!("[x86_64] GDT initialized");

        // Set up IST stack for double fault
        // Use a static stack for now
        static mut DOUBLE_FAULT_STACK: [u8; 4096 * 5] = [0; 4096 * 5];
        let stack_ptr = addr_of_mut!(DOUBLE_FAULT_STACK);
        let stack_top = (stack_ptr as *const u8).add((*stack_ptr).len()) as u64;
        gdt::set_ist(0, stack_top); // IST1 (index 0)

        // Initialize IDT
        idt::init();
        serial_println!("[x86_64] IDT initialized");

        // Initialize APIC
        apic::init();
    }
}

/// Start the system timer for preemptive scheduling
pub fn start_timer(frequency_hz: u32) {
    apic::start_timer(frequency_hz);
}

/// Get current timer tick count
pub fn timer_ticks() -> u64 {
    exceptions::ticks()
}

/// Set the scheduler callback for preemptive context switching
///
/// The callback is called on each timer interrupt with the current RSP
/// and should return the RSP to restore from.
///
/// # Safety
/// The callback must be valid and handle context switching correctly.
pub unsafe fn set_scheduler_callback(callback: exceptions::SchedulerCallback) {
    unsafe {
        exceptions::set_scheduler_callback(callback);
    }
}

/// Register a terminal tick callback (called at ~30 FPS from timer interrupt)
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_terminal_tick_callback(callback: fn()) {
    unsafe {
        exceptions::set_terminal_tick_callback(callback);
    }
}

/// Initialize the PS/2 keyboard controller (i8042)
///
/// Must be called before keyboard input will work. UEFI firmware may
/// leave the PS/2 controller disabled after ExitBootServices.
pub fn init_ps2_keyboard() {
    exceptions::init_ps2_keyboard();
}

/// Get a scancode from the keyboard buffer
pub fn get_scancode() -> Option<u8> {
    exceptions::get_scancode()
}

/// Poll i8042 directly for a scancode (fallback when IRQ1 doesn't fire)
///
/// # Safety
/// Must only be called from interrupt context (e.g., timer ISR).
pub unsafe fn poll_keyboard() -> Option<u8> {
    unsafe { exceptions::poll_keyboard() }
}

/// Read a byte from the serial port (COM1) if available
pub fn serial_read() -> Option<u8> {
    serial::read_byte()
}

/// Read a byte from the serial port without locking (for interrupt handlers)
///
/// # Safety
/// Must only be called from interrupt context.
pub unsafe fn serial_read_unsafe() -> Option<u8> {
    unsafe { serial::read_byte_unsafe() }
}

/// Register a keyboard interrupt callback (called on keyboard IRQ)
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_keyboard_callback(callback: fn()) {
    unsafe {
        exceptions::set_keyboard_callback(callback);
    }
}

/// Register a mouse interrupt callback (called on mouse IRQ 12)
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_mouse_callback(callback: fn()) {
    unsafe {
        exceptions::set_mouse_callback(callback);
    }
}

/// Register a TLB shootdown IPI callback (called on IPI from other CPUs)
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_tlb_shootdown_callback(callback: fn()) {
    unsafe {
        exceptions::set_tlb_shootdown_callback(callback);
    }
}

/// Re-export syscall user context type and getter
pub use syscall::{SyscallUserContext, get_user_context};

/// Re-export usermode transition functions and types
pub use usermode::{
    UserContext, enter_usermode, enter_usermode_with_context, jump_to_usermode, return_to_usermode,
};

/// Read the Time Stamp Counter
#[inline]
pub fn read_tsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}

/// Get TSC frequency in Hz (estimated)
///
/// This is a rough approximation and should be calibrated properly in production.
/// For now, we assume a typical modern CPU frequency.
pub fn tsc_frequency() -> u64 {
    // TODO: Calibrate properly using APIC timer or HPET
    // For now, assume ~2.5 GHz
    2_500_000_000
}

/// Delay for a given number of milliseconds using TSC
pub fn delay_ms(ms: u64) {
    let ticks_per_ms = tsc_frequency() / 1000;
    let end = read_tsc() + (ms * ticks_per_ms);
    while read_tsc() < end {
        core::hint::spin_loop();
    }
}

/// Delay for a given number of microseconds using TSC
pub fn delay_us(us: u64) {
    let ticks_per_us = tsc_frequency() / 1_000_000;
    let end = read_tsc() + (us * ticks_per_us);
    while read_tsc() < end {
        core::hint::spin_loop();
    }
}

// ============================================================================
// Kernel Preemption Control
// ============================================================================

use core::sync::atomic::{AtomicBool, Ordering};

const MAX_CPUS: usize = 256;

fn preempt_flag() -> &'static AtomicBool {
    let cpu = crate::apic::id() as usize;
    let idx = core::cmp::min(cpu, MAX_CPUS - 1);
    &KERNEL_PREEMPT_OK[idx]
}

/// Per-CPU flag indicating whether kernel code is currently safe to preempt.
/// A blocking syscall sets its CPU's flag before halting so the scheduler can
/// safely context switch without impacting other CPUs.
static KERNEL_PREEMPT_OK: [AtomicBool; MAX_CPUS] = [const { AtomicBool::new(false) }; MAX_CPUS];

/// Allow kernel preemption at current point
/// Call this before HLT in yielding syscalls like nanosleep
pub fn allow_kernel_preempt() {
    preempt_flag().store(true, Ordering::Release);
}

/// Disallow kernel preemption
pub fn disallow_kernel_preempt() {
    preempt_flag().store(false, Ordering::Release);
}

/// Check if kernel preemption is currently allowed
pub fn is_kernel_preempt_allowed() -> bool {
    preempt_flag().load(Ordering::Acquire)
}

/// Clear kernel preemption flag (called by scheduler after preempting)
pub fn clear_kernel_preempt() {
    preempt_flag().store(false, Ordering::Release);
}
