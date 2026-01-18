//! Main indexing daemon

use alloc::string::String;
use alloc::vec::Vec;

use efflux_hnsw::{HnswIndex, HnswConfig, FileId};
use efflux_embed::{EmbeddingModel, SimpleTfIdfModel, EmbeddingConfig, ContentExtractor, FileType};

use crate::{IndexResult, IndexError, IndexConfig};
use crate::watcher::{FsWatcher, FsEvent, EventKind};
use crate::queue::{IndexQueue, QueueItem};
use crate::ipc::{IpcServer, SearchRequest, SearchResponse, SearchResult};

/// Main indexing daemon
pub struct IndexDaemon {
    /// Configuration
    config: IndexConfig,
    /// Vector index
    index: HnswIndex,
    /// Embedding model
    model: SimpleTfIdfModel,
    /// Filesystem watcher
    watcher: FsWatcher,
    /// Indexing queue
    queue: IndexQueue,
    /// IPC server
    ipc: IpcServer,
    /// File ID counter
    next_file_id: u64,
    /// Path to file ID mapping
    path_to_id: alloc::collections::BTreeMap<String, FileId>,
    /// File ID to path mapping
    id_to_path: alloc::collections::BTreeMap<FileId, String>,
}

impl IndexDaemon {
    /// Create new daemon with configuration
    pub fn new(config: IndexConfig) -> Self {
        let hnsw_config = HnswConfig::default();
        let embed_config = EmbeddingConfig::default();

        IndexDaemon {
            ipc: IpcServer::new(config.socket_path.clone()),
            config,
            index: HnswIndex::new(hnsw_config),
            model: SimpleTfIdfModel::new(embed_config),
            watcher: FsWatcher::new(),
            queue: IndexQueue::new(),
            next_file_id: 1,
            path_to_id: alloc::collections::BTreeMap::new(),
            id_to_path: alloc::collections::BTreeMap::new(),
        }
    }

    /// Start the daemon
    pub fn start(&mut self) -> IndexResult<()> {
        // Set up watches
        for path in &self.config.watch_paths.clone() {
            self.watcher.watch(path.clone());
        }
        self.watcher.start();

        // Start IPC server
        self.ipc.start();

        Ok(())
    }

    /// Stop the daemon
    pub fn stop(&mut self) {
        self.watcher.stop();
        self.ipc.stop();
    }

    /// Process one iteration (call in event loop)
    pub fn tick(&mut self) -> IndexResult<()> {
        // Handle filesystem events
        while let Some(event) = self.watcher.poll() {
            self.handle_event(event)?;
        }

        // Process indexing queue
        while let Some(item) = self.queue.pop() {
            if let Err(e) = self.index_file(&item.path) {
                if item.retries < 3 {
                    let mut retry = item.clone();
                    retry.retries += 1;
                    retry.priority = 1; // Lower priority for retries
                    self.queue.push(retry);
                }
            }
        }

        Ok(())
    }

    /// Handle filesystem event
    fn handle_event(&mut self, event: FsEvent) -> IndexResult<()> {
        match event.kind {
            EventKind::Create | EventKind::Modify => {
                if self.config.should_index(&event.path) {
                    self.queue.push(QueueItem::new(event.path, 5));
                }
            }
            EventKind::Delete => {
                if let Some(&file_id) = self.path_to_id.get(&event.path) {
                    let _ = self.index.delete(file_id);
                    self.path_to_id.remove(&event.path);
                    self.id_to_path.remove(&file_id);
                }
            }
            EventKind::Rename => {
                // Handle as delete + create
                if let Some(old) = event.old_path {
                    if let Some(&file_id) = self.path_to_id.get(&old) {
                        // Update mappings
                        self.path_to_id.remove(&old);
                        self.path_to_id.insert(event.path.clone(), file_id);
                        self.id_to_path.insert(file_id, event.path);
                    }
                }
            }
        }
        Ok(())
    }

    /// Index a single file
    fn index_file(&mut self, path: &str) -> IndexResult<()> {
        // Read file content (stub - would actually read from fs)
        let content = self.read_file(path)?;

        // Detect file type
        let file_type = FileType::from_extension(
            path.rsplit('.').next().unwrap_or("")
        );

        // Extract text content
        let extracted = ContentExtractor::extract(&content, file_type)
            .map_err(|_| IndexError::EmbedError)?;

        if extracted.text.is_empty() {
            return Ok(());
        }

        // Generate embedding
        let embedding = self.model.embed(&extracted.text)
            .map_err(|_| IndexError::EmbedError)?;

        // Get or create file ID
        let file_id = if let Some(&id) = self.path_to_id.get(path) {
            id
        } else {
            let id = FileId(self.next_file_id);
            self.next_file_id += 1;
            self.path_to_id.insert(String::from(path), id);
            self.id_to_path.insert(id, String::from(path));
            id
        };

        // Insert into index
        self.index.insert(file_id, embedding)
            .map_err(|_| IndexError::IndexError)?;

        Ok(())
    }

    /// Read file content (stub)
    fn read_file(&self, _path: &str) -> IndexResult<Vec<u8>> {
        // In real implementation: read from filesystem
        Ok(Vec::new())
    }

    /// Handle search request
    pub fn search(&self, request: SearchRequest) -> SearchResponse {
        // Generate query embedding
        let query_embedding = match self.model.embed(&request.query) {
            Ok(emb) => emb,
            Err(_) => return SearchResponse::empty(),
        };

        // Search index
        let results = self.index.search(&query_embedding, request.limit);

        // Convert to response
        let search_results: Vec<SearchResult> = results.iter()
            .filter_map(|r| {
                let path = self.id_to_path.get(&r.id)?;

                // Apply filters
                if let Some(ref prefix) = request.path_prefix {
                    if !path.starts_with(prefix) {
                        return None;
                    }
                }

                Some(SearchResult {
                    path: path.clone(),
                    score: 1.0 - r.distance, // Convert distance to similarity
                    snippet: None,
                    size: 0,
                    modified: 0,
                })
            })
            .collect();

        SearchResponse {
            total: search_results.len(),
            results: search_results,
            duration_ms: 0,
        }
    }

    /// Get index statistics
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            indexed_files: self.index.len(),
            queue_size: self.queue.len(),
            watched_paths: self.watcher.watched_paths().len(),
        }
    }
}

/// Index statistics
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// Number of indexed files
    pub indexed_files: usize,
    /// Current queue size
    pub queue_size: usize,
    /// Number of watched paths
    pub watched_paths: usize,
}

impl Default for IndexDaemon {
    fn default() -> Self {
        Self::new(IndexConfig::default())
    }
}
