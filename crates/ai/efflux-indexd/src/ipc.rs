//! IPC interface for search requests

use alloc::string::String;
use alloc::vec::Vec;

/// Search request
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// Natural language query
    pub query: String,
    /// Maximum results
    pub limit: usize,
    /// Path prefix filter
    pub path_prefix: Option<String>,
    /// File type filters
    pub file_types: Vec<String>,
}

impl SearchRequest {
    /// Create new search request
    pub fn new(query: String, limit: usize) -> Self {
        SearchRequest {
            query,
            limit,
            path_prefix: None,
            file_types: Vec::new(),
        }
    }
}

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// File path
    pub path: String,
    /// Similarity score (0-1)
    pub score: f32,
    /// Text snippet
    pub snippet: Option<String>,
    /// File size
    pub size: u64,
    /// Last modified timestamp
    pub modified: u64,
}

/// Search response
#[derive(Debug, Clone)]
pub struct SearchResponse {
    /// Results
    pub results: Vec<SearchResult>,
    /// Total matches found
    pub total: usize,
    /// Search duration in milliseconds
    pub duration_ms: u64,
}

impl SearchResponse {
    /// Create empty response
    pub fn empty() -> Self {
        SearchResponse {
            results: Vec::new(),
            total: 0,
            duration_ms: 0,
        }
    }
}

/// IPC server for handling search requests
pub struct IpcServer {
    /// Socket path
    socket_path: String,
    /// Is running
    running: bool,
}

impl IpcServer {
    /// Create new IPC server
    pub fn new(socket_path: String) -> Self {
        IpcServer {
            socket_path,
            running: false,
        }
    }

    /// Start server
    pub fn start(&mut self) {
        self.running = true;
        // In real implementation: create Unix socket and listen
    }

    /// Stop server
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Get socket path
    pub fn socket_path(&self) -> &str {
        &self.socket_path
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.running
    }
}
