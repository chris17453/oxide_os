//! Text tokenization for embedding models

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Token ID type
pub type TokenId = u32;

/// Special tokens
pub const PAD_TOKEN: TokenId = 0;
pub const UNK_TOKEN: TokenId = 100;
pub const CLS_TOKEN: TokenId = 101;
pub const SEP_TOKEN: TokenId = 102;
pub const MASK_TOKEN: TokenId = 103;

/// Simple WordPiece tokenizer
pub struct Tokenizer {
    /// Vocabulary: token -> id
    vocab: BTreeMap<String, TokenId>,
    /// Reverse vocabulary: id -> token
    id_to_token: BTreeMap<TokenId, String>,
    /// Maximum sequence length
    max_len: usize,
}

impl Tokenizer {
    /// Create a new tokenizer with vocabulary
    pub fn new(vocab: BTreeMap<String, TokenId>, max_len: usize) -> Self {
        let id_to_token = vocab.iter().map(|(k, &v)| (v, k.clone())).collect();

        Tokenizer {
            vocab,
            id_to_token,
            max_len,
        }
    }

    /// Create a simple tokenizer for testing (word-level)
    pub fn simple(max_len: usize) -> Self {
        let mut vocab = BTreeMap::new();
        vocab.insert("[PAD]".into(), PAD_TOKEN);
        vocab.insert("[UNK]".into(), UNK_TOKEN);
        vocab.insert("[CLS]".into(), CLS_TOKEN);
        vocab.insert("[SEP]".into(), SEP_TOKEN);
        vocab.insert("[MASK]".into(), MASK_TOKEN);

        // Add some common words for testing
        let common_words = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been", "and", "or", "not", "of",
            "to", "in", "for", "on", "with", "machine", "learning", "deep", "neural", "network",
            "data", "file", "image", "text", "photo", "document", "video",
        ];

        for (i, word) in common_words.iter().enumerate() {
            vocab.insert((*word).into(), (i + 1000) as TokenId);
        }

        Tokenizer::new(vocab, max_len)
    }

    /// Tokenize text into token IDs
    pub fn encode(&self, text: &str) -> TokenizedOutput {
        let mut token_ids = Vec::new();
        let mut attention_mask = Vec::new();

        // Add [CLS]
        token_ids.push(CLS_TOKEN);
        attention_mask.push(1);

        // Tokenize words
        let normalized = text.to_lowercase();
        for word in normalized.split_whitespace() {
            if token_ids.len() >= self.max_len - 1 {
                break;
            }

            let token_id = self.vocab.get(word).copied().unwrap_or(UNK_TOKEN);
            token_ids.push(token_id);
            attention_mask.push(1);
        }

        // Add [SEP]
        if token_ids.len() < self.max_len {
            token_ids.push(SEP_TOKEN);
            attention_mask.push(1);
        }

        // Pad to max length
        while token_ids.len() < self.max_len {
            token_ids.push(PAD_TOKEN);
            attention_mask.push(0);
        }

        TokenizedOutput {
            input_ids: token_ids,
            attention_mask,
            token_type_ids: alloc::vec![0; self.max_len],
        }
    }

    /// Decode token IDs back to text
    pub fn decode(&self, token_ids: &[TokenId]) -> String {
        let mut tokens = Vec::new();
        for &id in token_ids {
            if id == PAD_TOKEN || id == CLS_TOKEN || id == SEP_TOKEN {
                continue;
            }
            if let Some(token) = self.id_to_token.get(&id) {
                tokens.push(token.as_str());
            }
        }
        tokens.join(" ")
    }

    /// Get vocabulary size
    pub fn vocab_size(&self) -> usize {
        self.vocab.len()
    }
}

/// Output of tokenization
#[derive(Debug, Clone)]
pub struct TokenizedOutput {
    /// Token IDs
    pub input_ids: Vec<TokenId>,
    /// Attention mask (1 for real tokens, 0 for padding)
    pub attention_mask: Vec<i32>,
    /// Token type IDs (for sentence pairs)
    pub token_type_ids: Vec<i32>,
}

impl Default for Tokenizer {
    fn default() -> Self {
        Self::simple(512)
    }
}
