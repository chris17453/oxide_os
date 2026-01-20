//! Virtual 8086 Mode DOS Emulation for OXIDE OS
//!
//! Provides DOS program execution on x86 platforms using V86 mode.

#![no_std]

extern crate alloc;

pub mod memory;
pub mod monitor;
pub mod int;
pub mod dos;

pub use memory::*;
pub use monitor::*;
pub use int::*;
pub use dos::*;

use alloc::string::String;
use bitflags::bitflags;

/// V86 general purpose registers
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct V86Registers {
    /// EAX register
    pub eax: u32,
    /// EBX register
    pub ebx: u32,
    /// ECX register
    pub ecx: u32,
    /// EDX register
    pub edx: u32,
    /// ESI register
    pub esi: u32,
    /// EDI register
    pub edi: u32,
    /// EBP register
    pub ebp: u32,
    /// ESP register
    pub esp: u32,
    /// EIP register
    pub eip: u32,
    /// EFLAGS register
    pub eflags: u32,
}

impl V86Registers {
    /// Create new registers
    pub fn new() -> Self {
        V86Registers {
            eflags: 0x20202, // VM=1, IF=1, reserved bit
            ..Default::default()
        }
    }

    /// Get 16-bit AX
    pub fn ax(&self) -> u16 {
        self.eax as u16
    }

    /// Set 16-bit AX
    pub fn set_ax(&mut self, val: u16) {
        self.eax = (self.eax & 0xFFFF0000) | val as u32;
    }

    /// Get 8-bit AL
    pub fn al(&self) -> u8 {
        self.eax as u8
    }

    /// Set 8-bit AL
    pub fn set_al(&mut self, val: u8) {
        self.eax = (self.eax & 0xFFFFFF00) | val as u32;
    }

    /// Get 8-bit AH
    pub fn ah(&self) -> u8 {
        (self.eax >> 8) as u8
    }

    /// Set 8-bit AH
    pub fn set_ah(&mut self, val: u8) {
        self.eax = (self.eax & 0xFFFF00FF) | ((val as u32) << 8);
    }

    /// Get 16-bit BX
    pub fn bx(&self) -> u16 {
        self.ebx as u16
    }

    /// Get 16-bit CX
    pub fn cx(&self) -> u16 {
        self.ecx as u16
    }

    /// Get 16-bit DX
    pub fn dx(&self) -> u16 {
        self.edx as u16
    }

    /// Set 16-bit DX
    pub fn set_dx(&mut self, val: u16) {
        self.edx = (self.edx & 0xFFFF0000) | val as u32;
    }

    /// Get 8-bit DL
    pub fn dl(&self) -> u8 {
        self.edx as u8
    }

    /// Set 8-bit DL
    pub fn set_dl(&mut self, val: u8) {
        self.edx = (self.edx & 0xFFFFFF00) | val as u32;
    }

    /// Get 8-bit DH
    pub fn dh(&self) -> u8 {
        (self.edx >> 8) as u8
    }

    /// Set 8-bit DH
    pub fn set_dh(&mut self, val: u8) {
        self.edx = (self.edx & 0xFFFF00FF) | ((val as u32) << 8);
    }

    /// Get IP (instruction pointer)
    pub fn ip(&self) -> u16 {
        self.eip as u16
    }

    /// Set IP
    pub fn set_ip(&mut self, val: u16) {
        self.eip = val as u32;
    }

    /// Get SP (stack pointer)
    pub fn sp(&self) -> u16 {
        self.esp as u16
    }

    /// Set SP
    pub fn set_sp(&mut self, val: u16) {
        self.esp = val as u32;
    }

    /// Get SI (source index)
    pub fn si(&self) -> u16 {
        self.esi as u16
    }

    /// Set SI
    pub fn set_si(&mut self, val: u16) {
        self.esi = (self.esi & 0xFFFF0000) | val as u32;
    }

    /// Get DI (destination index)
    pub fn di(&self) -> u16 {
        self.edi as u16
    }

    /// Set DI
    pub fn set_di(&mut self, val: u16) {
        self.edi = (self.edi & 0xFFFF0000) | val as u32;
    }

    /// Get BP (base pointer)
    pub fn bp(&self) -> u16 {
        self.ebp as u16
    }

    /// Set BP
    pub fn set_bp(&mut self, val: u16) {
        self.ebp = (self.ebp & 0xFFFF0000) | val as u32;
    }

    /// Get flags
    pub fn flags(&self) -> u16 {
        self.eflags as u16
    }

    /// Set carry flag
    pub fn set_carry(&mut self, val: bool) {
        if val {
            self.eflags |= 1;
        } else {
            self.eflags &= !1;
        }
    }

    /// Get carry flag
    pub fn carry(&self) -> bool {
        self.eflags & 1 != 0
    }

    /// Set zero flag
    pub fn set_zero(&mut self, val: bool) {
        if val {
            self.eflags |= 0x40;
        } else {
            self.eflags &= !0x40;
        }
    }

    /// Get zero flag
    pub fn zero(&self) -> bool {
        self.eflags & 0x40 != 0
    }
}

/// V86 segment registers
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct V86Segments {
    /// CS segment
    pub cs: u16,
    /// DS segment
    pub ds: u16,
    /// ES segment
    pub es: u16,
    /// SS segment
    pub ss: u16,
    /// FS segment
    pub fs: u16,
    /// GS segment
    pub gs: u16,
}

impl V86Segments {
    /// Create new segments
    pub fn new() -> Self {
        V86Segments::default()
    }

    /// Set all segments to same value
    pub fn set_all(&mut self, val: u16) {
        self.cs = val;
        self.ds = val;
        self.es = val;
        self.ss = val;
        self.fs = val;
        self.gs = val;
    }
}

/// V86 context
#[derive(Clone)]
pub struct V86Context {
    /// General registers
    pub regs: V86Registers,
    /// Segment registers
    pub segments: V86Segments,
    /// V86 memory
    pub memory: V86Memory,
    /// I/O port bitmap (8KB)
    pub io_bitmap: [u8; 8192],
    /// Interrupt redirection bitmap (32 bytes for 256 interrupts)
    pub int_redirect: [u8; 32],
    /// PSP (Program Segment Prefix) address
    pub psp_segment: u16,
    /// Environment segment
    pub env_segment: u16,
    /// Program name
    pub program_name: String,
    /// Exit code
    pub exit_code: Option<u8>,
}

impl V86Context {
    /// Create new context
    pub fn new() -> Self {
        V86Context {
            regs: V86Registers::new(),
            segments: V86Segments::new(),
            memory: V86Memory::new(),
            io_bitmap: [0; 8192],
            int_redirect: [0; 32],
            psp_segment: 0,
            env_segment: 0,
            program_name: String::new(),
            exit_code: None,
        }
    }

    /// Allow I/O port access
    pub fn allow_io_port(&mut self, port: u16) {
        let byte = (port / 8) as usize;
        let bit = port % 8;
        if byte < self.io_bitmap.len() {
            self.io_bitmap[byte] &= !(1 << bit);
        }
    }

    /// Deny I/O port access
    pub fn deny_io_port(&mut self, port: u16) {
        let byte = (port / 8) as usize;
        let bit = port % 8;
        if byte < self.io_bitmap.len() {
            self.io_bitmap[byte] |= 1 << bit;
        }
    }

    /// Check if I/O port is allowed
    pub fn is_io_allowed(&self, port: u16) -> bool {
        let byte = (port / 8) as usize;
        let bit = port % 8;
        if byte < self.io_bitmap.len() {
            self.io_bitmap[byte] & (1 << bit) == 0
        } else {
            false
        }
    }

    /// Set interrupt redirection
    pub fn redirect_int(&mut self, int_num: u8) {
        let byte = (int_num / 8) as usize;
        let bit = int_num % 8;
        if byte < self.int_redirect.len() {
            self.int_redirect[byte] |= 1 << bit;
        }
    }

    /// Check if interrupt should be redirected to V86 handler
    pub fn is_int_redirected(&self, int_num: u8) -> bool {
        let byte = (int_num / 8) as usize;
        let bit = int_num % 8;
        if byte < self.int_redirect.len() {
            self.int_redirect[byte] & (1 << bit) != 0
        } else {
            false
        }
    }

    /// Linear address from segment:offset
    pub fn linear_addr(&self, segment: u16, offset: u16) -> u32 {
        ((segment as u32) << 4) + offset as u32
    }

    /// Current instruction linear address
    pub fn current_ip(&self) -> u32 {
        self.linear_addr(self.segments.cs, self.regs.ip())
    }

    /// Check if program has exited
    pub fn has_exited(&self) -> bool {
        self.exit_code.is_some()
    }
}

impl Default for V86Context {
    fn default() -> Self {
        Self::new()
    }
}

/// V86 action to take after handling GPF
#[derive(Debug, Clone)]
pub enum V86Action {
    /// Continue V86 execution
    Continue,
    /// Emulate the instruction
    Emulate(EmulatedOp),
    /// Exit V86 mode with code
    Exit(i32),
    /// Reflect interrupt to V86 handler
    ReflectInt(u8),
}

/// Emulated operation
#[derive(Debug, Clone)]
pub enum EmulatedOp {
    /// CLI instruction
    Cli,
    /// STI instruction
    Sti,
    /// PUSHF instruction
    Pushf,
    /// POPF instruction
    Popf,
    /// INT instruction
    Int(u8),
    /// IRET instruction
    Iret,
    /// IN instruction
    In { port: u16, size: u8 },
    /// OUT instruction
    Out { port: u16, size: u8, value: u32 },
    /// HLT instruction
    Hlt,
}

bitflags! {
    /// V86 EFLAGS bits
    #[derive(Debug, Clone, Copy)]
    pub struct V86Flags: u32 {
        /// Carry flag
        const CF = 0x0001;
        /// Parity flag
        const PF = 0x0004;
        /// Auxiliary carry
        const AF = 0x0010;
        /// Zero flag
        const ZF = 0x0040;
        /// Sign flag
        const SF = 0x0080;
        /// Trap flag
        const TF = 0x0100;
        /// Interrupt enable
        const IF = 0x0200;
        /// Direction flag
        const DF = 0x0400;
        /// Overflow flag
        const OF = 0x0800;
        /// I/O privilege level (bits 12-13)
        const IOPL = 0x3000;
        /// Nested task
        const NT = 0x4000;
        /// Resume flag
        const RF = 0x10000;
        /// Virtual 8086 mode
        const VM = 0x20000;
    }
}

/// V86 error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V86Error {
    /// Invalid opcode
    InvalidOpcode,
    /// Invalid memory access
    InvalidMemory,
    /// I/O port not allowed
    IoNotAllowed,
    /// Stack overflow
    StackOverflow,
    /// Division by zero
    DivideByZero,
    /// Invalid interrupt
    InvalidInterrupt,
    /// Memory allocation failed
    OutOfMemory,
    /// Invalid COM/EXE file
    InvalidExecutable,
}

impl core::fmt::Display for V86Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidOpcode => write!(f, "invalid opcode"),
            Self::InvalidMemory => write!(f, "invalid memory access"),
            Self::IoNotAllowed => write!(f, "I/O port not allowed"),
            Self::StackOverflow => write!(f, "stack overflow"),
            Self::DivideByZero => write!(f, "division by zero"),
            Self::InvalidInterrupt => write!(f, "invalid interrupt"),
            Self::OutOfMemory => write!(f, "out of memory"),
            Self::InvalidExecutable => write!(f, "invalid executable"),
        }
    }
}
