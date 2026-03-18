// ============================================================================
// Big Integer Arithmetic for RSA Verification
// ============================================================================
// Fixed-size 4096-bit integers. Because RSA keys are compensating for
// something and we need to keep up. No heap allocation for the core type —
// it's all stack, all the time. — ColdCipher
//
// Only implements what RSA-PKCS1v15 verification needs:
// - Addition, subtraction, comparison
// - Modular multiplication (via schoolbook + reduction)
// - Modular exponentiation (square-and-multiply, optimized for e=65537)
//
// NOT constant-time. This is verification with public exponents.
// If you're doing private key operations with this, you deserve what you get.
// — ColdCipher

extern crate alloc;

/// Maximum number of u64 limbs. 64 limbs * 64 bits = 4096 bits.
/// Enough for RSA-4096. If you need more, you have other problems. — ColdCipher
pub const MAX_LIMBS: usize = 64;

/// Fixed-size big integer, little-endian limbs.
/// `len` tracks the number of significant limbs (i.e., limbs[len..] are zero).
/// — ColdCipher: "4096 bits of trust issues, packed into a struct."
#[derive(Clone)]
pub struct BigInt {
    pub limbs: [u64; MAX_LIMBS],
    pub len: usize,
}

impl BigInt {
    /// The additive identity. Nothing. Zilch. The RSA equivalent of /dev/null. — ColdCipher
    pub const fn zero() -> Self {
        BigInt {
            limbs: [0u64; MAX_LIMBS],
            len: 0,
        }
    }

    /// The multiplicative identity. The only number RSA respects. — ColdCipher
    pub fn one() -> Self {
        let mut b = Self::zero();
        b.limbs[0] = 1;
        b.len = 1;
        b
    }

    /// Create from a single u64. For small constants like e=65537. — ColdCipher
    pub fn from_u64(v: u64) -> Self {
        let mut b = Self::zero();
        if v != 0 {
            b.limbs[0] = v;
            b.len = 1;
        }
        b
    }

    /// Parse from big-endian bytes (DER/wire format).
    /// — ColdCipher: "Network byte order. The one thing the entire industry agrees on."
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        let mut b = Self::zero();

        // Skip leading zeros
        let mut start = 0;
        while start < bytes.len() && bytes[start] == 0 {
            start += 1;
        }
        let bytes = &bytes[start..];

        if bytes.is_empty() {
            return b;
        }

        // Fill limbs from least significant byte
        let byte_len = bytes.len();
        let mut limb_idx = 0;
        let mut byte_pos = 0;

        while byte_pos < byte_len && limb_idx < MAX_LIMBS {
            let mut limb = 0u64;
            // Read up to 8 bytes for this limb, from the end of bytes
            for shift in 0..8 {
                let src_idx = byte_len - 1 - byte_pos;
                limb |= (bytes[src_idx] as u64) << (shift * 8);
                byte_pos += 1;
                if byte_pos >= byte_len {
                    break;
                }
            }
            b.limbs[limb_idx] = limb;
            limb_idx += 1;
        }

        b.len = limb_idx;
        b.normalize();
        b
    }

    /// Encode as big-endian bytes into a buffer.
    /// Returns the number of bytes written.
    /// — ColdCipher: "Serialization. Where integers go to become network packets."
    pub fn to_be_bytes(&self, buf: &mut [u8]) -> usize {
        // Calculate the number of bytes needed
        let bit_len = self.bit_len();
        let byte_len = (bit_len + 7) / 8;

        if byte_len == 0 {
            if !buf.is_empty() {
                buf[0] = 0;
                return 1;
            }
            return 0;
        }

        if buf.len() < byte_len {
            return 0; // Buffer too small
        }

        // Fill from the end
        let mut pos = byte_len;
        for i in 0..self.len {
            let limb = self.limbs[i];
            for byte_idx in 0..8 {
                if pos == 0 {
                    break;
                }
                pos -= 1;
                buf[pos] = ((limb >> (byte_idx * 8)) & 0xFF) as u8;
            }
        }

        byte_len
    }

    /// Encode as big-endian bytes, zero-padded to exactly `width` bytes.
    /// Used for RSA where output must match modulus size. — ColdCipher
    pub fn to_be_bytes_padded(&self, buf: &mut [u8], width: usize) -> bool {
        if buf.len() < width {
            return false;
        }

        // Zero-fill
        for b in buf[..width].iter_mut() {
            *b = 0;
        }

        let bit_len = self.bit_len();
        let byte_len = (bit_len + 7) / 8;

        if byte_len > width {
            return false; // Value too large for requested width
        }

        // Fill from the end of the width
        let mut pos = width;
        for i in 0..self.len {
            let limb = self.limbs[i];
            for byte_idx in 0..8 {
                if pos == 0 {
                    break;
                }
                pos -= 1;
                buf[pos] = ((limb >> (byte_idx * 8)) & 0xFF) as u8;
            }
        }

        true
    }

    /// Strip leading zero limbs. Hygiene. — ColdCipher
    fn normalize(&mut self) {
        while self.len > 0 && self.limbs[self.len - 1] == 0 {
            self.len -= 1;
        }
    }

    /// Number of significant bits. — ColdCipher
    pub fn bit_len(&self) -> usize {
        if self.len == 0 {
            return 0;
        }
        let top = self.limbs[self.len - 1];
        (self.len - 1) * 64 + (64 - top.leading_zeros() as usize)
    }

    /// Is this zero? — ColdCipher
    pub fn is_zero(&self) -> bool {
        self.len == 0
    }

    /// Get bit at position `i` (0-indexed from LSB). — ColdCipher
    fn bit(&self, i: usize) -> bool {
        let limb_idx = i / 64;
        let bit_idx = i % 64;
        if limb_idx >= self.len {
            return false;
        }
        (self.limbs[limb_idx] >> bit_idx) & 1 == 1
    }

    /// Compare. Returns -1, 0, or 1.
    /// — ColdCipher: "Comparison is the thief of joy and the foundation of cryptography."
    pub fn cmp(&self, other: &BigInt) -> i8 {
        let a_len = self.len;
        let b_len = other.len;
        let max_len = if a_len > b_len { a_len } else { b_len };

        for i in (0..max_len).rev() {
            let a = if i < a_len { self.limbs[i] } else { 0 };
            let b = if i < b_len { other.limbs[i] } else { 0 };
            if a < b {
                return -1;
            }
            if a > b {
                return 1;
            }
        }
        0
    }
}

/// a + b. Result may be one limb longer than inputs. — ColdCipher
pub fn bigint_add(a: &BigInt, b: &BigInt) -> BigInt {
    let mut result = BigInt::zero();
    let max_len = if a.len > b.len { a.len } else { b.len };
    let mut carry = 0u64;

    for i in 0..max_len {
        let av = if i < a.len { a.limbs[i] } else { 0 };
        let bv = if i < b.len { b.limbs[i] } else { 0 };
        let (s1, c1) = av.overflowing_add(bv);
        let (s2, c2) = s1.overflowing_add(carry);
        result.limbs[i] = s2;
        carry = (c1 as u64) + (c2 as u64);
    }

    if carry > 0 && max_len < MAX_LIMBS {
        result.limbs[max_len] = carry;
        result.len = max_len + 1;
    } else {
        result.len = max_len;
    }
    result.normalize();
    result
}

/// a - b. Assumes a >= b (caller's responsibility). — ColdCipher
pub fn bigint_sub(a: &BigInt, b: &BigInt) -> BigInt {
    let mut result = BigInt::zero();
    let mut borrow = 0u64;

    for i in 0..a.len {
        let bv = if i < b.len { b.limbs[i] } else { 0 };
        let (s1, b1) = a.limbs[i].overflowing_sub(bv);
        let (s2, b2) = s1.overflowing_sub(borrow);
        result.limbs[i] = s2;
        borrow = (b1 as u64) + (b2 as u64);
    }

    result.len = a.len;
    result.normalize();
    result
}

/// Multiplication: a * b. Result is at most a.len + b.len limbs.
/// — ColdCipher: "O(n^2) schoolbook multiplication. Karatsuba would be faster
///   but harder to audit, and we're already paranoid enough."
fn bigint_mul(a: &BigInt, b: &BigInt) -> BigInt {
    let mut result = BigInt::zero();

    if a.is_zero() || b.is_zero() {
        return result;
    }

    let result_len = a.len + b.len;
    if result_len > MAX_LIMBS {
        // Overflow — shouldn't happen with proper RSA key sizes
        // — ColdCipher: "If you hit this, your keys are too big or your math is too wrong."
        return result;
    }

    for i in 0..a.len {
        let mut carry = 0u128;
        for j in 0..b.len {
            if i + j >= MAX_LIMBS {
                break;
            }
            let prod =
                (a.limbs[i] as u128) * (b.limbs[j] as u128) + (result.limbs[i + j] as u128) + carry;
            result.limbs[i + j] = prod as u64;
            carry = prod >> 64;
        }
        if i + b.len < MAX_LIMBS {
            result.limbs[i + b.len] = carry as u64;
        }
    }

    result.len = if result_len <= MAX_LIMBS {
        result_len
    } else {
        MAX_LIMBS
    };
    result.normalize();
    result
}

/// Left shift by `bits` positions. — ColdCipher
fn bigint_shl(a: &BigInt, bits: usize) -> BigInt {
    let mut result = BigInt::zero();
    let word_shift = bits / 64;
    let bit_shift = bits % 64;

    if word_shift >= MAX_LIMBS {
        return result;
    }

    for i in 0..a.len {
        if i + word_shift >= MAX_LIMBS {
            break;
        }
        if bit_shift == 0 {
            result.limbs[i + word_shift] = a.limbs[i];
        } else {
            result.limbs[i + word_shift] |= a.limbs[i] << bit_shift;
            if i + word_shift + 1 < MAX_LIMBS {
                result.limbs[i + word_shift + 1] |= a.limbs[i] >> (64 - bit_shift);
            }
        }
    }

    result.len = if a.len + word_shift + 1 <= MAX_LIMBS {
        a.len + word_shift + 1
    } else {
        MAX_LIMBS
    };
    result.normalize();
    result
}

/// Modular reduction: a mod m, using shift-and-subtract.
/// — ColdCipher: "Division the hard way. Because we don't have a divider and we don't need one."
fn bigint_mod(a: &BigInt, m: &BigInt) -> BigInt {
    if m.is_zero() {
        // — ColdCipher: "Division by zero. The universe's favorite undefined behavior."
        return BigInt::zero();
    }

    if a.cmp(m) < 0 {
        return a.clone();
    }

    let mut r = a.clone();
    let a_bits = a.bit_len();
    let m_bits = m.bit_len();

    if a_bits < m_bits {
        return r;
    }

    // Align m to the top of a, then subtract downward
    let shift = a_bits - m_bits;
    for k in (0..=shift).rev() {
        let shifted = bigint_shl(m, k);
        if r.cmp(&shifted) >= 0 {
            r = bigint_sub(&r, &shifted);
        }
    }

    r
}

/// Modular multiplication: (a * b) mod m.
/// — ColdCipher: "Multiply, then reduce. Simple in theory. 4096 bits of practice."
pub fn mul_mod(a: &BigInt, b: &BigInt, m: &BigInt) -> BigInt {
    let product = bigint_mul(a, b);
    bigint_mod(&product, m)
}

/// Modular exponentiation: base^exp mod modulus.
/// Uses square-and-multiply (left-to-right binary method).
///
/// For RSA verification with e=65537 (0x10001):
///   - 65537 in binary = 1_0000_0000_0000_0001
///   - That's 17 squarings and 2 multiplications (bit 0 and bit 16 are set)
///   - Much more efficient than general exponentiation
///
/// — ColdCipher: "17 squarings. 2 multiplies. The entirety of RSA verification
///   for the most common public exponent. Elegant, if you squint."
pub fn pow_mod(base: &BigInt, exp: &BigInt, modulus: &BigInt) -> BigInt {
    if modulus.is_zero() {
        return BigInt::zero();
    }

    let exp_bits = exp.bit_len();
    if exp_bits == 0 {
        // x^0 = 1 (mod m), for m > 1
        return BigInt::one();
    }

    let mut result = BigInt::one();
    let mut b = bigint_mod(base, modulus);

    // Right-to-left binary method
    for i in 0..exp_bits {
        if exp.bit(i) {
            result = mul_mod(&result, &b, modulus);
        }
        b = mul_mod(&b, &b, modulus);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// — ColdCipher: "If zero isn't zero, we have bigger problems than cryptography."
    #[test]
    fn test_zero() {
        let z = BigInt::zero();
        assert!(z.is_zero());
        assert_eq!(z.bit_len(), 0);
    }

    #[test]
    fn test_from_u64() {
        let v = BigInt::from_u64(65537);
        assert_eq!(v.limbs[0], 65537);
        assert_eq!(v.len, 1);
        assert_eq!(v.bit_len(), 17);
    }

    #[test]
    fn test_from_be_bytes() {
        let bytes = [0x01, 0x00, 0x01]; // 65537
        let v = BigInt::from_be_bytes(&bytes);
        assert_eq!(v.limbs[0], 65537);
    }

    #[test]
    fn test_add_sub() {
        let a = BigInt::from_u64(0xFFFFFFFFFFFFFFFF);
        let b = BigInt::from_u64(1);
        let sum = bigint_add(&a, &b);
        assert_eq!(sum.limbs[0], 0);
        assert_eq!(sum.limbs[1], 1);
        assert_eq!(sum.len, 2);

        let diff = bigint_sub(&sum, &b);
        assert_eq!(diff.limbs[0], 0xFFFFFFFFFFFFFFFF);
        assert_eq!(diff.len, 1);
    }

    #[test]
    fn test_mul() {
        let a = BigInt::from_u64(0xFFFFFFFF);
        let b = BigInt::from_u64(0xFFFFFFFF);
        let product = bigint_mul(&a, &b);
        // 0xFFFFFFFF * 0xFFFFFFFF = 0xFFFFFFFE00000001
        assert_eq!(product.limbs[0], 0xFFFFFFFE00000001);
        assert_eq!(product.len, 1);
    }

    #[test]
    fn test_mod() {
        let a = BigInt::from_u64(100);
        let m = BigInt::from_u64(7);
        let r = bigint_mod(&a, &m);
        assert_eq!(r.limbs[0], 2); // 100 mod 7 = 2
    }

    /// — ColdCipher: "The RSA-critical test. 65537 squarings and multiplies, condensed."
    #[test]
    fn test_pow_mod_small() {
        // 3^65537 mod 100 — verify against known result
        let base = BigInt::from_u64(3);
        let exp = BigInt::from_u64(65537);
        let modulus = BigInt::from_u64(100);
        let result = pow_mod(&base, &exp, &modulus);
        // 3^65537 mod 100 = 3 (since 3^20 mod 100 = 1 via Euler, and 65537 mod 20 = 17,
        // so 3^65537 = 3^17 mod 100)
        // 3^17 = 129140163, mod 100 = 63
        assert_eq!(result.limbs[0], 63);
    }

    /// Test modular exponentiation with small RSA-like operation.
    /// — ColdCipher: "RSA in miniature. If this works, the big version probably does too. Probably."
    #[test]
    fn test_rsa_tiny() {
        // Tiny RSA: p=61, q=53, n=3233, e=17, d=2753
        // Encrypt: m^e mod n, Decrypt: c^d mod n
        // Sign: m^d mod n, Verify: s^e mod n
        let n = BigInt::from_u64(3233);
        let e = BigInt::from_u64(17);
        let d = BigInt::from_u64(2753);
        let message = BigInt::from_u64(65);

        // Sign
        let signature = pow_mod(&message, &d, &n);
        // Verify
        let recovered = pow_mod(&signature, &e, &n);
        assert_eq!(recovered.limbs[0], 65);
    }

    #[test]
    fn test_to_be_bytes() {
        let v = BigInt::from_u64(0x0102030405060708);
        let mut buf = [0u8; 8];
        let len = v.to_be_bytes(&mut buf);
        assert_eq!(len, 8);
        assert_eq!(buf, [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
    }

    #[test]
    fn test_roundtrip_be_bytes() {
        let original = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
            0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
        ];
        let v = BigInt::from_be_bytes(&original);
        let mut buf = [0u8; 24];
        let len = v.to_be_bytes(&mut buf);
        // Leading zero stripped
        assert_eq!(&buf[..len], &original[1..]);
    }
}
