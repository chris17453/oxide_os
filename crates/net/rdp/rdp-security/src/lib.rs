//! RDP Security and TLS Support
//!
//! Provides TLS 1.2 encryption layer for RDP connections.
//! Uses AES-256-GCM for record encryption and X25519 for key exchange.

#![no_std]

extern crate alloc;

mod tls;
mod record;
mod keys;
mod cert;

pub use tls::{TlsSession, TlsState, TlsConfig};
pub use record::{TlsRecord, RecordType};
pub use keys::KeyMaterial;
pub use cert::{SelfSignedCert, Certificate};

use alloc::vec::Vec;
use crypto::{
    Aes256Gcm, AesKey,
    X25519PublicKey, X25519SecretKey, SharedSecret,
    sha256, hmac_sha256,
    random,
};
use rdp_traits::{RdpError, RdpResult};

/// TLS version 1.2
pub const TLS_VERSION_1_2: u16 = 0x0303;

/// Maximum TLS record size
pub const MAX_RECORD_SIZE: usize = 16384;

/// AES-GCM tag size
pub const GCM_TAG_SIZE: usize = 16;

/// Nonce size for AES-GCM
pub const NONCE_SIZE: usize = 12;

/// Implicit nonce size (from key derivation)
pub const IMPLICIT_NONCE_SIZE: usize = 4;

/// Explicit nonce size (per record)
pub const EXPLICIT_NONCE_SIZE: usize = 8;

/// Key exchange result
pub struct KeyExchangeResult {
    /// Server's ephemeral public key
    pub server_public: X25519PublicKey,
    /// Premaster secret (shared secret)
    pub premaster_secret: SharedSecret,
}

/// Perform X25519 key exchange
pub fn key_exchange(client_public: &[u8]) -> RdpResult<KeyExchangeResult> {
    // Parse client's public key
    let client_key = X25519PublicKey::from_bytes(client_public)
        .map_err(|_| RdpError::CryptoError)?;

    // Generate ephemeral server key pair
    let server_secret = X25519SecretKey::generate(&random::random_bytes());
    let server_public = server_secret.public_key();

    // Compute shared secret
    let premaster_secret = server_secret.diffie_hellman(&client_key);

    Ok(KeyExchangeResult {
        server_public,
        premaster_secret,
    })
}

/// TLS PRF (Pseudo-Random Function) using HMAC-SHA256
///
/// P_SHA256(secret, seed) = HMAC(secret, A(1) + seed) +
///                          HMAC(secret, A(2) + seed) + ...
/// where A(0) = seed
///       A(i) = HMAC(secret, A(i-1))
pub fn tls_prf(secret: &[u8], label: &[u8], seed: &[u8], output_len: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(output_len);

    // Concatenate label and seed
    let mut label_seed = Vec::with_capacity(label.len() + seed.len());
    label_seed.extend_from_slice(label);
    label_seed.extend_from_slice(seed);

    // A(0) = label + seed
    let mut a = label_seed.clone();

    while output.len() < output_len {
        // A(i) = HMAC(secret, A(i-1))
        a = hmac_sha256(secret, &a).to_vec();

        // P_SHA256 = HMAC(secret, A(i) + label + seed)
        let mut input = a.clone();
        input.extend_from_slice(&label_seed);
        let block = hmac_sha256(secret, &input);

        let remaining = output_len - output.len();
        output.extend_from_slice(&block[..remaining.min(32)]);
    }

    output.truncate(output_len);
    output
}

/// Derive master secret from premaster secret
pub fn derive_master_secret(
    premaster_secret: &[u8],
    client_random: &[u8; 32],
    server_random: &[u8; 32],
) -> [u8; 48] {
    let mut seed = Vec::with_capacity(64);
    seed.extend_from_slice(client_random);
    seed.extend_from_slice(server_random);

    let master = tls_prf(premaster_secret, b"master secret", &seed, 48);

    let mut result = [0u8; 48];
    result.copy_from_slice(&master);
    result
}

/// Derive key material from master secret
pub fn derive_key_material(
    master_secret: &[u8],
    client_random: &[u8; 32],
    server_random: &[u8; 32],
) -> KeyMaterial {
    let mut seed = Vec::with_capacity(64);
    seed.extend_from_slice(server_random);
    seed.extend_from_slice(client_random);

    // For AES-256-GCM we need:
    // - 32 bytes client write key
    // - 32 bytes server write key
    // - 4 bytes client write IV
    // - 4 bytes server write IV
    let key_block = tls_prf(master_secret, b"key expansion", &seed, 72);

    let mut client_write_key = [0u8; 32];
    let mut server_write_key = [0u8; 32];
    let mut client_write_iv = [0u8; 4];
    let mut server_write_iv = [0u8; 4];

    client_write_key.copy_from_slice(&key_block[0..32]);
    server_write_key.copy_from_slice(&key_block[32..64]);
    client_write_iv.copy_from_slice(&key_block[64..68]);
    server_write_iv.copy_from_slice(&key_block[68..72]);

    KeyMaterial {
        client_write_key,
        server_write_key,
        client_write_iv,
        server_write_iv,
    }
}

/// Generate random bytes
pub fn generate_random<const N: usize>() -> [u8; N] {
    random::random_bytes()
}

/// Compute finished verify data
pub fn compute_verify_data(
    master_secret: &[u8],
    label: &[u8],
    handshake_hash: &[u8],
) -> [u8; 12] {
    let verify = tls_prf(master_secret, label, handshake_hash, 12);
    let mut result = [0u8; 12];
    result.copy_from_slice(&verify);
    result
}

/// Encrypt a TLS record using AES-256-GCM
pub fn encrypt_record(
    key: &AesKey,
    implicit_nonce: &[u8; 4],
    explicit_nonce: u64,
    record_type: u8,
    plaintext: &[u8],
) -> Vec<u8> {
    // Build full nonce: implicit (4 bytes) + explicit (8 bytes)
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(implicit_nonce);
    nonce[4..].copy_from_slice(&explicit_nonce.to_be_bytes());

    // Build additional authenticated data (AAD)
    // seq_num (8) + type (1) + version (2) + length (2)
    let plaintext_len = plaintext.len() as u16;
    let mut aad = Vec::with_capacity(13);
    aad.extend_from_slice(&explicit_nonce.to_be_bytes()); // Sequence number
    aad.push(record_type);
    aad.extend_from_slice(&TLS_VERSION_1_2.to_be_bytes());
    aad.extend_from_slice(&plaintext_len.to_be_bytes());

    // Encrypt
    let cipher = Aes256Gcm::new(key);
    let ciphertext = cipher.encrypt(&nonce, plaintext, &aad);

    // Output: explicit_nonce (8) + ciphertext + tag (16)
    let mut output = Vec::with_capacity(8 + ciphertext.len());
    output.extend_from_slice(&explicit_nonce.to_be_bytes());
    output.extend_from_slice(&ciphertext);

    output
}

/// Decrypt a TLS record using AES-256-GCM
pub fn decrypt_record(
    key: &AesKey,
    implicit_nonce: &[u8; 4],
    record_type: u8,
    seq_num: u64,
    ciphertext: &[u8],
) -> RdpResult<Vec<u8>> {
    if ciphertext.len() < EXPLICIT_NONCE_SIZE + GCM_TAG_SIZE {
        return Err(RdpError::CryptoError);
    }

    // Extract explicit nonce
    let explicit_nonce = &ciphertext[..EXPLICIT_NONCE_SIZE];

    // Build full nonce
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(implicit_nonce);
    nonce[4..].copy_from_slice(explicit_nonce);

    // Build AAD
    let plaintext_len = (ciphertext.len() - EXPLICIT_NONCE_SIZE - GCM_TAG_SIZE) as u16;
    let mut aad = Vec::with_capacity(13);
    aad.extend_from_slice(&seq_num.to_be_bytes());
    aad.push(record_type);
    aad.extend_from_slice(&TLS_VERSION_1_2.to_be_bytes());
    aad.extend_from_slice(&plaintext_len.to_be_bytes());

    // Decrypt
    let cipher = Aes256Gcm::new(key);
    let encrypted_data = &ciphertext[EXPLICIT_NONCE_SIZE..];

    cipher
        .decrypt(&nonce, encrypted_data, &aad)
        .map_err(|_| RdpError::CryptoError)
}

/// Hash handshake messages for Finished verification
pub fn hash_handshake_messages(messages: &[u8]) -> [u8; 32] {
    sha256(messages)
}
