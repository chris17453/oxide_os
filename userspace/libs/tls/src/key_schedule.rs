//! TLS 1.3 Key Schedule (RFC 8446 Section 7.1)
//!
//! — ColdCipher: The key schedule is where X25519 shared secrets become
//! actual encryption keys. HKDF-Extract concentrates entropy, HKDF-Expand
//! derives specific keys. Every stage has its own secret — early, handshake,
//! master — each derived from the previous. Forward secrecy lives here:
//! the ephemeral X25519 key is the only input that matters. — ColdCipher

extern crate alloc;
use alloc::vec::Vec;
use oxide_crypto::hkdf;
use oxide_crypto::hmac;

/// — ColdCipher: HKDF-Expand-Label per RFC 8446 Section 7.1
/// This is the TLS 1.3 specific wrapper around HKDF-Expand.
/// Label format: "tls13 " + label (max 12 chars)
/// Context: usually a transcript hash (32 bytes for SHA-256)
pub fn hkdf_expand_label(
    secret: &[u8],
    label: &str,
    context: &[u8],
    length: usize,
) -> Vec<u8> {
    // Build HkdfLabel structure:
    // uint16 length
    // opaque label<7..255> = "tls13 " + label
    // opaque context<0..255>
    let tls_label = alloc::format!("tls13 {}", label);
    let tls_label_bytes = tls_label.as_bytes();

    let mut hkdf_label = Vec::with_capacity(2 + 1 + tls_label_bytes.len() + 1 + context.len());
    hkdf_label.push((length >> 8) as u8);
    hkdf_label.push((length & 0xFF) as u8);
    hkdf_label.push(tls_label_bytes.len() as u8);
    hkdf_label.extend_from_slice(tls_label_bytes);
    hkdf_label.push(context.len() as u8);
    hkdf_label.extend_from_slice(context);

    hkdf::hkdf_expand_sha256(secret, &hkdf_label, length)
}

/// Derive-Secret per RFC 8446 Section 7.1
/// Derive-Secret(Secret, Label, Messages) = HKDF-Expand-Label(Secret, Label, Transcript-Hash(Messages), Hash.length)
pub fn derive_secret(secret: &[u8], label: &str, transcript_hash: &[u8; 32]) -> [u8; 32] {
    let expanded = hkdf_expand_label(secret, label, transcript_hash, 32);
    let mut result = [0u8; 32];
    result.copy_from_slice(&expanded[..32]);
    result
}

/// — ColdCipher: The TLS 1.3 key schedule. Each step feeds into the next.
///
/// ```text
///              0 (all zeros)
///              |
///              v
///    PSK ->  HKDF-Extract = Early Secret
///              |
///              +-> Derive-Secret(., "derived", "")
///              |
///              v
///   ECDHE -> HKDF-Extract = Handshake Secret
///              |
///              +-> Derive-Secret(., "c hs traffic", CH..SH) = client_handshake_traffic_secret
///              +-> Derive-Secret(., "s hs traffic", CH..SH) = server_handshake_traffic_secret
///              +-> Derive-Secret(., "derived", "")
///              |
///              v
///     0 ->   HKDF-Extract = Master Secret
///              |
///              +-> Derive-Secret(., "c ap traffic", CH..SF) = client_application_traffic_secret
///              +-> Derive-Secret(., "s ap traffic", CH..SF) = server_application_traffic_secret
/// ```
/// — ColdCipher

/// Compute Early Secret (no PSK = all zeros)
pub fn early_secret() -> [u8; 32] {
    let zero_psk = [0u8; 32];
    let salt = [0u8; 32]; // No salt for early secret
    hkdf::hkdf_extract_sha256(&salt, &zero_psk)
}

/// Derive the intermediate value between Early and Handshake secrets
pub fn derive_intermediate(early_secret: &[u8; 32]) -> [u8; 32] {
    let empty_hash = oxide_crypto::sha256::sha256(&[]);
    derive_secret(early_secret, "derived", &empty_hash)
}

/// Compute Handshake Secret from shared_secret (X25519 output)
pub fn handshake_secret(derived: &[u8; 32], shared_secret: &[u8; 32]) -> [u8; 32] {
    hkdf::hkdf_extract_sha256(derived, shared_secret)
}

/// Derive client and server handshake traffic secrets
pub fn handshake_traffic_secrets(
    hs_secret: &[u8; 32],
    transcript_hash: &[u8; 32],
) -> ([u8; 32], [u8; 32]) {
    let client = derive_secret(hs_secret, "c hs traffic", transcript_hash);
    let server = derive_secret(hs_secret, "s hs traffic", transcript_hash);
    (client, server)
}

/// Derive the intermediate value between Handshake and Master secrets
pub fn derive_master_intermediate(hs_secret: &[u8; 32]) -> [u8; 32] {
    let empty_hash = oxide_crypto::sha256::sha256(&[]);
    derive_secret(hs_secret, "derived", &empty_hash)
}

/// Compute Master Secret
pub fn master_secret(derived: &[u8; 32]) -> [u8; 32] {
    let zero = [0u8; 32];
    hkdf::hkdf_extract_sha256(derived, &zero)
}

/// Derive client and server application traffic secrets
pub fn application_traffic_secrets(
    master: &[u8; 32],
    transcript_hash: &[u8; 32],
) -> ([u8; 32], [u8; 32]) {
    let client = derive_secret(master, "c ap traffic", transcript_hash);
    let server = derive_secret(master, "s ap traffic", transcript_hash);
    (client, server)
}

/// — ColdCipher: Derive actual AES key and IV from a traffic secret.
/// For TLS_AES_128_GCM_SHA256: key=16 bytes, iv=12 bytes.
/// For TLS_AES_256_GCM_SHA384: key=32 bytes, iv=12 bytes.
/// — ColdCipher
pub struct TrafficKeys {
    pub key: Vec<u8>,
    pub iv: [u8; 12],
}

pub fn derive_traffic_keys(secret: &[u8; 32], key_len: usize) -> TrafficKeys {
    let key = hkdf_expand_label(secret, "key", &[], key_len);
    let iv_vec = hkdf_expand_label(secret, "iv", &[], 12);
    let mut iv = [0u8; 12];
    iv.copy_from_slice(&iv_vec[..12]);
    TrafficKeys { key, iv }
}

/// Compute the Finished verify_data MAC
/// finished_key = HKDF-Expand-Label(base_key, "finished", "", Hash.length)
/// verify_data = HMAC(finished_key, transcript_hash)
pub fn compute_finished(base_key: &[u8; 32], transcript_hash: &[u8; 32]) -> [u8; 32] {
    let finished_key_vec = hkdf_expand_label(base_key, "finished", &[], 32);
    let mut finished_key = [0u8; 32];
    finished_key.copy_from_slice(&finished_key_vec);
    hmac::hmac_sha256(&finished_key, transcript_hash)
}
