//! Key revocation

use alloc::string::String;
use crate::{Timestamp, KeyId};

/// Revocation entry
#[derive(Debug, Clone)]
pub struct RevocationEntry {
    /// Key ID
    pub key_id: KeyId,
    /// Reason for revocation
    pub reason: String,
    /// Revocation timestamp
    pub revoked_at: Timestamp,
    /// Revoked by (key ID of revoking authority)
    pub revoked_by: Option<KeyId>,
}

impl RevocationEntry {
    /// Create new revocation entry
    pub fn new(key_id: KeyId, reason: &str, timestamp: Timestamp) -> Self {
        RevocationEntry {
            key_id,
            reason: String::from(reason),
            revoked_at: timestamp,
            revoked_by: None,
        }
    }

    /// Set revoking authority
    pub fn with_authority(mut self, authority: KeyId) -> Self {
        self.revoked_by = Some(authority);
        self
    }
}

/// Revocation reason codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationReason {
    /// Key compromised
    Compromised,
    /// Key superseded by new key
    Superseded,
    /// Affiliation changed
    AffiliationChanged,
    /// No longer used
    NoLongerUsed,
    /// Unspecified reason
    Unspecified,
}

impl RevocationReason {
    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            RevocationReason::Compromised => "key_compromised",
            RevocationReason::Superseded => "key_superseded",
            RevocationReason::AffiliationChanged => "affiliation_changed",
            RevocationReason::NoLongerUsed => "no_longer_used",
            RevocationReason::Unspecified => "unspecified",
        }
    }
}
