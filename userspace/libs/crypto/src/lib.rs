// ============================================================================
// oxide-crypto — Cryptographic primitives for OXIDE OS
// ============================================================================
// Pure Rust, no_std, no external deps. Just math, paranoia, and spite.
// — ColdCipher: "Every algorithm here exists because someone, somewhere,
//   decided that trusting strangers over the internet was a good idea."
//
// Implements:
//   - P-256 ECDSA signature verification (FIPS 186-4)
//   - RSA PKCS#1 v1.5 signature verification (RFC 8017)
//   - Big integer arithmetic for RSA (up to 4096-bit)
//
// These are the minimum viable cryptographic operations needed for
// TLS certificate chain validation. Not a general-purpose crypto library.
// If you need encryption, key exchange, or signing — build another crate.

#![no_std]

extern crate alloc;

/// Crypto error types.
/// — ColdCipher: "Every failure mode, catalogued and named. Know thy enemy."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    /// Input data was invalid (wrong length, bad format, etc.)
    InvalidInput,
    /// Key length was wrong for the algorithm
    InvalidKeyLength,
    /// Decryption failed (bad padding, auth tag mismatch, etc.)
    DecryptionFailed,
}

/// Result type for crypto operations. — ColdCipher
pub type CryptoResult<T> = Result<T, CryptoError>;

pub mod aes;
pub mod bigint;
pub mod chacha;
pub mod hkdf;
pub mod hmac;
pub mod p256;
pub mod random;
pub mod rsa;
pub mod sha256;
pub mod sha384;
pub mod sha512;
pub mod x25519;
