//! Text Embedding and Content Extraction
//!
//! Provides embedding generation for semantic search.

#![no_std]

extern crate alloc;

pub mod extract;
pub mod model;
pub mod tokenizer;

pub use extract::{ContentExtractor, ExtractedContent, FileType};
pub use model::{EmbeddingConfig, EmbeddingModel, SimpleTfIdfModel};
pub use tokenizer::Tokenizer;

/// Embedding dimension for all-MiniLM-L6-v2
pub const EMBEDDING_DIM: usize = 384;

/// Maximum sequence length
pub const MAX_SEQ_LEN: usize = 512;

/// Error type for embedding operations
#[derive(Debug, Clone)]
pub enum EmbedError {
    /// Text is empty
    EmptyInput,
    /// Text too long
    TooLong,
    /// Model not loaded
    ModelNotLoaded,
    /// Tokenization failed
    TokenizationError,
    /// Inference failed
    InferenceError,
    /// File read error
    FileReadError,
    /// Unsupported file type
    UnsupportedType,
}

impl core::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "empty input"),
            Self::TooLong => write!(f, "input too long"),
            Self::ModelNotLoaded => write!(f, "model not loaded"),
            Self::TokenizationError => write!(f, "tokenization error"),
            Self::InferenceError => write!(f, "inference error"),
            Self::FileReadError => write!(f, "file read error"),
            Self::UnsupportedType => write!(f, "unsupported file type"),
        }
    }
}

/// Result type for embedding operations
pub type EmbedResult<T> = Result<T, EmbedError>;
