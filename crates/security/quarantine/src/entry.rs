//! Quarantine entries

use alloc::string::String;
use alloc::vec::Vec;
use core::net::IpAddr;

/// Timestamp (Unix seconds)
pub type Timestamp = u64;

/// Quarantine entry
#[derive(Debug, Clone)]
pub struct QuarantineEntry {
    /// Path in quarantine directory
    pub path: String,
    /// Original path
    pub original_path: String,
    /// Source of the file
    pub source: QuarantineSource,
    /// Quarantine timestamp
    pub timestamp: Timestamp,
    /// Content hash (SHA-256)
    pub hash: [u8; 32],
    /// Current status
    pub status: QuarantineStatus,
    /// File size
    pub size: u64,
    /// MIME type (if detected)
    pub mime_type: Option<String>,
}

impl QuarantineEntry {
    /// Create new entry
    pub fn new(
        path: String,
        original_path: String,
        source: QuarantineSource,
        hash: [u8; 32],
    ) -> Self {
        QuarantineEntry {
            path,
            original_path,
            source,
            timestamp: 0, // Would be set to current time
            hash,
            status: QuarantineStatus::Pending,
            size: 0,
            mime_type: None,
        }
    }

    /// Set file size
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = size;
        self
    }

    /// Set MIME type
    pub fn with_mime_type(mut self, mime_type: String) -> Self {
        self.mime_type = Some(mime_type);
        self
    }

    /// Check if pending
    pub fn is_pending(&self) -> bool {
        matches!(self.status, QuarantineStatus::Pending)
    }

    /// Check if approved
    pub fn is_approved(&self) -> bool {
        matches!(
            self.status,
            QuarantineStatus::Approved(_) | QuarantineStatus::AutoApproved(_)
        )
    }

    /// Hash as hex string
    pub fn hash_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for byte in &self.hash {
            let _ = core::fmt::write(&mut s, format_args!("{:02x}", byte));
        }
        s
    }
}

/// Source of quarantined file
#[derive(Debug, Clone)]
pub enum QuarantineSource {
    /// External USB/storage device
    ExternalMedia {
        /// Device identifier
        device_id: String,
        /// Volume label
        label: Option<String>,
    },
    /// Downloaded from network
    Network {
        /// Remote address
        address: Option<IpAddr>,
        /// URL if available
        url: Option<String>,
    },
    /// Email attachment
    Email {
        /// Sender address
        sender: String,
        /// Subject
        subject: Option<String>,
    },
    /// Bluetooth transfer
    Bluetooth {
        /// Device name
        device_name: String,
    },
    /// Unknown source
    Unknown,
}

impl QuarantineSource {
    /// Create external media source
    pub fn external_media(device_id: String) -> Self {
        QuarantineSource::ExternalMedia {
            device_id,
            label: None,
        }
    }

    /// Create network source
    pub fn network(address: Option<IpAddr>) -> Self {
        QuarantineSource::Network { address, url: None }
    }

    /// Create email source
    pub fn email(sender: String) -> Self {
        QuarantineSource::Email {
            sender,
            subject: None,
        }
    }

    /// Get source type as string
    pub fn source_type(&self) -> &'static str {
        match self {
            QuarantineSource::ExternalMedia { .. } => "external_media",
            QuarantineSource::Network { .. } => "network",
            QuarantineSource::Email { .. } => "email",
            QuarantineSource::Bluetooth { .. } => "bluetooth",
            QuarantineSource::Unknown => "unknown",
        }
    }
}

/// Quarantine status
#[derive(Debug, Clone)]
pub enum QuarantineStatus {
    /// Awaiting user decision
    Pending,
    /// User approved
    Approved(Option<String>),
    /// User rejected
    Rejected,
    /// Auto-approved (signed by trusted key)
    AutoApproved(String),
}

impl QuarantineStatus {
    /// Get status as string
    pub fn as_str(&self) -> &'static str {
        match self {
            QuarantineStatus::Pending => "pending",
            QuarantineStatus::Approved(_) => "approved",
            QuarantineStatus::Rejected => "rejected",
            QuarantineStatus::AutoApproved(_) => "auto_approved",
        }
    }
}
