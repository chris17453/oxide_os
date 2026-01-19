//! Trust store implementation

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::RwLock;

use crate::{TrustResult, TrustError, Timestamp};
use crate::key::{TrustedKey, KeyId, TrustLevel};
use crate::revoke::RevocationEntry;

/// Trust store
pub struct TrustStore {
    /// Trusted public keys
    keys: RwLock<BTreeMap<KeyId, TrustedKey>>,
    /// Revoked keys
    revoked: RwLock<BTreeMap<KeyId, RevocationEntry>>,
    /// Trust levels
    levels: RwLock<BTreeMap<KeyId, TrustLevel>>,
}

impl TrustStore {
    /// Create new empty trust store
    pub fn new() -> Self {
        TrustStore {
            keys: RwLock::new(BTreeMap::new()),
            revoked: RwLock::new(BTreeMap::new()),
            levels: RwLock::new(BTreeMap::new()),
        }
    }

    /// Add a trusted key
    pub fn add_key(&self, key: TrustedKey, level: TrustLevel) -> TrustResult<()> {
        let key_id = key.key_id();

        // Check if revoked
        if self.revoked.read().contains_key(&key_id) {
            return Err(TrustError::KeyRevoked);
        }

        // Check if exists
        if self.keys.read().contains_key(&key_id) {
            return Err(TrustError::KeyExists);
        }

        // Add key
        self.keys.write().insert(key_id, key);
        self.levels.write().insert(key_id, level);

        Ok(())
    }

    /// Get a trusted key
    pub fn get_key(&self, key_id: &KeyId) -> TrustResult<TrustedKey> {
        // Check if revoked
        if self.revoked.read().contains_key(key_id) {
            return Err(TrustError::KeyRevoked);
        }

        self.keys
            .read()
            .get(key_id)
            .cloned()
            .ok_or(TrustError::KeyNotFound)
    }

    /// Get trust level for a key
    pub fn get_trust_level(&self, key_id: &KeyId) -> TrustResult<TrustLevel> {
        // Check if revoked
        if self.revoked.read().contains_key(key_id) {
            return Err(TrustError::KeyRevoked);
        }

        self.levels
            .read()
            .get(key_id)
            .copied()
            .ok_or(TrustError::KeyNotFound)
    }

    /// Set trust level for a key
    pub fn set_trust_level(&self, key_id: &KeyId, level: TrustLevel) -> TrustResult<()> {
        if !self.keys.read().contains_key(key_id) {
            return Err(TrustError::KeyNotFound);
        }

        self.levels.write().insert(*key_id, level);
        Ok(())
    }

    /// Remove a key
    pub fn remove_key(&self, key_id: &KeyId) -> TrustResult<()> {
        if self.keys.write().remove(key_id).is_none() {
            return Err(TrustError::KeyNotFound);
        }
        self.levels.write().remove(key_id);
        Ok(())
    }

    /// Revoke a key
    pub fn revoke_key(&self, key_id: &KeyId, reason: &str, timestamp: Timestamp) -> TrustResult<()> {
        let entry = RevocationEntry::new(*key_id, reason, timestamp);
        self.revoked.write().insert(*key_id, entry);
        Ok(())
    }

    /// Check if key is revoked
    pub fn is_revoked(&self, key_id: &KeyId) -> bool {
        self.revoked.read().contains_key(key_id)
    }

    /// Check if key is trusted
    pub fn is_trusted(&self, key_id: &KeyId) -> bool {
        if self.is_revoked(key_id) {
            return false;
        }

        if let Ok(level) = self.get_trust_level(key_id) {
            !matches!(level, TrustLevel::Untrusted)
        } else {
            false
        }
    }

    /// Verify a signature against trusted keys
    pub fn verify_signature(
        &self,
        message: &[u8],
        signature: &crypto::Signature,
        key_id: &KeyId,
    ) -> TrustResult<TrustLevel> {
        // Check if revoked
        if self.is_revoked(key_id) {
            return Err(TrustError::KeyRevoked);
        }

        // Get key
        let key = self.get_key(key_id)?;

        // Check expiry
        let now = current_timestamp();
        if let Some(expires) = key.expires {
            if now > expires {
                return Err(TrustError::KeyExpired);
            }
        }

        // Verify signature
        if !crypto::ed25519::verify(message, signature, &key.public_key) {
            return Err(TrustError::VerificationFailed);
        }

        // Return trust level
        self.get_trust_level(key_id)
    }

    /// List all trusted keys
    pub fn list_keys(&self) -> Vec<(KeyId, TrustedKey, TrustLevel)> {
        let keys = self.keys.read();
        let levels = self.levels.read();

        keys.iter()
            .filter_map(|(id, key)| {
                if self.is_revoked(id) {
                    None
                } else {
                    let level = levels.get(id).copied().unwrap_or(TrustLevel::Untrusted);
                    Some((*id, key.clone(), level))
                }
            })
            .collect()
    }

    /// List revoked keys
    pub fn list_revoked(&self) -> Vec<RevocationEntry> {
        self.revoked.read().values().cloned().collect()
    }

    /// Get number of trusted keys
    pub fn key_count(&self) -> usize {
        self.keys.read().len()
    }

    /// Get number of revoked keys
    pub fn revoked_count(&self) -> usize {
        self.revoked.read().len()
    }
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current timestamp (would be from kernel)
fn current_timestamp() -> Timestamp {
    0 // Placeholder
}
