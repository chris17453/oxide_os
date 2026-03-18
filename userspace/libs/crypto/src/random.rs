//! Cryptographic random number generation
//!
//! Thin wrapper around the kernel's getrandom syscall. Every byte comes
//! straight from the kernel CSPRNG — no userspace PRNG nonsense.
//! — ColdCipher: "Entropy from the kernel or nothing. There is no Plan B."

/// Fill buffer with cryptographically secure random bytes
///
/// Uses the kernel's getrandom syscall (SYS_318). Blocks until sufficient
/// entropy is available. Panics if the syscall fails — in crypto, partial
/// randomness is worse than no randomness.
/// — ColdCipher: "If the entropy pool is dry, we wait. Weak keys kill."
pub fn random_bytes(buf: &mut [u8]) {
    let ret = libc::syscall::sys_getrandom(buf, 0);
    if ret < 0 {
        panic!("getrandom syscall failed with error {}", ret);
    }
}

/// Generate a fixed-size array of random bytes
/// — ColdCipher: "32 bytes of chaos, courtesy of the kernel CSPRNG."
pub fn random_array<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    random_bytes(&mut buf);
    buf
}
