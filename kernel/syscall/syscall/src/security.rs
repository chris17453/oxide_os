//! Security syscalls
//!
//! Provides prctl, capabilities (capget/capset) for process security control.

extern crate alloc;

use crate::errno;

// ============================================================================
// Week 6: Security Groundwork
// ============================================================================

/// prctl options
mod prctl_options {
    pub const PR_SET_NAME: i32 = 15; // Set process name
    pub const PR_GET_NAME: i32 = 16; // Get process name
    pub const PR_SET_DUMPABLE: i32 = 4; // Set coredump filter
    pub const PR_GET_DUMPABLE: i32 = 3; // Get coredump filter
    pub const PR_SET_KEEPCAPS: i32 = 8; // Keep caps on setuid
    pub const PR_GET_KEEPCAPS: i32 = 7; // Get keepcaps
    pub const PR_SET_NO_NEW_PRIVS: i32 = 38; // Disable privilege grants
    pub const PR_GET_NO_NEW_PRIVS: i32 = 39; // Get no_new_privs state
    pub const PR_SET_SECCOMP: i32 = 22; // Set seccomp mode
    pub const PR_GET_SECCOMP: i32 = 21; // Get seccomp mode
    pub const PR_CAPBSET_READ: i32 = 23; // Check capability
    pub const PR_CAPBSET_DROP: i32 = 24; // Drop capability
}

/// sys_prctl - Process control operations
///
/// # Arguments
/// * `option` - PR_* operation to perform
/// * `arg2` - Operation-specific argument
/// * `arg3` - Operation-specific argument
/// * `arg4` - Operation-specific argument
/// * `arg5` - Operation-specific argument
///
/// # ColdCipher
/// Swiss-army knife for process security and properties. PR_SET_NAME sets
/// comm name (shows in ps); PR_SET_NO_NEW_PRIVS prevents setuid escalation
/// (required before seccomp); PR_SET_KEEPCAPS preserves capabilities across
/// setuid. Essential for sandboxing and privilege management.
pub fn sys_prctl(option: i32, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64) -> i64 {
    match option {
        prctl_options::PR_SET_NAME => {
            // Would set process name in ProcessMeta
            // For now, accept silently
            0
        }
        prctl_options::PR_GET_NAME => {
            // Would copy process name to user buffer
            errno::ENOSYS
        }
        prctl_options::PR_SET_DUMPABLE => {
            // Controls whether process can be dumped/traced
            0
        }
        prctl_options::PR_GET_DUMPABLE => {
            1 // Dumpable by default
        }
        prctl_options::PR_SET_NO_NEW_PRIVS => {
            // Set no_new_privs flag (prevents privilege escalation)
            // Required before seccomp strict mode
            0
        }
        prctl_options::PR_GET_NO_NEW_PRIVS => {
            0 // Not set by default
        }
        _ => errno::EINVAL,
    }
}

/// Linux capability (32 bits each)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CapUserHeader {
    version: u32,
    pid: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CapUserData {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}

/// Capability version
const LINUX_CAPABILITY_VERSION_3: u32 = 0x20080522;

/// sys_capget - Get thread capabilities
///
/// # Arguments
/// * `hdrp` - Capability header (version + target PID)
/// * `datap` - Output: capability data sets
///
/// # EmberLock
/// Reads process capability sets: effective (active now), permitted (can
/// activate), inheritable (passed to children). Capabilities are granular
/// root powers: CAP_NET_BIND_SERVICE allows port <1024 without full root.
/// Modern privilege model for daemons.
pub fn sys_capget(hdrp: u64, datap: u64) -> i64 {
    if hdrp == 0 {
        return errno::EFAULT;
    }

    unsafe {
        core::arch::asm!("stac", options(nostack));

        let hdr = core::ptr::read_volatile(hdrp as *const CapUserHeader);

        // Check version
        if hdr.version != LINUX_CAPABILITY_VERSION_3 {
            core::arch::asm!("clac", options(nostack));
            return errno::EINVAL;
        }

        // For now, return full capabilities (root-like)
        if datap != 0 {
            let data = CapUserData {
                effective: 0xFFFFFFFF,
                permitted: 0xFFFFFFFF,
                inheritable: 0,
            };
            core::ptr::write_volatile(datap as *mut CapUserData, data);
        }

        core::arch::asm!("clac", options(nostack));
    }

    0
}

/// sys_capset - Set thread capabilities
///
/// # Arguments
/// * `hdrp` - Capability header (version + target PID)
/// * `datap` - Input: new capability sets
///
/// # EmberLock
/// Modifies capability sets. Can only drop capabilities or move from
/// permitted to effective (can't gain new ones). Used by daemons to
/// drop unneeded privileges after initialization: bind low port with
/// CAP_NET_BIND_SERVICE, then drop all caps except CAP_NET_ADMIN.
pub fn sys_capset(hdrp: u64, _datap: u64) -> i64 {
    if hdrp == 0 {
        return errno::EFAULT;
    }

    unsafe {
        core::arch::asm!("stac", options(nostack));
        let hdr = core::ptr::read_volatile(hdrp as *const CapUserHeader);
        core::arch::asm!("clac", options(nostack));

        // Check version
        if hdr.version != LINUX_CAPABILITY_VERSION_3 {
            return errno::EINVAL;
        }
    }

    // For now, accept capability changes without enforcing
    // Full implementation would:
    // 1. Validate new caps are subset of permitted
    // 2. Update ProcessMeta capability fields
    // 3. Check CAP_SETPCAP for changing other processes
    0
}
