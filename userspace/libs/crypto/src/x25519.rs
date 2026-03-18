//! X25519 key exchange
//!
//! Elliptic curve Diffie-Hellman using Curve25519. Montgomery ladder
//! implementation — constant-time, no branching on secrets.
//! — ColdCipher: "One scalar multiplication to rule all key exchanges."

use crate::{CryptoError, CryptoResult};

/// X25519 secret key (32 bytes, clamped)
#[derive(Clone)]
pub struct X25519SecretKey([u8; 32]);

/// X25519 public key (32 bytes, u-coordinate on Curve25519)
#[derive(Clone, PartialEq, Eq)]
pub struct X25519PublicKey([u8; 32]);

/// Shared secret (32 bytes)
#[derive(Clone)]
pub struct SharedSecret([u8; 32]);

impl X25519SecretKey {
    /// Create from bytes — applies clamping per RFC 7748
    pub fn from_bytes(bytes: &[u8]) -> CryptoResult<Self> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength);
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(bytes);
        // Clamp: clear bottom 3 bits, clear top bit, set second-to-top bit
        key[0] &= 248;
        key[31] &= 127;
        key[31] |= 64;
        Ok(X25519SecretKey(key))
    }

    /// Generate from random bytes — caller provides 32 random bytes
    /// — ColdCipher: "Your entropy, my clamping. Together we make a proper scalar."
    pub fn generate(random: &[u8; 32]) -> Self {
        let mut key = *random;
        key[0] &= 248;
        key[31] &= 127;
        key[31] |= 64;
        X25519SecretKey(key)
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Compute public key via base point multiplication
    /// — ColdCipher: "u=9 times your scalar. The one multiplication that starts every handshake."
    pub fn public_key(&self) -> X25519PublicKey {
        let mut base = [0u8; 32];
        base[0] = 9; // Base point u = 9
        let result = x25519_scalarmult(&self.0, &base);
        X25519PublicKey(result)
    }

    /// Perform Diffie-Hellman key exchange
    pub fn diffie_hellman(&self, their_public: &X25519PublicKey) -> SharedSecret {
        let result = x25519_scalarmult(&self.0, &their_public.0);
        SharedSecret(result)
    }
}

impl X25519PublicKey {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> CryptoResult<Self> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength);
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(bytes);
        Ok(X25519PublicKey(key))
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl SharedSecret {
    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// X25519 scalar multiplication — Montgomery ladder on Curve25519
/// — ColdCipher: "255 iterations. Same cost whether bit is 0 or 1. Timing attackers weep."
fn x25519_scalarmult(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    // Field element: 10 limbs of ~25.5 bits each
    type Fe = [i64; 10];

    fn fe_0() -> Fe {
        [0; 10]
    }
    fn fe_1() -> Fe {
        let mut r = [0; 10];
        r[0] = 1;
        r
    }

    fn fe_add(f: &Fe, g: &Fe) -> Fe {
        let mut h = [0i64; 10];
        for i in 0..10 {
            h[i] = f[i] + g[i];
        }
        h
    }

    fn fe_sub(f: &Fe, g: &Fe) -> Fe {
        let mut h = [0i64; 10];
        for i in 0..10 {
            h[i] = f[i] - g[i];
        }
        h
    }

    fn fe_mul(f: &Fe, g: &Fe) -> Fe {
        let mut h = [0i128; 10];
        for i in 0..10 {
            for j in 0..10 {
                let idx = (i + j) % 10;
                let mult = if i + j >= 10 { 38 } else { 1 };
                h[idx] += (f[i] as i128) * (g[j] as i128) * mult as i128;
            }
        }
        let mut result = [0i64; 10];
        for i in 0..10 {
            result[i] = h[i] as i64;
        }
        fe_reduce(&result)
    }

    fn fe_sq(f: &Fe) -> Fe {
        fe_mul(f, f)
    }

    fn fe_reduce(h: &Fe) -> Fe {
        let mut r = *h;
        for i in 0..10 {
            let carry = r[i] >> 25;
            r[i] -= carry << 25;
            if i < 9 {
                r[i + 1] += carry;
            } else {
                r[0] += carry * 19;
            }
        }
        r
    }

    fn fe_invert(z: &Fe) -> Fe {
        // z^(p-2) where p = 2^255 - 19 — Fermat's little theorem inversion
        let mut t0 = fe_sq(z);
        let mut t1 = fe_sq(&t0);
        t1 = fe_sq(&t1);
        t1 = fe_mul(z, &t1);
        t0 = fe_mul(&t0, &t1);
        let mut t2 = fe_sq(&t0);
        t1 = fe_mul(&t1, &t2);
        t2 = fe_sq(&t1);
        for _ in 0..4 {
            t2 = fe_sq(&t2);
        }
        t1 = fe_mul(&t1, &t2);
        t2 = fe_sq(&t1);
        for _ in 0..9 {
            t2 = fe_sq(&t2);
        }
        t2 = fe_mul(&t2, &t1);
        let mut t3 = fe_sq(&t2);
        for _ in 0..19 {
            t3 = fe_sq(&t3);
        }
        t2 = fe_mul(&t2, &t3);
        t2 = fe_sq(&t2);
        for _ in 0..9 {
            t2 = fe_sq(&t2);
        }
        t1 = fe_mul(&t1, &t2);
        t2 = fe_sq(&t1);
        for _ in 0..49 {
            t2 = fe_sq(&t2);
        }
        t2 = fe_mul(&t2, &t1);
        t3 = fe_sq(&t2);
        for _ in 0..99 {
            t3 = fe_sq(&t3);
        }
        t2 = fe_mul(&t2, &t3);
        t2 = fe_sq(&t2);
        for _ in 0..49 {
            t2 = fe_sq(&t2);
        }
        t1 = fe_mul(&t1, &t2);
        t1 = fe_sq(&t1);
        for _ in 0..4 {
            t1 = fe_sq(&t1);
        }
        fe_mul(&t0, &t1)
    }

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
        h[9] = load3(&s[29..32]) << 2;
        fe_reduce(&h)
    }

    fn fe_tobytes(h: &Fe) -> [u8; 32] {
        let mut s = [0u8; 32];
        let q = fe_reduce(h);
        for i in 0..32 {
            s[i] = (q[i * 10 / 32] >> ((i * 10) % 32)) as u8;
        }
        s
    }

    fn load3(s: &[u8]) -> i64 {
        (s[0] as i64) | ((s[1] as i64) << 8) | ((s[2] as i64) << 16)
    }

    fn load4(s: &[u8]) -> i64 {
        (s[0] as i64) | ((s[1] as i64) << 8) | ((s[2] as i64) << 16) | ((s[3] as i64) << 24)
    }

    // Montgomery ladder — constant-time scalar multiplication
    let x1 = fe_frombytes(point);
    let mut x2 = fe_1();
    let mut z2 = fe_0();
    let mut x3 = x1;
    let mut z3 = fe_1();
    let mut swap: i64 = 0;

    for i in (0..255).rev() {
        let bit = ((scalar[i >> 3] >> (i & 7)) & 1) as i64;
        let swap_new = swap ^ bit;

        // Conditional swap — branchless, timing-safe
        for j in 0..10 {
            let dummy = swap_new * (x2[j] ^ x3[j]);
            x2[j] ^= dummy;
            x3[j] ^= dummy;
            let dummy = swap_new * (z2[j] ^ z3[j]);
            z2[j] ^= dummy;
            z3[j] ^= dummy;
        }
        swap = bit;

        let a = fe_add(&x2, &z2);
        let aa = fe_sq(&a);
        let b = fe_sub(&x2, &z2);
        let bb = fe_sq(&b);
        let e = fe_sub(&aa, &bb);
        let c = fe_add(&x3, &z3);
        let d = fe_sub(&x3, &z3);
        let da = fe_mul(&d, &a);
        let cb = fe_mul(&c, &b);
        x3 = fe_sq(&fe_add(&da, &cb));
        z3 = fe_mul(&x1, &fe_sq(&fe_sub(&da, &cb)));
        x2 = fe_mul(&aa, &bb);
        let a121665 = {
            let mut f = fe_0();
            f[0] = 121665;
            f
        };
        z2 = fe_mul(&e, &fe_add(&aa, &fe_mul(&a121665, &e)));
    }

    // Final conditional swap
    for j in 0..10 {
        let dummy = swap * (x2[j] ^ x3[j]);
        x2[j] ^= dummy;
        x3[j] ^= dummy;
        let dummy = swap * (z2[j] ^ z3[j]);
        z2[j] ^= dummy;
        z3[j] ^= dummy;
    }

    // Recover affine coordinate: x2 * z2^(-1)
    let z2_inv = fe_invert(&z2);
    let result = fe_mul(&x2, &z2_inv);
    fe_tobytes(&result)
}
