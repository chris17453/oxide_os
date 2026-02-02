//! Architecture-agnostic context types
//!
//! Common types for context switching, interrupt handling, and exception frames
//! — ThreadRogue

use crate::{ContextSwitch, InterruptContext};
use os_core::{PhysAddr, VirtAddr};

/// Generic architecture context wrapper
///
/// Provides a type-safe wrapper around architecture-specific context types
pub struct ArchContext<T: ContextSwitch> {
    inner: T::Context,
}

impl<T: ContextSwitch> ArchContext<T> {
    /// Create a new context for kernel thread
    pub fn new_kernel(entry: fn(usize) -> !, stack_top: usize, arg: usize) -> Self {
        Self {
            inner: T::new_context(entry, stack_top, arg),
        }
    }

    /// Get mutable reference to inner context
    pub fn inner_mut(&mut self) -> &mut T::Context {
        &mut self.inner
    }

    /// Get immutable reference to inner context
    pub fn inner(&self) -> &T::Context {
        &self.inner
    }

    /// Switch to this context from old context
    ///
    /// # Safety
    /// Both contexts must be valid
    pub unsafe fn switch_to(&self, old: &mut Self) {
        // SAFETY: Caller ensures both contexts are valid; delegates to trait impl
        // — ThreadRogue
        unsafe {
            T::switch(&mut old.inner as *mut _, &self.inner as *const _);
        }
    }
}

impl<T: ContextSwitch> Clone for ArchContext<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: ContextSwitch> Default for ArchContext<T> {
    fn default() -> Self {
        Self {
            inner: T::Context::default(),
        }
    }
}

/// Saved register state for process/thread
///
/// Architecture-independent view of CPU state
/// — ThreadRogue
#[derive(Debug, Clone)]
pub struct SavedRegisters {
    /// Program counter / instruction pointer
    pub pc: u64,
    /// Stack pointer
    pub sp: u64,
    /// Frame pointer / base pointer
    pub fp: u64,
    /// Link register (ARM) / return address
    pub link: u64,
    /// General purpose registers (architecture-dependent count)
    pub gprs: [u64; 31],
}

impl SavedRegisters {
    /// Create empty register state
    pub fn new() -> Self {
        Self {
            pc: 0,
            sp: 0,
            fp: 0,
            link: 0,
            gprs: [0; 31],
        }
    }

    /// Set program counter
    pub fn set_pc(&mut self, pc: u64) {
        self.pc = pc;
    }

    /// Set stack pointer
    pub fn set_sp(&mut self, sp: u64) {
        self.sp = sp;
    }

    /// Set first argument register (arg0)
    pub fn set_arg0(&mut self, val: u64) {
        self.gprs[0] = val;
    }
}

impl Default for SavedRegisters {
    fn default() -> Self {
        Self::new()
    }
}

/// Exception/fault information
///
/// Common exception types across architectures
/// — BlackLatch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionType {
    /// Division by zero or overflow
    DivideError,
    /// Debug trap/breakpoint
    Debug,
    /// Non-maskable interrupt
    Nmi,
    /// Breakpoint (int3 / brk)
    Breakpoint,
    /// Arithmetic overflow
    Overflow,
    /// Bounds check failed
    BoundRange,
    /// Invalid opcode / undefined instruction
    InvalidOpcode,
    /// FPU/coprocessor not available
    DeviceNotAvailable,
    /// Double fault (x86) / nested exception
    DoubleFault,
    /// Invalid TSS (x86) / invalid state
    InvalidTss,
    /// Segment not present (x86)
    SegmentNotPresent,
    /// Stack segment fault
    StackSegmentFault,
    /// General protection fault
    GeneralProtection,
    /// Page fault / TLB miss
    PageFault,
    /// x87 FPU error
    X87FpuException,
    /// Alignment check failed
    AlignmentCheck,
    /// Machine check exception
    MachineCheck,
    /// SIMD floating point exception
    SimdException,
    /// Virtualization exception
    VirtualizationException,
    /// Security exception (CET, etc.)
    SecurityException,
    /// Unknown/other exception
    Unknown(u64),
}

/// Page fault error details
///
/// Unified across architectures
/// — BlackLatch
#[derive(Debug, Clone, Copy)]
pub struct PageFaultError {
    /// Fault occurred on instruction fetch
    pub instruction_fetch: bool,
    /// Fault occurred on write
    pub write: bool,
    /// Fault occurred in user mode
    pub user_mode: bool,
    /// Page was present but access denied
    pub protection_violation: bool,
    /// Reserved bits were set
    pub reserved_bits: bool,
}

/// Exception information
///
/// Architecture-agnostic exception context
/// — BlackLatch
#[derive(Debug, Clone)]
pub struct ExceptionInfo {
    /// Type of exception
    pub exception_type: ExceptionType,
    /// Error code (if applicable)
    pub error_code: Option<u64>,
    /// Fault address (for page faults)
    pub fault_address: Option<VirtAddr>,
    /// Saved CPU context
    pub context: InterruptContext,
    /// Was exception in user mode?
    pub user_mode: bool,
}

impl ExceptionInfo {
    /// Create page fault exception info
    pub fn page_fault(
        fault_addr: VirtAddr,
        error: PageFaultError,
        context: InterruptContext,
    ) -> Self {
        Self {
            exception_type: ExceptionType::PageFault,
            error_code: None,
            fault_address: Some(fault_addr),
            context,
            user_mode: error.user_mode,
        }
    }

    /// Create general exception info
    pub fn new(
        exception_type: ExceptionType,
        error_code: Option<u64>,
        context: InterruptContext,
    ) -> Self {
        Self {
            exception_type,
            error_code,
            fault_address: None,
            context,
            user_mode: false,
        }
    }
}

/// Usermode entry parameters
///
/// Information needed to jump from kernel to userspace
/// — ThreadRogue
#[derive(Debug, Clone, Copy)]
pub struct UsermodeEntry {
    /// Entry point (instruction pointer)
    pub entry: VirtAddr,
    /// User stack pointer
    pub stack: VirtAddr,
    /// Argument to pass (in appropriate register)
    pub arg: u64,
    /// User address space root (page table)
    pub page_table: PhysAddr,
}

impl UsermodeEntry {
    /// Create new usermode entry params
    pub fn new(entry: VirtAddr, stack: VirtAddr, page_table: PhysAddr) -> Self {
        Self {
            entry,
            stack,
            arg: 0,
            page_table,
        }
    }

    /// Set argument to pass to user code
    pub fn with_arg(mut self, arg: u64) -> Self {
        self.arg = arg;
        self
    }
}

/// System call information
///
/// Architecture-agnostic syscall context
/// — ThreadRogue
#[derive(Debug, Clone, Copy)]
pub struct SyscallInfo {
    /// Syscall number
    pub number: usize,
    /// Arguments (up to 6)
    pub args: [usize; 6],
    /// Saved instruction pointer (for return)
    pub return_addr: u64,
    /// Saved stack pointer
    pub stack_ptr: u64,
}

impl SyscallInfo {
    /// Create new syscall info
    pub fn new(number: usize, args: [usize; 6]) -> Self {
        Self {
            number,
            args,
            return_addr: 0,
            stack_ptr: 0,
        }
    }

    /// Get syscall argument by index (0-5)
    pub fn arg(&self, index: usize) -> usize {
        if index < 6 {
            self.args[index]
        } else {
            0
        }
    }
}
