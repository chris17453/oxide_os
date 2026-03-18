//! HKDF (HMAC-based Key Derivation Function)
//!
//! RFC 5869 compliant. The two-phase extract-then-expand dance that turns
//! your shared secret into actual usable key material.
//! — ColdCipher: "Extract entropy from chaos, expand it into purpose."

extern crate alloc;
use alloc::vec::Vec;

use crate::hmac::{hmac_sha256, hmac_sha384, HmacSha256, HmacSha384};

// ============================================================================
// HKDF-SHA-256 — for TLS_AES_128_GCM_SHA256 and TLS_CHACHA20_POLY1305_SHA256
// ============================================================================

/// HKDF-Extract (SHA-256): derive a pseudorandom key from input keying material
///
/// PRK = HMAC-Hash(salt, IKM)
///
/// If salt is empty, uses a zero-filled key of HashLen (32) bytes per RFC 5869.
/// — ColdCipher: "Phase one — squeeze the entropy out of raw key material."
pub fn hkdf_extract_sha256(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let effective_salt = if salt.is_empty() {
        &[0u8; 32] as &[u8]
    } else {
        salt
    };
    hmac_sha256(effective_salt, ikm)
}

/// HKDF-Expand (SHA-256): expand a pseudorandom key into output keying material
///
/// OKM = T(1) || T(2) || ... where T(i) = HMAC-Hash(PRK, T(i-1) || info || i)
///
/// Maximum output length: 255 * HashLen (8160 bytes).
/// — ColdCipher: "Phase two — stretch 32 bytes of entropy into however many you need."
pub fn hkdf_expand_sha256(prk: &[u8], info: &[u8], length: usize) -> Vec<u8> {
    // RFC 5869 Section 2.3: L <= 255*HashLen
    assert!(length <= 255 * 32, "HKDF-Expand: requested length too large");
    assert!(length > 0, "HKDF-Expand: requested length must be > 0");

    let n = (length + 31) / 32; // ceil(L/HashLen)
    let mut okm = Vec::with_capacity(length);
    let mut t_prev: [u8; 32] = [0u8; 32];
    let mut t_prev_len: usize = 0; // T(0) = empty string

    for i in 1..=n {
        // T(i) = HMAC-Hash(PRK, T(i-1) || info || i)
        let mut hmac = HmacSha256::new(prk);
        if t_prev_len > 0 {
            hmac.update(&t_prev[..t_prev_len]);
        }
        hmac.update(info);
        hmac.update(&[i as u8]);
        t_prev = hmac.finalize();
        t_prev_len = 32;

        // Append to output, respecting final length
        let remaining = length - okm.len();
        let to_copy = core::cmp::min(remaining, 32);
        okm.extend_from_slice(&t_prev[..to_copy]);
    }

    okm
}

// ============================================================================
// HKDF-SHA-384 — for TLS_AES_256_GCM_SHA384
// ============================================================================

/// HKDF-Extract (SHA-384): derive a pseudorandom key from input keying material
///
/// PRK = HMAC-SHA384(salt, IKM)
/// — ColdCipher: "384-bit extract. For when your cipher suite demands the bigger hash."
pub fn hkdf_extract_sha384(salt: &[u8], ikm: &[u8]) -> [u8; 48] {
    let effective_salt = if salt.is_empty() {
        &[0u8; 48] as &[u8]
    } else {
        salt
    };
    hmac_sha384(effective_salt, ikm)
}

/// HKDF-Expand (SHA-384): expand a pseudorandom key into output keying material
///
/// Maximum output length: 255 * 48 (12240 bytes).
/// — ColdCipher: "48-byte blocks of derived keys. The luxury suite of KDFs."
pub fn hkdf_expand_sha384(prk: &[u8], info: &[u8], length: usize) -> Vec<u8> {
    // RFC 5869 Section 2.3: L <= 255*HashLen
    assert!(
        length <= 255 * 48,
        "HKDF-Expand-SHA384: requested length too large"
    );
    assert!(
        length > 0,
        "HKDF-Expand-SHA384: requested length must be > 0"
    );

    let hash_len = 48;
    let n = (length + hash_len - 1) / hash_len; // ceil(L/HashLen)
    let mut okm = Vec::with_capacity(length);
    let mut t_prev: [u8; 48] = [0u8; 48];
    let mut t_prev_len: usize = 0; // T(0) = empty string

    for i in 1..=n {
        // T(i) = HMAC-Hash(PRK, T(i-1) || info || i)
        let mut hmac = HmacSha384::new(prk);
        if t_prev_len > 0 {
            hmac.update(&t_prev[..t_prev_len]);
        }
        hmac.update(info);
        hmac.update(&[i as u8]);
        t_prev = hmac.finalize();
        t_prev_len = hash_len;

        // Append to output, respecting final length
        let remaining = length - okm.len();
        let to_copy = core::cmp::min(remaining, hash_len);
        okm.extend_from_slice(&t_prev[..to_copy]);
    }

    okm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hkdf_sha256_rfc5869_test1() {
        // RFC 5869 Test Case 1
        let ikm = [0x0bu8; 22];
        let salt = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        let info = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];

        let prk = hkdf_extract_sha256(&salt, &ikm);
        // Expected PRK: 077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5
        assert_eq!(prk[0], 0x07);
        assert_eq!(prk[1], 0x77);
        assert_eq!(prk[2], 0x09);

        let okm = hkdf_expand_sha256(&prk, &info, 42);
        // Expected OKM: 3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865
        assert_eq!(okm.len(), 42);
        assert_eq!(okm[0], 0x3c);
        assert_eq!(okm[1], 0xb2);
        assert_eq!(okm[2], 0x5f);
    }

    #[test]
    fn test_hkdf_sha256_empty_salt() {
        // RFC 5869 specifies empty salt should use HashLen zero bytes
        let ikm = [0x0bu8; 22];
        let prk = hkdf_extract_sha256(&[], &ikm);
        // Should produce a valid 32-byte PRK
        assert_eq!(prk.len(), 32);
    }

    #[test]
    fn test_hkdf_sha384_basic() {
        // Basic test — extract and expand should produce deterministic output
        let ikm = [0x0bu8; 22];
        let salt = [0x00u8; 48];
        let info = [0xf0u8; 10];

        let prk = hkdf_extract_sha384(&salt, &ikm);
        assert_eq!(prk.len(), 48);

        let okm = hkdf_expand_sha384(&prk, &info, 64);
        assert_eq!(okm.len(), 64);
    }

    #[test]
    fn test_hkdf_expand_exact_block_boundary() {
        // Request exactly one block (32 bytes)
        let prk = [0xaau8; 32];
        let info = b"test";
        let okm = hkdf_expand_sha256(&prk, info, 32);
        assert_eq!(okm.len(), 32);
    }
}
