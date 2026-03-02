//! — ColdCipher: Random bytes from the kernel's entropy pool.
pub fn fill_bytes(bytes: &mut [u8]) {
    oxide_rt::random::fill_random(bytes);
}
