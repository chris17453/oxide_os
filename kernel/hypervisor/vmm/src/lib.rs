//! Virtual Machine Monitor
//!
//! Core VMM functionality for managing virtual machines.

#![no_std]

extern crate alloc;

pub mod device;
pub mod exit;
pub mod memory;
pub mod vcpu;
pub mod vm;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};

pub use device::{DeviceType, VirtioDevice};
pub use exit::{
    CpuidInfo, CrAccessInfo, EptViolationInfo, ExceptionInfo, ExitData, ExitReason, HypercallInfo,
    IoInfo, MsrAccessInfo, VmExit,
};
pub use memory::{GpaRange, GuestMemory, GuestMemoryRegion};
pub use vcpu::{Vcpu, VcpuId, VcpuRegs};
pub use vm::{VirtualMachine, VmId, VmState};

/// VMM error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmmError {
    /// Hardware virtualization not supported
    NotSupported,
    /// VMX/SVM operation failed
    VmxError,
    /// Invalid VM ID
    InvalidVm,
    /// Invalid VCPU ID
    InvalidVcpu,
    /// Invalid memory region
    InvalidMemory,
    /// Resource exhausted
    ResourceExhausted,
    /// VM not in correct state
    InvalidState,
    /// Device error
    DeviceError,
    /// VMCS field error
    VmcsError,
}

/// VMM result type
pub type VmmResult<T> = Result<T, VmmError>;

/// VMM capability flags
#[derive(Debug, Clone, Copy)]
pub struct VmmCapabilities {
    /// Hardware virtualization available
    pub hw_virt: bool,
    /// Extended Page Tables (EPT/NPT)
    pub ept: bool,
    /// Virtual Processor ID (VPID/ASID)
    pub vpid: bool,
    /// Unrestricted guest mode
    pub unrestricted_guest: bool,
    /// Posted interrupts
    pub posted_interrupts: bool,
    /// Maximum VCPUs per VM
    pub max_vcpus: u32,
}

impl Default for VmmCapabilities {
    fn default() -> Self {
        VmmCapabilities {
            hw_virt: false,
            ept: false,
            vpid: false,
            unrestricted_guest: false,
            posted_interrupts: false,
            max_vcpus: 0,
        }
    }
}

/// Architecture-specific VMM backend
pub trait VmmBackend: Send + Sync {
    /// Check if hardware virtualization is supported
    fn is_supported(&self) -> bool;

    /// Get VMM capabilities
    fn capabilities(&self) -> VmmCapabilities;

    /// Enable virtualization on current CPU
    fn enable(&self) -> VmmResult<()>;

    /// Disable virtualization on current CPU
    fn disable(&self) -> VmmResult<()>;

    /// Create VCPU state structure
    fn create_vcpu_state(
        &self,
        vm: &VirtualMachine,
        vcpu_id: VcpuId,
    ) -> VmmResult<Box<dyn VcpuState>>;
}

/// Architecture-specific VCPU state
pub trait VcpuState: Send + Sync {
    /// Initialize VCPU state
    fn init(&mut self) -> VmmResult<()>;

    /// Load state onto current CPU
    fn load(&self) -> VmmResult<()>;

    /// Store state from current CPU
    fn store(&mut self) -> VmmResult<()>;

    /// Set guest registers
    fn set_regs(&mut self, regs: &VcpuRegs) -> VmmResult<()>;

    /// Get guest registers
    fn get_regs(&self) -> VmmResult<VcpuRegs>;

    /// Run guest (returns on VM exit)
    fn run(&mut self) -> VmmResult<VmExit>;

    /// Inject interrupt
    fn inject_interrupt(&mut self, vector: u8) -> VmmResult<()>;
}

/// Global VMM instance
static VMM: RwLock<Option<Vmm>> = RwLock::new(None);

/// VMM manager
pub struct Vmm {
    /// Backend implementation
    backend: Arc<dyn VmmBackend>,
    /// Virtual machines
    vms: Mutex<Vec<Arc<VirtualMachine>>>,
    /// Next VM ID
    next_vm_id: AtomicU64,
    /// Capabilities
    capabilities: VmmCapabilities,
}

impl Vmm {
    /// Create new VMM with backend
    pub fn new(backend: Arc<dyn VmmBackend>) -> VmmResult<Self> {
        if !backend.is_supported() {
            return Err(VmmError::NotSupported);
        }

        let caps = backend.capabilities();

        Ok(Vmm {
            backend,
            vms: Mutex::new(Vec::new()),
            next_vm_id: AtomicU64::new(1),
            capabilities: caps,
        })
    }

    /// Get capabilities
    pub fn capabilities(&self) -> VmmCapabilities {
        self.capabilities
    }

    /// Enable VMM on current CPU
    pub fn enable(&self) -> VmmResult<()> {
        self.backend.enable()
    }

    /// Disable VMM on current CPU
    pub fn disable(&self) -> VmmResult<()> {
        self.backend.disable()
    }

    /// Create new virtual machine
    pub fn create_vm(&self) -> VmmResult<Arc<VirtualMachine>> {
        let id = VmId(self.next_vm_id.fetch_add(1, Ordering::SeqCst));
        let vm = Arc::new(VirtualMachine::new(id, self.backend.clone())?);
        self.vms.lock().push(vm.clone());
        Ok(vm)
    }

    /// Get VM by ID
    pub fn get_vm(&self, id: VmId) -> Option<Arc<VirtualMachine>> {
        self.vms.lock().iter().find(|vm| vm.id() == id).cloned()
    }

    /// Destroy VM
    pub fn destroy_vm(&self, id: VmId) -> VmmResult<()> {
        let mut vms = self.vms.lock();
        if let Some(pos) = vms.iter().position(|vm| vm.id() == id) {
            let vm = vms.remove(pos);
            vm.shutdown()?;
            Ok(())
        } else {
            Err(VmmError::InvalidVm)
        }
    }
}

/// Initialize VMM subsystem with backend
pub fn init(backend: Arc<dyn VmmBackend>) -> VmmResult<()> {
    let vmm = Vmm::new(backend)?;
    *VMM.write() = Some(vmm);
    Ok(())
}

/// Get VMM instance
pub fn vmm() -> Option<&'static Vmm> {
    // Safety: We only set this once during init
    unsafe { VMM.read().as_ref().map(|v| &*(v as *const Vmm)) }
}

/// Create a new VM
pub fn create_vm() -> VmmResult<Arc<VirtualMachine>> {
    vmm().ok_or(VmmError::NotSupported)?.create_vm()
}

/// Destroy a VM
pub fn destroy_vm(id: VmId) -> VmmResult<()> {
    vmm().ok_or(VmmError::NotSupported)?.destroy_vm(id)
}
