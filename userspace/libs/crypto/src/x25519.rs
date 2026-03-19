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
///
/// 5×51-bit limb representation. Schoolbook multiply, carry-propagate reduce.
/// Every branch depends only on public loop indices — never on secret bits.
/// — ColdCipher: "I've seen implementations cut corners on reduction. They all end up here eventually."
fn x25519_scalarmult(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    // — ColdCipher: "Five limbs, 51 bits each. Clean, wide, and u128 products
    //   never overflow. The way Bernstein intended before everyone got clever."
    const MASK51: u64 = (1u64 << 51) - 1;

    /// Field element in GF(2^255-19): five u64 limbs, each < 2^51
    type Fe = [u64; 5];

    #[inline(always)]
    fn fe_zero() -> Fe {
        [0u64; 5]
    }

    #[inline(always)]
    fn fe_one() -> Fe {
        [1, 0, 0, 0, 0]
    }

    /// — ColdCipher: "Addition. No carry. Limbs stay bounded because we reduce after mul/sq."
    #[inline(always)]
    fn fe_add(f: &Fe, g: &Fe) -> Fe {
        [
            f[0] + g[0],
            f[1] + g[1],
            f[2] + g[2],
            f[3] + g[3],
            f[4] + g[4],
        ]
    }

    /// Subtraction with bias to keep limbs positive.
    /// — ColdCipher: "Add 2*p before subtracting. Underflow is the silent killer of field arithmetic."
    #[inline(always)]
    fn fe_sub(f: &Fe, g: &Fe) -> Fe {
        // Add 2*p to prevent underflow. p = [2^51-19, 2^51-1, 2^51-1, 2^51-1, 2^51-1]
        // — ColdCipher: "Get the bias wrong by a factor of 2 and watch 255 ladder steps
        //   each amplify the error. Learned that one the hard way."
        const BIAS: [u64; 5] = [
            0xFFFFFFFFFFFDA,  // 2 * (2^51 - 19) = 2^52 - 38
            0xFFFFFFFFFFFFE,  // 2 * (2^51 - 1)  = 2^52 - 2
            0xFFFFFFFFFFFFE,
            0xFFFFFFFFFFFFE,
            0xFFFFFFFFFFFFE,
        ];
        [
            (f[0] + BIAS[0]) - g[0],
            (f[1] + BIAS[1]) - g[1],
            (f[2] + BIAS[2]) - g[2],
            (f[3] + BIAS[3]) - g[3],
            (f[4] + BIAS[4]) - g[4],
        ]
    }

    /// Carry-propagate reduction. Brings all limbs back under 2^51.
    /// — ColdCipher: "Overflow from limb 4 wraps with factor 19. That's the whole 2^255≡19 trick."
    #[inline(always)]
    fn fe_reduce(h: &mut Fe) {
        let mut c: u64;
        c = h[0] >> 51; h[0] &= MASK51; h[1] += c;
        c = h[1] >> 51; h[1] &= MASK51; h[2] += c;
        c = h[2] >> 51; h[2] &= MASK51; h[3] += c;
        c = h[3] >> 51; h[3] &= MASK51; h[4] += c;
        c = h[4] >> 51; h[4] &= MASK51; h[0] += c * 19;
        // — ColdCipher: "One more pass. The *19 on limb 0 might push it over 2^51 again."
        c = h[0] >> 51; h[0] &= MASK51; h[1] += c;
    }

    /// Schoolbook 5×5 multiply mod 2^255-19.
    /// — ColdCipher: "25 multiplications, 5 reductions. No Karatsuba tricks needed at this size.
    ///   u128 intermediate products — 102 bits max per term, 25 terms per accumulator,
    ///   still miles below u128 overflow. Sleep soundly."
    fn fe_mul(f: &Fe, g: &Fe) -> Fe {
        let f0 = f[0] as u128; let f1 = f[1] as u128; let f2 = f[2] as u128;
        let f3 = f[3] as u128; let f4 = f[4] as u128;
        let g0 = g[0] as u128; let g1 = g[1] as u128; let g2 = g[2] as u128;
        let g3 = g[3] as u128; let g4 = g[4] as u128;

        // Pre-multiply by 19 for the wrap-around terms: 2^255 ≡ 19 (mod p)
        let g1_19 = (g[1] as u128) * 19;
        let g2_19 = (g[2] as u128) * 19;
        let g3_19 = (g[3] as u128) * 19;
        let g4_19 = (g[4] as u128) * 19;

        // Accumulate products into 5 limbs
        let h0 = f0*g0 + f1*g4_19 + f2*g3_19 + f3*g2_19 + f4*g1_19;
        let h1 = f0*g1 + f1*g0   + f2*g4_19 + f3*g3_19 + f4*g2_19;
        let h2 = f0*g2 + f1*g1   + f2*g0    + f3*g4_19 + f4*g3_19;
        let h3 = f0*g3 + f1*g2   + f2*g1    + f3*g0    + f4*g4_19;
        let h4 = f0*g4 + f1*g3   + f2*g2    + f3*g1    + f4*g0;

        // Carry chain — ColdCipher: "Squeeze 128-bit accumulators back into 51-bit limbs."
        let mut r = [0u64; 5];
        let mut c: u128;
        c = h0 >> 51;           r[0] = (h0 as u64) & MASK51;
        let h1 = h1 + c; c = h1 >> 51; r[1] = (h1 as u64) & MASK51;
        let h2 = h2 + c; c = h2 >> 51; r[2] = (h2 as u64) & MASK51;
        let h3 = h3 + c; c = h3 >> 51; r[3] = (h3 as u64) & MASK51;
        let h4 = h4 + c; c = h4 >> 51; r[4] = (h4 as u64) & MASK51;
        // Wrap top carry with ×19
        r[0] += (c as u64) * 19;
        c = (r[0] >> 51) as u128; r[0] &= MASK51; r[1] += c as u64;
        r
    }

    /// — ColdCipher: "Squaring is just mul(f,f) but with doubled cross-terms. Same limb count, fewer muls."
    fn fe_sq(f: &Fe) -> Fe {
        let f0 = f[0] as u128; let f1 = f[1] as u128; let f2 = f[2] as u128;
        let f3 = f[3] as u128; let f4 = f[4] as u128;

        let d0 = f0 * 2; let d1 = f1 * 2; let d2 = f2 * 2; let d3 = f3 * 2;
        let f3_19 = (f[3] as u128) * 19;
        let f4_19 = (f[4] as u128) * 19;

        let h0 = f0*f0     + d1*f4_19 + d2*f3_19;
        let h1 = d0*f1     + d2*f4_19 + f3*f3_19;
        let h2 = d0*f2     + f1*f1    + d3*f4_19;
        let h3 = d0*f3     + d1*f2    + f4*f4_19;
        let h4 = d0*f4     + d1*f3    + f2*f2;

        let mut r = [0u64; 5];
        let mut c: u128;
        c = h0 >> 51;           r[0] = (h0 as u64) & MASK51;
        let h1 = h1 + c; c = h1 >> 51; r[1] = (h1 as u64) & MASK51;
        let h2 = h2 + c; c = h2 >> 51; r[2] = (h2 as u64) & MASK51;
        let h3 = h3 + c; c = h3 >> 51; r[3] = (h3 as u64) & MASK51;
        let h4 = h4 + c; c = h4 >> 51; r[4] = (h4 as u64) & MASK51;
        r[0] += (c as u64) * 19;
        c = (r[0] >> 51) as u128; r[0] &= MASK51; r[1] += c as u64;
        r
    }

    /// Scalar × field element (small constant multiplier).
    /// — ColdCipher: "a24 = 121665. One little constant carries the weight of the whole curve."
    #[inline(always)]
    fn fe_mul_small(f: &Fe, s: u64) -> Fe {
        let s128 = s as u128;
        let mut h0 = (f[0] as u128) * s128;
        let mut h1 = (f[1] as u128) * s128;
        let mut h2 = (f[2] as u128) * s128;
        let mut h3 = (f[3] as u128) * s128;
        let mut h4 = (f[4] as u128) * s128;

        let c = h0 >> 51; h0 &= MASK51 as u128; h1 += c;
        let c = h1 >> 51; h1 &= MASK51 as u128; h2 += c;
        let c = h2 >> 51; h2 &= MASK51 as u128; h3 += c;
        let c = h3 >> 51; h3 &= MASK51 as u128; h4 += c;
        let c = h4 >> 51; h4 &= MASK51 as u128; h0 += c * 19;
        let c = h0 >> 51; h0 &= MASK51 as u128; h1 += c;

        [h0 as u64, h1 as u64, h2 as u64, h3 as u64, h4 as u64]
    }

    /// z^(p-2) — Fermat inversion. 255 squarings, 11 multiplications.
    /// — ColdCipher: "The most expensive function in the whole exchange. 253 squarings
    ///   and a carefully chosen addition chain. No shortcuts — Fermat didn't leave any."
    fn fe_invert(z: &Fe) -> Fe {
        // Addition chain for z^(p-2) = z^(2^255 - 21)
        // — ColdCipher: "2^255 - 21 = 2^255 - 32 + 11. So we need z^(2^255-32) * z^11."
        let z2 = fe_sq(z);                         // z^2
        let t = fe_sq(&z2);                        // z^4
        let t = fe_sq(&t);                         // z^8
        let z9 = fe_mul(&t, z);                    // z^9
        let z11 = fe_mul(&z9, &z2);                // z^11 — saved for the final step
        let t = fe_sq(&z11);                       // z^22
        let z_2_5_0 = fe_mul(&t, &z9);             // z^31 = z^(2^5 - 1)

        let z_2_10_0 = {                            // z^(2^10 - 1)
            let mut t = fe_sq(&z_2_5_0);
            for _ in 1..5 { t = fe_sq(&t); }        // z^(2^10 - 32)
            fe_mul(&t, &z_2_5_0)                    // z^(2^10 - 1)
        };
        let z_2_20_0 = {                            // z^(2^20 - 1)
            let mut t = fe_sq(&z_2_10_0);
            for _ in 1..10 { t = fe_sq(&t); }
            fe_mul(&t, &z_2_10_0)
        };
        let z_2_40_0 = {                            // z^(2^40 - 1)
            let mut t = fe_sq(&z_2_20_0);
            for _ in 1..20 { t = fe_sq(&t); }
            fe_mul(&t, &z_2_20_0)
        };
        let z_2_50_0 = {                            // z^(2^50 - 1)
            let mut t = fe_sq(&z_2_40_0);
            for _ in 1..10 { t = fe_sq(&t); }
            fe_mul(&t, &z_2_10_0)
        };
        let z_2_100_0 = {                           // z^(2^100 - 1)
            let mut t = fe_sq(&z_2_50_0);
            for _ in 1..50 { t = fe_sq(&t); }
            fe_mul(&t, &z_2_50_0)
        };
        let z_2_200_0 = {                           // z^(2^200 - 1)
            let mut t = fe_sq(&z_2_100_0);
            for _ in 1..100 { t = fe_sq(&t); }
            fe_mul(&t, &z_2_100_0)
        };
        let z_2_250_0 = {                           // z^(2^250 - 1)
            let mut t = fe_sq(&z_2_200_0);
            for _ in 1..50 { t = fe_sq(&t); }
            fe_mul(&t, &z_2_50_0)
        };
        let mut t = fe_sq(&z_2_250_0);             // z^(2^251 - 2)
        t = fe_sq(&t);                              // z^(2^252 - 4)
        t = fe_sq(&t);                              // z^(2^253 - 8)
        t = fe_sq(&t);                              // z^(2^254 - 16)
        t = fe_sq(&t);                              // z^(2^255 - 32)
        fe_mul(&t, &z11)                            // z^(2^255 - 32 + 11) = z^(2^255 - 21) = z^(p-2)
        // — ColdCipher: "z^(p-2). Fermat's little theorem. The only honest way to divide in a prime field."
    }

    /// Decode 32 little-endian bytes into 5×51-bit limbs.
    /// — ColdCipher: "Mask bit 255 per RFC 7748 — ignore the top bit, it's not yours to interpret."
    fn fe_frombytes(s: &[u8; 32]) -> Fe {
        let mut wide = [0u8; 32];
        wide.copy_from_slice(s);
        wide[31] &= 0x7F; // Clear top bit per RFC 7748

        // Load as little-endian u64s, then slice into 51-bit limbs
        let load64 = |offset: usize| -> u64 {
            let mut buf = [0u8; 8];
            let end = if offset + 8 <= 32 { offset + 8 } else { 32 };
            let len = end - offset;
            buf[..len].copy_from_slice(&wide[offset..end]);
            u64::from_le_bytes(buf)
        };

        [
            load64(0) & MASK51,
            (load64(6) >> 3) & MASK51,
            (load64(12) >> 6) & MASK51,
            (load64(19) >> 1) & MASK51,
            (load64(24) >> 12) & MASK51,
        ]
    }

    /// Encode field element to 32 little-endian bytes.
    /// — ColdCipher: "Full canonical reduction — subtract p if needed. No leaking non-canonical
    ///   representations. Every byte must be deterministic."
    fn fe_tobytes(h: &Fe) -> [u8; 32] {
        let mut t = *h;
        fe_reduce(&mut t);

        // Canonical reduction: if t >= p, subtract p.
        // p = 2^255 - 19, so in limbs: [2^51-19, 2^51-1, 2^51-1, 2^51-1, 2^51-1]
        // Check: add 19 and see if it overflows 2^255
        let mut q = t[0] + 19;
        q = (q >> 51) + t[1]; q >>= 51;
        q += t[2]; q >>= 51;
        q += t[3]; q >>= 51;
        q += t[4]; q >>= 51;
        // q is now 1 if t >= p, else 0

        t[0] += q * 19;
        // Propagate carry from the addition
        let c = t[0] >> 51; t[0] &= MASK51; t[1] += c;
        let c = t[1] >> 51; t[1] &= MASK51; t[2] += c;
        let c = t[2] >> 51; t[2] &= MASK51; t[3] += c;
        let c = t[3] >> 51; t[3] &= MASK51; t[4] += c;
        t[4] &= MASK51;

        // Pack 5×51-bit limbs into 32 bytes, little-endian
        let mut s = [0u8; 32];
        // Limb layout in a 256-bit number:
        //   bits   0..50  → t[0]
        //   bits  51..101 → t[1]
        //   bits 102..152 → t[2]
        //   bits 153..203 → t[3]
        //   bits 204..254 → t[4]
        // — ColdCipher: "No u256 available. Pack limb by limb, bit-shifting across byte boundaries."

        // t[0]: bits 0-50
        let v = t[0] as u64;
        s[0] = v as u8;
        s[1] = (v >> 8) as u8;
        s[2] = (v >> 16) as u8;
        s[3] = (v >> 24) as u8;
        s[4] = (v >> 32) as u8;
        s[5] = (v >> 40) as u8;
        // s[6] has bits 48..50 from t[0] in low 3 bits, and bits 0..4 of t[1] in upper 5 bits
        let v1 = t[1] as u64;
        s[6] = ((v >> 48) | (v1 << 3)) as u8;
        s[7] = (v1 >> 5) as u8;
        s[8] = (v1 >> 13) as u8;
        s[9] = (v1 >> 21) as u8;
        s[10] = (v1 >> 29) as u8;
        s[11] = (v1 >> 37) as u8;
        // bits 45..50 of t[1] into s[12] low 6 bits, t[2] bits 0..1 into upper 2 bits
        let v2 = t[2] as u64;
        s[12] = ((v1 >> 45) | (v2 << 6)) as u8;
        s[13] = (v2 >> 2) as u8;
        s[14] = (v2 >> 10) as u8;
        s[15] = (v2 >> 18) as u8;
        s[16] = (v2 >> 26) as u8;
        s[17] = (v2 >> 34) as u8;
        s[18] = (v2 >> 42) as u8;
        // bits 50 of t[2] into s[19] low 1 bit, t[3] bits 0..6 into upper 7 bits
        let v3 = t[3] as u64;
        s[19] = ((v2 >> 50) | (v3 << 1)) as u8;
        s[20] = (v3 >> 7) as u8;
        s[21] = (v3 >> 15) as u8;
        s[22] = (v3 >> 23) as u8;
        s[23] = (v3 >> 31) as u8;
        s[24] = (v3 >> 39) as u8;
        // bits 47..50 of t[3] into s[25] low 4 bits, t[4] bits 0..3 into upper 4 bits
        let v4 = t[4] as u64;
        s[25] = ((v3 >> 47) | (v4 << 4)) as u8;
        s[26] = (v4 >> 4) as u8;
        s[27] = (v4 >> 12) as u8;
        s[28] = (v4 >> 20) as u8;
        s[29] = (v4 >> 28) as u8;
        s[30] = (v4 >> 36) as u8;
        s[31] = (v4 >> 44) as u8;

        s
    }

    /// Constant-time conditional swap. If swap == 1, swaps a and b. If 0, no-op.
    /// — ColdCipher: "XOR-mask swap. No branches. The CPU's branch predictor learns nothing."
    #[inline(always)]
    fn fe_cswap(a: &mut Fe, b: &mut Fe, swap: u64) {
        let mask = 0u64.wrapping_sub(swap); // 0xFFFF...FF if swap==1, 0 if swap==0
        for i in 0..5 {
            let t = mask & (a[i] ^ b[i]);
            a[i] ^= t;
            b[i] ^= t;
        }
    }

    // =========================================================================
    // Montgomery ladder — RFC 7748 Section 5
    // — ColdCipher: "The ladder. 255 rungs. Every step identical whether the bit is
    //   set or not. Side-channel analysts can stare all day — they'll see nothing."
    // =========================================================================

    let u = fe_frombytes(point);

    let mut x_2 = fe_one();
    let mut z_2 = fe_zero();
    let mut x_3 = u;
    let mut z_3 = fe_one();

    let mut swap: u64 = 0;

    // Iterate bits 254 down to 0 (bit 255 is always 0 after clamping)
    let mut pos: i32 = 254;
    while pos >= 0 {
        let byte_idx = (pos >> 3) as usize;
        let bit_idx = (pos & 7) as u32;
        let k_t = ((scalar[byte_idx] >> bit_idx) & 1) as u64;

        swap ^= k_t;
        fe_cswap(&mut x_2, &mut x_3, swap);
        fe_cswap(&mut z_2, &mut z_3, swap);
        swap = k_t;

        let a = fe_add(&x_2, &z_2);
        let aa = fe_sq(&a);
        let b = fe_sub(&x_2, &z_2);
        let bb = fe_sq(&b);
        let e = fe_sub(&aa, &bb);
        let c = fe_add(&x_3, &z_3);
        let d = fe_sub(&x_3, &z_3);
        let da = fe_mul(&d, &a);
        let cb = fe_mul(&c, &b);

        x_3 = fe_add(&da, &cb);
        x_3 = fe_sq(&x_3);
        z_3 = fe_sub(&da, &cb);
        z_3 = fe_sq(&z_3);
        z_3 = fe_mul(&z_3, &u);

        x_2 = fe_mul(&aa, &bb);
        // a24 = 121665 = (A-2)/4 where A=486662 for Curve25519
        // — ColdCipher: "Off by one in a24 and every handshake silently produces garbage. Ask me how I know."
        let e_a24 = fe_mul_small(&e, 121665);
        let tmp = fe_add(&aa, &e_a24);
        z_2 = fe_mul(&e, &tmp);

        pos -= 1;
    }

    // Final conditional swap
    fe_cswap(&mut x_2, &mut x_3, swap);
    fe_cswap(&mut z_2, &mut z_3, swap);

    // — ColdCipher: "Invert z, multiply by x. One division to recover the affine coordinate.
    //   Then serialize and forget — the scalar never touched a branch."
    let z_inv = fe_invert(&z_2);
    let result = fe_mul(&x_2, &z_inv);
    fe_tobytes(&result)
}
