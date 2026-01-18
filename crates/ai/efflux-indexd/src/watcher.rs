//! Filesystem watcher

use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::VecDeque;
use spin::Mutex;

/// Event kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// File created
    Create,
    /// File modified
    Modify,
    /// File deleted
    Delete,
    /// File renamed
    Rename,
}

/// Filesystem event
#[derive(Debug, Clone)]
pub struct FsEvent {
    /// Event kind
    pub kind: EventKind,
    /// File path
    pub path: String,
    /// Old path (for renames)
    pub old_path: Option<String>,
}

/// Filesystem watcher
pub struct FsWatcher {
    /// Paths being watched
    watched_paths: Vec<String>,
    /// Event queue
    events: Mutex<VecDeque<FsEvent>>,
    /// Is running
    running: bool,
}

impl FsWatcher {
    /// Create new watcher
    pub fn new() -> Self {
        FsWatcher {
            watched_paths: Vec::new(),
            events: Mutex::new(VecDeque::new()),
            running: false,
        }
    }

    /// Add path to watch
    pub fn watch(&mut self, path: String) {
        if !self.watched_paths.contains(&path) {
            self.watched_paths.push(path);
        }
    }

    /// Remove path from watch
    pub fn unwatch(&mut self, path: &str) {
        self.watched_paths.retain(|p| p != path);
    }

    /// Start watching
    pub fn start(&mut self) {
        self.running = true;
        // In a real implementation, this would set up inotify/kqueue/etc.
    }

    /// Stop watching
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Poll for events
    pub fn poll(&self) -> Option<FsEvent> {
        self.events.lock().pop_front()
    }

    /// Push an event (for testing or kernel integration)
    pub fn push_event(&self, event: FsEvent) {
        self.events.lock().push_back(event);
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get watched paths
    pub fn watched_paths(&self) -> &[String] {
        &self.watched_paths
    }
}

impl Default for FsWatcher {
    fn default() -> Self {
        Self::new()
    }
}
