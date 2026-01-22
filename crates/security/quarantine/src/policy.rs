//! Quarantine policies

use crate::entry::QuarantineSource;
use alloc::string::String;
use alloc::vec::Vec;

/// Policy action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyAction {
    /// Quarantine the file
    Quarantine,
    /// Allow without quarantine
    Allow,
    /// Block completely
    Block,
    /// Prompt user
    Prompt,
}

/// Quarantine policy
#[derive(Debug, Clone)]
pub struct QuarantinePolicy {
    /// Default action for external media
    pub external_media: PolicyAction,
    /// Default action for network downloads
    pub network: PolicyAction,
    /// Default action for email attachments
    pub email: PolicyAction,
    /// Default action for bluetooth transfers
    pub bluetooth: PolicyAction,
    /// Default action for unknown sources
    pub unknown: PolicyAction,
    /// Allowed file extensions (bypass quarantine)
    pub allowed_extensions: Vec<String>,
    /// Blocked file extensions (always block)
    pub blocked_extensions: Vec<String>,
    /// Maximum file size for auto-allow (bytes)
    pub max_auto_allow_size: u64,
}

impl Default for QuarantinePolicy {
    fn default() -> Self {
        QuarantinePolicy {
            external_media: PolicyAction::Quarantine,
            network: PolicyAction::Quarantine,
            email: PolicyAction::Quarantine,
            bluetooth: PolicyAction::Quarantine,
            unknown: PolicyAction::Quarantine,
            allowed_extensions: Vec::new(),
            blocked_extensions: alloc::vec![
                String::from("exe"),
                String::from("bat"),
                String::from("cmd"),
                String::from("ps1"),
                String::from("vbs"),
                String::from("scr"),
            ],
            max_auto_allow_size: 0, // No auto-allow by default
        }
    }
}

impl QuarantinePolicy {
    /// Create strict policy (quarantine everything)
    pub fn strict() -> Self {
        QuarantinePolicy {
            external_media: PolicyAction::Quarantine,
            network: PolicyAction::Quarantine,
            email: PolicyAction::Quarantine,
            bluetooth: PolicyAction::Quarantine,
            unknown: PolicyAction::Block,
            ..Default::default()
        }
    }

    /// Create permissive policy (allow most things)
    pub fn permissive() -> Self {
        QuarantinePolicy {
            external_media: PolicyAction::Prompt,
            network: PolicyAction::Allow,
            email: PolicyAction::Quarantine,
            bluetooth: PolicyAction::Prompt,
            unknown: PolicyAction::Quarantine,
            max_auto_allow_size: 1024 * 1024, // 1MB
            ..Default::default()
        }
    }

    /// Check policy for a source
    pub fn check(&self, source: &QuarantineSource) -> PolicyAction {
        match source {
            QuarantineSource::ExternalMedia { .. } => self.external_media,
            QuarantineSource::Network { .. } => self.network,
            QuarantineSource::Email { .. } => self.email,
            QuarantineSource::Bluetooth { .. } => self.bluetooth,
            QuarantineSource::Unknown => self.unknown,
        }
    }

    /// Check if extension is blocked
    pub fn is_blocked_extension(&self, ext: &str) -> bool {
        self.blocked_extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Check if extension is allowed
    pub fn is_allowed_extension(&self, ext: &str) -> bool {
        self.allowed_extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Add allowed extension
    pub fn allow_extension(&mut self, ext: String) {
        if !self.allowed_extensions.contains(&ext) {
            self.allowed_extensions.push(ext);
        }
    }

    /// Add blocked extension
    pub fn block_extension(&mut self, ext: String) {
        if !self.blocked_extensions.contains(&ext) {
            self.blocked_extensions.push(ext);
        }
    }
}

/// Policy rule
#[derive(Debug, Clone)]
pub struct PolicyRule {
    /// Rule name
    pub name: String,
    /// Condition
    pub condition: PolicyCondition,
    /// Action
    pub action: PolicyAction,
    /// Priority (higher = checked first)
    pub priority: u32,
}

/// Policy condition
#[derive(Debug, Clone)]
pub enum PolicyCondition {
    /// Source type matches
    SourceType(String),
    /// File extension matches
    Extension(String),
    /// File size less than
    SizeLessThan(u64),
    /// File size greater than
    SizeGreaterThan(u64),
    /// Signed by trusted key
    SignedByTrusted,
    /// All conditions match
    All(Vec<PolicyCondition>),
    /// Any condition matches
    Any(Vec<PolicyCondition>),
}
