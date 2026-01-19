//! Signal delivery mechanism
//!
//! Handles delivering signals to processes, including setting up
//! signal frames for user-space signal handlers.

use crate::action::{SigAction, SigFlags, SigHandler, SigInfo};
use crate::pending::PendingSignal;
use crate::sigset::SigSet;
use crate::signal::{default_action, DefaultAction, SIGKILL, SIGSTOP, NSIG};

/// Signal frame structure pushed onto user stack
///
/// This is used to save the process context when delivering a signal
/// to a user-space handler, so it can be restored on sigreturn.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SignalFrame {
    /// Return address (points to sigreturn trampoline)
    pub retaddr: u64,
    /// Signal number
    pub signo: i32,
    /// Padding
    _pad0: i32,
    /// Signal info (if SA_SIGINFO)
    pub info: SigInfo,
    /// Saved signal mask
    pub saved_mask: SigSet,
    /// Saved instruction pointer
    pub saved_rip: u64,
    /// Saved stack pointer
    pub saved_rsp: u64,
    /// Saved flags
    pub saved_rflags: u64,
    /// Saved general registers
    pub saved_rax: u64,
    pub saved_rbx: u64,
    pub saved_rcx: u64,
    pub saved_rdx: u64,
    pub saved_rsi: u64,
    pub saved_rdi: u64,
    pub saved_rbp: u64,
    pub saved_r8: u64,
    pub saved_r9: u64,
    pub saved_r10: u64,
    pub saved_r11: u64,
    pub saved_r12: u64,
    pub saved_r13: u64,
    pub saved_r14: u64,
    pub saved_r15: u64,
}

impl Default for SignalFrame {
    fn default() -> Self {
        SignalFrame {
            retaddr: 0,
            signo: 0,
            _pad0: 0,
            info: SigInfo::default(),
            saved_mask: SigSet::empty(),
            saved_rip: 0,
            saved_rsp: 0,
            saved_rflags: 0,
            saved_rax: 0,
            saved_rbx: 0,
            saved_rcx: 0,
            saved_rdx: 0,
            saved_rsi: 0,
            saved_rdi: 0,
            saved_rbp: 0,
            saved_r8: 0,
            saved_r9: 0,
            saved_r10: 0,
            saved_r11: 0,
            saved_r12: 0,
            saved_r13: 0,
            saved_r14: 0,
            saved_r15: 0,
        }
    }
}

/// Result of signal delivery decision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalResult {
    /// No signal to deliver
    None,
    /// Execute default action (terminate)
    Terminate,
    /// Execute default action (terminate with core dump)
    CoreDump,
    /// Stop the process
    Stop,
    /// Continue the process
    Continue,
    /// Ignore the signal
    Ignore,
    /// Call user handler
    UserHandler {
        /// Handler address
        handler: u64,
        /// Signal number
        signo: i32,
        /// Signal info
        info: Option<SigInfo>,
        /// Flags
        flags: SigFlags,
        /// Mask to apply during handler
        handler_mask: SigSet,
    },
}

/// Determine what action to take for a pending signal
pub fn determine_action(
    signal: &PendingSignal,
    action: &SigAction,
    current_mask: &SigSet,
) -> SignalResult {
    let sig = signal.signo;

    // SIGKILL and SIGSTOP always use default action
    if sig == SIGKILL {
        return SignalResult::Terminate;
    }
    if sig == SIGSTOP {
        return SignalResult::Stop;
    }

    match action.handler() {
        SigHandler::Ignore => SignalResult::Ignore,
        SigHandler::Default => {
            match default_action(sig) {
                DefaultAction::Terminate => SignalResult::Terminate,
                DefaultAction::CoreDump => SignalResult::CoreDump,
                DefaultAction::Ignore => SignalResult::Ignore,
                DefaultAction::Stop => SignalResult::Stop,
                DefaultAction::Continue => SignalResult::Continue,
            }
        }
        SigHandler::Handler(addr) => {
            // Calculate mask to apply during handler execution
            let flags = action.flags();
            let mut handler_mask = current_mask.union(&action.sa_mask);

            // Unless SA_NODEFER, block the signal during its handler
            if !flags.contains(SigFlags::SA_NODEFER) {
                handler_mask.add(sig);
            }

            SignalResult::UserHandler {
                handler: addr,
                signo: sig,
                info: signal.info,
                flags,
                handler_mask,
            }
        }
    }
}

/// Setup parameters for signal handler invocation
///
/// Returns (new_rip, new_rsp, frame) where frame should be written to new_rsp
pub fn setup_signal_handler(
    handler: u64,
    signo: i32,
    info: Option<SigInfo>,
    _flags: SigFlags,
    restorer: u64,
    saved_mask: SigSet,
    current_rip: u64,
    current_rsp: u64,
    current_rflags: u64,
    regs: &SavedRegisters,
) -> (u64, u64, SignalFrame) {
    // Create signal frame
    let frame = SignalFrame {
        retaddr: restorer, // Return to sigreturn trampoline
        signo,
        _pad0: 0,
        info: info.unwrap_or_default(),
        saved_mask,
        saved_rip: current_rip,
        saved_rsp: current_rsp,
        saved_rflags: current_rflags,
        saved_rax: regs.rax,
        saved_rbx: regs.rbx,
        saved_rcx: regs.rcx,
        saved_rdx: regs.rdx,
        saved_rsi: regs.rsi,
        saved_rdi: regs.rdi,
        saved_rbp: regs.rbp,
        saved_r8: regs.r8,
        saved_r9: regs.r9,
        saved_r10: regs.r10,
        saved_r11: regs.r11,
        saved_r12: regs.r12,
        saved_r13: regs.r13,
        saved_r14: regs.r14,
        saved_r15: regs.r15,
    };

    // Calculate new stack pointer (frame goes below current RSP)
    // Align to 16 bytes as required by x86_64 ABI
    let frame_size = core::mem::size_of::<SignalFrame>() as u64;
    let new_rsp = (current_rsp - frame_size) & !0xF;

    // New RIP is the handler address
    let new_rip = handler;

    (new_rip, new_rsp, frame)
}

/// Saved registers for signal frame
#[derive(Debug, Clone, Copy, Default)]
pub struct SavedRegisters {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
}

/// Restore context from signal frame
///
/// Returns the saved state that should be restored
pub fn restore_from_frame(frame: &SignalFrame) -> (u64, u64, u64, SigSet, SavedRegisters) {
    let regs = SavedRegisters {
        rax: frame.saved_rax,
        rbx: frame.saved_rbx,
        rcx: frame.saved_rcx,
        rdx: frame.saved_rdx,
        rsi: frame.saved_rsi,
        rdi: frame.saved_rdi,
        rbp: frame.saved_rbp,
        r8: frame.saved_r8,
        r9: frame.saved_r9,
        r10: frame.saved_r10,
        r11: frame.saved_r11,
        r12: frame.saved_r12,
        r13: frame.saved_r13,
        r14: frame.saved_r14,
        r15: frame.saved_r15,
    };

    (
        frame.saved_rip,
        frame.saved_rsp,
        frame.saved_rflags,
        frame.saved_mask,
        regs,
    )
}

/// Check if a process should be interrupted due to signals
///
/// Called when a process is blocked (e.g., in a sleep or wait syscall)
pub fn should_interrupt_for_signal(pending: &SigSet, blocked: &SigSet, sigactions: &[SigAction; NSIG]) -> bool {
    // Check each pending signal that isn't blocked
    let deliverable = pending.difference(blocked);

    for sig in deliverable.iter() {
        if sig >= 1 && sig <= NSIG as i32 {
            let action = &sigactions[(sig - 1) as usize];

            // SIGKILL and SIGSTOP always interrupt
            if sig == SIGKILL || sig == SIGSTOP {
                return true;
            }

            // Ignored signals don't interrupt
            if matches!(action.handler(), SigHandler::Ignore) {
                continue;
            }

            // Default action that ignores doesn't interrupt
            if matches!(action.handler(), SigHandler::Default) {
                if matches!(default_action(sig), DefaultAction::Ignore) {
                    continue;
                }
            }

            // All other signals interrupt
            return true;
        }
    }

    false
}
