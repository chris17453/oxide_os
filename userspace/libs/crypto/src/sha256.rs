//! SHA-256 hash function
//!
//! FIPS 180-4 compliant. Ripped from the kernel side, reforged for userspace.
//! Every bit accounted for, every round constant verified against the primes.
//! — ColdCipher: "Trust no hash you haven't watched compress."

/// SHA-256 initial hash values — first 32 bits of fractional parts of
/// square roots of the first 8 primes (2..19)
const H256_INIT: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 round constants — first 32 bits of fractional parts of
/// cube roots of the first 64 primes (2..311)
const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 hasher state
/// — ColdCipher: "256 bits of mathematical certainty in a world of lies."
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    /// Create a new SHA-256 hasher
    pub fn new() -> Self {
        Self {
            state: H256_INIT,
            buffer: [0u8; 64],
            buffer_len: 0,
            total_len: 0,
        }
    }

    /// Update hash with data
    pub fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u64;
        let mut offset = 0;

        // Fill buffer if partially full
        if self.buffer_len > 0 {
            let to_copy = core::cmp::min(64 - self.buffer_len, data.len());
            self.buffer[self.buffer_len..self.buffer_len + to_copy]
                .copy_from_slice(&data[..to_copy]);
            self.buffer_len += to_copy;
            offset = to_copy;

            if self.buffer_len == 64 {
                self.compress();
                self.buffer_len = 0;
            }
        }

        // Process complete blocks
        while offset + 64 <= data.len() {
            self.buffer.copy_from_slice(&data[offset..offset + 64]);
            self.compress();
            offset += 64;
        }

        // Store remaining bytes
        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buffer[..remaining].copy_from_slice(&data[offset..]);
            self.buffer_len = remaining;
        }
    }

    /// Finalize and return the 32-byte hash
    pub fn finalize(mut self) -> [u8; 32] {
        // Padding — the ritual bit-stuffing dance mandated by FIPS 180-4
        let bit_len = self.total_len * 8;

        // Append 0x80
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;

        // If not enough room for length, pad and compress
        if self.buffer_len > 56 {
            for i in self.buffer_len..64 {
                self.buffer[i] = 0;
            }
            self.compress();
            self.buffer_len = 0;
        }

        // Pad with zeros up to length field
        for i in self.buffer_len..56 {
            self.buffer[i] = 0;
        }

        // Append bit length as big-endian 64-bit
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        self.compress();

        // Output hash — serialize state to bytes
        let mut result = [0u8; 32];
        for (i, &h) in self.state.iter().enumerate() {
            result[i * 4..(i + 1) * 4].copy_from_slice(&h.to_be_bytes());
        }
        result
    }

    fn compress(&mut self) {
        let mut w = [0u32; 64];

        // Parse block into 16 32-bit words
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                self.buffer[i * 4],
                self.buffer[i * 4 + 1],
                self.buffer[i * 4 + 2],
                self.buffer[i * 4 + 3],
            ]);
        }

        // Extend to 64 words — message schedule
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
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

        // 64 rounds of compression — where the real work happens
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K256[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
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

        // Add compressed chunk to running state
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

/// Compute SHA-256 hash of data in one shot
/// — ColdCipher: "One call, 32 bytes of truth."
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        let hash = sha256(b"");
        // Known SHA-256 of empty string: e3b0c44298fc1c14...
        assert_eq!(
            &hash[..8],
            &[0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14]
        );
    }

    #[test]
    fn test_sha256_abc() {
        let hash = sha256(b"abc");
        // Known SHA-256 of "abc": ba7816bf8f01cfea...
        assert_eq!(
            &hash[..8],
            &[0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea]
        );
    }
}
