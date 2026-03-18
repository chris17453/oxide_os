//! TLS 1.3 Handshake Transcript (RFC 8446 Section 4.4.1)
//!
//! — ColdCipher: The transcript is a running SHA-256 hash of every handshake
//! message sent and received. It binds the Finished MACs to the entire
//! conversation — tamper with one byte anywhere and the hash diverges.
//! The server can't lie about what it said, and neither can we. — ColdCipher

extern crate alloc;
use alloc::vec::Vec;
use oxide_crypto::sha256;

/// — ColdCipher: Running handshake transcript hash.
/// Every handshake message (ClientHello, ServerHello, EncryptedExtensions,
/// Certificate, CertificateVerify, Finished) gets fed in order.
///
/// Note: Since Sha256 doesn't implement Clone, we keep the raw messages
/// and recompute the hash each time. This is fine for TLS — the transcript
/// is small (~2KB) and hash() is called <10 times per handshake. — ColdCipher
pub struct Transcript {
    messages: Vec<u8>,
}

impl Transcript {
    pub fn new() -> Self {
        Transcript {
            messages: Vec::new(),
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.messages.extend_from_slice(data);
    }

    /// Get the current transcript hash (SHA-256, 32 bytes)
    pub fn hash(&self) -> [u8; 32] {
        sha256::sha256(&self.messages)
    }

    pub fn raw_messages(&self) -> &[u8] {
        &self.messages
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }
}
