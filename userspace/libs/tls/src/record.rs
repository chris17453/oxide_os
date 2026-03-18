//! TLS 1.3 Record Layer (RFC 8446 Section 5)
//!
//! — ColdCipher: The record layer is the envelope. Every byte that crosses
//! the wire is wrapped in a TLS record. After the handshake, every record
//! is AEAD-encrypted. Before it, they're plaintext. The content type byte
//! is the last byte of the plaintext (hidden from the wire). — ColdCipher

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

/// TLS record content types
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum ContentType {
    ChangeCipherSpec = 20,
    Alert = 21,
    Handshake = 22,
    ApplicationData = 23,
}

/// TLS protocol version (on the wire)
pub const TLS_12: u16 = 0x0303; // TLS 1.2 — used in record layer even for TLS 1.3
pub const TLS_13: u16 = 0x0304; // TLS 1.3 — used in supported_versions extension

/// Maximum TLS record payload (2^14 = 16384)
pub const MAX_PLAINTEXT_LENGTH: usize = 16384;
/// Maximum encrypted record (plaintext + content type byte + AEAD tag)
pub const MAX_CIPHERTEXT_LENGTH: usize = MAX_PLAINTEXT_LENGTH + 256;

/// — ColdCipher: A raw TLS record as it appears on the wire.
/// 5-byte header: content_type(1) + legacy_version(2) + length(2)
/// Followed by `length` bytes of fragment data. — ColdCipher
#[derive(Debug)]
pub struct TlsRecord {
    pub content_type: ContentType,
    pub legacy_version: u16,
    pub fragment: Vec<u8>,
}

impl TlsRecord {
    /// Encode record to wire format (header + fragment)
    pub fn encode(&self) -> Vec<u8> {
        let len = self.fragment.len();
        let mut buf = Vec::with_capacity(5 + len);
        buf.push(self.content_type as u8);
        buf.push((self.legacy_version >> 8) as u8);
        buf.push((self.legacy_version & 0xFF) as u8);
        buf.push((len >> 8) as u8);
        buf.push((len & 0xFF) as u8);
        buf.extend_from_slice(&self.fragment);
        buf
    }

    /// Read a TLS record from a byte buffer.
    /// Returns (record, bytes_consumed) or None if not enough data.
    pub fn decode(data: &[u8]) -> Option<(Self, usize)> {
        if data.len() < 5 {
            return None;
        }
        let content_type = match data[0] {
            20 => ContentType::ChangeCipherSpec,
            21 => ContentType::Alert,
            22 => ContentType::Handshake,
            23 => ContentType::ApplicationData,
            _ => return None,
        };
        let legacy_version = ((data[1] as u16) << 8) | data[2] as u16;
        let length = ((data[3] as u16) << 8) | data[4] as u16;
        let total = 5 + length as usize;
        if data.len() < total {
            return None;
        }
        let fragment = data[5..total].to_vec();
        Some((TlsRecord { content_type, legacy_version, fragment }, total))
    }

    /// Create a plaintext handshake record
    pub fn handshake(data: Vec<u8>) -> Self {
        TlsRecord {
            content_type: ContentType::Handshake,
            legacy_version: TLS_12,
            fragment: data,
        }
    }

    /// Create an application data record (for encrypted payloads)
    pub fn application_data(data: Vec<u8>) -> Self {
        TlsRecord {
            content_type: ContentType::ApplicationData,
            legacy_version: TLS_12,
            fragment: data,
        }
    }
}

/// — ColdCipher: AEAD nonce construction per RFC 8446 Section 5.3.
/// The per-record nonce is the XOR of the IV and the 64-bit sequence number
/// (left-padded to IV length). This ensures every record gets a unique nonce
/// even though the IV is reused for the entire connection. — ColdCipher
pub fn build_nonce(iv: &[u8; 12], seq: u64) -> [u8; 12] {
    let mut nonce = *iv;
    let seq_bytes = seq.to_be_bytes();
    // XOR the sequence number into the last 8 bytes of the IV
    for i in 0..8 {
        nonce[12 - 8 + i] ^= seq_bytes[i];
    }
    nonce
}

/// — ColdCipher: Encrypt a TLS 1.3 record with AEAD.
/// Input: plaintext + real content type. Output: encrypted record.
/// The content type is appended to plaintext before encryption (inner content type).
/// The outer record always says ApplicationData (0x17). — ColdCipher
pub fn encrypt_record(
    key: &[u8],
    iv: &[u8; 12],
    seq: u64,
    content_type: ContentType,
    plaintext: &[u8],
) -> Vec<u8> {
    let nonce = build_nonce(iv, seq);

    // Inner plaintext = data + content type byte
    let mut inner = Vec::with_capacity(plaintext.len() + 1);
    inner.extend_from_slice(plaintext);
    inner.push(content_type as u8);

    // AAD = outer record header (type=0x17, version=0x0303, length=inner_len+16)
    let encrypted_len = inner.len() + 16; // 16 = GCM tag
    let aad = [
        ContentType::ApplicationData as u8,
        0x03, 0x03, // TLS 1.2 legacy version
        (encrypted_len >> 8) as u8,
        (encrypted_len & 0xFF) as u8,
    ];

    // — ColdCipher: AES-GCM encryption. The nonce is unique per record.
    // The AAD binds the ciphertext to the record header so it can't be spliced.
    if key.len() == 16 {
        let mut aes_key = [0u8; 16];
        aes_key.copy_from_slice(key);
        let cipher = oxide_crypto::aes::Aes128Gcm::new(&aes_key);
        cipher.encrypt(&nonce, &inner, &aad)
    } else {
        let mut aes_key = [0u8; 32];
        aes_key.copy_from_slice(&key[..32]);
        let cipher = oxide_crypto::aes::Aes256Gcm::new(&aes_key);
        cipher.encrypt(&nonce, &inner, &aad)
    }
}

/// — ColdCipher: Decrypt a TLS 1.3 record. Returns (content_type, plaintext).
/// The inner content type is the last byte of the decrypted data. — ColdCipher
pub fn decrypt_record(
    key: &[u8],
    iv: &[u8; 12],
    seq: u64,
    encrypted_fragment: &[u8],
) -> Option<(ContentType, Vec<u8>)> {
    let nonce = build_nonce(iv, seq);

    // AAD = outer record header
    let aad = [
        ContentType::ApplicationData as u8,
        0x03, 0x03,
        (encrypted_fragment.len() >> 8) as u8,
        (encrypted_fragment.len() & 0xFF) as u8,
    ];

    let inner = if key.len() == 16 {
        let mut aes_key = [0u8; 16];
        aes_key.copy_from_slice(key);
        let cipher = oxide_crypto::aes::Aes128Gcm::new(&aes_key);
        cipher.decrypt(&nonce, encrypted_fragment, &aad).ok()?
    } else {
        let mut aes_key = [0u8; 32];
        aes_key.copy_from_slice(&key[..32]);
        let cipher = oxide_crypto::aes::Aes256Gcm::new(&aes_key);
        cipher.decrypt(&nonce, encrypted_fragment, &aad).ok()?
    };

    if inner.is_empty() {
        return None;
    }

    // — ColdCipher: Strip trailing zeros and extract real content type.
    // TLS 1.3 allows padding zeros before the content type byte.
    let mut end = inner.len() - 1;
    while end > 0 && inner[end] == 0 {
        end -= 1;
    }

    let ct = match inner[end] {
        20 => ContentType::ChangeCipherSpec,
        21 => ContentType::Alert,
        22 => ContentType::Handshake,
        23 => ContentType::ApplicationData,
        _ => return None,
    };

    Some((ct, inner[..end].to_vec()))
}
