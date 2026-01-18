//! Cryptographic Primitives for EFFLUX OS
//!
//! Provides Ed25519 signing, AES-256-GCM and ChaCha20-Poly1305 encryption,
//! X25519 key exchange, and Argon2id password hashing.

#![no_std]

extern crate alloc;

pub mod ed25519;
pub mod aes;
pub mod chacha;
pub mod x25519;
pub mod argon2;
pub mod random;

pub use ed25519::{SecretKey, PublicKey, Signature, Keypair};
pub use aes::{Aes256Gcm, AesKey, AesNonce};
pub use chacha::{ChaCha20Poly1305, ChaChaKey, ChaChaNonce};
pub use x25519::{X25519SecretKey, X25519PublicKey, SharedSecret};
pub use argon2::{Argon2Params, argon2id_hash};

/// Cryptographic error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    /// Invalid key length
    InvalidKeyLength,
    /// Invalid nonce length
    InvalidNonceLength,
    /// Invalid signature
    InvalidSignature,
    /// Decryption failed (authentication tag mismatch)
    DecryptionFailed,
    /// Random number generation failed
    RandomFailed,
    /// Invalid input
    InvalidInput,
}

impl core::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidKeyLength => write!(f, "invalid key length"),
            Self::InvalidNonceLength => write!(f, "invalid nonce length"),
            Self::InvalidSignature => write!(f, "invalid signature"),
            Self::DecryptionFailed => write!(f, "decryption failed"),
            Self::RandomFailed => write!(f, "random generation failed"),
            Self::InvalidInput => write!(f, "invalid input"),
        }
    }
}

/// Result type for crypto operations
pub type CryptoResult<T> = Result<T, CryptoError>;

/// File signature format
#[repr(C)]
#[derive(Debug, Clone)]
pub struct FileSignature {
    /// Magic number "ESIG"
    pub magic: [u8; 4],
    /// Version (currently 1)
    pub version: u32,
    /// Algorithm (1 = Ed25519)
    pub algorithm: u32,
    /// Key ID (SHA-256 of public key)
    pub key_id: [u8; 32],
    /// Timestamp (Unix timestamp)
    pub timestamp: u64,
    /// Ed25519 signature
    pub signature: [u8; 64],
}

impl FileSignature {
    /// Magic bytes
    pub const MAGIC: [u8; 4] = *b"ESIG";
    /// Current version
    pub const VERSION: u32 = 1;
    /// Ed25519 algorithm ID
    pub const ALG_ED25519: u32 = 1;

    /// Create new file signature
    pub fn new(key_id: [u8; 32], timestamp: u64, signature: [u8; 64]) -> Self {
        FileSignature {
            magic: Self::MAGIC,
            version: Self::VERSION,
            algorithm: Self::ALG_ED25519,
            key_id,
            timestamp,
            signature,
        }
    }

    /// Check if magic is valid
    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version == Self::VERSION
    }
}

/// Encrypted file header
#[repr(C)]
#[derive(Debug, Clone)]
pub struct EncryptedFileHeader {
    /// Magic number "EENC"
    pub magic: [u8; 4],
    /// Version (currently 1)
    pub version: u32,
    /// Algorithm (1 = AES-256-GCM, 2 = ChaCha20-Poly1305)
    pub algorithm: u32,
    /// Random nonce
    pub nonce: [u8; 12],
    /// Key derivation method (1 = password, 2 = key_id)
    pub key_derivation: u32,
    /// Salt for password derivation (if key_derivation == 1)
    pub salt: [u8; 32],
    /// Argon2 iterations (if key_derivation == 1)
    pub iterations: u32,
    /// Ciphertext length
    pub ciphertext_len: u64,
}

impl EncryptedFileHeader {
    /// Magic bytes
    pub const MAGIC: [u8; 4] = *b"EENC";
    /// Current version
    pub const VERSION: u32 = 1;
    /// AES-256-GCM algorithm ID
    pub const ALG_AES_GCM: u32 = 1;
    /// ChaCha20-Poly1305 algorithm ID
    pub const ALG_CHACHA: u32 = 2;
    /// Password-based key derivation
    pub const KDF_PASSWORD: u32 = 1;
    /// Public key-based key derivation
    pub const KDF_PUBKEY: u32 = 2;

    /// Check if magic is valid
    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version == Self::VERSION
    }
}
