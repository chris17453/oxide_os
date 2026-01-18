//! Key management

use alloc::string::String;
use efflux_crypto::PublicKey;
use crate::Timestamp;

/// Key identifier (SHA-256 hash of public key)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyId(pub [u8; 32]);

impl KeyId {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        KeyId(*bytes)
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Format as hex string
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for byte in &self.0 {
            let _ = core::fmt::write(&mut s, format_args!("{:02x}", byte));
        }
        s
    }

    /// Short form (first 8 hex chars)
    pub fn short(&self) -> String {
        let mut s = String::with_capacity(8);
        for byte in &self.0[..4] {
            let _ = core::fmt::write(&mut s, format_args!("{:02x}", byte));
        }
        s
    }
}

/// Trust level for a key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustLevel {
    /// System key (OS vendor, full trust)
    System,
    /// User imported key (prompted on first use)
    User,
    /// One-time trust
    Once,
    /// Explicitly untrusted
    Untrusted,
}

impl TrustLevel {
    /// Check if this level allows execution
    pub fn allows_execute(&self) -> bool {
        matches!(self, TrustLevel::System | TrustLevel::User)
    }

    /// Check if this level is fully trusted
    pub fn is_full_trust(&self) -> bool {
        matches!(self, TrustLevel::System)
    }
}

/// A trusted public key
#[derive(Debug, Clone)]
pub struct TrustedKey {
    /// Ed25519 public key
    pub public_key: PublicKey,
    /// Display name
    pub name: String,
    /// Email address
    pub email: Option<String>,
    /// Fingerprint (same as key ID)
    pub fingerprint: [u8; 32],
    /// Creation timestamp
    pub created: Timestamp,
    /// Expiration timestamp
    pub expires: Option<Timestamp>,
    /// Comment
    pub comment: Option<String>,
}

impl TrustedKey {
    /// Create new trusted key
    pub fn new(public_key: PublicKey, name: String) -> Self {
        let fingerprint = public_key.key_id();
        TrustedKey {
            public_key,
            name,
            email: None,
            fingerprint,
            created: 0, // Would be set to current time
            expires: None,
            comment: None,
        }
    }

    /// Set email
    pub fn with_email(mut self, email: String) -> Self {
        self.email = Some(email);
        self
    }

    /// Set expiration
    pub fn with_expiry(mut self, expires: Timestamp) -> Self {
        self.expires = Some(expires);
        self
    }

    /// Set comment
    pub fn with_comment(mut self, comment: String) -> Self {
        self.comment = Some(comment);
        self
    }

    /// Get key ID
    pub fn key_id(&self) -> KeyId {
        KeyId(self.fingerprint)
    }

    /// Check if expired
    pub fn is_expired(&self, now: Timestamp) -> bool {
        if let Some(expires) = self.expires {
            now > expires
        } else {
            false
        }
    }

    /// Fingerprint as hex string
    pub fn fingerprint_hex(&self) -> String {
        self.key_id().to_hex()
    }
}
