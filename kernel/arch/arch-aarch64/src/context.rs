//! ARM64 context structures
//! — ThreadRogue

/// ARM64 context for thread switching
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    // Callee-saved registers (x19-x30)
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64, // Frame pointer
    pub x30: u64, // Link register
    pub sp: u64,  // Stack pointer
    pub pc: u64,  // Program counter
}

impl Default for Context {
    fn default() -> Self {
        Self {
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
            x30: 0,
            sp: 0,
            pc: 0,
        }
    }
}
