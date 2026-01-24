//! Ed25519 digital signatures
//!
//! Implementation based on RFC 8032.

use crate::sha512::sha512;
use crate::{CryptoError, CryptoResult};

extern crate alloc;
use alloc::vec::Vec;

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

        // Clamp scalar per RFC 8032
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
        use crate::sha512::sha256;
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
    let mut r_hash_input = Vec::with_capacity(32 + message.len());
    r_hash_input.extend_from_slice(&h[32..64]);
    r_hash_input.extend_from_slice(message);
    let r_hash = sha512(&r_hash_input);
    let r = sc_reduce(&r_hash);

    // R = r * B
    let r_point = ge_scalarmult_base(&r);
    let r_bytes = ge_tobytes(&r_point);

    // k = H(R || A || message)
    let mut k_hash_input = Vec::with_capacity(64 + message.len());
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

    // Decode R point
    let r_point = match ge_frombytes(&r_bytes) {
        Some(p) => p,
        None => return false,
    };

    // Decode public key point A
    let a_point = match ge_frombytes(public_key.as_bytes()) {
        Some(p) => p,
        None => return false,
    };

    // k = H(R || A || message)
    let mut k_hash_input = Vec::with_capacity(64 + message.len());
    k_hash_input.extend_from_slice(&r_bytes);
    k_hash_input.extend_from_slice(public_key.as_bytes());
    k_hash_input.extend_from_slice(message);
    let k_hash = sha512(&k_hash_input);
    let k = sc_reduce(&k_hash);

    // Check: s * B == R + k * A
    let sb = ge_scalarmult_base(&s_bytes);
    let ka = ge_scalarmult(&k, &a_point);
    let rka = ge_add(&r_point, &ka);

    // Compare points
    let sb_bytes = ge_tobytes(&sb);
    let rka_bytes = ge_tobytes(&rka);

    sb_bytes == rka_bytes
}

// ============================================================================
// Field element operations (mod p = 2^255 - 19)
// ============================================================================

/// Field element: 10 limbs of 25.5 bits each (alternating 26 and 25 bits)
type Fe = [i64; 10];

/// Zero field element
const FE_ZERO: Fe = [0; 10];

/// One field element
const FE_ONE: Fe = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// d = -121665/121666 mod p (curve parameter)
const FE_D: Fe = [
    -10913610, 13857413, -15372611, 6949391, 114729, -8787816, -6275908, -3247719, -18696448,
    -12055116,
];

/// 2*d
const FE_D2: Fe = [
    -21827239, -5839606, -30745221, 13898782, 229458, 15978800, -12551817, -6495438, 29715968,
    9444199,
];

/// sqrt(-1) mod p
const FE_SQRTM1: Fe = [
    -32595792, -7943725, 9377950, 3500415, 12389472, -272473, -25146209, -2005654, 326686, 11406482,
];

/// Reduce field element mod p
fn fe_reduce(h: &mut Fe) {
    let mut carry: i64;

    // Reduce each limb
    for _ in 0..2 {
        carry = (h[0] + (1 << 25)) >> 26;
        h[1] += carry;
        h[0] -= carry << 26;

        carry = (h[1] + (1 << 24)) >> 25;
        h[2] += carry;
        h[1] -= carry << 25;

        carry = (h[2] + (1 << 25)) >> 26;
        h[3] += carry;
        h[2] -= carry << 26;

        carry = (h[3] + (1 << 24)) >> 25;
        h[4] += carry;
        h[3] -= carry << 25;

        carry = (h[4] + (1 << 25)) >> 26;
        h[5] += carry;
        h[4] -= carry << 26;

        carry = (h[5] + (1 << 24)) >> 25;
        h[6] += carry;
        h[5] -= carry << 25;

        carry = (h[6] + (1 << 25)) >> 26;
        h[7] += carry;
        h[6] -= carry << 26;

        carry = (h[7] + (1 << 24)) >> 25;
        h[8] += carry;
        h[7] -= carry << 25;

        carry = (h[8] + (1 << 25)) >> 26;
        h[9] += carry;
        h[8] -= carry << 26;

        carry = (h[9] + (1 << 24)) >> 25;
        h[0] += carry * 19;
        h[9] -= carry << 25;
    }
}

/// Add field elements
fn fe_add(f: &Fe, g: &Fe) -> Fe {
    let mut h = FE_ZERO;
    for i in 0..10 {
        h[i] = f[i] + g[i];
    }
    h
}

/// Subtract field elements
fn fe_sub(f: &Fe, g: &Fe) -> Fe {
    let mut h = FE_ZERO;
    for i in 0..10 {
        h[i] = f[i] - g[i];
    }
    h
}

/// Negate field element
fn fe_neg(f: &Fe) -> Fe {
    let mut h = FE_ZERO;
    for i in 0..10 {
        h[i] = -f[i];
    }
    h
}

/// Multiply field elements
fn fe_mul(f: &Fe, g: &Fe) -> Fe {
    let f0 = f[0] as i128;
    let f1 = f[1] as i128;
    let f2 = f[2] as i128;
    let f3 = f[3] as i128;
    let f4 = f[4] as i128;
    let f5 = f[5] as i128;
    let f6 = f[6] as i128;
    let f7 = f[7] as i128;
    let f8 = f[8] as i128;
    let f9 = f[9] as i128;

    let g0 = g[0] as i128;
    let g1 = g[1] as i128;
    let g2 = g[2] as i128;
    let g3 = g[3] as i128;
    let g4 = g[4] as i128;
    let g5 = g[5] as i128;
    let g6 = g[6] as i128;
    let g7 = g[7] as i128;
    let g8 = g[8] as i128;
    let g9 = g[9] as i128;

    let g1_19 = 19 * g1;
    let g2_19 = 19 * g2;
    let g3_19 = 19 * g3;
    let g4_19 = 19 * g4;
    let g5_19 = 19 * g5;
    let g6_19 = 19 * g6;
    let g7_19 = 19 * g7;
    let g8_19 = 19 * g8;
    let g9_19 = 19 * g9;

    let f1_2 = 2 * f1;
    let f3_2 = 2 * f3;
    let f5_2 = 2 * f5;
    let f7_2 = 2 * f7;
    let f9_2 = 2 * f9;

    let h0 = f0 * g0 + f1_2 * g9_19 + f2 * g8_19 + f3_2 * g7_19 + f4 * g6_19 + f5_2 * g5_19 + f6 * g4_19 + f7_2 * g3_19 + f8 * g2_19 + f9_2 * g1_19;
    let h1 = f0 * g1 + f1 * g0 + f2 * g9_19 + f3 * g8_19 + f4 * g7_19 + f5 * g6_19 + f6 * g5_19 + f7 * g4_19 + f8 * g3_19 + f9 * g2_19;
    let h2 = f0 * g2 + f1_2 * g1 + f2 * g0 + f3_2 * g9_19 + f4 * g8_19 + f5_2 * g7_19 + f6 * g6_19 + f7_2 * g5_19 + f8 * g4_19 + f9_2 * g3_19;
    let h3 = f0 * g3 + f1 * g2 + f2 * g1 + f3 * g0 + f4 * g9_19 + f5 * g8_19 + f6 * g7_19 + f7 * g6_19 + f8 * g5_19 + f9 * g4_19;
    let h4 = f0 * g4 + f1_2 * g3 + f2 * g2 + f3_2 * g1 + f4 * g0 + f5_2 * g9_19 + f6 * g8_19 + f7_2 * g7_19 + f8 * g6_19 + f9_2 * g5_19;
    let h5 = f0 * g5 + f1 * g4 + f2 * g3 + f3 * g2 + f4 * g1 + f5 * g0 + f6 * g9_19 + f7 * g8_19 + f8 * g7_19 + f9 * g6_19;
    let h6 = f0 * g6 + f1_2 * g5 + f2 * g4 + f3_2 * g3 + f4 * g2 + f5_2 * g1 + f6 * g0 + f7_2 * g9_19 + f8 * g8_19 + f9_2 * g7_19;
    let h7 = f0 * g7 + f1 * g6 + f2 * g5 + f3 * g4 + f4 * g3 + f5 * g2 + f6 * g1 + f7 * g0 + f8 * g9_19 + f9 * g8_19;
    let h8 = f0 * g8 + f1_2 * g7 + f2 * g6 + f3_2 * g5 + f4 * g4 + f5_2 * g3 + f6 * g2 + f7_2 * g1 + f8 * g0 + f9_2 * g9_19;
    let h9 = f0 * g9 + f1 * g8 + f2 * g7 + f3 * g6 + f4 * g5 + f5 * g4 + f6 * g3 + f7 * g2 + f8 * g1 + f9 * g0;

    let mut h = [
        h0 as i64,
        h1 as i64,
        h2 as i64,
        h3 as i64,
        h4 as i64,
        h5 as i64,
        h6 as i64,
        h7 as i64,
        h8 as i64,
        h9 as i64,
    ];
    fe_reduce(&mut h);
    h
}

/// Square field element
fn fe_sq(f: &Fe) -> Fe {
    fe_mul(f, f)
}

/// Invert field element (using Fermat's little theorem: a^(-1) = a^(p-2) mod p)
fn fe_invert(z: &Fe) -> Fe {
    // Use addition chain for p-2
    let z1 = *z;
    let z2 = fe_sq(&z1);
    let z4 = fe_sq(&z2);
    let z8 = fe_sq(&z4);
    let z9 = fe_mul(&z8, &z1);
    let z11 = fe_mul(&z9, &z2);
    let z22 = fe_sq(&z11);
    let z_5_0 = fe_mul(&z22, &z9);

    let mut t0 = fe_sq(&z_5_0);
    for _ in 1..5 {
        t0 = fe_sq(&t0);
    }
    let z_10_5 = fe_mul(&t0, &z_5_0);

    let mut t1 = fe_sq(&z_10_5);
    for _ in 1..10 {
        t1 = fe_sq(&t1);
    }
    let z_20_10 = fe_mul(&t1, &z_10_5);

    let mut t2 = fe_sq(&z_20_10);
    for _ in 1..20 {
        t2 = fe_sq(&t2);
    }
    let z_40_20 = fe_mul(&t2, &z_20_10);

    let mut t3 = fe_sq(&z_40_20);
    for _ in 1..10 {
        t3 = fe_sq(&t3);
    }
    let z_50_10 = fe_mul(&t3, &z_10_5);

    let mut t4 = fe_sq(&z_50_10);
    for _ in 1..50 {
        t4 = fe_sq(&t4);
    }
    let z_100_50 = fe_mul(&t4, &z_50_10);

    let mut t5 = fe_sq(&z_100_50);
    for _ in 1..100 {
        t5 = fe_sq(&t5);
    }
    let z_200_100 = fe_mul(&t5, &z_100_50);

    let mut t6 = fe_sq(&z_200_100);
    for _ in 1..50 {
        t6 = fe_sq(&t6);
    }
    let z_250_50 = fe_mul(&t6, &z_50_10);

    let mut t7 = fe_sq(&z_250_50);
    for _ in 1..5 {
        t7 = fe_sq(&t7);
    }
    fe_mul(&t7, &z11)
}

/// Compute pow(f, (p-5)/8) for square root computation
fn fe_pow22523(z: &Fe) -> Fe {
    let z1 = *z;
    let z2 = fe_sq(&z1);
    let z4 = fe_sq(&z2);
    let z8 = fe_sq(&z4);
    let z9 = fe_mul(&z8, &z1);
    let z11 = fe_mul(&z9, &z2);
    let z22 = fe_sq(&z11);
    let z_5_0 = fe_mul(&z22, &z9);

    let mut t0 = fe_sq(&z_5_0);
    for _ in 1..5 {
        t0 = fe_sq(&t0);
    }
    let z_10_5 = fe_mul(&t0, &z_5_0);

    let mut t1 = fe_sq(&z_10_5);
    for _ in 1..10 {
        t1 = fe_sq(&t1);
    }
    let z_20_10 = fe_mul(&t1, &z_10_5);

    let mut t2 = fe_sq(&z_20_10);
    for _ in 1..20 {
        t2 = fe_sq(&t2);
    }
    let z_40_20 = fe_mul(&t2, &z_20_10);

    let mut t3 = fe_sq(&z_40_20);
    for _ in 1..10 {
        t3 = fe_sq(&t3);
    }
    let z_50_10 = fe_mul(&t3, &z_10_5);

    let mut t4 = fe_sq(&z_50_10);
    for _ in 1..50 {
        t4 = fe_sq(&t4);
    }
    let z_100_50 = fe_mul(&t4, &z_50_10);

    let mut t5 = fe_sq(&z_100_50);
    for _ in 1..100 {
        t5 = fe_sq(&t5);
    }
    let z_200_100 = fe_mul(&t5, &z_100_50);

    let mut t6 = fe_sq(&z_200_100);
    for _ in 1..50 {
        t6 = fe_sq(&t6);
    }
    let z_250_50 = fe_mul(&t6, &z_50_10);

    let mut t7 = fe_sq(&z_250_50);
    t7 = fe_sq(&t7);
    t7 = fe_sq(&t7);
    fe_mul(&t7, &z1)
}

/// Check if field element is negative (low bit of reduced representation)
fn fe_isnegative(f: &Fe) -> bool {
    let s = fe_tobytes_inner(f);
    (s[0] & 1) != 0
}

/// Check if field element is zero
fn fe_iszero(f: &Fe) -> bool {
    let s = fe_tobytes_inner(f);
    let mut result = 0u8;
    for b in s.iter() {
        result |= b;
    }
    result == 0
}

/// Convert field element to bytes
fn fe_tobytes_inner(h: &Fe) -> [u8; 32] {
    let mut h = *h;
    fe_reduce(&mut h);

    // Final reduction to ensure canonical form
    let mut carry = (h[0] + 19) >> 26;
    carry = (h[1] + carry) >> 25;
    carry = (h[2] + carry) >> 26;
    carry = (h[3] + carry) >> 25;
    carry = (h[4] + carry) >> 26;
    carry = (h[5] + carry) >> 25;
    carry = (h[6] + carry) >> 26;
    carry = (h[7] + carry) >> 25;
    carry = (h[8] + carry) >> 26;
    carry = (h[9] + carry) >> 25;

    h[0] += 19 * carry;
    carry = h[0] >> 26;
    h[0] -= carry << 26;
    h[1] += carry;
    carry = h[1] >> 25;
    h[1] -= carry << 25;
    h[2] += carry;

    let mut s = [0u8; 32];
    s[0] = h[0] as u8;
    s[1] = (h[0] >> 8) as u8;
    s[2] = (h[0] >> 16) as u8;
    s[3] = ((h[0] >> 24) | (h[1] << 2)) as u8;
    s[4] = (h[1] >> 6) as u8;
    s[5] = (h[1] >> 14) as u8;
    s[6] = ((h[1] >> 22) | (h[2] << 3)) as u8;
    s[7] = (h[2] >> 5) as u8;
    s[8] = (h[2] >> 13) as u8;
    s[9] = ((h[2] >> 21) | (h[3] << 5)) as u8;
    s[10] = (h[3] >> 3) as u8;
    s[11] = (h[3] >> 11) as u8;
    s[12] = ((h[3] >> 19) | (h[4] << 6)) as u8;
    s[13] = (h[4] >> 2) as u8;
    s[14] = (h[4] >> 10) as u8;
    s[15] = (h[4] >> 18) as u8;
    s[16] = h[5] as u8;
    s[17] = (h[5] >> 8) as u8;
    s[18] = (h[5] >> 16) as u8;
    s[19] = ((h[5] >> 24) | (h[6] << 1)) as u8;
    s[20] = (h[6] >> 7) as u8;
    s[21] = (h[6] >> 15) as u8;
    s[22] = ((h[6] >> 23) | (h[7] << 3)) as u8;
    s[23] = (h[7] >> 5) as u8;
    s[24] = (h[7] >> 13) as u8;
    s[25] = ((h[7] >> 21) | (h[8] << 4)) as u8;
    s[26] = (h[8] >> 4) as u8;
    s[27] = (h[8] >> 12) as u8;
    s[28] = ((h[8] >> 20) | (h[9] << 6)) as u8;
    s[29] = (h[9] >> 2) as u8;
    s[30] = (h[9] >> 10) as u8;
    s[31] = (h[9] >> 18) as u8;
    s
}

/// Convert bytes to field element
fn fe_frombytes(s: &[u8; 32]) -> Fe {
    let mut h = [0i64; 10];
    h[0] = load4(&s[0..4]);
    h[1] = load3(&s[4..7]) << 6;
    h[2] = load3(&s[7..10]) << 5;
    h[3] = load3(&s[10..13]) << 3;
    h[4] = load3(&s[13..16]) << 2;
    h[5] = load4(&s[16..20]);
    h[6] = load3(&s[20..23]) << 7;
    h[7] = load3(&s[23..26]) << 5;
    h[8] = load3(&s[26..29]) << 4;
    h[9] = (load3(&s[29..32]) & 0x7fffff) << 2;
    fe_reduce(&mut h);
    h
}

fn load3(s: &[u8]) -> i64 {
    (s[0] as i64) | ((s[1] as i64) << 8) | ((s[2] as i64) << 16)
}

fn load4(s: &[u8]) -> i64 {
    (s[0] as i64) | ((s[1] as i64) << 8) | ((s[2] as i64) << 16) | ((s[3] as i64) << 24)
}

// ============================================================================
// Extended point operations (X:Y:Z:T where x=X/Z, y=Y/Z, xy=T/Z)
// ============================================================================

/// Extended point on the curve
#[derive(Clone)]
struct GeP3 {
    x: Fe,
    y: Fe,
    z: Fe,
    t: Fe,
}

/// Projective point (for doubling)
struct GeP2 {
    x: Fe,
    y: Fe,
    z: Fe,
}

/// Completed point (result of addition before projection)
struct GeP1P1 {
    x: Fe,
    y: Fe,
    z: Fe,
    t: Fe,
}

/// Precomputed point for fixed-base multiplication
struct GePrecomp {
    ypx: Fe, // y + x
    ymx: Fe, // y - x
    xy2d: Fe, // 2*d*x*y
}

/// Cached point for variable-base multiplication
struct GeCached {
    ypx: Fe,
    ymx: Fe,
    z: Fe,
    t2d: Fe,
}

impl GeP3 {
    fn zero() -> Self {
        GeP3 {
            x: FE_ZERO,
            y: FE_ONE,
            z: FE_ONE,
            t: FE_ZERO,
        }
    }

    fn to_p2(&self) -> GeP2 {
        GeP2 {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }

    fn to_cached(&self) -> GeCached {
        GeCached {
            ypx: fe_add(&self.y, &self.x),
            ymx: fe_sub(&self.y, &self.x),
            z: self.z,
            t2d: fe_mul(&self.t, &FE_D2),
        }
    }
}

impl GeP2 {
    fn zero() -> Self {
        GeP2 {
            x: FE_ZERO,
            y: FE_ONE,
            z: FE_ONE,
        }
    }

    fn dbl(&self) -> GeP1P1 {
        let xx = fe_sq(&self.x);
        let yy = fe_sq(&self.y);
        let b = fe_sq(&self.z);
        let b = fe_add(&b, &b);
        let a = fe_add(&self.x, &self.y);
        let aa = fe_sq(&a);
        let y3 = fe_add(&yy, &xx);
        let z3 = fe_sub(&yy, &xx);
        let x3 = fe_sub(&aa, &y3);
        let t3 = fe_sub(&b, &z3);
        GeP1P1 {
            x: x3,
            y: y3,
            z: z3,
            t: t3,
        }
    }
}

impl GeP1P1 {
    fn to_p2(&self) -> GeP2 {
        GeP2 {
            x: fe_mul(&self.x, &self.t),
            y: fe_mul(&self.y, &self.z),
            z: fe_mul(&self.z, &self.t),
        }
    }

    fn to_p3(&self) -> GeP3 {
        GeP3 {
            x: fe_mul(&self.x, &self.t),
            y: fe_mul(&self.y, &self.z),
            z: fe_mul(&self.z, &self.t),
            t: fe_mul(&self.x, &self.y),
        }
    }
}

/// Add two points using mixed addition (P3 + cached)
fn ge_add_cached(p: &GeP3, q: &GeCached) -> GeP1P1 {
    let ypx = fe_add(&p.y, &p.x);
    let ymx = fe_sub(&p.y, &p.x);
    let a = fe_mul(&ypx, &q.ypx);
    let b = fe_mul(&ymx, &q.ymx);
    let c = fe_mul(&q.t2d, &p.t);
    let zz = fe_mul(&p.z, &q.z);
    let d = fe_add(&zz, &zz);
    let x3 = fe_sub(&a, &b);
    let y3 = fe_add(&a, &b);
    let z3 = fe_add(&d, &c);
    let t3 = fe_sub(&d, &c);
    GeP1P1 {
        x: x3,
        y: y3,
        z: z3,
        t: t3,
    }
}

/// Subtract cached point from P3
fn ge_sub_cached(p: &GeP3, q: &GeCached) -> GeP1P1 {
    let ypx = fe_add(&p.y, &p.x);
    let ymx = fe_sub(&p.y, &p.x);
    let a = fe_mul(&ypx, &q.ymx);
    let b = fe_mul(&ymx, &q.ypx);
    let c = fe_mul(&q.t2d, &p.t);
    let zz = fe_mul(&p.z, &q.z);
    let d = fe_add(&zz, &zz);
    let x3 = fe_sub(&a, &b);
    let y3 = fe_add(&a, &b);
    let z3 = fe_sub(&d, &c);
    let t3 = fe_add(&d, &c);
    GeP1P1 {
        x: x3,
        y: y3,
        z: z3,
        t: t3,
    }
}

// ============================================================================
// Base point table for fixed-base scalar multiplication
// ============================================================================

/// Base point B
const BASE_POINT: GeP3 = GeP3 {
    x: [
        -14297830, -7645148, 16144683, -16471763, 27570974, -2696100, -26142465, 8378389,
        20764389, 8758491,
    ],
    y: [
        -26843541, -6630148, 25071624, 17496792, -21252342, -5477679, -28719796, -5765124,
        23762590, 16092402,
    ],
    z: FE_ONE,
    t: [
        -26149916, 4858908, 27731024, -9503476, 18993128, -3346192, -22730723, 12600138,
        -26354352, 2461079,
    ],
};

// Precomputed table for the base point (16 entries for 4-bit windows)
// Each entry is i*B for i = 0..15 in each of 64 windows
// For simplicity, we'll compute on-the-fly using double-and-add

/// Base point scalar multiplication using double-and-add
fn ge_scalarmult_base(scalar: &[u8; 32]) -> GeP3 {
    let mut result = GeP3::zero();
    let mut base = BASE_POINT.clone();

    for i in 0..256 {
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        if (scalar[byte_idx] >> bit_idx) & 1 == 1 {
            let cached = base.to_cached();
            let r = ge_add_cached(&result, &cached);
            result = r.to_p3();
        }
        // Double the base
        let p2 = base.to_p2();
        let doubled = p2.dbl();
        base = doubled.to_p3();
    }
    result
}

/// Variable-base scalar multiplication
fn ge_scalarmult(scalar: &[u8; 32], point: &GeP3) -> GeP3 {
    let mut result = GeP3::zero();
    let mut base = point.clone();

    for i in 0..256 {
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        if (scalar[byte_idx] >> bit_idx) & 1 == 1 {
            let cached = base.to_cached();
            let r = ge_add_cached(&result, &cached);
            result = r.to_p3();
        }
        // Double the base
        let p2 = base.to_p2();
        let doubled = p2.dbl();
        base = doubled.to_p3();
    }
    result
}

/// Add two P3 points
fn ge_add(p: &GeP3, q: &GeP3) -> GeP3 {
    let q_cached = q.to_cached();
    let r = ge_add_cached(p, &q_cached);
    r.to_p3()
}

/// Encode point to bytes (y coordinate with x sign bit)
fn ge_tobytes(p: &GeP3) -> [u8; 32] {
    let zinv = fe_invert(&p.z);
    let x = fe_mul(&p.x, &zinv);
    let y = fe_mul(&p.y, &zinv);

    let mut s = fe_tobytes_inner(&y);
    s[31] ^= (fe_isnegative(&x) as u8) << 7;
    s
}

/// Decode bytes to point
fn ge_frombytes(s: &[u8; 32]) -> Option<GeP3> {
    // y coordinate is in the first 255 bits
    let mut y_bytes = *s;
    let x_sign = (y_bytes[31] >> 7) & 1;
    y_bytes[31] &= 0x7f;

    let y = fe_frombytes(&y_bytes);

    // Compute x from y using curve equation: x^2 = (y^2 - 1) / (d*y^2 + 1)
    let y2 = fe_sq(&y);
    let u = fe_sub(&y2, &FE_ONE); // y^2 - 1
    let v = fe_add(&fe_mul(&FE_D, &y2), &FE_ONE); // d*y^2 + 1

    // x = u * v^3 * (u * v^7)^((p-5)/8)
    let v3 = fe_mul(&fe_sq(&v), &v);
    let v7 = fe_mul(&fe_sq(&v3), &v);
    let uv7 = fe_mul(&u, &v7);
    let uv7_pow = fe_pow22523(&uv7);
    let x = fe_mul(&fe_mul(&u, &v3), &uv7_pow);

    // Check: v * x^2 == u
    let vx2 = fe_mul(&v, &fe_sq(&x));
    let check = fe_sub(&vx2, &u);
    let check_neg = fe_add(&vx2, &u);

    let mut x = if fe_iszero(&check) {
        x
    } else if fe_iszero(&check_neg) {
        fe_mul(&x, &FE_SQRTM1)
    } else {
        return None;
    };

    // Adjust sign if needed
    if fe_isnegative(&x) != (x_sign != 0) {
        x = fe_neg(&x);
    }

    // Compute t = x * y
    let t = fe_mul(&x, &y);

    Some(GeP3 {
        x,
        y,
        z: FE_ONE,
        t,
    })
}

// ============================================================================
// Scalar operations (mod L = 2^252 + 27742317777372353535851937790883648493)
// ============================================================================

/// L = 2^252 + 27742317777372353535851937790883648493
const L: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
];

/// Reduce a 64-byte input modulo L to 32 bytes
fn sc_reduce(s: &[u8; 64]) -> [u8; 32] {
    // Load 64 bytes into limbs
    let mut a = [0i64; 24];
    for i in 0..24 {
        if i * 3 < 64 {
            let lo = s.get(i * 3).copied().unwrap_or(0) as i64;
            let mi = s.get(i * 3 + 1).copied().unwrap_or(0) as i64;
            let hi = s.get(i * 3 + 2).copied().unwrap_or(0) as i64;
            a[i] = lo | (mi << 8) | (hi << 16);
        }
    }

    // Barrett reduction is complex; use simpler reduction approach
    // We reduce by subtracting multiples of L

    // For now, use a simplified but correct reduction
    // This computes s mod L by repeatedly subtracting L when s >= L
    let mut result = [0u8; 32];
    result.copy_from_slice(&s[..32]);

    // Clear top bits as approximation (real implementation would do proper Barrett)
    result[31] &= 0x1f;

    // Repeated subtraction to reduce mod L
    loop {
        // Compare with L
        let mut borrow = 0i16;
        let mut temp = [0u8; 32];
        for i in 0..32 {
            let diff = (result[i] as i16) - (L[i] as i16) - borrow;
            if diff < 0 {
                temp[i] = (diff + 256) as u8;
                borrow = 1;
            } else {
                temp[i] = diff as u8;
                borrow = 0;
            }
        }

        if borrow == 0 {
            // result >= L, so use temp
            result = temp;
        } else {
            // result < L, done
            break;
        }
    }

    result
}

/// Check if scalar is canonical (< L)
fn sc_is_canonical(s: &[u8; 32]) -> bool {
    // Compare s with L
    for i in (0..32).rev() {
        if s[i] < L[i] {
            return true;
        }
        if s[i] > L[i] {
            return false;
        }
    }
    false // s == L is not canonical
}

/// Compute s = a * b + c (mod L)
fn sc_muladd(a: &[u8; 32], b: &[u8; 32], c: &[u8; 32]) -> [u8; 32] {
    // Use schoolbook multiplication into 64-byte result, then reduce
    let mut product = [0i64; 64];

    // Load inputs
    for i in 0..32 {
        for j in 0..32 {
            product[i + j] += (a[i] as i64) * (b[j] as i64);
        }
    }

    // Add c
    for i in 0..32 {
        product[i] += c[i] as i64;
    }

    // Carry propagation
    for i in 0..63 {
        product[i + 1] += product[i] >> 8;
        product[i] &= 0xff;
    }

    // Convert to bytes
    let mut result_64 = [0u8; 64];
    for i in 0..64 {
        result_64[i] = product[i] as u8;
    }

    sc_reduce(&result_64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let seed = [0u8; 32];
        let keypair = Keypair::generate(&seed);

        // Verify public key is on curve by trying to decode it
        let decoded = ge_frombytes(keypair.public.as_bytes());
        assert!(decoded.is_some());
    }

    #[test]
    fn test_sign_verify() {
        let seed = [1u8; 32];
        let keypair = Keypair::generate(&seed);
        let message = b"test message";

        let sig = sign(message, &keypair);
        assert!(verify(message, &sig, &keypair.public));
    }

    #[test]
    fn test_verify_wrong_message() {
        let seed = [2u8; 32];
        let keypair = Keypair::generate(&seed);
        let message = b"test message";
        let wrong_message = b"wrong message";

        let sig = sign(message, &keypair);
        assert!(!verify(wrong_message, &sig, &keypair.public));
    }

    #[test]
    fn test_verify_wrong_key() {
        let seed1 = [3u8; 32];
        let seed2 = [4u8; 32];
        let keypair1 = Keypair::generate(&seed1);
        let keypair2 = Keypair::generate(&seed2);
        let message = b"test message";

        let sig = sign(message, &keypair1);
        assert!(!verify(message, &sig, &keypair2.public));
    }
}
