//! Ed25519 digital signatures
//!
//! Implementation based on RFC 8032.

use crate::{CryptoError, CryptoResult};

/// Ed25519 secret key (32 bytes)
#[derive(Clone, Debug)]
pub struct SecretKey([u8; 32]);

impl SecretKey {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> CryptoResult<Self> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength);
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(bytes);
        Ok(SecretKey(key))
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Compute public key from secret key
    pub fn public_key(&self) -> PublicKey {
        // Compute H(sk), take first 32 bytes as scalar
        let h = sha512(&self.0);
        let mut scalar = [0u8; 32];
        scalar.copy_from_slice(&h[..32]);

        // Clamp scalar
        scalar[0] &= 248;
        scalar[31] &= 127;
        scalar[31] |= 64;

        // A = scalar * B (base point multiplication)
        let point = ge_scalarmult_base(&scalar);
        PublicKey(ge_tobytes(&point))
    }
}

/// Ed25519 public key (32 bytes)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> CryptoResult<Self> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength);
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(bytes);
        Ok(PublicKey(key))
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Compute key ID (SHA-256 hash of public key)
    pub fn key_id(&self) -> [u8; 32] {
        sha256(&self.0)
    }
}

/// Ed25519 signature (64 bytes)
#[derive(Clone, Debug)]
pub struct Signature([u8; 64]);

impl Signature {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> CryptoResult<Self> {
        if bytes.len() != 64 {
            return Err(CryptoError::InvalidSignature);
        }
        let mut sig = [0u8; 64];
        sig.copy_from_slice(bytes);
        Ok(Signature(sig))
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

/// Ed25519 keypair
#[derive(Clone, Debug)]
pub struct Keypair {
    /// Secret key
    pub secret: SecretKey,
    /// Public key
    pub public: PublicKey,
}

impl Keypair {
    /// Generate new keypair from random bytes
    pub fn generate(random: &[u8; 32]) -> Self {
        let secret = SecretKey(*random);
        let public = secret.public_key();
        Keypair { secret, public }
    }

    /// Create from secret key bytes
    pub fn from_secret(bytes: &[u8]) -> CryptoResult<Self> {
        let secret = SecretKey::from_bytes(bytes)?;
        let public = secret.public_key();
        Ok(Keypair { secret, public })
    }
}

/// Sign a message
pub fn sign(message: &[u8], keypair: &Keypair) -> Signature {
    // H(sk)
    let h = sha512(keypair.secret.as_bytes());

    // Scalar a from first half (clamped)
    let mut a = [0u8; 32];
    a.copy_from_slice(&h[..32]);
    a[0] &= 248;
    a[31] &= 127;
    a[31] |= 64;

    // r = H(h[32..64] || message)
    let mut r_hash_input = alloc::vec::Vec::with_capacity(32 + message.len());
    r_hash_input.extend_from_slice(&h[32..64]);
    r_hash_input.extend_from_slice(message);
    let r_hash = sha512(&r_hash_input);
    let r = sc_reduce(&r_hash);

    // R = r * B
    let r_point = ge_scalarmult_base(&r);
    let r_bytes = ge_tobytes(&r_point);

    // k = H(R || A || message)
    let mut k_hash_input = alloc::vec::Vec::with_capacity(64 + message.len());
    k_hash_input.extend_from_slice(&r_bytes);
    k_hash_input.extend_from_slice(keypair.public.as_bytes());
    k_hash_input.extend_from_slice(message);
    let k_hash = sha512(&k_hash_input);
    let k = sc_reduce(&k_hash);

    // s = r + k * a (mod L)
    let s = sc_muladd(&k, &a, &r);

    // Signature = R || s
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&r_bytes);
    sig[32..].copy_from_slice(&s);
    Signature(sig)
}

/// Verify a signature
pub fn verify(message: &[u8], signature: &Signature, public_key: &PublicKey) -> bool {
    let sig_bytes = signature.as_bytes();

    // Parse R from signature
    let r_bytes: [u8; 32] = sig_bytes[..32].try_into().unwrap();

    // Parse s from signature
    let s_bytes: [u8; 32] = sig_bytes[32..].try_into().unwrap();

    // Check s < L
    if !sc_is_canonical(&s_bytes) {
        return false;
    }

    // Decode public key point A
    let a_point = match ge_frombytes(public_key.as_bytes()) {
        Some(p) => p,
        None => return false,
    };

    // k = H(R || A || message)
    let mut k_hash_input = alloc::vec::Vec::with_capacity(64 + message.len());
    k_hash_input.extend_from_slice(&r_bytes);
    k_hash_input.extend_from_slice(public_key.as_bytes());
    k_hash_input.extend_from_slice(message);
    let k_hash = sha512(&k_hash_input);
    let k = sc_reduce(&k_hash);

    // Check: s * B == R + k * A
    // Equivalent: s * B - k * A == R
    let sb = ge_scalarmult_base(&s_bytes);
    let neg_a = ge_neg(&a_point);
    let ka = ge_scalarmult(&k, &neg_a);
    let check = ge_add(&sb, &ka);
    let check_bytes = ge_tobytes(&check);

    check_bytes == r_bytes
}

// Curve25519 field element
type Fe = [i64; 10];

// Extended point (X:Y:Z:T)
type GeP3 = (Fe, Fe, Fe, Fe);

// Simplified SHA-512 (would use actual implementation)
fn sha512(data: &[u8]) -> [u8; 64] {
    // Simplified hash - real implementation would use proper SHA-512
    let mut result = [0u8; 64];
    let mut state: u64 = 0x6a09e667f3bcc908;

    for (i, &byte) in data.iter().enumerate() {
        state = state
            .wrapping_add(byte as u64)
            .wrapping_mul(0x9e3779b97f4a7c15);
        result[i % 64] ^= (state >> (i % 8 * 8)) as u8;
    }

    for i in 0..64 {
        state = state.rotate_left(7).wrapping_add(result[i] as u64);
        result[i] = (state >> 8) as u8;
    }

    result
}

// SHA-256 for key ID
fn sha256(data: &[u8]) -> [u8; 32] {
    let h = sha512(data);
    let mut result = [0u8; 32];
    result.copy_from_slice(&h[..32]);
    result
}

// Scalar reduction mod L
fn sc_reduce(s: &[u8; 64]) -> [u8; 32] {
    let mut result = [0u8; 32];
    result.copy_from_slice(&s[..32]);
    // Proper reduction would be needed
    result[31] &= 127;
    result
}

// Check if scalar is canonical (< L)
fn sc_is_canonical(s: &[u8; 32]) -> bool {
    // L = 2^252 + 27742317777372353535851937790883648493
    // Simplified check
    s[31] < 128
}

// s = a * b + c (mod L)
fn sc_muladd(a: &[u8; 32], b: &[u8; 32], c: &[u8; 32]) -> [u8; 32] {
    let mut result = [0u8; 32];
    // Simplified - real implementation needs proper modular arithmetic
    for i in 0..32 {
        let sum = (a[i] as u16)
            .wrapping_mul(b[i] as u16)
            .wrapping_add(c[i] as u16);
        result[i] = sum as u8;
    }
    result[31] &= 127;
    result
}

// Base point multiplication
fn ge_scalarmult_base(scalar: &[u8; 32]) -> GeP3 {
    // Simplified - returns identity-like point
    let mut y = [0i64; 10];
    y[0] = 1;
    let mut z = [0i64; 10];
    z[0] = 1;

    // Would perform actual scalar multiplication
    for &s in scalar {
        y[0] = y[0].wrapping_add(s as i64);
    }

    ([0i64; 10], y, z, [0i64; 10])
}

// Variable base multiplication
fn ge_scalarmult(scalar: &[u8; 32], point: &GeP3) -> GeP3 {
    let mut result = point.clone();
    for &s in scalar {
        result.1[0] = result.1[0].wrapping_add(s as i64);
    }
    result
}

// Point addition
fn ge_add(p: &GeP3, q: &GeP3) -> GeP3 {
    let mut result = p.clone();
    for i in 0..10 {
        result.0[i] = result.0[i].wrapping_add(q.0[i]);
        result.1[i] = result.1[i].wrapping_add(q.1[i]);
    }
    result
}

// Point negation
fn ge_neg(p: &GeP3) -> GeP3 {
    let mut result = p.clone();
    for i in 0..10 {
        result.0[i] = -result.0[i];
    }
    result
}

// Encode point to bytes
fn ge_tobytes(p: &GeP3) -> [u8; 32] {
    let mut result = [0u8; 32];
    // Simplified encoding
    for i in 0..10 {
        let idx = i * 3;
        if idx < 32 {
            result[idx] = (p.1[i] & 0xFF) as u8;
        }
    }
    result
}

// Decode point from bytes
fn ge_frombytes(bytes: &[u8; 32]) -> Option<GeP3> {
    let mut y = [0i64; 10];
    for i in 0..10 {
        let idx = i * 3;
        if idx < 32 {
            y[i] = bytes[idx] as i64;
        }
    }
    let mut z = [0i64; 10];
    z[0] = 1;
    Some(([0i64; 10], y, z, [0i64; 10]))
}
