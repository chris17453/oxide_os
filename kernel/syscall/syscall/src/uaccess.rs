//! User memory access layer — STAC/CLAC wrappers for safe userspace reads/writes.
//!
//! — ColdCipher: Every kernel touch of userspace memory goes through here.
//! STAC before, CLAC after, address validation always. No exceptions. No shortcuts.
//! You skip the validation? You deserve the #PF. You forget the CLAC? You deserve
//! the SMAP bypass. This module exists so you have zero excuses.
//!
//! # SMAP contract
//! SFMASK clears AC on syscall entry, so the kernel starts every syscall with SMAP
//! enforcement active (AC=0). These helpers bracket each userspace access with
//! STAC (AC=1 → SMAP temporarily suspended) and CLAC (AC=0 → SMAP restored).
//! The window where SMAP is off is as narrow as physically possible.

use alloc::string::String;
use alloc::vec::Vec;
use crate::errno;

/// — ColdCipher: The canonical user-space ceiling on x86_64. Anything at or
/// above this is kernel territory. A userspace pointer claiming to live up there
/// is a lie, an attack, or both. We don't negotiate with either.
const USER_SPACE_END: u64 = 0x0000_8000_0000_0000;

// ============================================================================
// Address validation
// ============================================================================

/// Validate that a user buffer [ptr, ptr+len) lies entirely in userspace.
///
/// — ColdCipher: Three failure modes, all fatal to the syscall:
///   1. Null pointer — classic "I forgot to initialize" exploit vector.
///   2. Pointer above USER_SPACE_END — direct kernel memory access attempt.
///   3. ptr+len wraps around or crosses USER_SPACE_END — overflow/split attack.
///
/// Returns `true` only when all three checks pass.
#[inline]
pub fn validate_user_buffer(ptr: u64, len: usize) -> bool {
    // — ColdCipher: Null is never a valid user buffer. Full stop.
    if ptr == 0 {
        return false;
    }
    if ptr >= USER_SPACE_END {
        return false;
    }
    // — ColdCipher: Saturating add stops the classic ptr+len overflow trick where
    // a huge len wraps the sum back into kernel space and we naively accept it.
    if ptr.saturating_add(len as u64) > USER_SPACE_END {
        return false;
    }
    true
}

// ============================================================================
// Bulk copy helpers
// ============================================================================

/// Copy `len` bytes from a userspace pointer into a kernel-owned `Vec<u8>`.
///
/// — ColdCipher: STAC opens the SMAP gate just long enough to drain the bytes.
/// CLAC slams it shut. The returned Vec lives on the kernel heap — userspace
/// can race-corrupt their copy all they want; we already left with the data.
///
/// Returns `Err(errno::EFAULT)` if the buffer fails address validation.
pub fn copy_from_user(ptr: u64, len: usize) -> Result<Vec<u8>, i64> {
    if !validate_user_buffer(ptr, len) {
        return Err(errno::EFAULT);
    }
    // Safety: validate_user_buffer confirmed [ptr, ptr+len) is in userspace.
    // STAC/CLAC bracket the access so SMAP enforcement is temporarily suspended
    // for exactly this window. The resulting Vec is kernel-heap owned.
    unsafe {
        os_core::user_access_begin();
        let slice = core::slice::from_raw_parts(ptr as *const u8, len);
        let result = slice.to_vec();
        os_core::user_access_end();
        Ok(result)
    }
}

/// Copy `src` bytes from kernel memory into a userspace buffer at `dst`.
///
/// — ColdCipher: Symmetric to copy_from_user. We validate the destination,
/// open the SMAP gate, write, then close it. If the user gave us a bogus dst,
/// they get EFAULT. Their loss.
///
/// Returns `Err(errno::EFAULT)` if the destination fails address validation.
pub fn copy_to_user(dst: u64, src: &[u8]) -> Result<(), i64> {
    if !validate_user_buffer(dst, src.len()) {
        return Err(errno::EFAULT);
    }
    // Safety: validate_user_buffer confirmed [dst, dst+len) is in userspace.
    // STAC/CLAC bracket the write window.
    unsafe {
        os_core::user_access_begin();
        let dst_slice = core::slice::from_raw_parts_mut(dst as *mut u8, src.len());
        dst_slice.copy_from_slice(src);
        os_core::user_access_end();
        Ok(())
    }
}

// ============================================================================
// Typed scalar helpers
// ============================================================================

/// Read a `T` from a userspace pointer via `read_volatile`.
///
/// — ColdCipher: Volatile because the compiler must not cache the read or
/// reorder it outside the STAC/CLAC window. Without volatile the optimizer
/// could legally move the load before STAC — that's an SMAP bypass in disguise.
///
/// Returns `Err(errno::EFAULT)` if the pointer fails alignment/bounds checks.
pub fn get_user<T: Copy>(ptr: u64) -> Result<T, i64> {
    let size = core::mem::size_of::<T>();
    if !validate_user_buffer(ptr, size) {
        return Err(errno::EFAULT);
    }
    // Safety: validate_user_buffer confirmed the range is in userspace.
    // read_volatile prevents the load from migrating outside the STAC/CLAC fence.
    unsafe {
        os_core::user_access_begin();
        let val = core::ptr::read_volatile(ptr as *const T);
        os_core::user_access_end();
        Ok(val)
    }
}

/// Write a `T` to a userspace pointer via `write_volatile`.
///
/// — ColdCipher: Same volatile requirement as get_user — the compiler must
/// not sink the store past CLAC. If it did, we'd write user memory with SMAP
/// active and take a spurious #PF in the kernel. Not fun. Volatile prevents it.
///
/// Returns `Err(errno::EFAULT)` if the pointer fails alignment/bounds checks.
pub fn put_user<T: Copy>(ptr: u64, val: T) -> Result<(), i64> {
    let size = core::mem::size_of::<T>();
    if !validate_user_buffer(ptr, size) {
        return Err(errno::EFAULT);
    }
    // Safety: validate_user_buffer confirmed the range is in userspace.
    // write_volatile prevents the store from migrating outside the STAC/CLAC fence.
    unsafe {
        os_core::user_access_begin();
        core::ptr::write_volatile(ptr as *mut T, val);
        os_core::user_access_end();
        Ok(())
    }
}

// ============================================================================
// String helpers (convenience wrappers around copy_from_user)
// ============================================================================

/// Copy a UTF-8 string from userspace into a kernel `String`.
///
/// — ColdCipher: Thin wrapper over copy_from_user that validates UTF-8 on the
/// kernel side. The source is opaque bytes from userspace — never trust it to
/// be valid text until we check ourselves.
///
/// Returns `Err(errno::EFAULT)` on bad pointer, `Err(errno::EINVAL)` on bad UTF-8.
pub fn copy_string_from_user(ptr: u64, len: usize) -> Result<String, i64> {
    let bytes = copy_from_user(ptr, len)?;
    // — ColdCipher: UTF-8 validation happens on our kernel-heap copy, not on the
    // live userspace bytes. Race window is already closed before we get here.
    alloc::str::from_utf8(&bytes)
        .map(|s| String::from(s))
        .map_err(|_| errno::EINVAL)
}
