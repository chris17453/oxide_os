//! SHA-512 hash function
//!
//! FIPS 180-4 compliant. The big brother of SHA-256 — twice the state,
//! twice the paranoia, same unbreakable compression.
//! — ColdCipher: "512 bits. Because 256 wasn't enough to contain my trust issues."

/// SHA-512 initial hash values (H0 through H7) — first 64 bits of fractional
/// parts of square roots of the first 8 primes (2..19)
const H512: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

/// SHA-512 round constants — first 64 bits of fractional parts of
/// cube roots of the first 80 primes
const K: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
    0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
    0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
    0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
    0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
    0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
    0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
    0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
    0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
    0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

/// SHA-512 hasher state
/// — ColdCipher: "128-byte blocks, 80 rounds. The compression function that never sleeps."
pub struct Sha512 {
    state: [u64; 8],
    buffer: [u8; 128],
    buffer_len: usize,
    total_len: u128,
}

impl Default for Sha512 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha512 {
    /// Create a new SHA-512 hasher
    pub fn new() -> Self {
        Self {
            state: H512,
            buffer: [0u8; 128],
            buffer_len: 0,
            total_len: 0,
        }
    }

    /// Create a SHA-512 hasher with custom initial state (used by SHA-384)
    pub(crate) fn with_state(state: [u64; 8]) -> Self {
        Self {
            state,
            buffer: [0u8; 128],
            buffer_len: 0,
            total_len: 0,
        }
    }

    /// Update hash with data
    pub fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u128;
        let mut offset = 0;

        // Fill buffer if partially full
        if self.buffer_len > 0 {
            let to_copy = core::cmp::min(128 - self.buffer_len, data.len());
            self.buffer[self.buffer_len..self.buffer_len + to_copy]
                .copy_from_slice(&data[..to_copy]);
            self.buffer_len += to_copy;
            offset = to_copy;

            if self.buffer_len == 128 {
                self.compress();
                self.buffer_len = 0;
            }
        }

        // Process complete blocks
        while offset + 128 <= data.len() {
            self.buffer.copy_from_slice(&data[offset..offset + 128]);
            self.compress();
            offset += 128;
        }

        // Store remaining bytes
        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buffer[..remaining].copy_from_slice(&data[offset..]);
            self.buffer_len = remaining;
        }
    }

    /// Finalize and return the full 64-byte hash
    pub fn finalize(mut self) -> [u8; 64] {
        // Padding
        let bit_len = self.total_len * 8;

        // Append 0x80
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;

        // If not enough room for length, pad and compress
        if self.buffer_len > 112 {
            for i in self.buffer_len..128 {
                self.buffer[i] = 0;
            }
            self.compress();
            self.buffer_len = 0;
        }

        // Pad with zeros up to length field
        for i in self.buffer_len..112 {
            self.buffer[i] = 0;
        }

        // Append bit length as big-endian 128-bit
        self.buffer[112..128].copy_from_slice(&bit_len.to_be_bytes());
        self.compress();

        // Output hash
        let mut result = [0u8; 64];
        for (i, &h) in self.state.iter().enumerate() {
            result[i * 8..(i + 1) * 8].copy_from_slice(&h.to_be_bytes());
        }
        result
    }

    fn compress(&mut self) {
        let mut w = [0u64; 80];

        // Parse block into 16 64-bit words
        for i in 0..16 {
            w[i] = u64::from_be_bytes([
                self.buffer[i * 8],
                self.buffer[i * 8 + 1],
                self.buffer[i * 8 + 2],
                self.buffer[i * 8 + 3],
                self.buffer[i * 8 + 4],
                self.buffer[i * 8 + 5],
                self.buffer[i * 8 + 6],
                self.buffer[i * 8 + 7],
            ]);
        }

        // Extend to 80 words — message schedule expansion
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        // Initialize working variables
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        // 80 rounds — the SHA-512 grind
        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        // Add to state
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

/// Compute SHA-512 hash of data in one shot
/// — ColdCipher: "64 bytes of finality."
pub fn sha512(data: &[u8]) -> [u8; 64] {
    let mut hasher = Sha512::new();
    hasher.update(data);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha512_empty() {
        let hash = sha512(b"");
        // Known SHA-512 of empty string: cf83e1357eefb8bd...
        assert_eq!(
            &hash[..8],
            &[0xcf, 0x83, 0xe1, 0x35, 0x7e, 0xef, 0xb8, 0xbd]
        );
    }

    #[test]
    fn test_sha512_abc() {
        let hash = sha512(b"abc");
        // Known SHA-512 of "abc": ddaf35a193617aba...
        assert_eq!(
            &hash[..8],
            &[0xdd, 0xaf, 0x35, 0xa1, 0x93, 0x61, 0x7a, 0xba]
        );
    }
}
