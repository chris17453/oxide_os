//! Inter-Processor Interrupts (IPI)
//!
//! Used for communication between CPUs.

use crate::CpuId;

/// IPI target specification
#[derive(Debug, Clone, Copy)]
pub enum IpiTarget {
    /// Send to specific CPU
    Cpu(CpuId),
    /// Send to all CPUs except self
    AllExceptSelf,
    /// Send to all CPUs including self
    All,
    /// Send to self only
    Self_,
}

/// IPI vector numbers
pub mod vector {
    /// Reschedule IPI - triggers scheduler on target CPU
    pub const RESCHEDULE: u8 = 0xF0;
    /// TLB shootdown IPI - invalidate TLB entries
    pub const TLB_SHOOTDOWN: u8 = 0xF1;
    /// Function call IPI - execute function on target CPU
    pub const CALL_FUNCTION: u8 = 0xF2;
    /// Stop IPI - halt the target CPU
    pub const STOP: u8 = 0xF3;
}

/// IPI handler function type
pub type IpiHandler = fn(u8);

/// Registered IPI handlers
static mut IPI_HANDLERS: [Option<IpiHandler>; 256] = [None; 256];

/// Register an IPI handler
///
/// # Safety
/// Must be called before any IPIs are sent.
pub unsafe fn register_handler(vector: u8, handler: IpiHandler) {
    IPI_HANDLERS[vector as usize] = Some(handler);
}

/// Handle an IPI
///
/// Called from the IPI interrupt handler.
pub fn handle_ipi(vector: u8) {
    unsafe {
        if let Some(handler) = IPI_HANDLERS[vector as usize] {
            handler(vector);
        }
    }
}

/// Send an IPI
///
/// The actual sending is architecture-specific and implemented
/// in the architecture crate. This is a placeholder interface.
pub fn send_ipi(target: IpiTarget, vector: u8) {
    // Architecture-specific implementation would go here
    // For x86_64, this involves writing to the APIC ICR registers

    match target {
        IpiTarget::Cpu(cpu_id) => {
            // Send to specific CPU via APIC ID
            send_ipi_to_cpu(cpu_id, vector);
        }
        IpiTarget::AllExceptSelf => {
            // Shorthand: all excluding self
            send_ipi_broadcast(vector, false);
        }
        IpiTarget::All => {
            // Shorthand: all including self
            send_ipi_broadcast(vector, true);
        }
        IpiTarget::Self_ => {
            // Self IPI
            send_ipi_self(vector);
        }
    }
}

/// Send IPI to a specific CPU
fn send_ipi_to_cpu(cpu_id: CpuId, vector: u8) {
    // Get the APIC ID for the target CPU
    let apic_id = match crate::cpu::get_apic_id(cpu_id) {
        Some(id) => id,
        None => return, // CPU not present
    };

    // Send IPI to specific APIC ID
    arch_x86_64::apic::send_ipi(
        apic_id as u8,
        vector,
        arch_x86_64::apic::DeliveryMode::Fixed,
        arch_x86_64::apic::DestShorthand::None,
    );
}

/// Send broadcast IPI
fn send_ipi_broadcast(vector: u8, include_self: bool) {
    let shorthand = if include_self {
        arch_x86_64::apic::DestShorthand::All
    } else {
        arch_x86_64::apic::DestShorthand::AllExceptSelf
    };

    arch_x86_64::apic::send_ipi(
        0, // Ignored for broadcast
        vector,
        arch_x86_64::apic::DeliveryMode::Fixed,
        shorthand,
    );
}

/// Send self IPI
fn send_ipi_self(vector: u8) {
    arch_x86_64::apic::send_ipi(
        0, // Ignored for self
        vector,
        arch_x86_64::apic::DeliveryMode::Fixed,
        arch_x86_64::apic::DestShorthand::Self_,
    );
}

/// Send a reschedule IPI to a CPU
pub fn send_reschedule(cpu_id: CpuId) {
    send_ipi(IpiTarget::Cpu(cpu_id), vector::RESCHEDULE);
}

/// Send a reschedule IPI to all other CPUs
pub fn send_reschedule_all() {
    send_ipi(IpiTarget::AllExceptSelf, vector::RESCHEDULE);
}
