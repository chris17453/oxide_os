//! MIPS64 exception handling
//! — BlackLatch

/// MIPS64 exception frame
///
/// State saved when an exception occurs on MIPS64.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ExceptionFrame {
    /// General purpose registers (32 registers on MIPS64)
    pub regs: [u64; 32],
    /// Stack pointer (register $29)
    pub sp: u64,
    /// Exception Program Counter (CP0 EPC)
    pub epc: u64,
    /// Status register (CP0 Status)
    pub status: u64,
    /// Cause register (CP0 Cause) - exception type
    pub cause: u64,
    /// Bad virtual address (CP0 BadVAddr) - for address exceptions
    pub badvaddr: u64,
}

impl Default for ExceptionFrame {
    fn default() -> Self {
        Self {
            regs: [0; 32],
            sp: 0,
            epc: 0,
            status: 0,
            cause: 0,
            badvaddr: 0,
        }
    }
}

/// MIPS64 exception codes (CP0 Cause register, bits 6:2)
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionCode {
    Interrupt = 0,
    TlbModification = 1,
    TlbLoadMiss = 2,
    TlbStoreMiss = 3,
    AddressErrorLoad = 4,
    AddressErrorStore = 5,
    BusErrorInst = 6,
    BusErrorData = 7,
    Syscall = 8,
    Breakpoint = 9,
    ReservedInstruction = 10,
    CoprocessorUnusable = 11,
    ArithmeticOverflow = 12,
    Trap = 13,
    FloatingPoint = 15,
    Watch = 23,
}
