//! Embedding model abstraction

use alloc::string::String;
use alloc::vec::Vec;
use crate::{EmbedResult, EmbedError, EMBEDDING_DIM};
use crate::tokenizer::Tokenizer;

/// Square root approximation for no_std
fn sqrt_f32(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    // Newton-Raphson iteration
    let mut guess = x / 2.0;
    for _ in 0..10 {
        guess = (guess + x / guess) / 2.0;
    }
    guess
}

/// Embedding model configuration
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Model name
    pub name: String,
    /// Embedding dimension
    pub dim: usize,
    /// Maximum sequence length
    pub max_seq_len: usize,
    /// Normalize embeddings
    pub normalize: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            name: String::from("all-MiniLM-L6-v2"),
            dim: EMBEDDING_DIM,
            max_seq_len: 512,
            normalize: true,
        }
    }
}

/// Embedding model trait
pub trait EmbeddingModel {
    /// Generate embedding for text
    fn embed(&self, text: &str) -> EmbedResult<Vec<f32>>;

    /// Generate embeddings for multiple texts (batch)
    fn embed_batch(&self, texts: &[&str]) -> EmbedResult<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Get embedding dimension
    fn dim(&self) -> usize;
}

/// Simple TF-IDF style embedding (for testing without ML runtime)
pub struct SimpleTfIdfModel {
    /// Tokenizer
    tokenizer: Tokenizer,
    /// Configuration
    config: EmbeddingConfig,
}

impl SimpleTfIdfModel {
    /// Create new TF-IDF model
    pub fn new(config: EmbeddingConfig) -> Self {
        SimpleTfIdfModel {
            tokenizer: Tokenizer::simple(config.max_seq_len),
            config,
        }
    }
}

impl EmbeddingModel for SimpleTfIdfModel {
    fn embed(&self, text: &str) -> EmbedResult<Vec<f32>> {
        if text.is_empty() {
            return Err(EmbedError::EmptyInput);
        }

        let tokens = self.tokenizer.encode(text);

        // Create a simple embedding based on token IDs
        // This is a placeholder - real model would use transformer
        let mut embedding = alloc::vec![0.0f32; self.config.dim];

        // Hash tokens into embedding dimensions
        for (i, &token_id) in tokens.input_ids.iter().enumerate() {
            if tokens.attention_mask[i] == 0 {
                continue;
            }
            // Simple hash-based embedding
            let hash = (token_id as usize * 31 + i * 17) % self.config.dim;
            embedding[hash] += 1.0;

            // Also spread to nearby dimensions
            let hash2 = (token_id as usize * 37 + i * 13) % self.config.dim;
            embedding[hash2] += 0.5;
        }

        // Normalize
        if self.config.normalize {
            let mut norm = 0.0f32;
            for &v in &embedding {
                norm += v * v;
            }
            norm = sqrt_f32(norm);
            if norm > 1e-10 {
                for v in &mut embedding {
                    *v /= norm;
                }
            }
        }

        Ok(embedding)
    }

    fn dim(&self) -> usize {
        self.config.dim
    }
}

impl Default for SimpleTfIdfModel {
    fn default() -> Self {
        Self::new(EmbeddingConfig::default())
    }
}

/// Mean pooling for embeddings
pub fn mean_pool(embeddings: &[Vec<f32>], attention_mask: &[i32]) -> Vec<f32> {
    if embeddings.is_empty() {
        return Vec::new();
    }

    let dim = embeddings[0].len();
    let mut result = alloc::vec![0.0f32; dim];
    let mut count = 0.0f32;

    for (i, emb) in embeddings.iter().enumerate() {
        if attention_mask[i] != 0 {
            for (j, &v) in emb.iter().enumerate() {
                result[j] += v;
            }
            count += 1.0;
        }
    }

    if count > 0.0 {
        for v in &mut result {
            *v /= count;
        }
    }

    result
}
