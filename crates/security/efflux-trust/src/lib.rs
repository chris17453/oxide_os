//! Trust Store and Key Management for EFFLUX OS
//!
//! Manages trusted public keys, revocation, and trust levels.

#![no_std]

extern crate alloc;

pub mod store;
pub mod key;
pub mod revoke;
pub mod share;

pub use store::TrustStore;
pub use key::{TrustedKey, KeyId, TrustLevel};
pub use revoke::RevocationEntry;
pub use share::{TrustExport, ExportFormat};

use alloc::string::String;

/// Trust store error types
#[derive(Debug, Clone)]
pub enum TrustError {
    /// Key not found
    KeyNotFound,
    /// Key already exists
    KeyExists,
    /// Key is revoked
    KeyRevoked,
    /// Invalid key format
    InvalidKey,
    /// Storage error
    StorageError,
    /// Verification failed
    VerificationFailed,
    /// Expired key
    KeyExpired,
}

impl core::fmt::Display for TrustError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::KeyNotFound => write!(f, "key not found"),
            Self::KeyExists => write!(f, "key already exists"),
            Self::KeyRevoked => write!(f, "key is revoked"),
            Self::InvalidKey => write!(f, "invalid key format"),
            Self::StorageError => write!(f, "storage error"),
            Self::VerificationFailed => write!(f, "verification failed"),
            Self::KeyExpired => write!(f, "key has expired"),
        }
    }
}

/// Result type for trust operations
pub type TrustResult<T> = Result<T, TrustError>;

/// Timestamp (Unix seconds)
pub type Timestamp = u64;

/// Trust store configuration
#[derive(Debug, Clone)]
pub struct TrustConfig {
    /// Path to trust store directory
    pub store_path: String,
    /// Allow system keys
    pub allow_system: bool,
    /// Allow user imports
    pub allow_user_import: bool,
    /// Require confirmation for new keys
    pub require_confirmation: bool,
}

impl Default for TrustConfig {
    fn default() -> Self {
        TrustConfig {
            store_path: String::from("/etc/efflux/trust"),
            allow_system: true,
            allow_user_import: true,
            require_confirmation: true,
        }
    }
}
