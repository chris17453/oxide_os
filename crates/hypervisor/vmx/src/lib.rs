//! Intel VT-x (VMX) Support
//!
//! Provides VMX virtualization for x86_64.

#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]

extern crate alloc;

pub mod vmcs;
pub mod ept;
pub mod vmx;

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use vmm::{
    VmmBackend, VmmCapabilities, VmmResult, VmmError,
    VcpuState, VcpuRegs, VmExit, ExitReason, ExitData,
    VirtualMachine, VcpuId,
    IoInfo, CpuidInfo, MsrAccessInfo, EptViolationInfo,
};

pub use vmcs::{Vmcs, VmcsField};
pub use ept::{Ept, EptEntry, EptViolation};
pub use vmx::{vmxon, vmxoff, vmclear, vmptrld, vmlaunch, vmresume, vmread, vmwrite};

/// Read MSR
#[inline]
fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack)
        );
    }
    ((high as u64) << 32) | (low as u64)
}

/// Execute CPUID
#[inline]
fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
    unsafe {
        // rbx is reserved by LLVM, so we save/restore it manually
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx = out(reg) ebx,
            inout("ecx") 0u32 => ecx,
            out("edx") edx,
            options(nostack)
        );
    }
    (eax, ebx, ecx, edx)
}

/// VMX backend
pub struct VmxBackend {
    /// VMX enabled
    enabled: AtomicBool,
    /// Capabilities
    capabilities: VmmCapabilities,
}

impl VmxBackend {
    /// Create new VMX backend
    pub fn new() -> Self {
        let caps = Self::detect_capabilities();
        VmxBackend {
            enabled: AtomicBool::new(false),
            capabilities: caps,
        }
    }

    /// Detect VMX capabilities
    fn detect_capabilities() -> VmmCapabilities {
        // Check CPUID for VMX support
        let (_, _, ecx, _) = cpuid(1);
        let vmx_supported = (ecx & (1 << 5)) != 0;

        if !vmx_supported {
            return VmmCapabilities::default();
        }

        // Read VMX capabilities from MSRs
        let vmx_procbased_ctls = rdmsr(0x482);
        let vmx_procbased_ctls2 = rdmsr(0x48B);

        // Check secondary controls
        let has_secondary = (vmx_procbased_ctls >> 32) & (1 << 31) != 0;

        let ept = if has_secondary {
            (vmx_procbased_ctls2 >> 32) & (1 << 1) != 0
        } else {
            false
        };

        let vpid = if has_secondary {
            (vmx_procbased_ctls2 >> 32) & (1 << 5) != 0
        } else {
            false
        };

        let unrestricted_guest = if has_secondary {
            (vmx_procbased_ctls2 >> 32) & (1 << 7) != 0
        } else {
            false
        };

        VmmCapabilities {
            hw_virt: true,
            ept,
            vpid,
            unrestricted_guest,
            posted_interrupts: false,
            max_vcpus: 255,
        }
    }
}

impl Default for VmxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VmmBackend for VmxBackend {
    fn is_supported(&self) -> bool {
        self.capabilities.hw_virt
    }

    fn capabilities(&self) -> VmmCapabilities {
        self.capabilities
    }

    fn enable(&self) -> VmmResult<()> {
        if !self.is_supported() {
            return Err(VmmError::NotSupported);
        }

        // Set CR4.VMXE
        unsafe {
            let mut cr4: u64;
            core::arch::asm!("mov {}, cr4", out(reg) cr4);
            cr4 |= 1 << 13; // VMXE bit
            core::arch::asm!("mov cr4, {}", in(reg) cr4);
        }

        // Allocate VMXON region (4KB aligned)
        let vmxon_region = unsafe {
            alloc::alloc::alloc(
                alloc::alloc::Layout::from_size_align(4096, 4096).unwrap()
            ) as u64
        };

        // Set revision ID
        let revision_id = rdmsr(0x480) as u32;
        unsafe {
            *(vmxon_region as *mut u32) = revision_id;
        }

        // Execute VMXON
        let result = unsafe { vmxon(vmxon_region) };

        if result {
            self.enabled.store(true, Ordering::SeqCst);
            Ok(())
        } else {
            Err(VmmError::VmxError)
        }
    }

    fn disable(&self) -> VmmResult<()> {
        if self.enabled.load(Ordering::SeqCst) {
            unsafe { vmxoff() };
            self.enabled.store(false, Ordering::SeqCst);
        }
        Ok(())
    }

    fn create_vcpu_state(&self, vm: &VirtualMachine, vcpu_id: VcpuId) -> VmmResult<Box<dyn VcpuState>> {
        let state = VmxVcpuState::new(vm, vcpu_id)?;
        Ok(Box::new(state))
    }
}

/// VMX VCPU state
pub struct VmxVcpuState {
    /// VMCS
    vmcs: Vmcs,
    /// EPT
    ept: Arc<Mutex<Ept>>,
    /// Cached registers
    regs: VcpuRegs,
    /// VM ID
    _vm_id: u64,
    /// VCPU ID
    _vcpu_id: VcpuId,
}

impl VmxVcpuState {
    /// Create new VMX VCPU state
    pub fn new(vm: &VirtualMachine, vcpu_id: VcpuId) -> VmmResult<Self> {
        let vmcs = Vmcs::new()?;
        let ept = Arc::new(Mutex::new(Ept::new()?));

        Ok(VmxVcpuState {
            vmcs,
            ept,
            regs: VcpuRegs::default(),
            _vm_id: vm.id().0,
            _vcpu_id: vcpu_id,
        })
    }
}

impl VcpuState for VmxVcpuState {
    fn init(&mut self) -> VmmResult<()> {
        // Clear and load VMCS
        self.vmcs.clear()?;
        self.vmcs.load()?;

        // Initialize VMCS fields
        self.vmcs.init_host_state()?;
        self.vmcs.init_guest_state()?;
        self.vmcs.init_controls()?;

        // Set up EPT
        let ept_ptr = self.ept.lock().pointer();
        self.vmcs.set_ept_pointer(ept_ptr)?;

        Ok(())
    }

    fn load(&self) -> VmmResult<()> {
        self.vmcs.load()
    }

    fn store(&mut self) -> VmmResult<()> {
        // Read guest state from VMCS
        self.regs.rsp = vmread(VmcsField::GuestRsp as u32).unwrap_or(0);
        self.regs.rip = vmread(VmcsField::GuestRip as u32).unwrap_or(0);
        self.regs.rflags = vmread(VmcsField::GuestRflags as u32).unwrap_or(0);

        Ok(())
    }

    fn set_regs(&mut self, regs: &VcpuRegs) -> VmmResult<()> {
        self.regs = regs.clone();

        // Write to VMCS
        vmwrite(VmcsField::GuestRsp as u32, regs.rsp).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestRip as u32, regs.rip).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestRflags as u32, regs.rflags).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestCr0 as u32, regs.cr0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestCr3 as u32, regs.cr3).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestCr4 as u32, regs.cr4).map_err(|_| VmmError::VmcsError)?;

        Ok(())
    }

    fn get_regs(&self) -> VmmResult<VcpuRegs> {
        Ok(self.regs.clone())
    }

    fn run(&mut self) -> VmmResult<VmExit> {
        // VM entry
        let launched = self.vmcs.is_launched();
        let success = if launched {
            unsafe { vmresume() }
        } else {
            let result = unsafe { vmlaunch() };
            if result {
                self.vmcs.set_launched();
            }
            result
        };

        if !success {
            return Err(VmmError::VmxError);
        }

        // Read exit reason
        let exit_reason = vmread(VmcsField::VmExitReason as u32).unwrap_or(0) as u32;
        let reason = ExitReason::from(exit_reason & 0xFFFF);

        let data = match reason {
            ExitReason::IoInstruction => {
                let qualification = vmread(VmcsField::ExitQualification as u32).unwrap_or(0);
                ExitData::Io(IoInfo {
                    port: ((qualification >> 16) & 0xFFFF) as u16,
                    size: (((qualification & 0x7) + 1) as u8),
                    is_write: (qualification & 0x8) == 0,
                    is_string: (qualification & 0x10) != 0,
                    is_rep: (qualification & 0x20) != 0,
                    data: self.regs.rax as u32,
                })
            }
            ExitReason::Cpuid => {
                ExitData::Cpuid(CpuidInfo {
                    leaf: self.regs.rax as u32,
                    subleaf: self.regs.rcx as u32,
                })
            }
            ExitReason::Rdmsr | ExitReason::Wrmsr => {
                ExitData::MsrAccess(MsrAccessInfo {
                    index: self.regs.rcx as u32,
                    is_write: reason == ExitReason::Wrmsr,
                    value: ((self.regs.rdx << 32) | (self.regs.rax & 0xFFFF_FFFF)),
                })
            }
            ExitReason::EptViolation => {
                let qualification = vmread(VmcsField::ExitQualification as u32).unwrap_or(0);
                let gpa = vmread(VmcsField::GuestPhysicalAddress as u32).unwrap_or(0);
                ExitData::EptViolation(EptViolationInfo {
                    gpa,
                    gla: None,
                    read: (qualification & 0x1) != 0,
                    write: (qualification & 0x2) != 0,
                    execute: (qualification & 0x4) != 0,
                    gla_valid: (qualification & 0x80) != 0,
                })
            }
            _ => ExitData::None,
        };

        Ok(VmExit::with_data(reason, data))
    }

    fn inject_interrupt(&mut self, vector: u8) -> VmmResult<()> {
        // Set VM-entry interrupt info field
        let info = (vector as u64) | (0 << 8) | (1 << 31); // External interrupt, valid
        vmwrite(VmcsField::VmEntryIntInfo as u32, info).map_err(|_| VmmError::VmcsError)?;
        Ok(())
    }
}
