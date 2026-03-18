//! SHA-384 hash function
//!
//! FIPS 180-4 compliant. Same compression function as SHA-512, different IV,
//! output truncated to 48 bytes. The middle child of the SHA-2 family.
//! — ColdCipher: "384 bits — for when 256 feels inadequate but 512 feels excessive."

use crate::sha512::Sha512;

/// SHA-384 initial hash values — first 64 bits of fractional parts of
/// square roots of the 9th through 16th primes (23, 29, 31, 37, 41, 43, 47, 53)
const H384: [u64; 8] = [
    0xcbbb9d5dc1059ed8,
    0x629a292a367cd507,
    0x9159015a3070dd17,
    0x152fecd8f70e5939,
    0x67332667ffc00b31,
    0x8eb44a8768581511,
    0xdb0c2e0d64f98fa7,
    0x47b5481dbefa4fa4,
];

/// SHA-384 hasher — wraps SHA-512 with different initial state
/// — ColdCipher: "Same engine, different ignition. The truncation is the feature."
pub struct Sha384 {
    inner: Sha512,
}

impl Default for Sha384 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha384 {
    /// Create a new SHA-384 hasher
    pub fn new() -> Self {
        Self {
            inner: Sha512::with_state(H384),
        }
    }

    /// Update hash with data
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Finalize and return the 48-byte hash
    /// — ColdCipher: "Chop the last 16 bytes. They knew too much anyway."
    pub fn finalize(self) -> [u8; 48] {
        let full = self.inner.finalize();
        let mut result = [0u8; 48];
        result.copy_from_slice(&full[..48]);
        result
    }
}

/// Compute SHA-384 hash of data in one shot
pub fn sha384(data: &[u8]) -> [u8; 48] {
    let mut hasher = Sha384::new();
    hasher.update(data);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha384_empty() {
        let hash = sha384(b"");
        // Known SHA-384 of empty string: 38b060a751ac9638...
        assert_eq!(
            &hash[..8],
            &[0x38, 0xb0, 0x60, 0xa7, 0x51, 0xac, 0x96, 0x38]
        );
    }

    #[test]
    fn test_sha384_abc() {
        let hash = sha384(b"abc");
        // Known SHA-384 of "abc": cb00753f45a35e8b...
        assert_eq!(
            &hash[..8],
            &[0xcb, 0x00, 0x75, 0x3f, 0x45, 0xa3, 0x5e, 0x8b]
        );
    }
}
