//! — EmberLock: File permission enforcement. No more free rides.
//! Every file operation goes through here or it doesn't happen.
//!
//! Basic discretionary access control (DAC): owner/group/other triple.
//! No ACLs. No capabilities. No excuses. If your bits aren't set, you're not
//! getting through this door. Root always gets the VIP pass though — because
//! someone decided chaos should have an escape hatch.

/// Access mode flags (matching POSIX access(2))
pub const R_OK: u32 = 4; // Read permission
pub const W_OK: u32 = 2; // Write permission
pub const X_OK: u32 = 1; // Execute permission
pub const F_OK: u32 = 0; // File existence check only

/// Check if a caller can access a file with the given mode bits.
///
/// — EmberLock: Standard POSIX DAC check. Owner beats group beats other.
/// First match wins — if you're the owner, group bits don't apply even if
/// they'd give you MORE access. That's the spec. Don't @ me.
///
/// Returns true if access is permitted, false if denied.
pub fn check_permission(
    file_uid: u32,
    file_gid: u32,
    file_mode: u32,
    caller_uid: u32,
    caller_gid: u32,
    access: u32,
) -> bool {
    // — EmberLock: Root bypasses all DAC checks. uid 0 is god mode.
    // The original sin of UNIX security design, preserved for compatibility.
    if caller_uid == 0 {
        return true;
    }

    // — EmberLock: Extract the applicable permission bits.
    // Owner check first: if you own the file, owner bits apply exclusively.
    // Then group. Then fall through to world (other) bits. First match, full stop.
    let perm_bits = if caller_uid == file_uid {
        (file_mode >> 6) & 7 // Owner rwx bits
    } else if caller_gid == file_gid {
        (file_mode >> 3) & 7 // Group rwx bits
    } else {
        file_mode & 7 // Other rwx bits
    };

    // — EmberLock: F_OK just tests existence — the vnode lookup already proved
    // it exists if we got here. No permission bits needed for "does it exist?"
    if access == F_OK {
        return true;
    }

    // — EmberLock: Check that every requested bit is set. If you asked for
    // read+write (access=6) and the file only allows read (perm=4), you get -EACCES.
    // Partial permission is no permission.
    let needed = access & 7;
    (perm_bits & needed) == needed
}

/// Check if the caller is root (effective uid 0).
///
/// — EmberLock: The one privilege check that never fails. Convenient.
/// Slightly terrifying. Welcome to UNIX.
#[inline]
pub fn is_root(uid: u32) -> bool {
    uid == 0
}

/// Check if the caller can send a signal to the target process.
///
/// POSIX rules: sender's effective uid must match target's real or effective uid,
/// OR the sender is root. SIGCONT has special group rules in real kernels, but
/// for now this covers the non-root case that matters: you can't SIGKILL a root
/// process from a user account just by knowing its PID.
///
/// — EmberLock: This is the check that prevents unprivileged processes from
/// murdering each other at will. One less way for your shell script to go full
/// scorched-earth. You're welcome.
pub fn can_signal(sender_euid: u32, target_uid: u32) -> bool {
    // — EmberLock: Root sends signals to anyone. Democracy dies here.
    if sender_euid == 0 {
        return true;
    }

    // — EmberLock: Non-root can only signal processes they own.
    // Your euid matches their uid — your process, your rules. Otherwise, no.
    sender_euid == target_uid
}

/// Determine the required access mode from open flags.
///
/// — EmberLock: Translates O_RDONLY/O_WRONLY/O_RDWR into R_OK/W_OK for
/// permission checking. The flags encoding is a mess (0=RDONLY, 1=WRONLY,
/// 2=RDWR) but we deal with it because that's what userspace hands us.
pub fn access_from_open_flags(flags: u32) -> u32 {
    // — EmberLock: Bottom two bits of open flags encode read/write mode.
    // O_RDONLY = 0, O_WRONLY = 1, O_RDWR = 2. Classic "enum disguised as bits".
    match flags & 3 {
        0 => R_OK,           // O_RDONLY
        1 => W_OK,           // O_WRONLY
        2 => R_OK | W_OK,    // O_RDWR
        _ => R_OK | W_OK,    // unknown — conservative: require both
    }
}
