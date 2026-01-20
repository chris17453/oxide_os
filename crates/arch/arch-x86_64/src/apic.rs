//! Local APIC driver for x86_64
//!
//! The Local APIC provides per-CPU interrupt handling and timer functionality.

use core::ptr::{read_volatile, write_volatile};

use boot_proto::PHYS_MAP_BASE;

/// Physical address of the Local APIC (default)
const APIC_BASE_PHYS: u64 = 0xFEE0_0000;

/// APIC register offsets
mod reg {
    pub const ID: u32 = 0x020;
    pub const VERSION: u32 = 0x030;
    pub const TPR: u32 = 0x080;        // Task Priority Register
    pub const EOI: u32 = 0x0B0;        // End of Interrupt
    pub const SPURIOUS: u32 = 0x0F0;   // Spurious Interrupt Vector
    pub const ICR_LOW: u32 = 0x300;    // Interrupt Command Register (low)
    pub const ICR_HIGH: u32 = 0x310;   // Interrupt Command Register (high)
    pub const LVT_TIMER: u32 = 0x320;  // LVT Timer Register
    pub const LVT_LINT0: u32 = 0x350;  // LVT LINT0
    pub const LVT_LINT1: u32 = 0x360;  // LVT LINT1
    pub const LVT_ERROR: u32 = 0x370;  // LVT Error
    pub const TIMER_INIT: u32 = 0x380; // Timer Initial Count
    pub const TIMER_CURR: u32 = 0x390; // Timer Current Count
    pub const TIMER_DIV: u32 = 0x3E0;  // Timer Divide Configuration
}

/// Timer mode
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum TimerMode {
    OneShot = 0b00 << 17,
    Periodic = 0b01 << 17,
    TscDeadline = 0b10 << 17,
}

/// Timer divider
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum TimerDivide {
    Div2 = 0b0000,
    Div4 = 0b0001,
    Div8 = 0b0010,
    Div16 = 0b0011,
    Div32 = 0b1000,
    Div64 = 0b1001,
    Div128 = 0b1010,
    Div1 = 0b1011,
}

/// Virtual address of APIC (through direct physical map)
fn apic_base() -> u64 {
    PHYS_MAP_BASE + APIC_BASE_PHYS
}

/// Read an APIC register
fn read(offset: u32) -> u32 {
    unsafe {
        read_volatile((apic_base() + offset as u64) as *const u32)
    }
}

/// Write an APIC register
fn write(offset: u32, value: u32) {
    unsafe {
        write_volatile((apic_base() + offset as u64) as *mut u32, value);
    }
}

/// Check if APIC is available via CPUID
pub fn is_available() -> bool {
    let cpuid_result: u32;
    unsafe {
        // rbx is used by LLVM, so we need to save/restore it
        core::arch::asm!(
            "push rbx",
            "mov eax, 1",
            "cpuid",
            "mov {0:e}, edx",
            "pop rbx",
            out(reg) cpuid_result,
            out("eax") _,
            out("ecx") _,
            out("edx") _,
            options(nomem, nostack)
        );
    }
    // APIC bit is bit 9 of EDX
    (cpuid_result & (1 << 9)) != 0
}

/// Enable the Local APIC
pub fn enable() {
    // Enable APIC via MSR
    let mut msr: u64;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0x1B_u32,  // IA32_APIC_BASE MSR
            out("eax") msr,
            out("edx") _,
            options(nomem, nostack)
        );
    }

    // Set the enable bit (bit 11)
    msr |= 1 << 11;

    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0x1B_u32,
            in("eax") msr as u32,
            in("edx") (msr >> 32) as u32,
            options(nomem, nostack)
        );
    }

    // Set spurious interrupt vector and enable APIC (bit 8)
    write(reg::SPURIOUS, 0xFF | (1 << 8));

    // Set task priority to 0 (accept all interrupts)
    write(reg::TPR, 0);
}

/// Get the APIC ID
pub fn id() -> u8 {
    ((read(reg::ID) >> 24) & 0xFF) as u8
}

/// Get the APIC version
pub fn version() -> u8 {
    (read(reg::VERSION) & 0xFF) as u8
}

/// Send end-of-interrupt signal
pub fn end_of_interrupt() {
    write(reg::EOI, 0);
}

/// Configure and start the APIC timer
///
/// - `vector`: Interrupt vector to use for timer
/// - `mode`: Timer mode (one-shot or periodic)
/// - `divider`: Clock divider
/// - `initial_count`: Initial counter value
pub fn configure_timer(vector: u8, mode: TimerMode, divider: TimerDivide, initial_count: u32) {
    // Set divider
    write(reg::TIMER_DIV, divider as u32);

    // Configure LVT timer register
    let lvt = (vector as u32) | (mode as u32);
    write(reg::LVT_TIMER, lvt);

    // Set initial count (starts the timer)
    write(reg::TIMER_INIT, initial_count);
}

/// Stop the APIC timer
pub fn stop_timer() {
    // Mask the timer interrupt
    let lvt = read(reg::LVT_TIMER) | (1 << 16);
    write(reg::LVT_TIMER, lvt);
}

/// Get current timer count
pub fn timer_current() -> u32 {
    read(reg::TIMER_CURR)
}

/// Calibrate the APIC timer using PIT
///
/// Returns the number of APIC timer ticks per millisecond.
pub fn calibrate_timer() -> u32 {
    const PIT_FREQUENCY: u32 = 1193182;
    const CALIBRATION_MS: u32 = 10;

    // Set up PIT channel 2 for calibration
    // We'll use one-shot mode and count down from a known value
    let pit_count = (PIT_FREQUENCY / 1000) * CALIBRATION_MS;

    unsafe {
        // Set PIT to one-shot mode, channel 2
        crate::outb(0x61, (crate::inb(0x61) & 0xFD) | 0x01);  // Gate high, speaker off
        crate::outb(0x43, 0xB0);  // Channel 2, lobyte/hibyte, mode 0, binary

        // Set PIT count
        crate::outb(0x42, (pit_count & 0xFF) as u8);
        crate::outb(0x42, ((pit_count >> 8) & 0xFF) as u8);
    }

    // Set APIC timer to maximum count
    write(reg::TIMER_DIV, TimerDivide::Div16 as u32);
    write(reg::LVT_TIMER, 1 << 16);  // Masked, one-shot
    write(reg::TIMER_INIT, 0xFFFF_FFFF);

    // Wait for PIT to count down
    unsafe {
        // Reset PIT gate to start counting
        let val = crate::inb(0x61) & 0xFE;
        crate::outb(0x61, val);
        crate::outb(0x61, val | 0x01);

        // Wait for PIT output to go high (count reached zero)
        while crate::inb(0x61) & 0x20 == 0 {}
    }

    // Read APIC timer current count
    let elapsed = 0xFFFF_FFFF - timer_current();

    // Stop APIC timer
    stop_timer();

    // Calculate ticks per millisecond
    elapsed / CALIBRATION_MS
}

/// Initialize the APIC
pub fn init() {
    if !is_available() {
        panic!("APIC not available!");
    }

    enable();

    crate::serial_println!("[APIC] Initialized, ID: {}, Version: {}",
        id(), version());

    // Initialize IOAPIC for legacy IRQs
    init_ioapic();
}

// ============================================================================
// IOAPIC Support
// ============================================================================

/// IOAPIC base address (standard location)
const IOAPIC_BASE: u64 = 0xFEC0_0000;

/// IOAPIC register select
const IOAPIC_REGSEL: u32 = 0x00;
/// IOAPIC data register
const IOAPIC_DATA: u32 = 0x10;

/// IOAPIC registers
mod ioapic_reg {
    pub const ID: u32 = 0x00;
    pub const VERSION: u32 = 0x01;
    pub const REDTBL_BASE: u32 = 0x10;
}

/// IOAPIC virtual address
fn ioapic_base() -> u64 {
    boot_proto::PHYS_MAP_BASE + IOAPIC_BASE
}

/// Write to IOAPIC register select
fn ioapic_select(reg: u32) {
    unsafe {
        write_volatile((ioapic_base() + IOAPIC_REGSEL as u64) as *mut u32, reg);
    }
}

/// Read from IOAPIC data register
fn ioapic_read() -> u32 {
    unsafe {
        read_volatile((ioapic_base() + IOAPIC_DATA as u64) as *const u32)
    }
}

/// Write to IOAPIC data register
fn ioapic_write(value: u32) {
    unsafe {
        write_volatile((ioapic_base() + IOAPIC_DATA as u64) as *mut u32, value);
    }
}

/// Read IOAPIC register
fn ioapic_read_reg(reg: u32) -> u32 {
    ioapic_select(reg);
    ioapic_read()
}

/// Write IOAPIC register
fn ioapic_write_reg(reg: u32, value: u32) {
    ioapic_select(reg);
    ioapic_write(value);
}

/// Configure IOAPIC redirection entry
///
/// - `irq`: Legacy IRQ number (0-23)
/// - `vector`: Interrupt vector to route to
/// - `dest_apic`: Destination APIC ID
/// - `masked`: Whether the interrupt is masked
fn ioapic_set_irq(irq: u8, vector: u8, dest_apic: u8, masked: bool) {
    let reg_base = ioapic_reg::REDTBL_BASE + (irq as u32 * 2);

    // Low 32 bits: vector, delivery mode (0=fixed), destination mode (0=physical),
    // polarity (0=high), trigger (0=edge), mask
    let low: u32 = (vector as u32)
        | (0 << 8)   // Delivery mode: Fixed
        | (0 << 11)  // Destination mode: Physical
        | (0 << 13)  // Polarity: Active high
        | (0 << 15)  // Trigger mode: Edge
        | (if masked { 1 << 16 } else { 0 }); // Mask

    // High 32 bits: destination APIC ID
    let high: u32 = (dest_apic as u32) << 24;

    ioapic_write_reg(reg_base, low);
    ioapic_write_reg(reg_base + 1, high);
}

/// Initialize IOAPIC
fn init_ioapic() {
    // Check IOAPIC version
    let version = ioapic_read_reg(ioapic_reg::VERSION);
    let max_redir = ((version >> 16) & 0xFF) as u8;
    crate::serial_println!("[IOAPIC] Version: {:#x}, Max redirections: {}",
        version & 0xFF, max_redir + 1);

    // Mask all IRQs first
    for irq in 0..=max_redir {
        ioapic_set_irq(irq, 0, 0, true);
    }

    // Get local APIC ID
    let apic_id = id();

    // Route keyboard IRQ 1 to vector 33
    ioapic_set_irq(1, crate::idt::vector::KEYBOARD, apic_id, false);
    crate::serial_println!("[IOAPIC] Keyboard IRQ 1 -> vector {} (APIC {})",
        crate::idt::vector::KEYBOARD, apic_id);
}

/// Start the APIC timer for scheduling
///
/// - `frequency_hz`: Desired interrupt frequency in Hz
pub fn start_timer(frequency_hz: u32) {
    let ticks_per_ms = calibrate_timer();
    crate::serial_println!("[APIC] Timer calibrated: {} ticks/ms", ticks_per_ms);

    // Calculate initial count for desired frequency
    let interval_ms = 1000 / frequency_hz;
    let initial_count = ticks_per_ms * interval_ms;

    crate::serial_println!("[APIC] Starting timer at {}Hz (count: {})",
        frequency_hz, initial_count);

    configure_timer(
        crate::idt::vector::TIMER,
        TimerMode::Periodic,
        TimerDivide::Div16,
        initial_count,
    );
}
