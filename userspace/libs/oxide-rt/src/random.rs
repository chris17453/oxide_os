//! Random number generation — getrandom syscall wrapper.
//!
//! — ColdCipher: Entropy from the kernel. If the kernel's PRNG is broken,
//! that's a kernel problem, not ours. We just pass the bytes along.

use crate::syscall::*;
use crate::nr;

/// Flags for getrandom
pub const GRND_NONBLOCK: u32 = 0x0001;
pub const GRND_RANDOM: u32 = 0x0002;

/// getrandom — fill buffer with random bytes from kernel entropy pool
pub fn getrandom(buf: &mut [u8], flags: u32) -> isize {
    syscall3(
        nr::GETRANDOM,
        buf.as_mut_ptr() as usize,
        buf.len(),
        flags as usize,
    ) as isize
}

/// Convenience: fill a buffer with random bytes (blocking)
pub fn fill_random(buf: &mut [u8]) -> bool {
    let ret = getrandom(buf, 0);
    ret >= 0 && ret as usize == buf.len()
}
