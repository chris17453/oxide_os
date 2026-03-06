//! Inter-Processor Interrupts (IPI)
//!
//! — NeonRoot: generic IPI interface. The actual hardware dispatch (APIC ICR
//! on x86, GIC SGI on ARM) lives in the arch crate's SmpOps impl. This module
//! provides the arch-independent API that the rest of the kernel calls.

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
/// — NeonRoot: dispatches to arch SmpOps. No APIC, no GIC — just the trait.
pub fn send_ipi(target: IpiTarget, vector: u8) {
    match target {
        IpiTarget::Cpu(cpu_id) => {
            send_ipi_to_cpu(cpu_id, vector);
        }
        IpiTarget::AllExceptSelf => {
            send_ipi_broadcast_impl(vector, false);
        }
        IpiTarget::All => {
            send_ipi_broadcast_impl(vector, true);
        }
        IpiTarget::Self_ => {
            send_ipi_self_impl(vector);
        }
    }
}

/// Send IPI to a specific CPU — looks up hardware ID, delegates to arch
fn send_ipi_to_cpu(cpu_id: CpuId, vector: u8) {
    let hw_id = match crate::cpu::get_apic_id(cpu_id) {
        Some(id) => id,
        None => return,
    };

    // — NeonRoot: arch-agnostic IPI dispatch. No APIC, no GIC — os_core handles it.
    os_core::smp_send_ipi_to(hw_id, vector);
}

/// Send broadcast IPI — delegates to arch
fn send_ipi_broadcast_impl(vector: u8, include_self: bool) {
    // — NeonRoot: broadcast through os_core — the arch crate figures out shorthand vs mask
    os_core::smp_send_ipi_broadcast(vector, include_self);
}

/// Send self IPI — delegates to arch
fn send_ipi_self_impl(vector: u8) {
    // — NeonRoot: self-IPI, useful for deferred work. os_core routes it.
    os_core::smp_send_ipi_self(vector);
}

/// Send a reschedule IPI to a CPU
pub fn send_reschedule(cpu_id: CpuId) {
    send_ipi(IpiTarget::Cpu(cpu_id), vector::RESCHEDULE);
}

/// Send a reschedule IPI to all other CPUs
pub fn send_reschedule_all() {
    send_ipi(IpiTarget::AllExceptSelf, vector::RESCHEDULE);
}
