//! VMCS Management

#![allow(unsafe_op_in_unsafe_fn)]

use crate::vmx::{vmclear, vmptrld, vmread, vmwrite};
use vmm::{VmmError, VmmResult};

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

/// VMCS field encodings
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum VmcsField {
    // 16-bit guest state
    GuestEsSelector = 0x0800,
    GuestCsSelector = 0x0802,
    GuestSsSelector = 0x0804,
    GuestDsSelector = 0x0806,
    GuestFsSelector = 0x0808,
    GuestGsSelector = 0x080A,
    GuestLdtrSelector = 0x080C,
    GuestTrSelector = 0x080E,

    // 16-bit host state
    HostEsSelector = 0x0C00,
    HostCsSelector = 0x0C02,
    HostSsSelector = 0x0C04,
    HostDsSelector = 0x0C06,
    HostFsSelector = 0x0C08,
    HostGsSelector = 0x0C0A,
    HostTrSelector = 0x0C0C,

    // 64-bit control fields
    EptPointer = 0x201A,

    // 64-bit read-only fields
    GuestPhysicalAddress = 0x2400,

    // 64-bit guest state
    VmcsLinkPointer = 0x2800,

    // 64-bit host state
    HostIa32Efer = 0x2C02,

    // 32-bit control fields
    PinBasedVmExecControls = 0x4000,
    CpuBasedVmExecControls = 0x4002,
    ExceptionBitmap = 0x4004,
    VmExitControls = 0x400C,
    VmEntryControls = 0x4012,
    VmEntryIntInfo = 0x4016,
    SecondaryVmExecControls = 0x401E,

    // 32-bit read-only fields
    VmExitReason = 0x4402,

    // 32-bit guest state
    GuestEsLimit = 0x4800,
    GuestCsLimit = 0x4802,
    GuestSsLimit = 0x4804,
    GuestDsLimit = 0x4806,
    GuestFsLimit = 0x4808,
    GuestGsLimit = 0x480A,
    GuestLdtrLimit = 0x480C,
    GuestTrLimit = 0x480E,
    GuestGdtrLimit = 0x4810,
    GuestIdtrLimit = 0x4812,
    GuestEsAccessRights = 0x4814,
    GuestCsAccessRights = 0x4816,
    GuestSsAccessRights = 0x4818,
    GuestDsAccessRights = 0x481A,
    GuestFsAccessRights = 0x481C,
    GuestGsAccessRights = 0x481E,
    GuestLdtrAccessRights = 0x4820,
    GuestTrAccessRights = 0x4822,
    GuestInterruptibilityState = 0x4824,
    GuestActivityState = 0x4826,
    HostIa32SysenterCs = 0x4C00,

    // Natural-width control fields
    Cr0GuestHostMask = 0x6000,
    Cr4GuestHostMask = 0x6002,
    Cr0ReadShadow = 0x6004,
    Cr4ReadShadow = 0x6006,

    // Natural-width read-only fields
    ExitQualification = 0x6400,

    // Natural-width guest state
    GuestCr0 = 0x6800,
    GuestCr3 = 0x6802,
    GuestCr4 = 0x6804,
    GuestEsBase = 0x6806,
    GuestCsBase = 0x6808,
    GuestSsBase = 0x680A,
    GuestDsBase = 0x680C,
    GuestFsBase = 0x680E,
    GuestGsBase = 0x6810,
    GuestLdtrBase = 0x6812,
    GuestTrBase = 0x6814,
    GuestGdtrBase = 0x6816,
    GuestIdtrBase = 0x6818,
    GuestDr7 = 0x681A,
    GuestRsp = 0x681C,
    GuestRip = 0x681E,
    GuestRflags = 0x6820,
    GuestPendingDbgExceptions = 0x6822,

    // Natural-width host state
    HostCr0 = 0x6C00,
    HostCr3 = 0x6C02,
    HostCr4 = 0x6C04,
    HostFsBase = 0x6C06,
    HostGsBase = 0x6C08,
    HostTrBase = 0x6C0A,
    HostGdtrBase = 0x6C0C,
    HostIdtrBase = 0x6C0E,
    HostRsp = 0x6C14,
    HostRip = 0x6C16,
}

/// VMCS structure
pub struct Vmcs {
    /// Physical address of VMCS region
    phys_addr: u64,
    /// Virtual address of VMCS region
    virt_addr: *mut u8,
    /// Is VMCS launched
    launched: bool,
}

// VMCS is single-threaded use per-VCPU
unsafe impl Send for Vmcs {}
unsafe impl Sync for Vmcs {}

impl Vmcs {
    /// Create new VMCS
    pub fn new() -> VmmResult<Self> {
        // Allocate 4KB aligned region
        let layout = alloc::alloc::Layout::from_size_align(4096, 4096)
            .map_err(|_| VmmError::ResourceExhausted)?;
        let virt_addr = unsafe { alloc::alloc::alloc_zeroed(layout) };

        if virt_addr.is_null() {
            return Err(VmmError::ResourceExhausted);
        }

        // For now, assume physical = virtual (identity mapped)
        let phys_addr = virt_addr as u64;

        // Write revision ID
        let revision_id = rdmsr(0x480) as u32;
        unsafe {
            *(virt_addr as *mut u32) = revision_id;
        }

        Ok(Vmcs {
            phys_addr,
            virt_addr,
            launched: false,
        })
    }

    /// Get physical address
    pub fn phys_addr(&self) -> u64 {
        self.phys_addr
    }

    /// Clear VMCS
    pub fn clear(&self) -> VmmResult<()> {
        if unsafe { vmclear(self.phys_addr) } {
            Ok(())
        } else {
            Err(VmmError::VmxError)
        }
    }

    /// Load VMCS as current
    pub fn load(&self) -> VmmResult<()> {
        if unsafe { vmptrld(self.phys_addr) } {
            Ok(())
        } else {
            Err(VmmError::VmxError)
        }
    }

    /// Check if launched
    pub fn is_launched(&self) -> bool {
        self.launched
    }

    /// Mark as launched
    pub fn set_launched(&mut self) {
        self.launched = true;
    }

    /// Initialize host state fields
    pub fn init_host_state(&self) -> VmmResult<()> {
        // Host CR0/CR3/CR4
        let cr0: u64;
        let cr3: u64;
        let cr4: u64;
        unsafe {
            core::arch::asm!("mov {}, cr0", out(reg) cr0);
            core::arch::asm!("mov {}, cr3", out(reg) cr3);
            core::arch::asm!("mov {}, cr4", out(reg) cr4);
        }

        vmwrite(VmcsField::HostCr0 as u32, cr0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostCr3 as u32, cr3).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostCr4 as u32, cr4).map_err(|_| VmmError::VmcsError)?;

        // Host segment selectors
        let cs: u16;
        let ss: u16;
        let ds: u16;
        let es: u16;
        let fs: u16;
        let gs: u16;
        let tr: u16;
        unsafe {
            core::arch::asm!("mov {:x}, cs", out(reg) cs);
            core::arch::asm!("mov {:x}, ss", out(reg) ss);
            core::arch::asm!("mov {:x}, ds", out(reg) ds);
            core::arch::asm!("mov {:x}, es", out(reg) es);
            core::arch::asm!("mov {:x}, fs", out(reg) fs);
            core::arch::asm!("mov {:x}, gs", out(reg) gs);
            core::arch::asm!("str {:x}", out(reg) tr);
        }

        vmwrite(VmcsField::HostCsSelector as u32, cs as u64).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostSsSelector as u32, ss as u64).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostDsSelector as u32, ds as u64).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostEsSelector as u32, es as u64).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostFsSelector as u32, fs as u64).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostGsSelector as u32, gs as u64).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostTrSelector as u32, tr as u64).map_err(|_| VmmError::VmcsError)?;

        // Host GDTR/IDTR base
        let mut gdtr: [u8; 10] = [0; 10];
        let mut idtr: [u8; 10] = [0; 10];
        unsafe {
            core::arch::asm!("sgdt [{}]", in(reg) gdtr.as_mut_ptr());
            core::arch::asm!("sidt [{}]", in(reg) idtr.as_mut_ptr());
        }
        let gdtr_base = u64::from_le_bytes([
            gdtr[2], gdtr[3], gdtr[4], gdtr[5], gdtr[6], gdtr[7], gdtr[8], gdtr[9],
        ]);
        let idtr_base = u64::from_le_bytes([
            idtr[2], idtr[3], idtr[4], idtr[5], idtr[6], idtr[7], idtr[8], idtr[9],
        ]);

        vmwrite(VmcsField::HostGdtrBase as u32, gdtr_base).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostIdtrBase as u32, idtr_base).map_err(|_| VmmError::VmcsError)?;

        // Host RSP/RIP will be set at VM entry
        vmwrite(VmcsField::HostRsp as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostRip as u32, vmexit_handler as u64)
            .map_err(|_| VmmError::VmcsError)?;

        // Host FS/GS base
        let fs_base = rdmsr(0xC0000100);
        let gs_base = rdmsr(0xC0000101);
        vmwrite(VmcsField::HostFsBase as u32, fs_base).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::HostGsBase as u32, gs_base).map_err(|_| VmmError::VmcsError)?;

        // Host EFER
        let efer = rdmsr(0xC0000080);
        vmwrite(VmcsField::HostIa32Efer as u32, efer).map_err(|_| VmmError::VmcsError)?;

        Ok(())
    }

    /// Initialize guest state fields
    pub fn init_guest_state(&self) -> VmmResult<()> {
        // Guest CR0/CR3/CR4
        let cr0 = 0x20u64; // NE bit set
        let cr3 = 0u64;
        let cr4 = 0x2000u64; // VMXE bit

        vmwrite(VmcsField::GuestCr0 as u32, cr0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestCr3 as u32, cr3).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestCr4 as u32, cr4).map_err(|_| VmmError::VmcsError)?;

        // CS
        vmwrite(VmcsField::GuestCsSelector as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestCsBase as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestCsLimit as u32, 0xFFFF).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestCsAccessRights as u32, 0x9B).map_err(|_| VmmError::VmcsError)?;

        // DS/ES/FS/GS/SS
        for (sel, base, limit, ar) in [
            (
                VmcsField::GuestDsSelector,
                VmcsField::GuestDsBase,
                VmcsField::GuestDsLimit,
                VmcsField::GuestDsAccessRights,
            ),
            (
                VmcsField::GuestEsSelector,
                VmcsField::GuestEsBase,
                VmcsField::GuestEsLimit,
                VmcsField::GuestEsAccessRights,
            ),
            (
                VmcsField::GuestFsSelector,
                VmcsField::GuestFsBase,
                VmcsField::GuestFsLimit,
                VmcsField::GuestFsAccessRights,
            ),
            (
                VmcsField::GuestGsSelector,
                VmcsField::GuestGsBase,
                VmcsField::GuestGsLimit,
                VmcsField::GuestGsAccessRights,
            ),
            (
                VmcsField::GuestSsSelector,
                VmcsField::GuestSsBase,
                VmcsField::GuestSsLimit,
                VmcsField::GuestSsAccessRights,
            ),
        ] {
            vmwrite(sel as u32, 0).map_err(|_| VmmError::VmcsError)?;
            vmwrite(base as u32, 0).map_err(|_| VmmError::VmcsError)?;
            vmwrite(limit as u32, 0xFFFF).map_err(|_| VmmError::VmcsError)?;
            vmwrite(ar as u32, 0x93).map_err(|_| VmmError::VmcsError)?;
        }

        // LDTR (unusable)
        vmwrite(VmcsField::GuestLdtrSelector as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestLdtrBase as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestLdtrLimit as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestLdtrAccessRights as u32, 0x10000)
            .map_err(|_| VmmError::VmcsError)?;

        // TR
        vmwrite(VmcsField::GuestTrSelector as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestTrBase as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestTrLimit as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestTrAccessRights as u32, 0x8B).map_err(|_| VmmError::VmcsError)?;

        // GDTR/IDTR
        vmwrite(VmcsField::GuestGdtrBase as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestGdtrLimit as u32, 0xFFFF).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestIdtrBase as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestIdtrLimit as u32, 0xFFFF).map_err(|_| VmmError::VmcsError)?;

        // RIP/RSP/RFLAGS
        vmwrite(VmcsField::GuestRip as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestRsp as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::GuestRflags as u32, 0x2).map_err(|_| VmmError::VmcsError)?;

        // DR7
        vmwrite(VmcsField::GuestDr7 as u32, 0x400).map_err(|_| VmmError::VmcsError)?;

        // Activity state (active)
        vmwrite(VmcsField::GuestActivityState as u32, 0).map_err(|_| VmmError::VmcsError)?;

        // Interruptibility state
        vmwrite(VmcsField::GuestInterruptibilityState as u32, 0)
            .map_err(|_| VmmError::VmcsError)?;

        // Pending debug exceptions
        vmwrite(VmcsField::GuestPendingDbgExceptions as u32, 0).map_err(|_| VmmError::VmcsError)?;

        // VMCS link pointer
        vmwrite(VmcsField::VmcsLinkPointer as u32, 0xFFFF_FFFF_FFFF_FFFF)
            .map_err(|_| VmmError::VmcsError)?;

        Ok(())
    }

    /// Initialize VM execution control fields
    pub fn init_controls(&self) -> VmmResult<()> {
        // Read capability MSRs
        let pin_based_caps = rdmsr(0x481);
        let proc_based_caps = rdmsr(0x482);
        let exit_caps = rdmsr(0x483);
        let entry_caps = rdmsr(0x484);

        // Pin-based controls
        let pin_based = adjust_controls(0, pin_based_caps);
        vmwrite(VmcsField::PinBasedVmExecControls as u32, pin_based as u64)
            .map_err(|_| VmmError::VmcsError)?;

        // Primary processor-based controls
        let mut proc_based = (1u32 << 7) | (1 << 24) | (1 << 28) | (1 << 31);
        proc_based = adjust_controls(proc_based, proc_based_caps);
        vmwrite(VmcsField::CpuBasedVmExecControls as u32, proc_based as u64)
            .map_err(|_| VmmError::VmcsError)?;

        // Secondary processor-based controls
        let proc_based2_caps = rdmsr(0x48B);
        let mut proc_based2 = (1u32 << 1) | (1 << 5);
        proc_based2 = adjust_controls(proc_based2, proc_based2_caps);
        vmwrite(
            VmcsField::SecondaryVmExecControls as u32,
            proc_based2 as u64,
        )
        .map_err(|_| VmmError::VmcsError)?;

        // VM-exit controls
        let mut exit_controls = (1u32 << 9) | (1 << 15);
        exit_controls = adjust_controls(exit_controls, exit_caps);
        vmwrite(VmcsField::VmExitControls as u32, exit_controls as u64)
            .map_err(|_| VmmError::VmcsError)?;

        // VM-entry controls
        let mut entry_controls = 1u32 << 9;
        entry_controls = adjust_controls(entry_controls, entry_caps);
        vmwrite(VmcsField::VmEntryControls as u32, entry_controls as u64)
            .map_err(|_| VmmError::VmcsError)?;

        // Exception bitmap
        vmwrite(VmcsField::ExceptionBitmap as u32, 0).map_err(|_| VmmError::VmcsError)?;

        // CR0/CR4 guest/host masks and shadows
        vmwrite(VmcsField::Cr0GuestHostMask as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::Cr0ReadShadow as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::Cr4GuestHostMask as u32, 0).map_err(|_| VmmError::VmcsError)?;
        vmwrite(VmcsField::Cr4ReadShadow as u32, 0).map_err(|_| VmmError::VmcsError)?;

        Ok(())
    }

    /// Set EPT pointer
    pub fn set_ept_pointer(&self, ept_ptr: u64) -> VmmResult<()> {
        vmwrite(VmcsField::EptPointer as u32, ept_ptr).map_err(|_| VmmError::VmcsError)
    }
}

impl Drop for Vmcs {
    fn drop(&mut self) {
        let layout = alloc::alloc::Layout::from_size_align(4096, 4096).unwrap();
        unsafe {
            alloc::alloc::dealloc(self.virt_addr, layout);
        }
    }
}

/// Adjust control value based on capabilities
fn adjust_controls(value: u32, caps: u64) -> u32 {
    let allowed_0 = caps as u32;
    let allowed_1 = (caps >> 32) as u32;
    (value | allowed_0) & allowed_1
}

/// VM exit handler (placeholder)
extern "C" fn vmexit_handler() {
    // This is called after VM exit
}
