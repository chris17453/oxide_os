//! Indexing Daemon for Semantic File Search
//!
//! Watches filesystem and maintains vector index for semantic search.

#![no_std]

extern crate alloc;

pub mod config;
pub mod daemon;
pub mod ipc;
pub mod queue;
pub mod watcher;

pub use config::IndexConfig;
pub use daemon::IndexDaemon;
pub use ipc::{IpcServer, SearchRequest, SearchResponse};
pub use queue::IndexQueue;
pub use watcher::{EventKind, FsEvent, FsWatcher};

/// Extended attribute names for indexed metadata
pub mod xattr {
    /// Embedding vector (compressed)
    pub const EMBED: &str = "user.oxide.embed";
    /// Content hash for change detection
    pub const HASH: &str = "user.oxide.hash";
    /// Last indexed timestamp
    pub const INDEXED: &str = "user.oxide.indexed";
    /// Extracted text summary
    pub const SUMMARY: &str = "user.oxide.summary";
    /// Auto-generated tags
    pub const TAGS: &str = "user.oxide.tags";
}

/// Result type for indexd operations
pub type IndexResult<T> = Result<T, IndexError>;

/// Error type for indexd operations
#[derive(Debug, Clone)]
pub enum IndexError {
    /// File not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// IO error
    IoError,
    /// Index error
    IndexError,
    /// Embedding error
    EmbedError,
    /// Configuration error
    ConfigError,
    /// IPC error
    IpcError,
}

impl core::fmt::Display for IndexError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotFound => write!(f, "file not found"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::IoError => write!(f, "IO error"),
            Self::IndexError => write!(f, "index error"),
            Self::EmbedError => write!(f, "embedding error"),
            Self::ConfigError => write!(f, "configuration error"),
            Self::IpcError => write!(f, "IPC error"),
        }
    }
}
