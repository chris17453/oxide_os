//! Hierarchical Navigable Small World (HNSW) Vector Index
//!
//! Fast approximate nearest neighbor search for embeddings.

#![no_std]

extern crate alloc;

pub mod distance;
pub mod index;
pub mod layer;
pub mod node;

pub use distance::{cosine_distance, euclidean_distance, Distance};
pub use index::HnswIndex;
pub use layer::Layer;
pub use node::{Node, NodeId};

/// File identifier for indexed documents
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u64);

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// File ID
    pub id: FileId,
    /// Distance to query (lower is closer)
    pub distance: f32,
}

/// Configuration for HNSW index
#[derive(Debug, Clone)]
pub struct HnswConfig {
    /// Embedding dimensions
    pub dim: usize,
    /// Maximum connections per node at layer 0
    pub m: usize,
    /// Maximum connections per node at higher layers
    pub m_max: usize,
    /// Size of dynamic candidate list during construction
    pub ef_construction: usize,
    /// Size of dynamic candidate list during search
    pub ef_search: usize,
    /// Level multiplier (1/ln(m))
    pub ml: f64,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            dim: 384, // all-MiniLM-L6-v2 dimension
            m: 16,
            m_max: 32,
            ef_construction: 200,
            ef_search: 100,
            ml: 0.36067977, // 1/ln(16)
        }
    }
}
