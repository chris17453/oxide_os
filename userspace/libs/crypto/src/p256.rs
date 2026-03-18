// ============================================================================
// NIST P-256 (secp256r1) ECDSA Signature Verification
// ============================================================================
// Pure verification. No signing. No side-channel countermeasures for signing
// because we don't sign. If you're here looking for key generation, you're
// in the wrong crate and possibly the wrong career. — ColdCipher
//
// NIST FIPS 186-4, Section 6.4 — ECDSA Verification
// SEC 2: Recommended Elliptic Curve Domain Parameters, Section 2.7.1
//
// Test vector (NIST ECDSA, from FIPS 186-4 test vectors):
//   Message hash (SHA-256): 44acf6b7e36c1342c2c5897204fe09504e1e2efb1a900377dbc4e7a6a133ec56
//   Qx: 1ccbe91c075fc7f4f033bfa248db8fccd3565de94bbfb12f3c59ff46c271bf83
//   Qy: ce4014c68811f9a21a1fdb2c0e6113e06db7ca93b7404e78dc7ccd5ca89a4ca9
//   r:  f3ac8061b514795b8843e3d6629527ed2afd6b1f6a555a7acabb5e6f79c8c2ac
//   s:  8bf77819ca05a6b2786c76262bf7371cef97b218e96f175a3ccdda2acc058903
//   Result: valid

extern crate alloc;

/// 256-bit unsigned integer as 4 little-endian u64 limbs.
/// limbs[0] is the least significant. — ColdCipher
type U256 = [u64; 4];

// ============================================================================
// Curve parameters — memorized like scripture, because NIST said so
// ============================================================================

/// p = 2^256 - 2^224 + 2^192 + 2^96 - 1
/// The field prime. Not negotiable. — ColdCipher
const P: U256 = [
    0xffffffffffffffff,
    0x00000000ffffffff,
    0x0000000000000000,
    0xffffffff00000001,
];

/// Curve order n — the number of points on the curve.
/// Every scalar lives in [0, n). Deal with it. — ColdCipher
const N: U256 = [
    0xf3b9cac2fc632551,
    0xbce6faada7179e84,
    0xffffffffffffffff,
    0xffffffff00000000,
];

/// a = p - 3. Because NIST loves their special forms. — ColdCipher
const A: U256 = [
    0xfffffffffffffffc,
    0x00000000ffffffff,
    0x0000000000000000,
    0xffffffff00000001,
];

/// b coefficient. The one ugly constant we can't avoid. — ColdCipher
#[allow(dead_code)]
const B: U256 = [
    0x3bce3c3e27d2604b,
    0x651d06b0cc53b0f6,
    0xb3ebbd55769886bc,
    0x5ac635d8aa3a93e7,
];

/// Generator point Gx — ColdCipher
const GX: U256 = [
    0xf4a13945d898c296,
    0x77037d812deb33a0,
    0xf8bce6e563a440f2,
    0x6b17d1f2e12c4247,
];

/// Generator point Gy — ColdCipher
const GY: U256 = [
    0xcbb6406837bf51f5,
    0x2bce33576b315ece,
    0x8ee7eb4a7c0f9e16,
    0x4fe342e2fe1a7f9b,
];

const ZERO: U256 = [0, 0, 0, 0];
const ONE: U256 = [1, 0, 0, 0];

// ============================================================================
// 256-bit arithmetic — hand-rolled because we're in no_std purgatory
// ============================================================================

/// Compare two U256. Returns -1, 0, or 1. — ColdCipher
fn u256_cmp(a: &U256, b: &U256) -> i8 {
    for i in (0..4).rev() {
        if a[i] < b[i] {
            return -1;
        }
        if a[i] > b[i] {
            return 1;
        }
    }
    0
}

fn u256_is_zero(a: &U256) -> bool {
    a[0] == 0 && a[1] == 0 && a[2] == 0 && a[3] == 0
}

/// a + b, returns (result, carry). — ColdCipher
fn u256_add(a: &U256, b: &U256) -> (U256, bool) {
    let mut result = ZERO;
    let mut carry = 0u64;
    for i in 0..4 {
        let (s1, c1) = a[i].overflowing_add(b[i]);
        let (s2, c2) = s1.overflowing_add(carry);
        result[i] = s2;
        carry = (c1 as u64) + (c2 as u64);
    }
    (result, carry > 0)
}

/// a - b, returns (result, borrow). — ColdCipher
fn u256_sub(a: &U256, b: &U256) -> (U256, bool) {
    let mut result = ZERO;
    let mut borrow = 0u64;
    for i in 0..4 {
        let (s1, b1) = a[i].overflowing_sub(b[i]);
        let (s2, b2) = s1.overflowing_sub(borrow);
        result[i] = s2;
        borrow = (b1 as u64) + (b2 as u64);
    }
    (result, borrow > 0)
}

// ============================================================================
// Field arithmetic mod p — where the real fun begins
// ============================================================================

/// Reduce mod p. Because raw addition doesn't know its place. — ColdCipher
fn fp_add(a: &U256, b: &U256) -> U256 {
    let (sum, carry) = u256_add(a, b);
    if carry || u256_cmp(&sum, &P) >= 0 {
        let (r, _) = u256_sub(&sum, &P);
        r
    } else {
        sum
    }
}

/// Subtraction mod p. Wraps around like a proper field element. — ColdCipher
fn fp_sub(a: &U256, b: &U256) -> U256 {
    let (diff, borrow) = u256_sub(a, b);
    if borrow {
        let (r, _) = u256_add(&diff, &P);
        r
    } else {
        diff
    }
}

/// Negation mod p. Because sometimes you just need the opposite. — ColdCipher
fn fp_neg(a: &U256) -> U256 {
    if u256_is_zero(a) {
        ZERO
    } else {
        let (r, _) = u256_sub(&P, a);
        r
    }
}

/// 512-bit intermediate for multiplication. [u64; 8], little-endian. — ColdCipher
type U512 = [u64; 8];

/// Full 256x256 -> 512 multiplication. No shortcuts. — ColdCipher
fn u256_mul_wide(a: &U256, b: &U256) -> U512 {
    let mut result = [0u64; 8];
    for i in 0..4 {
        let mut carry = 0u128;
        for j in 0..4 {
            let prod = (a[i] as u128) * (b[j] as u128) + (result[i + j] as u128) + carry;
            result[i + j] = prod as u64;
            carry = prod >> 64;
        }
        result[i + 4] = carry as u64;
    }
    result
}

/// Reduce a 512-bit value mod p using Barrett-like manual reduction.
/// P-256 has special structure: p = 2^256 - 2^224 + 2^192 + 2^96 - 1.
/// FIPS 186-4 references the fast reduction from "Solinas, Generalized
/// Mersenne Numbers" — but honestly, we just do schoolbook reduction
/// because correctness > cleverness in verification code. — ColdCipher
fn fp_reduce_512(t: &U512) -> U256 {
    // We'll use the NIST P-256 fast reduction formulas.
    // Split the 512-bit number into 32-bit words for the reduction.
    // s = (c7, c6, c5, c4, c3, c2, c1, c0) where each ci is 32 bits...
    // Actually, let's work with the Solinas reduction directly with 32-bit limbs.
    // — ColdCipher: "Premature optimization is the root of all evil, but this
    //   reduction is mandatory for not being glacially slow."

    let d = extract_32bit_words(t);

    // NIST SP 800-186 / FIPS 186-4, Section D.2.3: P-256 reduction
    // result = s1 + 2*s2 + 2*s3 + s4 + s5 - s6 - s7 - s8 - s9 (mod p)
    //
    // The si are 256-bit values assembled from 32-bit words of the 512-bit product.
    // d[0]..d[15] are the 32-bit words (little-endian) of the 512-bit input.

    // s1 = (d[7], d[6], d[5], d[4], d[3], d[2], d[1], d[0])
    let s1 = w256(d[7], d[6], d[5], d[4], d[3], d[2], d[1], d[0]);
    // s2 = (d[15], d[14], d[13], d[12], d[11], 0, 0, 0)
    let s2 = w256(d[15], d[14], d[13], d[12], d[11], 0, 0, 0);
    // s3 = (0, d[15], d[14], d[13], d[12], 0, 0, 0)
    let s3 = w256(0, d[15], d[14], d[13], d[12], 0, 0, 0);
    // s4 = (d[15], d[14], 0, 0, 0, d[10], d[9], d[8])
    let s4 = w256(d[15], d[14], 0, 0, 0, d[10], d[9], d[8]);
    // s5 = (d[8], d[13], d[15], d[14], d[13], d[11], d[10], d[9])
    let s5 = w256(d[8], d[13], d[15], d[14], d[13], d[11], d[10], d[9]);
    // s6 = (d[10], d[8], 0, 0, 0, d[13], d[12], d[11])
    let s6 = w256(d[10], d[8], 0, 0, 0, d[13], d[12], d[11]);
    // s7 = (d[11], d[9], 0, 0, d[15], d[14], d[13], d[12])
    let s7 = w256(d[11], d[9], 0, 0, d[15], d[14], d[13], d[12]);
    // s8 = (d[12], 0, d[10], d[9], d[8], d[15], d[14], d[13])
    let s8 = w256(d[12], 0, d[10], d[9], d[8], d[15], d[14], d[13]);
    // s9 = (d[13], 0, d[11], d[10], d[9], 0, d[15], d[14])
    let s9 = w256(d[13], 0, d[11], d[10], d[9], 0, d[15], d[14]);

    // Accumulate using signed arithmetic with extra precision
    // result = s1 + 2*s2 + 2*s3 + s4 + s5 - s6 - s7 - s8 - s9 (mod p)
    // — ColdCipher: "Addition is easy. Subtraction is where paranoia lives."

    // We accumulate into a wider type to handle carries/borrows
    let mut acc = [0i64; 9]; // 9 "limbs" of 32-bit conceptual width, but we use i64 for headroom

    // Helper: add a 256-bit value (as 8 x u32) to accumulator
    fn acc_add(acc: &mut [i64; 9], v: &[u32; 8]) {
        for i in 0..8 {
            acc[i] += v[i] as i64;
        }
    }
    fn acc_add2(acc: &mut [i64; 9], v: &[u32; 8]) {
        for i in 0..8 {
            acc[i] += 2 * (v[i] as i64);
        }
    }
    fn acc_sub(acc: &mut [i64; 9], v: &[u32; 8]) {
        for i in 0..8 {
            acc[i] -= v[i] as i64;
        }
    }

    acc_add(&mut acc, &s1);
    acc_add2(&mut acc, &s2);
    acc_add2(&mut acc, &s3);
    acc_add(&mut acc, &s4);
    acc_add(&mut acc, &s5);
    acc_sub(&mut acc, &s6);
    acc_sub(&mut acc, &s7);
    acc_sub(&mut acc, &s8);
    acc_sub(&mut acc, &s9);

    // Propagate carries through 32-bit limbs
    // — ColdCipher: "Carry propagation: the digital plumbing nobody respects until it leaks."
    for i in 0..8 {
        let carry = acc[i] >> 32;
        // We need floor division for negative values
        let carry = if acc[i] < 0 && (acc[i] & 0xFFFFFFFF) != 0 {
            carry - 1
        } else {
            carry
        };
        acc[i] -= carry << 32;
        acc[i + 1] += carry;
    }

    // Now assemble back into U256 from the 32-bit words
    let mut result = ZERO;
    for i in 0..4 {
        result[i] = (acc[2 * i] as u64) | ((acc[2 * i + 1] as u64) << 32);
    }

    // Final reduction: result might still be >= p (or negative from subtraction underflow)
    // We handle the top carry/borrow
    let top = acc[8];
    if top < 0 {
        // Underflow — add p back enough times
        let mut r = result;
        for _ in 0..(-top as u64) {
            let (nr, _) = u256_add(&r, &P);
            r = nr;
        }
        // Ensure in range
        while u256_cmp(&r, &P) >= 0 {
            let (nr, _) = u256_sub(&r, &P);
            r = nr;
        }
        r
    } else {
        // Overflow — subtract p enough times
        let mut r = result;
        for _ in 0..top as u64 {
            let (nr, _) = u256_sub(&r, &P);
            r = nr;
        }
        while u256_cmp(&r, &P) >= 0 {
            let (nr, _) = u256_sub(&r, &P);
            r = nr;
        }
        r
    }
}

/// Extract 32-bit words from a 512-bit value. Little-endian. — ColdCipher
fn extract_32bit_words(t: &U512) -> [u32; 16] {
    let mut d = [0u32; 16];
    for i in 0..8 {
        d[2 * i] = t[i] as u32;
        d[2 * i + 1] = (t[i] >> 32) as u32;
    }
    d
}

/// Assemble 8 x u32 (big-endian order: d7 is MSW) into a [u32; 8] in little-endian order.
/// Arguments: d7 (MSW) .. d0 (LSW). — ColdCipher
fn w256(d7: u32, d6: u32, d5: u32, d4: u32, d3: u32, d2: u32, d1: u32, d0: u32) -> [u32; 8] {
    [d0, d1, d2, d3, d4, d5, d6, d7]
}

/// Field multiplication: (a * b) mod p — ColdCipher
fn fp_mul(a: &U256, b: &U256) -> U256 {
    let wide = u256_mul_wide(a, b);
    fp_reduce_512(&wide)
}

/// Field squaring — just mul(a, a). Could be optimized but clarity > speed
/// for verification-only code. — ColdCipher
fn fp_sqr(a: &U256) -> U256 {
    fp_mul(a, a)
}

/// Modular exponentiation mod p via square-and-multiply.
/// — ColdCipher: "Power corrupts. Modular power reduces."
fn fp_pow(base: &U256, exp: &U256) -> U256 {
    let mut result = ONE;
    let mut b = *base;

    for i in 0..4 {
        let mut word = exp[i];
        for _ in 0..64 {
            if word & 1 == 1 {
                result = fp_mul(&result, &b);
            }
            b = fp_sqr(&b);
            word >>= 1;
        }
    }
    result
}

/// Field inversion via Fermat's little theorem: a^(-1) = a^(p-2) mod p.
/// — ColdCipher: "Fermat did this on paper. We need 4096-bit registers. Progress."
fn fp_inv(a: &U256) -> U256 {
    // p - 2
    let p_minus_2: U256 = [
        0xfffffffffffffffd,
        0x00000000ffffffff,
        0x0000000000000000,
        0xffffffff00000001,
    ];
    fp_pow(a, &p_minus_2)
}

// ============================================================================
// Scalar arithmetic mod n (curve order)
// ============================================================================

/// Reduce mod n using repeated subtraction (values are at most ~2n after add).
/// — ColdCipher: "Elegant? No. Correct? Provably."
fn mod_n_reduce(a: &U256) -> U256 {
    let mut r = *a;
    while u256_cmp(&r, &N) >= 0 {
        let (sub, _) = u256_sub(&r, &N);
        r = sub;
    }
    r
}

/// Multiplication mod n — full schoolbook with Barrett-style reduction.
/// — ColdCipher: "No fast reduction tricks for n. It's not a Mersenne-adjacent prime.
///   We suffer through the general case."
fn scalar_mul_mod_n(a: &U256, b: &U256) -> U256 {
    let wide = u256_mul_wide(a, b);
    // General reduction mod n: we use trial subtraction with shifting.
    // For values up to 512 bits, we reduce by repeated conditional subtraction.
    // This is O(256) subtractions worst case — acceptable for verification.
    reduce_512_mod_n(&wide)
}

/// Reduce a 512-bit value mod n.
/// — ColdCipher: "Brute force reduction. Inelegant but immune to clever bugs."
fn reduce_512_mod_n(t: &U512) -> U256 {
    // We'll do shift-and-subtract division. Work from the top.
    // Represent as a mutable 512-bit number and subtract shifted copies of n.
    let mut r = [0u64; 9]; // extra limb for safety
    for i in 0..8 {
        r[i] = t[i];
    }

    // n is 256 bits. We need to subtract n << k for k from 255 down to 0.
    for k in (0..=255).rev() {
        // Check if r >= n << k
        let word_shift = k / 64;
        let bit_shift = k % 64;

        // Extract the relevant portion of r starting at word_shift
        let mut shifted_n = [0u64; 9];
        if bit_shift == 0 {
            for i in 0..4 {
                if i + word_shift < 9 {
                    shifted_n[i + word_shift] = N[i];
                }
            }
        } else {
            for i in 0..4 {
                if i + word_shift < 9 {
                    shifted_n[i + word_shift] |= N[i] << bit_shift;
                }
                if i + word_shift + 1 < 9 {
                    shifted_n[i + word_shift + 1] |= N[i] >> (64 - bit_shift);
                }
            }
        }

        // Compare r >= shifted_n (9 limbs)
        let mut ge = true;
        for i in (0..9).rev() {
            if r[i] < shifted_n[i] {
                ge = false;
                break;
            }
            if r[i] > shifted_n[i] {
                break;
            }
        }

        if ge {
            let mut borrow = 0u64;
            for i in 0..9 {
                let (s1, b1) = r[i].overflowing_sub(shifted_n[i]);
                let (s2, b2) = s1.overflowing_sub(borrow);
                r[i] = s2;
                borrow = (b1 as u64) + (b2 as u64);
            }
        }
    }

    [r[0], r[1], r[2], r[3]]
}

/// Modular exponentiation mod n. — ColdCipher
fn scalar_pow_mod_n(base: &U256, exp: &U256) -> U256 {
    let mut result = ONE;
    let mut b = *base;

    for i in 0..4 {
        let mut word = exp[i];
        for _ in 0..64 {
            if word & 1 == 1 {
                result = scalar_mul_mod_n(&result, &b);
            }
            b = scalar_mul_mod_n(&b, &b);
            word >>= 1;
        }
    }
    result
}

/// Modular inverse mod n via Fermat: a^(n-2) mod n.
/// — ColdCipher: "Same trick, different modulus. Fermat remains undefeated."
fn scalar_inv_mod_n(a: &U256) -> U256 {
    let n_minus_2: U256 = [
        0xf3b9cac2fc63254f,
        0xbce6faada7179e84,
        0xffffffffffffffff,
        0xffffffff00000000,
    ];
    scalar_pow_mod_n(a, &n_minus_2)
}

/// Addition mod n. — ColdCipher
fn scalar_add_mod_n(a: &U256, b: &U256) -> U256 {
    let (sum, carry) = u256_add(a, b);
    if carry || u256_cmp(&sum, &N) >= 0 {
        let (r, _) = u256_sub(&sum, &N);
        r
    } else {
        sum
    }
}

// ============================================================================
// Elliptic curve point operations — Jacobian coordinates
// ============================================================================
// Using Jacobian projective coordinates (X, Y, Z) where the affine point
// is (X/Z^2, Y/Z^3). The point at infinity is represented by Z = 0.
// — ColdCipher: "Projective coordinates: because division is expensive and
//   we have trust issues with denominators."

/// A point in Jacobian projective coordinates. — ColdCipher
struct JacobianPoint {
    x: U256,
    y: U256,
    z: U256,
}

impl JacobianPoint {
    fn infinity() -> Self {
        JacobianPoint {
            x: ONE,
            y: ONE,
            z: ZERO,
        }
    }

    fn is_infinity(&self) -> bool {
        u256_is_zero(&self.z)
    }

    fn from_affine(x: &U256, y: &U256) -> Self {
        JacobianPoint {
            x: *x,
            y: *y,
            z: ONE,
        }
    }

    /// Convert back to affine coordinates. Returns None for point at infinity.
    /// — ColdCipher: "Back to reality. Hope you enjoyed projective space."
    fn to_affine(&self) -> Option<(U256, U256)> {
        if self.is_infinity() {
            return None;
        }
        let z_inv = fp_inv(&self.z);
        let z_inv2 = fp_sqr(&z_inv);
        let z_inv3 = fp_mul(&z_inv2, &z_inv);
        let ax = fp_mul(&self.x, &z_inv2);
        let ay = fp_mul(&self.y, &z_inv3);
        Some((ax, ay))
    }
}

/// Point doubling in Jacobian coordinates.
/// Formula from "Guide to Elliptic Curve Cryptography" (Hankerson, Menezes, Vanstone),
/// Algorithm 3.21, optimized for a = -3.
/// — ColdCipher: "Double or nothing. Mostly double."
fn point_double(p: &JacobianPoint) -> JacobianPoint {
    if p.is_infinity() {
        return JacobianPoint::infinity();
    }

    // For a = -3 (which P-256 uses):
    // M = 3*(X - Z^2)*(X + Z^2)
    // S = 4*X*Y^2
    // X' = M^2 - 2*S
    // Y' = M*(S - X') - 8*Y^4
    // Z' = 2*Y*Z

    let y2 = fp_sqr(&p.y);
    let z2 = fp_sqr(&p.z);

    // S = 4*X*Y^2
    let s = fp_mul(&p.x, &y2);
    let s = fp_add(&s, &s);
    let s = fp_add(&s, &s);

    // M = 3*(X - Z^2)*(X + Z^2) = 3*(X^2 - Z^4) = 3*X^2 + a*Z^4
    // Since a = -3: M = 3*(X - Z^2)*(X + Z^2)
    let xpz2 = fp_add(&p.x, &z2);
    let xmz2 = fp_sub(&p.x, &z2);
    let m = fp_mul(&xpz2, &xmz2);
    let m = fp_add(&fp_add(&m, &m), &m); // 3 * m

    // X' = M^2 - 2*S
    let x3 = fp_sqr(&m);
    let x3 = fp_sub(&x3, &s);
    let x3 = fp_sub(&x3, &s);

    // Y' = M*(S - X') - 8*Y^4
    let y4 = fp_sqr(&y2);
    let y4_8 = fp_add(&y4, &y4);
    let y4_8 = fp_add(&y4_8, &y4_8);
    let y4_8 = fp_add(&y4_8, &y4_8);
    let y3 = fp_mul(&m, &fp_sub(&s, &x3));
    let y3 = fp_sub(&y3, &y4_8);

    // Z' = 2*Y*Z
    let z3 = fp_mul(&p.y, &p.z);
    let z3 = fp_add(&z3, &z3);

    JacobianPoint {
        x: x3,
        y: y3,
        z: z3,
    }
}

/// Point addition in Jacobian coordinates (mixed: Q is affine, P is Jacobian).
/// — ColdCipher: "Two points walk into a curve. One comes out."
fn point_add_mixed(p: &JacobianPoint, qx: &U256, qy: &U256) -> JacobianPoint {
    if p.is_infinity() {
        return JacobianPoint::from_affine(qx, qy);
    }

    // U1 = X1, U2 = X2*Z1^2
    // S1 = Y1, S2 = Y2*Z1^3
    let z1_sq = fp_sqr(&p.z);
    let z1_cu = fp_mul(&z1_sq, &p.z);

    let u2 = fp_mul(qx, &z1_sq);
    let s2 = fp_mul(qy, &z1_cu);

    // H = U2 - X1, R = S2 - Y1
    let h = fp_sub(&u2, &p.x);
    let r = fp_sub(&s2, &p.y);

    if u256_is_zero(&h) {
        if u256_is_zero(&r) {
            // Points are the same — double
            return point_double(p);
        }
        // Points are inverses — result is infinity
        return JacobianPoint::infinity();
    }

    let h2 = fp_sqr(&h);
    let h3 = fp_mul(&h2, &h);

    // X3 = R^2 - H^3 - 2*X1*H^2
    let x1h2 = fp_mul(&p.x, &h2);
    let x3 = fp_sqr(&r);
    let x3 = fp_sub(&x3, &h3);
    let x3 = fp_sub(&x3, &x1h2);
    let x3 = fp_sub(&x3, &x1h2);

    // Y3 = R*(X1*H^2 - X3) - Y1*H^3
    let y3 = fp_mul(&r, &fp_sub(&x1h2, &x3));
    let y3 = fp_sub(&y3, &fp_mul(&p.y, &h3));

    // Z3 = Z1 * H
    let z3 = fp_mul(&p.z, &h);

    JacobianPoint {
        x: x3,
        y: y3,
        z: z3,
    }
}

/// Point addition: two Jacobian points.
/// — ColdCipher: "General addition. Slower, but handles all the edge cases
///   that mixed addition smugly ignores."
fn point_add(p: &JacobianPoint, q: &JacobianPoint) -> JacobianPoint {
    if p.is_infinity() {
        return JacobianPoint {
            x: q.x,
            y: q.y,
            z: q.z,
        };
    }
    if q.is_infinity() {
        return JacobianPoint {
            x: p.x,
            y: p.y,
            z: p.z,
        };
    }

    let z1_sq = fp_sqr(&p.z);
    let z2_sq = fp_sqr(&q.z);
    let z1_cu = fp_mul(&z1_sq, &p.z);
    let z2_cu = fp_mul(&z2_sq, &q.z);

    let u1 = fp_mul(&p.x, &z2_sq);
    let u2 = fp_mul(&q.x, &z1_sq);
    let s1 = fp_mul(&p.y, &z2_cu);
    let s2 = fp_mul(&q.y, &z1_cu);

    let h = fp_sub(&u2, &u1);
    let r = fp_sub(&s2, &s1);

    if u256_is_zero(&h) {
        if u256_is_zero(&r) {
            return point_double(p);
        }
        return JacobianPoint::infinity();
    }

    let h2 = fp_sqr(&h);
    let h3 = fp_mul(&h2, &h);
    let u1h2 = fp_mul(&u1, &h2);

    let x3 = fp_sqr(&r);
    let x3 = fp_sub(&x3, &h3);
    let x3 = fp_sub(&x3, &u1h2);
    let x3 = fp_sub(&x3, &u1h2);

    let y3 = fp_mul(&r, &fp_sub(&u1h2, &x3));
    let y3 = fp_sub(&y3, &fp_mul(&s1, &h3));

    let z3 = fp_mul(&p.z, &q.z);
    let z3 = fp_mul(&z3, &h);

    JacobianPoint {
        x: x3,
        y: y3,
        z: z3,
    }
}

/// Scalar multiplication using double-and-add (left-to-right).
/// — ColdCipher: "Constant-time? For verification, timing attacks are moot.
///   The public key is... public."
fn scalar_mul(k: &U256, px: &U256, py: &U256) -> JacobianPoint {
    let mut result = JacobianPoint::infinity();

    // Scan from the highest set bit
    let mut started = false;
    for i in (0..4).rev() {
        for bit in (0..64).rev() {
            if started {
                result = point_double(&result);
            }
            if (k[i] >> bit) & 1 == 1 {
                if !started {
                    result = JacobianPoint::from_affine(px, py);
                    started = true;
                } else {
                    result = point_add_mixed(&result, px, py);
                }
            }
        }
    }
    result
}

/// Shamir's trick for simultaneous scalar multiplication: u1*G + u2*Q.
/// Uses a simple interleaved double-and-add.
/// — ColdCipher: "Two birds, one exponentiation. Shamir was efficient like that."
fn shamir_mul(u1: &U256, u2: &U256, qx: &U256, qy: &U256) -> JacobianPoint {
    // Precompute G+Q for the 2-bit lookup
    let g = JacobianPoint::from_affine(&GX, &GY);
    let q = JacobianPoint::from_affine(qx, qy);
    let gq = point_add(&g, &q);

    let mut result = JacobianPoint::infinity();

    // Scan from MSB
    for i in (0..4).rev() {
        for bit in (0..64).rev() {
            result = point_double(&result);
            let b1 = (u1[i] >> bit) & 1;
            let b2 = (u2[i] >> bit) & 1;

            match (b1, b2) {
                (1, 1) => {
                    // Add G+Q
                    if gq.is_infinity() {
                        // Shouldn't happen for valid inputs, but handle it
                    } else {
                        let (gqx, gqy) = gq.to_affine().unwrap();
                        result = point_add_mixed(&result, &gqx, &gqy);
                    }
                }
                (1, 0) => {
                    result = point_add_mixed(&result, &GX, &GY);
                }
                (0, 1) => {
                    result = point_add_mixed(&result, qx, qy);
                }
                _ => {}
            }
        }
    }

    result
}

// ============================================================================
// Byte <-> U256 conversions
// ============================================================================

/// Parse a 32-byte big-endian array into a U256 (little-endian limbs).
/// — ColdCipher: "Endianness: the silent killer of interoperability."
fn u256_from_be_bytes(bytes: &[u8; 32]) -> U256 {
    let mut result = ZERO;
    for i in 0..4 {
        let offset = 24 - i * 8;
        result[i] = u64::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);
    }
    result
}

/// Encode a U256 as 32-byte big-endian array.
/// — ColdCipher: "Back to wire format. The network demands big-endian tribute."
fn u256_to_be_bytes(v: &U256) -> [u8; 32] {
    let mut result = [0u8; 32];
    for i in 0..4 {
        let offset = 24 - i * 8;
        let bytes = v[i].to_be_bytes();
        result[offset..offset + 8].copy_from_slice(&bytes);
    }
    result
}

// ============================================================================
// Public API
// ============================================================================

/// ECDSA P-256 public key. Just the (x, y) affine coordinates.
/// — ColdCipher: "Your identity, reduced to two field elements. How humbling."
#[derive(Clone)]
pub struct P256PublicKey {
    pub x: [u64; 4],
    pub y: [u64; 4],
}

/// Parse an uncompressed P-256 public key (0x04 || x || y, 65 bytes).
/// Returns None if the format is wrong or the point isn't on the curve.
/// — ColdCipher: "Trust but verify. Actually, just verify."
pub fn p256_pubkey_from_uncompressed(bytes: &[u8]) -> Option<P256PublicKey> {
    if bytes.len() != 65 || bytes[0] != 0x04 {
        return None;
    }

    let mut xb = [0u8; 32];
    let mut yb = [0u8; 32];
    xb.copy_from_slice(&bytes[1..33]);
    yb.copy_from_slice(&bytes[33..65]);

    let x = u256_from_be_bytes(&xb);
    let y = u256_from_be_bytes(&yb);

    // Verify point is on the curve: y^2 = x^3 + ax + b (mod p)
    // — ColdCipher: "If it's not on the curve, it's not a key. It's a liability."
    if u256_cmp(&x, &P) >= 0 || u256_cmp(&y, &P) >= 0 {
        return None;
    }

    let y2 = fp_sqr(&y);
    let x3 = fp_mul(&fp_sqr(&x), &x);
    let ax = fp_mul(&A, &x);
    let rhs = fp_add(&fp_add(&x3, &ax), &B);

    if u256_cmp(&y2, &rhs) != 0 {
        return None;
    }

    // Check point is not at infinity (trivially satisfied if x, y < p and on curve)
    // Also check point has order n (i.e., n*Q = O)
    // For P-256, the cofactor is 1, so any point on the curve (except O) has order n.
    // — ColdCipher: "Cofactor 1. One of the few gifts NIST gives us."

    Some(P256PublicKey { x, y })
}

/// Verify an ECDSA P-256 signature.
///
/// - `hash`: 32-byte SHA-256 hash of the message
/// - `signature`: 64-byte signature (r || s), each 32 bytes big-endian
/// - `pubkey`: the signer's public key
///
/// Returns true iff the signature is valid.
///
/// — ColdCipher: "The moment of truth. Literally. One boolean stands between
///   you and trusting a stranger's certificate."
pub fn p256_verify(hash: &[u8; 32], signature: &[u8; 64], pubkey: &P256PublicKey) -> bool {
    // Parse r and s from signature
    let mut rb = [0u8; 32];
    let mut sb = [0u8; 32];
    rb.copy_from_slice(&signature[..32]);
    sb.copy_from_slice(&signature[32..]);

    let r = u256_from_be_bytes(&rb);
    let s = u256_from_be_bytes(&sb);
    let z = u256_from_be_bytes(hash);

    // Step 1: Check r, s in [1, n-1]
    // — ColdCipher: "Zero is not a valid scalar. Neither is n. Basic hygiene."
    if u256_is_zero(&r) || u256_cmp(&r, &N) >= 0 {
        return false;
    }
    if u256_is_zero(&s) || u256_cmp(&s, &N) >= 0 {
        return false;
    }

    // Step 2: w = s^(-1) mod n
    let w = scalar_inv_mod_n(&s);

    // Step 3: u1 = z * w mod n
    let u1 = scalar_mul_mod_n(&z, &w);

    // Step 4: u2 = r * w mod n
    let u2 = scalar_mul_mod_n(&r, &w);

    // Step 5: P = u1*G + u2*Q (using Shamir's trick)
    let point = shamir_mul(&u1, &u2, &pubkey.x, &pubkey.y);

    // Step 6: Check P.x mod n == r
    // — ColdCipher: "One coordinate. One comparison. All of PKI rests on this."
    match point.to_affine() {
        None => false, // Point at infinity = invalid
        Some((px, _)) => {
            let px_mod_n = mod_n_reduce(&px);
            u256_cmp(&px_mod_n, &r) == 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test basic field arithmetic identities. — ColdCipher
    #[test]
    fn test_fp_add_sub_identity() {
        let a: U256 = [0x123456789abcdef0, 0xfedcba9876543210, 0, 0];
        let b: U256 = [0xaabbccddeeff0011, 0x1122334455667788, 0, 0];
        let sum = fp_add(&a, &b);
        let diff = fp_sub(&sum, &b);
        assert_eq!(a, diff);
    }

    /// Test that multiplication by 1 is identity. — ColdCipher
    #[test]
    fn test_fp_mul_identity() {
        let a: U256 = [0xdeadbeefcafebabe, 0x0123456789abcdef, 0, 0];
        let result = fp_mul(&a, &ONE);
        assert_eq!(a, result);
    }

    /// Test that a * a^(-1) = 1 mod p. — ColdCipher: "The inversion test. Fail this and nothing else matters."
    #[test]
    fn test_fp_inv() {
        let a: U256 = [0xdeadbeefcafebabe, 0x0123456789abcdef, 0x1111111111111111, 0x2222222222222222];
        let a_inv = fp_inv(&a);
        let product = fp_mul(&a, &a_inv);
        assert_eq!(product, ONE);
    }

    /// Test scalar inverse mod n. — ColdCipher
    #[test]
    fn test_scalar_inv() {
        let a: U256 = [0xdeadbeef, 0xcafebabe, 0x12345678, 0x9abcdef0];
        let a_inv = scalar_inv_mod_n(&a);
        let product = scalar_mul_mod_n(&a, &a_inv);
        assert_eq!(product, ONE);
    }

    /// Test generator point is on the curve. — ColdCipher
    #[test]
    fn test_generator_on_curve() {
        let y2 = fp_sqr(&GY);
        let x3 = fp_mul(&fp_sqr(&GX), &GX);
        let ax = fp_mul(&A, &GX);
        let rhs = fp_add(&fp_add(&x3, &ax), &B);
        assert_eq!(y2, rhs, "Generator point is not on the curve — abandon all hope");
    }

    /// Test that n*G = O (point at infinity). — ColdCipher
    #[test]
    fn test_generator_order() {
        let result = scalar_mul(&N, &GX, &GY);
        assert!(result.is_infinity(), "n*G should be the point at infinity");
    }

    /// Test point doubling: 2*G should be on the curve. — ColdCipher
    #[test]
    fn test_point_double() {
        let g = JacobianPoint::from_affine(&GX, &GY);
        let g2 = point_double(&g);
        let (x2, y2) = g2.to_affine().unwrap();

        // Verify 2G is on the curve
        let lhs = fp_sqr(&y2);
        let x3 = fp_mul(&fp_sqr(&x2), &x2);
        let ax = fp_mul(&A, &x2);
        let rhs = fp_add(&fp_add(&x3, &ax), &B);
        assert_eq!(lhs, rhs, "2G is not on the curve");
    }

    /// Test pubkey parsing from uncompressed format. — ColdCipher
    #[test]
    fn test_pubkey_parse() {
        let mut key_bytes = [0u8; 65];
        key_bytes[0] = 0x04;
        let gx_bytes = u256_to_be_bytes(&GX);
        let gy_bytes = u256_to_be_bytes(&GY);
        key_bytes[1..33].copy_from_slice(&gx_bytes);
        key_bytes[33..65].copy_from_slice(&gy_bytes);

        let pk = p256_pubkey_from_uncompressed(&key_bytes);
        assert!(pk.is_some(), "Failed to parse generator point as public key");
    }

    /// Verify a known ECDSA signature (NIST test vector from FIPS 186-4).
    /// — ColdCipher: "If this fails, our entire curve implementation is wrong. No pressure."
    #[test]
    fn test_ecdsa_verify_nist_vector() {
        // NIST P-256 ECDSA test vector
        // Source: NIST CAVP ECDSA test vectors (SigVer)
        let msg_hash: [u8; 32] = [
            0x44, 0xac, 0xf6, 0xb7, 0xe3, 0x6c, 0x13, 0x42,
            0xc2, 0xc5, 0x89, 0x72, 0x04, 0xfe, 0x09, 0x50,
            0x4e, 0x1e, 0x2e, 0xfb, 0x1a, 0x90, 0x03, 0x77,
            0xdb, 0xc4, 0xe7, 0xa6, 0xa1, 0x33, 0xec, 0x56,
        ];

        // Public key Q
        let qx_bytes: [u8; 32] = [
            0x1c, 0xcb, 0xe9, 0x1c, 0x07, 0x5f, 0xc7, 0xf4,
            0xf0, 0x33, 0xbf, 0xa2, 0x48, 0xdb, 0x8f, 0xcc,
            0xd3, 0x56, 0x5d, 0xe9, 0x4b, 0xbf, 0xb1, 0x2f,
            0x3c, 0x59, 0xff, 0x46, 0xc2, 0x71, 0xbf, 0x83,
        ];
        let qy_bytes: [u8; 32] = [
            0xce, 0x40, 0x14, 0xc6, 0x88, 0x11, 0xf9, 0xa2,
            0x1a, 0x1f, 0xdb, 0x2c, 0x0e, 0x61, 0x13, 0xe0,
            0x6d, 0xb7, 0xca, 0x93, 0xb7, 0x40, 0x4e, 0x78,
            0xdc, 0x7c, 0xcd, 0x5c, 0xa8, 0x9a, 0x4c, 0xa9,
        ];

        let mut pk_bytes = [0u8; 65];
        pk_bytes[0] = 0x04;
        pk_bytes[1..33].copy_from_slice(&qx_bytes);
        pk_bytes[33..65].copy_from_slice(&qy_bytes);

        let pubkey = p256_pubkey_from_uncompressed(&pk_bytes).expect("Failed to parse test pubkey");

        // Signature (r || s)
        let signature: [u8; 64] = [
            // r
            0xf3, 0xac, 0x80, 0x61, 0xb5, 0x14, 0x79, 0x5b,
            0x88, 0x43, 0xe3, 0xd6, 0x62, 0x95, 0x27, 0xed,
            0x2a, 0xfd, 0x6b, 0x1f, 0x6a, 0x55, 0x5a, 0x7a,
            0xca, 0xbb, 0x5e, 0x6f, 0x79, 0xc8, 0xc2, 0xac,
            // s
            0x8b, 0xf7, 0x78, 0x19, 0xca, 0x05, 0xa6, 0xb2,
            0x78, 0x6c, 0x76, 0x26, 0x2b, 0xf7, 0x37, 0x1c,
            0xef, 0x97, 0xb2, 0x18, 0xe9, 0x6f, 0x17, 0x5a,
            0x3c, 0xcd, 0xda, 0x2a, 0xcc, 0x05, 0x89, 0x03,
        ];

        assert!(
            p256_verify(&msg_hash, &signature, &pubkey),
            "NIST P-256 ECDSA test vector verification failed — time to question everything"
        );
    }

    /// Verify that a tampered signature fails. — ColdCipher
    #[test]
    fn test_ecdsa_verify_tampered() {
        let msg_hash: [u8; 32] = [0x44; 32]; // arbitrary
        let mut pk_bytes = [0u8; 65];
        pk_bytes[0] = 0x04;
        let gx_bytes = u256_to_be_bytes(&GX);
        let gy_bytes = u256_to_be_bytes(&GY);
        pk_bytes[1..33].copy_from_slice(&gx_bytes);
        pk_bytes[33..65].copy_from_slice(&gy_bytes);

        let pubkey = p256_pubkey_from_uncompressed(&pk_bytes).unwrap();

        // Random garbage signature
        let signature = [0xABu8; 64];

        assert!(
            !p256_verify(&msg_hash, &signature, &pubkey),
            "Random garbage signature should not verify"
        );
    }
}
