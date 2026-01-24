//! HMAC (Hash-based Message Authentication Code)
//!
//! Implementation based on RFC 2104.

use crate::sha512::{sha256, sha512, Sha256, Sha512};

/// HMAC-SHA-512
pub struct HmacSha512 {
    inner: Sha512,
    outer_key: [u8; 128],
}

impl HmacSha512 {
    /// Block size for SHA-512
    const BLOCK_SIZE: usize = 128;

    /// Create new HMAC-SHA-512 with key
    pub fn new(key: &[u8]) -> Self {
        let mut key_block = [0u8; Self::BLOCK_SIZE];

        // If key is longer than block size, hash it first
        if key.len() > Self::BLOCK_SIZE {
            let hashed = sha512(key);
            key_block[..64].copy_from_slice(&hashed);
        } else {
            key_block[..key.len()].copy_from_slice(key);
        }

        // Compute inner and outer padded keys
        let mut inner_key = [0x36u8; Self::BLOCK_SIZE];
        let mut outer_key = [0x5cu8; Self::BLOCK_SIZE];

        for i in 0..Self::BLOCK_SIZE {
            inner_key[i] ^= key_block[i];
            outer_key[i] ^= key_block[i];
        }

        // Initialize inner hash with inner key
        let mut inner = Sha512::new();
        inner.update(&inner_key);

        Self { inner, outer_key }
    }

    /// Update HMAC with data
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Finalize and return the 64-byte MAC
    pub fn finalize(self) -> [u8; 64] {
        // Compute inner hash
        let inner_hash = self.inner.finalize();

        // Compute outer hash: H(outer_key || inner_hash)
        let mut outer = Sha512::new();
        outer.update(&self.outer_key);
        outer.update(&inner_hash);
        outer.finalize()
    }
}

/// HMAC-SHA-256
pub struct HmacSha256 {
    inner: Sha256,
    outer_key: [u8; 64],
}

impl HmacSha256 {
    /// Block size for SHA-256
    const BLOCK_SIZE: usize = 64;

    /// Create new HMAC-SHA-256 with key
    pub fn new(key: &[u8]) -> Self {
        let mut key_block = [0u8; Self::BLOCK_SIZE];

        // If key is longer than block size, hash it first
        if key.len() > Self::BLOCK_SIZE {
            let hashed = sha256(key);
            key_block[..32].copy_from_slice(&hashed);
        } else {
            key_block[..key.len()].copy_from_slice(key);
        }

        // Compute inner and outer padded keys
        let mut inner_key = [0x36u8; Self::BLOCK_SIZE];
        let mut outer_key = [0x5cu8; Self::BLOCK_SIZE];

        for i in 0..Self::BLOCK_SIZE {
            inner_key[i] ^= key_block[i];
            outer_key[i] ^= key_block[i];
        }

        // Initialize inner hash with inner key
        let mut inner = Sha256::new();
        inner.update(&inner_key);

        Self { inner, outer_key }
    }

    /// Update HMAC with data
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Finalize and return the 32-byte MAC
    pub fn finalize(self) -> [u8; 32] {
        // Compute inner hash
        let inner_hash = self.inner.finalize();

        // Compute outer hash: H(outer_key || inner_hash)
        let mut outer = Sha256::new();
        outer.update(&self.outer_key);
        outer.update(&inner_hash);
        outer.finalize()
    }
}

/// Compute HMAC-SHA-512 of data with key
pub fn hmac_sha512(key: &[u8], data: &[u8]) -> [u8; 64] {
    let mut hmac = HmacSha512::new(key);
    hmac.update(data);
    hmac.finalize()
}

/// Compute HMAC-SHA-256 of data with key
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut hmac = HmacSha256::new(key);
    hmac.update(data);
    hmac.finalize()
}

/// Constant-time comparison for MACs
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_sha256_rfc4231_test1() {
        // Test Case 1 from RFC 4231
        let key = [0x0bu8; 20];
        let data = b"Hi There";
        let mac = hmac_sha256(&key, data);
        // Expected: b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7
        assert_eq!(mac[0], 0xb0);
        assert_eq!(mac[1], 0x34);
        assert_eq!(mac[2], 0x4c);
    }

    #[test]
    fn test_hmac_sha512_rfc4231_test1() {
        // Test Case 1 from RFC 4231
        let key = [0x0bu8; 20];
        let data = b"Hi There";
        let mac = hmac_sha512(&key, data);
        // Expected starts with: 87aa7cdea5ef619d4ff0b4241a1d6cb0
        assert_eq!(mac[0], 0x87);
        assert_eq!(mac[1], 0xaa);
        assert_eq!(mac[2], 0x7c);
    }

    #[test]
    fn test_constant_time_eq() {
        let a = [1, 2, 3, 4];
        let b = [1, 2, 3, 4];
        let c = [1, 2, 3, 5];
        assert!(constant_time_eq(&a, &b));
        assert!(!constant_time_eq(&a, &c));
    }
}
