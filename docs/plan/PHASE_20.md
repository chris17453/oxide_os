# Phase 20: AI Indexing

**Stage:** 4 - Advanced
**Status:** Not Started
**Dependencies:** Phase 11 (Storage)

---

## Goal

Implement semantic file indexing with embeddings for intelligent search.

---

## Deliverables

| Item | Status |
|------|--------|
| efflux-indexd daemon | [ ] |
| Candle embedding runtime | [ ] |
| HNSW vector index | [ ] |
| Extended metadata on effluxfs | [ ] |
| Overlay metadata for other FS | [ ] |
| Semantic search API | [ ] |

---

## Architecture Status

| Arch | indexd | Candle | HNSW | API | Done |
|------|--------|--------|------|-----|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## AI Indexing Architecture

```
┌─────────────────────────────────────────────────────┐
│                   User Query                         │
│               "photos from vacation"                 │
└──────────────────────┬──────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────┐
│              efflux-indexd Daemon                    │
│  ┌─────────────────────────────────────────────┐   │
│  │           Query Embedding                    │   │
│  │    "photos from vacation" → [0.1, 0.3, ...]  │   │
│  └─────────────────────────────────────────────┘   │
│                       │                              │
│                       ▼                              │
│  ┌─────────────────────────────────────────────┐   │
│  │           HNSW Vector Index                  │   │
│  │    Find nearest neighbors in embedding space │   │
│  └─────────────────────────────────────────────┘   │
│                       │                              │
│                       ▼                              │
│  ┌─────────────────────────────────────────────┐   │
│  │         Results: [beach.jpg, sunset.png]     │   │
│  └─────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

---

## Indexing Pipeline

```
File Created/Modified
        │
        ▼
┌───────────────────┐
│ Content Extraction│
│  - Text files     │
│  - PDF text       │
│  - Image EXIF     │
│  - Audio metadata │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Embedding Model   │
│  (Candle runtime) │
│  all-MiniLM-L6-v2 │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Vector: [f32; 384]│
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ HNSW Index Insert │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ effluxfs xattr    │
│ user.efflux.embed │
└───────────────────┘
```

---

## Embedding Model

```rust
// Using Candle (Rust ML framework)
use candle_core::{Device, Tensor};
use candle_transformers::models::bert;

pub struct EmbeddingModel {
    model: bert::BertModel,
    tokenizer: tokenizers::Tokenizer,
    device: Device,
}

impl EmbeddingModel {
    /// Load model from disk
    pub fn load(model_path: &Path) -> Result<Self>;

    /// Generate embedding for text
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let tokens = self.tokenizer.encode(text, true)?;
        let input_ids = Tensor::new(tokens.get_ids(), &self.device)?;

        let embeddings = self.model.forward(&input_ids, &attention_mask)?;

        // Mean pooling
        let pooled = embeddings.mean(1)?;
        Ok(pooled.to_vec1()?)
    }
}
```

---

## HNSW Vector Index

```rust
// Hierarchical Navigable Small World graph
pub struct HnswIndex {
    /// Embedding dimensions
    dim: usize,

    /// Maximum connections per node
    m: usize,

    /// Size of dynamic candidate list
    ef_construction: usize,

    /// Layers of the graph
    layers: Vec<Layer>,

    /// Entry point
    entry_point: Option<NodeId>,
}

impl HnswIndex {
    /// Insert vector with metadata
    pub fn insert(&mut self, id: FileId, vector: &[f32]) -> Result<()>;

    /// Search for k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(FileId, f32)>;

    /// Delete by file ID
    pub fn delete(&mut self, id: FileId) -> Result<()>;

    /// Persist to disk
    pub fn save(&self, path: &Path) -> Result<()>;

    /// Load from disk
    pub fn load(path: &Path) -> Result<Self>;
}
```

---

## effluxfs Extended Attributes

```rust
// Extended attributes for AI metadata
// Stored in inode's xattr area

/// Embedding vector (compressed)
const XATTR_EMBED: &str = "user.efflux.embed";

/// Content hash (for change detection)
const XATTR_HASH: &str = "user.efflux.hash";

/// Last indexed timestamp
const XATTR_INDEXED: &str = "user.efflux.indexed";

/// Extracted text summary
const XATTR_SUMMARY: &str = "user.efflux.summary";

/// Tags (auto-generated)
const XATTR_TAGS: &str = "user.efflux.tags";

// For non-effluxfs filesystems, store in overlay database
// ~/.efflux/metadata.db
```

---

## indexd Daemon

```rust
pub struct IndexDaemon {
    /// Vector index
    index: HnswIndex,

    /// Embedding model
    model: EmbeddingModel,

    /// File system watcher
    watcher: FsWatcher,

    /// Index queue
    queue: VecDeque<PathBuf>,

    /// Configuration
    config: IndexConfig,
}

impl IndexDaemon {
    /// Watch for file changes
    fn watch_filesystem(&mut self);

    /// Process indexing queue
    fn process_queue(&mut self);

    /// Handle search request
    fn search(&self, query: &str, limit: usize) -> Vec<SearchResult>;

    /// Handle file event
    fn on_file_event(&mut self, event: FsEvent);
}

// IPC interface via Unix socket
// /run/efflux-indexd.sock
```

---

## Search API

```rust
// Search request
pub struct SearchRequest {
    /// Natural language query
    pub query: String,

    /// Maximum results
    pub limit: usize,

    /// Optional path filter
    pub path_prefix: Option<PathBuf>,

    /// File type filter
    pub file_types: Option<Vec<String>>,
}

// Search result
pub struct SearchResult {
    /// File path
    pub path: PathBuf,

    /// Similarity score (0-1)
    pub score: f32,

    /// Snippet (if text file)
    pub snippet: Option<String>,

    /// Metadata
    pub metadata: FileMetadata,
}
```

---

## Key Files

```
crates/ai/efflux-indexd/src/
├── lib.rs
├── daemon.rs          # Main daemon
├── watcher.rs         # Filesystem watcher
├── queue.rs           # Indexing queue
└── ipc.rs             # Unix socket API

crates/ai/efflux-embed/src/
├── lib.rs
├── model.rs           # Candle model wrapper
├── tokenizer.rs       # Text tokenization
└── extract.rs         # Content extraction

crates/ai/efflux-hnsw/src/
├── lib.rs
├── index.rs           # HNSW implementation
├── node.rs            # Graph nodes
└── distance.rs        # Distance metrics

userspace/ai/
├── efflux-search      # CLI search tool
└── efflux-index       # Manual indexing
```

---

## Exit Criteria

- [ ] indexd daemon runs at boot
- [ ] New files automatically indexed
- [ ] Embedding model generates vectors
- [ ] HNSW index provides fast search
- [ ] Semantic search returns relevant results
- [ ] Works on all 8 architectures

---

## Test: Semantic Search

```bash
# Start indexing daemon
$ systemctl start efflux-indexd

# Create some test files
$ echo "Machine learning and neural networks" > ~/docs/ml.txt
$ echo "Cooking recipes for pasta" > ~/docs/pasta.txt
$ echo "Deep learning with transformers" > ~/docs/dl.txt

# Wait for indexing
$ sleep 5

# Search
$ efflux-search "artificial intelligence"
Score: 0.92  ~/docs/ml.txt
  "Machine learning and neural networks"
Score: 0.87  ~/docs/dl.txt
  "Deep learning with transformers"

$ efflux-search "food"
Score: 0.91  ~/docs/pasta.txt
  "Cooking recipes for pasta"
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 20 of EFFLUX Implementation*
