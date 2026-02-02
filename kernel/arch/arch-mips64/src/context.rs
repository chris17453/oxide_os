//! MIPS64 context structures
//! — ThreadRogue

/// MIPS64 context for thread switching
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    // Callee-saved registers (s0-s7)
    pub s0: u64,  // $16
    pub s1: u64,  // $17
    pub s2: u64,  // $18
    pub s3: u64,  // $19
    pub s4: u64,  // $20
    pub s5: u64,  // $21
    pub s6: u64,  // $22
    pub s7: u64,  // $23
    pub gp: u64,  // $28 - Global pointer
    pub sp: u64,  // $29 - Stack pointer
    pub fp: u64,  // $30 - Frame pointer
    pub ra: u64,  // $31 - Return address
}

impl Default for Context {
    fn default() -> Self {
        Self {
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            gp: 0,
            sp: 0,
            fp: 0,
            ra: 0,
        }
    }
}
