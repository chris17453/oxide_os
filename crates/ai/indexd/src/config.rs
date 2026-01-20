//! Indexd configuration

use alloc::string::String;
use alloc::vec::Vec;

/// Indexd configuration
#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// Paths to watch
    pub watch_paths: Vec<String>,
    /// Paths to exclude
    pub exclude_paths: Vec<String>,
    /// File extensions to index
    pub include_extensions: Vec<String>,
    /// File extensions to exclude
    pub exclude_extensions: Vec<String>,
    /// Maximum file size to index (bytes)
    pub max_file_size: u64,
    /// Index database path
    pub db_path: String,
    /// IPC socket path
    pub socket_path: String,
    /// Number of worker threads
    pub num_workers: usize,
    /// Batch size for indexing
    pub batch_size: usize,
    /// Embedding model name
    pub model_name: String,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            watch_paths: alloc::vec![String::from("/home")],
            exclude_paths: alloc::vec![
                String::from("/home/*/.cache"),
                String::from("/home/*/.local/share/Trash"),
            ],
            include_extensions: alloc::vec![
                String::from("txt"), String::from("md"),
                String::from("rs"), String::from("py"), String::from("js"),
                String::from("html"), String::from("json"), String::from("xml"),
                String::from("c"), String::from("cpp"), String::from("h"),
            ],
            exclude_extensions: alloc::vec![
                String::from("o"), String::from("so"), String::from("a"),
                String::from("pyc"), String::from("class"),
            ],
            max_file_size: 10 * 1024 * 1024, // 10MB
            db_path: String::from("/var/lib/oxide/index"),
            socket_path: String::from("/run/indexd.sock"),
            num_workers: 4,
            batch_size: 100,
            model_name: String::from("all-MiniLM-L6-v2"),
        }
    }
}

impl IndexConfig {
    /// Check if a path should be indexed
    pub fn should_index(&self, path: &str) -> bool {
        // Check exclusions first
        for exclude in &self.exclude_paths {
            if Self::glob_match(exclude, path) {
                return false;
            }
        }

        // Check if in watch paths
        let in_watch = self.watch_paths.iter().any(|p| path.starts_with(p));
        if !in_watch {
            return false;
        }

        // Check extension
        if let Some(ext) = Self::get_extension(path) {
            if !self.exclude_extensions.is_empty() {
                if self.exclude_extensions.iter().any(|e| e == ext) {
                    return false;
                }
            }
            if !self.include_extensions.is_empty() {
                return self.include_extensions.iter().any(|e| e == ext);
            }
        }

        true
    }

    /// Simple glob matching
    fn glob_match(pattern: &str, path: &str) -> bool {
        if pattern.contains('*') {
            // Very basic glob: just handle leading/trailing *
            if pattern.starts_with('*') && pattern.ends_with('*') {
                let inner = &pattern[1..pattern.len()-1];
                path.contains(inner)
            } else if pattern.starts_with('*') {
                path.ends_with(&pattern[1..])
            } else if pattern.ends_with('*') {
                path.starts_with(&pattern[..pattern.len()-1])
            } else {
                // Middle * - split and check
                let parts: Vec<_> = pattern.split('*').collect();
                if parts.len() == 2 {
                    path.starts_with(parts[0]) && path.ends_with(parts[1])
                } else {
                    pattern == path
                }
            }
        } else {
            pattern == path
        }
    }

    /// Get file extension
    fn get_extension(path: &str) -> Option<&str> {
        path.rsplit('.').next().filter(|&ext| ext != path)
    }
}
