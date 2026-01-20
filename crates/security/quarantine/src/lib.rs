//! File Quarantine System for OXIDE OS
//!
//! Manages quarantine of untrusted files from external sources.

#![no_std]

extern crate alloc;

pub mod entry;
pub mod policy;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::RwLock;

pub use entry::{QuarantineEntry, QuarantineSource, QuarantineStatus};
pub use policy::{QuarantinePolicy, PolicyAction};

/// Quarantine error types
#[derive(Debug, Clone)]
pub enum QuarantineError {
    /// Entry not found
    NotFound,
    /// Already quarantined
    AlreadyQuarantined,
    /// Permission denied
    PermissionDenied,
    /// Storage error
    StorageError,
    /// Invalid hash
    InvalidHash,
}

impl core::fmt::Display for QuarantineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotFound => write!(f, "entry not found"),
            Self::AlreadyQuarantined => write!(f, "already quarantined"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::StorageError => write!(f, "storage error"),
            Self::InvalidHash => write!(f, "invalid hash"),
        }
    }
}

/// Result type for quarantine operations
pub type QuarantineResult<T> = Result<T, QuarantineError>;

/// Quarantine entry ID (content hash)
pub type EntryId = [u8; 32];

/// Quarantine manager
pub struct QuarantineManager {
    /// Quarantine entries
    entries: RwLock<BTreeMap<EntryId, QuarantineEntry>>,
    /// Policy
    policy: QuarantinePolicy,
    /// Quarantine directory path
    quarantine_dir: String,
}

impl QuarantineManager {
    /// Create new quarantine manager
    pub fn new(quarantine_dir: String) -> Self {
        QuarantineManager {
            entries: RwLock::new(BTreeMap::new()),
            policy: QuarantinePolicy::default(),
            quarantine_dir,
        }
    }

    /// Quarantine a file
    pub fn quarantine(
        &self,
        original_path: String,
        source: QuarantineSource,
        hash: EntryId,
    ) -> QuarantineResult<()> {
        // Check if already quarantined
        if self.entries.read().contains_key(&hash) {
            return Err(QuarantineError::AlreadyQuarantined);
        }

        // Create quarantine path
        let mut quarantine_path = self.quarantine_dir.clone();
        quarantine_path.push('/');
        for byte in &hash[..8] {
            let _ = core::fmt::write(&mut quarantine_path, format_args!("{:02x}", byte));
        }

        // Create entry
        let entry = QuarantineEntry::new(quarantine_path, original_path, source, hash);

        // Store entry
        self.entries.write().insert(hash, entry);

        Ok(())
    }

    /// Get quarantine entry
    pub fn get(&self, hash: &EntryId) -> QuarantineResult<QuarantineEntry> {
        self.entries
            .read()
            .get(hash)
            .cloned()
            .ok_or(QuarantineError::NotFound)
    }

    /// Approve quarantined file
    pub fn approve(&self, hash: &EntryId, reason: Option<String>) -> QuarantineResult<()> {
        let mut entries = self.entries.write();
        let entry = entries.get_mut(hash).ok_or(QuarantineError::NotFound)?;
        entry.status = QuarantineStatus::Approved(reason);
        Ok(())
    }

    /// Reject quarantined file
    pub fn reject(&self, hash: &EntryId) -> QuarantineResult<()> {
        let mut entries = self.entries.write();
        let entry = entries.get_mut(hash).ok_or(QuarantineError::NotFound)?;
        entry.status = QuarantineStatus::Rejected;
        Ok(())
    }

    /// Remove from quarantine (after approval)
    pub fn release(&self, hash: &EntryId) -> QuarantineResult<QuarantineEntry> {
        let mut entries = self.entries.write();
        let entry = entries.get(hash).ok_or(QuarantineError::NotFound)?;

        match &entry.status {
            QuarantineStatus::Approved(_) | QuarantineStatus::AutoApproved(_) => {
                let entry = entries.remove(hash).unwrap();
                Ok(entry)
            }
            _ => Err(QuarantineError::PermissionDenied),
        }
    }

    /// Delete quarantined file
    pub fn delete(&self, hash: &EntryId) -> QuarantineResult<()> {
        self.entries
            .write()
            .remove(hash)
            .ok_or(QuarantineError::NotFound)?;
        Ok(())
    }

    /// List all quarantined entries
    pub fn list(&self) -> Vec<QuarantineEntry> {
        self.entries.read().values().cloned().collect()
    }

    /// List pending entries
    pub fn list_pending(&self) -> Vec<QuarantineEntry> {
        self.entries
            .read()
            .values()
            .filter(|e| matches!(e.status, QuarantineStatus::Pending))
            .cloned()
            .collect()
    }

    /// Check if path is quarantined
    pub fn is_quarantined(&self, hash: &EntryId) -> bool {
        self.entries.read().contains_key(hash)
    }

    /// Get quarantine directory
    pub fn quarantine_dir(&self) -> &str {
        &self.quarantine_dir
    }

    /// Set policy
    pub fn set_policy(&mut self, policy: QuarantinePolicy) {
        self.policy = policy;
    }

    /// Get policy
    pub fn policy(&self) -> &QuarantinePolicy {
        &self.policy
    }

    /// Check policy for a source
    pub fn check_policy(&self, source: &QuarantineSource) -> PolicyAction {
        self.policy.check(source)
    }

    /// Count entries by status
    pub fn count_by_status(&self) -> (usize, usize, usize) {
        let entries = self.entries.read();
        let mut pending = 0;
        let mut approved = 0;
        let mut rejected = 0;

        for entry in entries.values() {
            match &entry.status {
                QuarantineStatus::Pending => pending += 1,
                QuarantineStatus::Approved(_) | QuarantineStatus::AutoApproved(_) => approved += 1,
                QuarantineStatus::Rejected => rejected += 1,
            }
        }

        (pending, approved, rejected)
    }
}

impl Default for QuarantineManager {
    fn default() -> Self {
        Self::new(String::from("/var/quarantine"))
    }
}
