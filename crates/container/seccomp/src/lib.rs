//! Seccomp (Secure Computing) Syscall Filtering
//!
//! Implements BPF-based syscall filtering for sandboxing.

#![no_std]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

/// Seccomp error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeccompError {
    /// Invalid filter
    InvalidFilter,
    /// Filter too large
    FilterTooLarge,
    /// Already in strict mode
    AlreadyStrict,
    /// Permission denied
    PermissionDenied,
}

/// Seccomp result type
pub type SeccompResult<T> = Result<T, SeccompError>;

/// Seccomp operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SeccompOp {
    /// Set strict mode (only read, write, exit, sigreturn)
    SetModeStrict = 0,
    /// Set filter mode
    SetModeFilter = 1,
    /// Get notification FD (supervisor mode)
    GetNotifFd = 2,
}

/// Seccomp action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SeccompAction {
    /// Kill the process/thread
    Kill = 0x00000000,
    /// Send SIGSYS to the thread
    Trap = 0x00030000,
    /// Return errno
    Errno = 0x00050000,
    /// Notify userspace tracer
    Trace = 0x7ff00000,
    /// Log and allow
    Log = 0x7ffc0000,
    /// Allow the syscall
    Allow = 0x7fff0000,
}

impl SeccompAction {
    /// Create errno action with error number
    pub fn errno(err: u16) -> u32 {
        0x00050000 | (err as u32)
    }

    /// Create trace action with data
    pub fn trace(data: u16) -> u32 {
        0x7ff00000 | (data as u32)
    }
}

/// BPF instruction
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BpfInsn {
    /// Opcode
    pub code: u16,
    /// Jump true
    pub jt: u8,
    /// Jump false
    pub jf: u8,
    /// Constant/offset
    pub k: u32,
}

/// BPF instruction classes
pub mod bpf {
    // Instruction classes
    pub const BPF_LD: u16 = 0x00;
    pub const BPF_LDX: u16 = 0x01;
    pub const BPF_ST: u16 = 0x02;
    pub const BPF_STX: u16 = 0x03;
    pub const BPF_ALU: u16 = 0x04;
    pub const BPF_JMP: u16 = 0x05;
    pub const BPF_RET: u16 = 0x06;
    pub const BPF_MISC: u16 = 0x07;

    // LD/LDX fields
    pub const BPF_W: u16 = 0x00;
    pub const BPF_H: u16 = 0x08;
    pub const BPF_B: u16 = 0x10;
    pub const BPF_IMM: u16 = 0x00;
    pub const BPF_ABS: u16 = 0x20;
    pub const BPF_IND: u16 = 0x40;
    pub const BPF_MEM: u16 = 0x60;
    pub const BPF_LEN: u16 = 0x80;
    pub const BPF_MSH: u16 = 0xa0;

    // ALU/JMP fields
    pub const BPF_ADD: u16 = 0x00;
    pub const BPF_SUB: u16 = 0x10;
    pub const BPF_MUL: u16 = 0x20;
    pub const BPF_DIV: u16 = 0x30;
    pub const BPF_OR: u16 = 0x40;
    pub const BPF_AND: u16 = 0x50;
    pub const BPF_LSH: u16 = 0x60;
    pub const BPF_RSH: u16 = 0x70;
    pub const BPF_NEG: u16 = 0x80;
    pub const BPF_JA: u16 = 0x00;
    pub const BPF_JEQ: u16 = 0x10;
    pub const BPF_JGT: u16 = 0x20;
    pub const BPF_JGE: u16 = 0x30;
    pub const BPF_JSET: u16 = 0x40;

    pub const BPF_K: u16 = 0x00;
    pub const BPF_X: u16 = 0x08;
}

/// Seccomp data passed to BPF
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SeccompData {
    /// Syscall number
    pub nr: i32,
    /// Architecture (AUDIT_ARCH_*)
    pub arch: u32,
    /// Instruction pointer
    pub instruction_pointer: u64,
    /// Syscall arguments
    pub args: [u64; 6],
}

/// Architecture audit constants
pub mod arch {
    pub const AUDIT_ARCH_X86_64: u32 = 0xc000003e;
    pub const AUDIT_ARCH_I386: u32 = 0x40000003;
    pub const AUDIT_ARCH_AARCH64: u32 = 0xc00000b7;
    pub const AUDIT_ARCH_ARM: u32 = 0x40000028;
    pub const AUDIT_ARCH_RISCV64: u32 = 0xc00000f3;
    pub const AUDIT_ARCH_RISCV32: u32 = 0x400000f3;
}

/// Seccomp filter
#[derive(Clone)]
pub struct SeccompFilter {
    /// BPF program
    program: Vec<BpfInsn>,
}

impl SeccompFilter {
    /// Create new filter from BPF program
    pub fn new(program: Vec<BpfInsn>) -> SeccompResult<Self> {
        if program.is_empty() {
            return Err(SeccompError::InvalidFilter);
        }

        // Validate program (simplified)
        // Real implementation would check:
        // - Program terminates (no infinite loops)
        // - All jumps are within bounds
        // - Last instruction is RET

        Ok(SeccompFilter { program })
    }

    /// Create a simple filter that allows all syscalls
    pub fn allow_all() -> Self {
        SeccompFilter {
            program: alloc::vec![
                BpfInsn {
                    code: bpf::BPF_RET | bpf::BPF_K,
                    jt: 0,
                    jf: 0,
                    k: SeccompAction::Allow as u32,
                },
            ],
        }
    }

    /// Create a simple filter that blocks a specific syscall
    pub fn block_syscall(syscall_nr: u32, errno: u16) -> Self {
        SeccompFilter {
            program: alloc::vec![
                // Load syscall number
                BpfInsn {
                    code: bpf::BPF_LD | bpf::BPF_W | bpf::BPF_ABS,
                    jt: 0,
                    jf: 0,
                    k: 0, // offset of nr in seccomp_data
                },
                // Compare with target syscall
                BpfInsn {
                    code: bpf::BPF_JMP | bpf::BPF_JEQ | bpf::BPF_K,
                    jt: 0, // if equal, go to next (return errno)
                    jf: 1, // if not equal, skip to allow
                    k: syscall_nr,
                },
                // Return errno
                BpfInsn {
                    code: bpf::BPF_RET | bpf::BPF_K,
                    jt: 0,
                    jf: 0,
                    k: SeccompAction::errno(errno),
                },
                // Allow
                BpfInsn {
                    code: bpf::BPF_RET | bpf::BPF_K,
                    jt: 0,
                    jf: 0,
                    k: SeccompAction::Allow as u32,
                },
            ],
        }
    }

    /// Evaluate filter against syscall data
    pub fn evaluate(&self, data: &SeccompData) -> u32 {
        let data_bytes = unsafe {
            core::slice::from_raw_parts(
                data as *const SeccompData as *const u8,
                core::mem::size_of::<SeccompData>(),
            )
        };

        let mut a: u32 = 0; // Accumulator
        let mut x: u32 = 0; // Index register
        let mut mem = [0u32; 16]; // Memory
        let mut pc = 0usize;

        while pc < self.program.len() {
            let insn = &self.program[pc];
            let class = insn.code & 0x07;

            match class {
                0x00 => {
                    // BPF_LD
                    let size = insn.code & 0x18;
                    let mode = insn.code & 0xe0;

                    a = match mode {
                        0x00 => insn.k, // BPF_IMM
                        0x20 => {
                            // BPF_ABS
                            let offset = insn.k as usize;
                            match size {
                                0x00 => {
                                    // BPF_W (4 bytes)
                                    if offset + 4 <= data_bytes.len() {
                                        u32::from_ne_bytes([
                                            data_bytes[offset],
                                            data_bytes[offset + 1],
                                            data_bytes[offset + 2],
                                            data_bytes[offset + 3],
                                        ])
                                    } else {
                                        0
                                    }
                                }
                                0x08 => {
                                    // BPF_H (2 bytes)
                                    if offset + 2 <= data_bytes.len() {
                                        u16::from_ne_bytes([
                                            data_bytes[offset],
                                            data_bytes[offset + 1],
                                        ]) as u32
                                    } else {
                                        0
                                    }
                                }
                                0x10 => {
                                    // BPF_B (1 byte)
                                    if offset < data_bytes.len() {
                                        data_bytes[offset] as u32
                                    } else {
                                        0
                                    }
                                }
                                _ => 0,
                            }
                        }
                        0x60 => {
                            // BPF_MEM
                            mem[insn.k as usize & 0xf]
                        }
                        _ => 0,
                    };
                }
                0x01 => {
                    // BPF_LDX
                    x = insn.k;
                }
                0x02 => {
                    // BPF_ST
                    mem[insn.k as usize & 0xf] = a;
                }
                0x03 => {
                    // BPF_STX
                    mem[insn.k as usize & 0xf] = x;
                }
                0x04 => {
                    // BPF_ALU
                    let op = insn.code & 0xf0;
                    let src = if insn.code & 0x08 != 0 { x } else { insn.k };

                    a = match op {
                        0x00 => a.wrapping_add(src),
                        0x10 => a.wrapping_sub(src),
                        0x20 => a.wrapping_mul(src),
                        0x30 => {
                            if src != 0 {
                                a / src
                            } else {
                                0
                            }
                        }
                        0x40 => a | src,
                        0x50 => a & src,
                        0x60 => a << src,
                        0x70 => a >> src,
                        0x80 => (!a).wrapping_add(1),
                        _ => a,
                    };
                }
                0x05 => {
                    // BPF_JMP
                    let op = insn.code & 0xf0;
                    let src = if insn.code & 0x08 != 0 { x } else { insn.k };

                    let cond = match op {
                        0x00 => true, // BPF_JA
                        0x10 => a == src,
                        0x20 => a > src,
                        0x30 => a >= src,
                        0x40 => a & src != 0,
                        _ => false,
                    };

                    if op == 0x00 {
                        pc = pc.wrapping_add(insn.k as usize);
                    } else if cond {
                        pc = pc.wrapping_add(insn.jt as usize);
                    } else {
                        pc = pc.wrapping_add(insn.jf as usize);
                    }
                }
                0x06 => {
                    // BPF_RET
                    return if insn.code & 0x08 != 0 { a } else { insn.k };
                }
                0x07 => {
                    // BPF_MISC
                    if insn.code & 0x08 != 0 {
                        a = x;
                    } else {
                        x = a;
                    }
                }
                _ => {}
            }

            pc += 1;
        }

        SeccompAction::Kill as u32
    }
}

/// Seccomp state for a process
pub struct SeccompState {
    /// Strict mode active
    strict_mode: bool,
    /// Active filters (newest first)
    filters: Vec<Arc<SeccompFilter>>,
}

impl SeccompState {
    /// Create new seccomp state
    pub fn new() -> Self {
        SeccompState {
            strict_mode: false,
            filters: Vec::new(),
        }
    }

    /// Set strict mode
    pub fn set_strict(&mut self) -> SeccompResult<()> {
        if self.strict_mode {
            return Err(SeccompError::AlreadyStrict);
        }
        self.strict_mode = true;
        Ok(())
    }

    /// Add a filter
    pub fn add_filter(&mut self, filter: Arc<SeccompFilter>) -> SeccompResult<()> {
        if self.strict_mode {
            return Err(SeccompError::AlreadyStrict);
        }
        self.filters.push(filter);
        Ok(())
    }

    /// Check syscall
    pub fn check_syscall(&self, data: &SeccompData) -> u32 {
        if self.strict_mode {
            // Strict mode only allows: read, write, _exit, sigreturn
            match data.nr {
                0 | 1 | 60 | 15 => return SeccompAction::Allow as u32,
                _ => return SeccompAction::Kill as u32,
            }
        }

        // Evaluate all filters (newest to oldest)
        for filter in self.filters.iter().rev() {
            let result = filter.evaluate(data);

            // If not ALLOW, return immediately
            if result != SeccompAction::Allow as u32 {
                return result;
            }
        }

        SeccompAction::Allow as u32
    }

    /// Check if filtering is active
    pub fn is_active(&self) -> bool {
        self.strict_mode || !self.filters.is_empty()
    }

    /// Clone state for fork
    pub fn fork(&self) -> Self {
        SeccompState {
            strict_mode: self.strict_mode,
            filters: self.filters.clone(),
        }
    }
}

impl Default for SeccompState {
    fn default() -> Self {
        Self::new()
    }
}

/// Global default seccomp policy
static DEFAULT_POLICY: RwLock<Option<Arc<SeccompFilter>>> = RwLock::new(None);

/// Set default seccomp policy
pub fn set_default_policy(filter: Arc<SeccompFilter>) {
    *DEFAULT_POLICY.write() = Some(filter);
}

/// Get default seccomp policy
pub fn default_policy() -> Option<Arc<SeccompFilter>> {
    DEFAULT_POLICY.read().clone()
}
