//! MIPS64 syscall mechanism
//! — ThreadRogue

/// MIPS64 syscall frame
///
/// State captured when a syscall occurs.
/// MIPS64 uses SYSCALL instruction which triggers exception code 8.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SyscallFrame {
    /// Zero register (always 0)
    pub zero: u64, // $0

    /// Assembler temporary (caller-saved)
    pub at: u64,   // $1

    /// Return values
    pub v0: u64,   // $2 - Also syscall number
    pub v1: u64,   // $3

    /// Syscall arguments
    pub a0: u64,   // $4
    pub a1: u64,   // $5
    pub a2: u64,   // $6
    pub a3: u64,   // $7
    pub a4: u64,   // $8 (on MIPS64, extended args)
    pub a5: u64,   // $9
    pub a6: u64,   // $10
    pub a7: u64,   // $11

    /// Temporary registers (caller-saved)
    pub t0: u64,   // $12
    pub t1: u64,   // $13
    pub t2: u64,   // $14
    pub t3: u64,   // $15

    /// Saved registers (callee-saved)
    pub s0: u64,   // $16
    pub s1: u64,   // $17
    pub s2: u64,   // $18
    pub s3: u64,   // $19
    pub s4: u64,   // $20
    pub s5: u64,   // $21
    pub s6: u64,   // $22
    pub s7: u64,   // $23

    /// More temporaries (caller-saved)
    pub t8: u64,   // $24
    pub t9: u64,   // $25

    /// Kernel temporaries (not used in userspace)
    pub k0: u64,   // $26
    pub k1: u64,   // $27

    /// Global pointer
    pub gp: u64,   // $28

    /// Stack pointer
    pub sp: u64,   // $29

    /// Frame pointer
    pub fp: u64,   // $30

    /// Return address
    pub ra: u64,   // $31

    /// Exception Program Counter (where to return)
    pub epc: u64,

    /// Status register
    pub status: u64,
}

impl Default for SyscallFrame {
    fn default() -> Self {
        Self {
            zero: 0,
            at: 0,
            v0: 0,
            v1: 0,
            a0: 0,
            a1: 0,
            a2: 0,
            a3: 0,
            a4: 0,
            a5: 0,
            a6: 0,
            a7: 0,
            t0: 0,
            t1: 0,
            t2: 0,
            t3: 0,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            t8: 0,
            t9: 0,
            k0: 0,
            k1: 0,
            gp: 0,
            sp: 0,
            fp: 0,
            ra: 0,
            epc: 0,
            status: 0,
        }
    }
}
