//! Local APIC driver for x86_64
//!
//! The Local APIC provides per-CPU interrupt handling and timer functionality.

use core::ptr::{read_volatile, write_volatile};

use crate::exceptions::write_u32_via_oslog;
use boot_proto::PHYS_MAP_BASE;

/// Physical address of the Local APIC (default)
const APIC_BASE_PHYS: u64 = 0xFEE0_0000;

/// APIC register offsets
mod reg {
    pub const ID: u32 = 0x020;
    pub const VERSION: u32 = 0x030;
    pub const TPR: u32 = 0x080; // Task Priority Register
    pub const EOI: u32 = 0x0B0; // End of Interrupt
    pub const SPURIOUS: u32 = 0x0F0; // Spurious Interrupt Vector
    pub const ICR_LOW: u32 = 0x300; // Interrupt Command Register (low)
    pub const ICR_HIGH: u32 = 0x310; // Interrupt Command Register (high)
    pub const LVT_TIMER: u32 = 0x320; // LVT Timer Register
    pub const LVT_LINT0: u32 = 0x350; // LVT LINT0
    pub const LVT_LINT1: u32 = 0x360; // LVT LINT1
    pub const LVT_ERROR: u32 = 0x370; // LVT Error
    pub const TIMER_INIT: u32 = 0x380; // Timer Initial Count
    pub const TIMER_CURR: u32 = 0x390; // Timer Current Count
    pub const TIMER_DIV: u32 = 0x3E0; // Timer Divide Configuration
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

/// IPI Delivery Mode
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum DeliveryMode {
    Fixed = 0b000 << 8,
    LowestPriority = 0b001 << 8,
    Smi = 0b010 << 8,
    Nmi = 0b100 << 8,
    Init = 0b101 << 8,
    Startup = 0b110 << 8,
}

/// IPI Destination Shorthand
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum DestShorthand {
    None = 0b00 << 18,          // Use destination field
    Self_ = 0b01 << 18,         // Send to self
    All = 0b10 << 18,           // Send to all CPUs
    AllExceptSelf = 0b11 << 18, // Send to all except self
}

/// Virtual address of APIC (through direct physical map)
fn apic_base() -> u64 {
    PHYS_MAP_BASE + APIC_BASE_PHYS
}

/// Read an APIC register
fn read(offset: u32) -> u32 {
    unsafe { read_volatile((apic_base() + offset as u64) as *const u32) }
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

    // BlackLatch: Mask LINT0 and LINT1 to prevent firmware-configured
    // pass-through interrupts. LINT0 is typically wired as ExtINT (PIC INTR)
    // by UEFI firmware; a stale PIC IRQ sitting in the PIC's IRR can fire
    // through LINT0 the instant `sti` executes, even after OCW1 masking.
    // Mask bit = bit 16 of LVT entry.
    write(reg::LVT_LINT0, 1 << 16); // Mask LINT0
    write(reg::LVT_LINT1, 1 << 16); // Mask LINT1
    write(reg::LVT_ERROR, 1 << 16); // Mask error interrupt too

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

/// Send an Inter-Processor Interrupt
///
/// - `dest_apic`: Destination APIC ID (ignored if shorthand is not None)
/// - `vector`: Interrupt vector (0-255)
/// - `mode`: Delivery mode (INIT, STARTUP, etc.)
/// - `shorthand`: Destination shorthand (None, All, AllExceptSelf, etc.)
///
/// For INIT-SIPI-SIPI sequence:
/// 1. send_ipi(apic_id, 0, DeliveryMode::Init, DestShorthand::None) - INIT
/// 2. Wait 10ms
/// 3. send_ipi(apic_id, start_page, DeliveryMode::Startup, DestShorthand::None) - SIPI
/// 4. Wait 200us
/// 5. send_ipi(apic_id, start_page, DeliveryMode::Startup, DestShorthand::None) - SIPI
pub fn send_ipi(dest_apic: u8, vector: u8, mode: DeliveryMode, shorthand: DestShorthand) {
    // Wait for any pending IPI to complete
    while (read(reg::ICR_LOW) & (1 << 12)) != 0 {
        core::hint::spin_loop();
    }

    // Write destination APIC ID to ICR high (bits 56-63 = bits 24-31 of high word)
    write(reg::ICR_HIGH, (dest_apic as u32) << 24);

    // Build ICR low value
    let icr_low: u32 = (vector as u32)        // Vector (bits 0-7)
        | (mode as u32)                       // Delivery mode (bits 8-10)
        | (0 << 11)                           // Destination mode: physical (bit 11)
        | (1 << 14)                           // Level: assert (bit 14)
        | (0 << 15)                           // Trigger mode: edge (bit 15)
        | (shorthand as u32); // Destination shorthand (bits 18-19)

    // Write ICR low to send the IPI
    write(reg::ICR_LOW, icr_low);

    // Wait for IPI to be accepted
    while (read(reg::ICR_LOW) & (1 << 12)) != 0 {
        core::hint::spin_loop();
    }
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

/// Cached calibration result — BSP calibrates via PIT, APs reuse the value.
/// The PIT is shared hardware (ports 0x42/0x43/0x61); concurrent access from
/// multiple APs corrupts the calibration and produces garbage timer counts.
/// — SableWire
static CACHED_TICKS_PER_MS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Calibrate the APIC timer using PIT
///
/// Returns the number of APIC timer ticks per millisecond.
/// First caller does the real PIT calibration; subsequent callers reuse the
/// cached result to avoid the PIT data race.
///
/// — TorqueJax: Linux-style calibration with explicit state reset. The PIT OUT
/// signal may be HIGH from BIOS/previous use; we must force it LOW before
/// measuring, else the wait loop exits immediately and we get garbage.
pub fn calibrate_timer() -> u32 {
    // SableWire: Fast path — return cached result if BSP already calibrated
    let cached = CACHED_TICKS_PER_MS.load(core::sync::atomic::Ordering::Acquire);
    if cached != 0 {
        return cached;
    }

    crate::serial_println!("[APIC-CAL] Starting calibration...");

    const PIT_FREQUENCY: u32 = 1193182;
    const CALIBRATION_MS: u32 = 10;

    // Set up PIT channel 2 for calibration
    let pit_count = (PIT_FREQUENCY / 1000) * CALIBRATION_MS;

    unsafe {
        // TorqueJax: Step 1 — Force gate LOW to reset PIT channel 2 state.
        let port61_initial = crate::inb(0x61);
        crate::serial_println!("[APIC-CAL] port61 initial: {:#x}", port61_initial);

        crate::outb(0x61, (port61_initial & 0xFC) | 0x00); // Gate LOW, speaker off

        // Small delay to let hardware settle
        for _ in 0..100 {
            core::hint::spin_loop();
        }

        let port61_after_low = crate::inb(0x61);
        crate::serial_println!("[APIC-CAL] port61 after gate LOW: {:#x}", port61_after_low);

        // Step 2 — Program PIT channel 2 in mode 0
        crate::outb(0x43, 0xB0);
        crate::outb(0x42, (pit_count & 0xFF) as u8);
        crate::outb(0x42, ((pit_count >> 8) & 0xFF) as u8);
    }

    // Set APIC timer divider and configure for one-shot, masked
    write(reg::TIMER_DIV, TimerDivide::Div16 as u32);
    write(reg::LVT_TIMER, 1 << 16); // Masked, one-shot

    // TorqueJax: Write initial count LAST — this starts the timer counting
    write(reg::TIMER_INIT, 0xFFFF_FFFF);

    let apic_start = timer_current();
    crate::serial_println!("[APIC-CAL] APIC timer started at: {:#x}", apic_start);

    unsafe {
        // Step 4 — Set gate HIGH to start PIT countdown
        let port61 = crate::inb(0x61);
        crate::outb(0x61, (port61 & 0xFC) | 0x01); // Gate HIGH, speaker off

        let port61_after_high = crate::inb(0x61);
        crate::serial_println!(
            "[APIC-CAL] port61 after gate HIGH: {:#x}",
            port61_after_high
        );

        // Step 5 — Verify OUT went LOW (sanity check)
        let mut sanity = 0u32;
        while (crate::inb(0x61) & 0x20) != 0 && sanity < 1000 {
            sanity += 1;
            core::hint::spin_loop();
        }
        crate::serial_println!("[APIC-CAL] OUT sanity check took {} iterations", sanity);

        // Step 6 — Wait for PIT OUT to go HIGH (count reached zero)
        let mut wait_count = 0u32;
        while (crate::inb(0x61) & 0x20) == 0 {
            wait_count += 1;
        }
        crate::serial_println!("[APIC-CAL] PIT wait took {} iterations", wait_count);
    }

    // Read APIC timer current count
    let apic_end = timer_current();
    let elapsed = 0xFFFF_FFFF - apic_end;

    crate::serial_println!("[APIC-CAL] APIC end: {:#x}, elapsed: {}", apic_end, elapsed);

    // Stop APIC timer
    stop_timer();

    // Calculate ticks per millisecond
    let ticks_per_ms = elapsed / CALIBRATION_MS;

    crate::serial_println!("[APIC-CAL] Result: {} ticks/ms", ticks_per_ms);

    // TorqueJax: Sanity check — typical values are 10k-500k ticks/ms
    if ticks_per_ms < 1000 || ticks_per_ms > 10_000_000 {
        let fallback = 62500; // ~100MHz bus / 16 div at 100Hz = 62500 ticks/ms
        CACHED_TICKS_PER_MS.store(fallback, core::sync::atomic::Ordering::Release);
        crate::serial_println!(
            "[APIC-CAL] FAILED! {} invalid, using fallback {}",
            ticks_per_ms,
            fallback
        );
        return fallback;
    }

    // SableWire: Cache for APs
    CACHED_TICKS_PER_MS.store(ticks_per_ms, core::sync::atomic::Ordering::Release);

    ticks_per_ms
}

/// Initialize the APIC
pub fn init() {
    if !is_available() {
        panic!("APIC not available!");
    }

    // BlackLatch: Kill the 8259 PIC before it kills us. Legacy PIC maps IRQ 0-7
    // to vectors 0x08-0x0F which OVERLAP with CPU exception vectors (double fault
    // = 0x08). A stray PIC timer tick at `sti` fires vector 8, the double-fault
    // handler reads a non-existent error code, stack goes sideways, triple fault.
    // Remap to 0x20-0x2F first (safety net for spurious IRQs), then mask everything.
    disable_legacy_pic();

    enable();

    crate::serial_println!("[APIC] Initialized, ID: {}, Version: {}", id(), version());

    // SableWire: Calibrate TSC frequency using PIT (BSP only, APs reuse cached value)
    // Must happen after PIC disabled to avoid interference
    let tsc_freq = crate::calibrate_tsc();
    crate::serial_println!(
        "[APIC] TSC calibrated: {} Hz (~{} MHz)",
        tsc_freq,
        tsc_freq / 1_000_000
    );

    // Initialize IOAPIC for legacy IRQs
    init_ioapic();
}

/// Disable the legacy 8259 PIC
///
/// — BlackLatch: The 8259 PIC must die before the APIC takes over. Without this,
/// PIC IRQs fire into exception vector space (IRQ0 → vector 0x08 = #DF) and
/// corrupt the handler stack frame. Remap first so even spurious IRQs land on
/// harmless vectors, then mask everything.
fn disable_legacy_pic() {
    unsafe {
        // ICW1: Begin initialization sequence (ICW4 needed)
        crate::outb(0x20, 0x11); // PIC1 command
        crate::outb(0xA0, 0x11); // PIC2 command

        // ICW2: Remap to non-conflicting vectors
        crate::outb(0x21, 0x20); // PIC1 → vectors 0x20-0x27
        crate::outb(0xA1, 0x28); // PIC2 → vectors 0x28-0x2F

        // ICW3: Cascade wiring
        crate::outb(0x21, 0x04); // PIC1: slave on IRQ2
        crate::outb(0xA1, 0x02); // PIC2: cascade identity = 2

        // ICW4: 8086 mode
        crate::outb(0x21, 0x01);
        crate::outb(0xA1, 0x01);

        // OCW1: Mask ALL interrupts on both PICs — flatline
        crate::outb(0x21, 0xFF);
        crate::outb(0xA1, 0xFF);
    }

    crate::serial_println!("[PIC] Legacy 8259 disabled — remapped to 0x20-0x2F and masked");
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
    unsafe { read_volatile((ioapic_base() + IOAPIC_DATA as u64) as *const u32) }
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
///
/// — BlackLatch: All IOAPIC entries start MASKED. The BSP unmasks keyboard/mouse
/// only after `sti` via `unmask_io_irqs()`. This prevents stale hardware IRQs
/// from firing into the LAPIC before the BSP is ready to handle them.
fn init_ioapic() {
    // Check IOAPIC version
    let version = ioapic_read_reg(ioapic_reg::VERSION);
    let max_redir = ((version >> 16) & 0xFF) as u8;
    crate::serial_println!(
        "[IOAPIC] Version: {:#x}, Max redirections: {}",
        version & 0xFF,
        max_redir + 1
    );

    // Mask all IRQs — stay dark until BSP is fully ready
    for irq in 0..=max_redir {
        ioapic_set_irq(irq, 0, 0, true);
    }

    // Get local APIC ID
    let apic_id = id();

    // Route keyboard IRQ 1 to vector 33 — but keep MASKED
    ioapic_set_irq(1, crate::idt::vector::KEYBOARD, apic_id, true);
    crate::serial_println!(
        "[IOAPIC] Keyboard IRQ 1 -> vector {} (APIC {}) [masked]",
        crate::idt::vector::KEYBOARD,
        apic_id
    );

    // Route mouse IRQ 12 to vector 44 — but keep MASKED
    ioapic_set_irq(12, crate::idt::vector::MOUSE, apic_id, true);
    crate::serial_println!(
        "[IOAPIC] Mouse IRQ 12 -> vector {} (APIC {}) [masked]",
        crate::idt::vector::MOUSE,
        apic_id
    );
}

/// Unmask keyboard and mouse IRQs in the IOAPIC
///
/// — BlackLatch: Call this AFTER `sti` and after all interrupt handlers are live.
/// The PS2 controller may have pending data from init; this lets those IRQs flow.
pub fn unmask_io_irqs() {
    let apic_id = id();
    ioapic_set_irq(1, crate::idt::vector::KEYBOARD, apic_id, false);
    ioapic_set_irq(12, crate::idt::vector::MOUSE, apic_id, false);
    crate::serial_println!("[IOAPIC] Keyboard and mouse IRQs unmasked");
}

/// Start the APIC timer for scheduling
///
/// - `frequency_hz`: Desired interrupt frequency in Hz
pub fn start_timer(frequency_hz: u32) {
    let mut ticks_per_ms = calibrate_timer();
    if ticks_per_ms == 0 {
        // SableWire: Fallback — never leave the timer dead
        ticks_per_ms = 1_000;
        unsafe {
            os_log::write_str_raw("[APIC] Timer cal=0, fallback\n");
        }
    }

    // Calculate initial count for desired frequency
    let interval_ms = 1000 / frequency_hz;
    let initial_count = ticks_per_ms * interval_ms;

    // — GraveShift: Only BSP prints calibration info. APs use cached value and
    // printing from all CPUs simultaneously garbles output and wastes cycles.
    let is_bsp = id() == 0;
    if is_bsp {
        unsafe {
            os_log::write_str_raw("[APIC] Timer calibrated: ");
            write_u32_via_oslog(ticks_per_ms);
            os_log::write_str_raw(" ticks/ms\n");
            os_log::write_str_raw("[APIC] Starting timer at ");
            write_u32_via_oslog(frequency_hz);
            os_log::write_str_raw("Hz (count: ");
            write_u32_via_oslog(initial_count);
            os_log::write_str_raw(")\n");
        }
    }

    configure_timer(
        crate::idt::vector::TIMER,
        TimerMode::Periodic,
        TimerDivide::Div16,
        initial_count,
    );
}
