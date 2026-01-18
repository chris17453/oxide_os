//! VMX Instructions

#![allow(unsafe_op_in_unsafe_fn)]

/// Execute VMXON instruction
///
/// # Safety
/// Must be called with valid VMXON region physical address
pub unsafe fn vmxon(vmxon_region: u64) -> bool {
    let mut rflags: u64;
    unsafe {
        core::arch::asm!(
            "vmxon [{region}]",
            "pushfq",
            "pop {rflags}",
            region = in(reg) &vmxon_region,
            rflags = out(reg) rflags,
            options(nostack)
        );
    }
    (rflags & 0x41) == 0
}

/// Execute VMXOFF instruction
///
/// # Safety
/// Must be in VMX operation
pub unsafe fn vmxoff() {
    unsafe {
        core::arch::asm!("vmxoff", options(nostack));
    }
}

/// Execute VMCLEAR instruction
///
/// # Safety
/// Must be in VMX operation with valid VMCS physical address
pub unsafe fn vmclear(vmcs_phys: u64) -> bool {
    let mut rflags: u64;
    unsafe {
        core::arch::asm!(
            "vmclear [{vmcs}]",
            "pushfq",
            "pop {rflags}",
            vmcs = in(reg) &vmcs_phys,
            rflags = out(reg) rflags,
            options(nostack)
        );
    }
    (rflags & 0x41) == 0
}

/// Execute VMPTRLD instruction
///
/// # Safety
/// Must be in VMX operation with valid VMCS physical address
pub unsafe fn vmptrld(vmcs_phys: u64) -> bool {
    let mut rflags: u64;
    unsafe {
        core::arch::asm!(
            "vmptrld [{vmcs}]",
            "pushfq",
            "pop {rflags}",
            vmcs = in(reg) &vmcs_phys,
            rflags = out(reg) rflags,
            options(nostack)
        );
    }
    (rflags & 0x41) == 0
}

/// Execute VMREAD instruction
pub fn vmread(field: u32) -> Result<u64, ()> {
    let mut value: u64;
    let mut rflags: u64;
    unsafe {
        core::arch::asm!(
            "vmread {value}, {field}",
            "pushfq",
            "pop {rflags}",
            field = in(reg) field as u64,
            value = out(reg) value,
            rflags = out(reg) rflags,
            options(nostack)
        );
    }
    if (rflags & 0x41) == 0 {
        Ok(value)
    } else {
        Err(())
    }
}

/// Execute VMWRITE instruction
pub fn vmwrite(field: u32, value: u64) -> Result<(), ()> {
    let mut rflags: u64;
    unsafe {
        core::arch::asm!(
            "vmwrite {field}, {value}",
            "pushfq",
            "pop {rflags}",
            field = in(reg) field as u64,
            value = in(reg) value,
            rflags = out(reg) rflags,
            options(nostack)
        );
    }
    if (rflags & 0x41) == 0 {
        Ok(())
    } else {
        Err(())
    }
}

/// Execute VMLAUNCH instruction
///
/// # Safety
/// VMCS must be properly initialized
pub unsafe fn vmlaunch() -> bool {
    let mut rflags: u64;
    unsafe {
        core::arch::asm!(
            "vmlaunch",
            "pushfq",
            "pop {rflags}",
            rflags = out(reg) rflags,
            options(nostack)
        );
    }
    (rflags & 0x41) == 0
}

/// Execute VMRESUME instruction
///
/// # Safety
/// VMCS must be properly set up and previously launched
pub unsafe fn vmresume() -> bool {
    let mut rflags: u64;
    unsafe {
        core::arch::asm!(
            "vmresume",
            "pushfq",
            "pop {rflags}",
            rflags = out(reg) rflags,
            options(nostack)
        );
    }
    (rflags & 0x41) == 0
}
