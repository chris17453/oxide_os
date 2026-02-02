//! ARM64 exception handling
//! — BlackLatch

/// ARM64 exception frame
///
/// This is the state saved when an exception occurs on ARM64.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ExceptionFrame {
    /// General purpose registers x0-x30
    pub x: [u64; 31],
    /// Stack pointer
    pub sp: u64,
    /// Program counter (ELR_EL1)
    pub elr: u64,
    /// Saved processor state (SPSR_EL1)
    pub spsr: u64,
}

impl Default for ExceptionFrame {
    fn default() -> Self {
        Self {
            x: [0; 31],
            sp: 0,
            elr: 0,
            spsr: 0,
        }
    }
}
