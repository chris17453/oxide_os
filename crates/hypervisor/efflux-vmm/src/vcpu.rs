//! Virtual CPU Management

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::{VmmResult, VmmError, VcpuState, VmExit, VirtualMachine};

/// VCPU identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VcpuId(pub u32);

/// VCPU registers
#[derive(Debug, Clone, Default)]
pub struct VcpuRegs {
    // General purpose registers
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,

    // Instruction pointer and flags
    pub rip: u64,
    pub rflags: u64,

    // Segment registers
    pub cs: SegmentReg,
    pub ds: SegmentReg,
    pub es: SegmentReg,
    pub fs: SegmentReg,
    pub gs: SegmentReg,
    pub ss: SegmentReg,
    pub tr: SegmentReg,
    pub ldtr: SegmentReg,

    // Control registers
    pub cr0: u64,
    pub cr2: u64,
    pub cr3: u64,
    pub cr4: u64,
    pub cr8: u64,
    pub efer: u64,

    // Descriptor tables
    pub gdtr: DescriptorTable,
    pub idtr: DescriptorTable,
}

/// Segment register
#[derive(Debug, Clone, Default)]
pub struct SegmentReg {
    pub selector: u16,
    pub base: u64,
    pub limit: u32,
    pub access: u32,
}

impl SegmentReg {
    /// Create real mode segment
    pub fn real_mode(selector: u16) -> Self {
        SegmentReg {
            selector,
            base: (selector as u64) << 4,
            limit: 0xFFFF,
            access: 0x93, // Present, RW, accessed
        }
    }

    /// Create flat protected mode code segment
    pub fn flat_code() -> Self {
        SegmentReg {
            selector: 0x08,
            base: 0,
            limit: 0xFFFF_FFFF,
            access: 0xA09B, // Present, code, readable, long mode
        }
    }

    /// Create flat protected mode data segment
    pub fn flat_data() -> Self {
        SegmentReg {
            selector: 0x10,
            base: 0,
            limit: 0xFFFF_FFFF,
            access: 0xC093, // Present, data, writable, 32-bit
        }
    }
}

/// Descriptor table register
#[derive(Debug, Clone, Default)]
pub struct DescriptorTable {
    pub base: u64,
    pub limit: u16,
}

/// VCPU run state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuRunState {
    /// Ready to run
    Ready,
    /// Currently running
    Running,
    /// Stopped
    Stopped,
    /// Waiting for interrupt
    Halted,
}

/// Virtual CPU
pub struct Vcpu {
    /// VCPU ID
    id: VcpuId,
    /// Parent VM
    vm: Arc<VirtualMachine>,
    /// Architecture-specific state
    state: Mutex<Box<dyn VcpuState>>,
    /// Run state
    run_state: Mutex<VcpuRunState>,
    /// Should exit
    should_exit: AtomicBool,
}

impl Vcpu {
    /// Create new VCPU
    pub fn new(
        id: VcpuId,
        vm: Arc<VirtualMachine>,
        mut state: Box<dyn VcpuState>,
    ) -> VmmResult<Self> {
        state.init()?;

        Ok(Vcpu {
            id,
            vm,
            state: Mutex::new(state),
            run_state: Mutex::new(VcpuRunState::Ready),
            should_exit: AtomicBool::new(false),
        })
    }

    /// Get VCPU ID
    pub fn id(&self) -> VcpuId {
        self.id
    }

    /// Get parent VM
    pub fn vm(&self) -> &Arc<VirtualMachine> {
        &self.vm
    }

    /// Get run state
    pub fn run_state(&self) -> VcpuRunState {
        *self.run_state.lock()
    }

    /// Set guest registers
    pub fn set_regs(&self, regs: &VcpuRegs) -> VmmResult<()> {
        self.state.lock().set_regs(regs)
    }

    /// Get guest registers
    pub fn get_regs(&self) -> VmmResult<VcpuRegs> {
        self.state.lock().get_regs()
    }

    /// Run VCPU until VM exit
    pub fn run(&self) -> VmmResult<VmExit> {
        // Check state
        {
            let mut run_state = self.run_state.lock();
            match *run_state {
                VcpuRunState::Stopped => return Err(VmmError::InvalidState),
                VcpuRunState::Running => return Err(VmmError::InvalidState),
                _ => {}
            }
            *run_state = VcpuRunState::Running;
        }

        // Load state and run
        let result = {
            let mut state = self.state.lock();
            state.load()?;
            let exit = state.run()?;
            state.store()?;
            exit
        };

        // Update run state based on exit
        {
            let mut run_state = self.run_state.lock();
            *run_state = match result.reason {
                crate::ExitReason::Hlt => VcpuRunState::Halted,
                crate::ExitReason::Shutdown => VcpuRunState::Stopped,
                _ => VcpuRunState::Ready,
            };
        }

        Ok(result)
    }

    /// Signal VCPU to exit
    pub fn request_exit(&self) {
        self.should_exit.store(true, Ordering::SeqCst);
    }

    /// Check if exit requested
    pub fn should_exit(&self) -> bool {
        self.should_exit.load(Ordering::SeqCst)
    }

    /// Stop VCPU
    pub fn stop(&self) -> VmmResult<()> {
        self.request_exit();
        *self.run_state.lock() = VcpuRunState::Stopped;
        Ok(())
    }

    /// Inject interrupt
    pub fn inject_interrupt(&self, vector: u8) -> VmmResult<()> {
        self.state.lock().inject_interrupt(vector)
    }

    /// Wake halted VCPU
    pub fn wake(&self) -> VmmResult<()> {
        let mut run_state = self.run_state.lock();
        if *run_state == VcpuRunState::Halted {
            *run_state = VcpuRunState::Ready;
        }
        Ok(())
    }
}
