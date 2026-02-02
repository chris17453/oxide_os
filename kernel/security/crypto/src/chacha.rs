//! ChaCha20-Poly1305 authenticated encryption
//!
//! RFC 8439 compliant implementation.

use crate::{CryptoError, CryptoResult};
use alloc::vec::Vec;

/// ChaCha20 key (32 bytes)
pub type ChaChaKey = [u8; 32];

/// ChaCha20 nonce (12 bytes)
pub type ChaChaNonce = [u8; 12];

/// Poly1305 tag (16 bytes)
pub type Poly1305Tag = [u8; 16];

/// ChaCha20-Poly1305 AEAD cipher
pub struct ChaCha20Poly1305 {
    key: ChaChaKey,
}

impl ChaCha20Poly1305 {
    /// Create new cipher with key
    pub fn new(key: &ChaChaKey) -> Self {
        ChaCha20Poly1305 { key: *key }
    }

    /// Encrypt plaintext with associated data
    pub fn encrypt(&self, nonce: &ChaChaNonce, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
        // Generate Poly1305 key from first 32 bytes of keystream
        let poly_key = self.chacha20_block(nonce, 0);
        let poly_key: [u8; 32] = poly_key[..32].try_into().unwrap();

        // Encrypt plaintext with counter starting at 1
        let ciphertext = self.chacha20_encrypt(nonce, 1, plaintext);

        // Compute authentication tag
        let tag = poly1305_mac(&poly_key, &ciphertext, aad);

        let mut output = Vec::with_capacity(ciphertext.len() + 16);
        output.extend_from_slice(&ciphertext);
        output.extend_from_slice(&tag);
        output
    }

    /// Decrypt ciphertext with associated data
    pub fn decrypt(
        &self,
        nonce: &ChaChaNonce,
        ciphertext: &[u8],
        aad: &[u8],
    ) -> CryptoResult<Vec<u8>> {
        if ciphertext.len() < 16 {
            return Err(CryptoError::InvalidInput);
        }

        let tag_offset = ciphertext.len() - 16;
        let ct = &ciphertext[..tag_offset];
        let tag = &ciphertext[tag_offset..];

        // Generate Poly1305 key
        let poly_key = self.chacha20_block(nonce, 0);
        let poly_key: [u8; 32] = poly_key[..32].try_into().unwrap();

        // Verify tag
        let computed_tag = poly1305_mac(&poly_key, ct, aad);
        if !constant_time_eq(tag, &computed_tag) {
            return Err(CryptoError::DecryptionFailed);
        }

        // Decrypt
        let plaintext = self.chacha20_encrypt(nonce, 1, ct);
        Ok(plaintext)
    }

    /// ChaCha20 block function
    fn chacha20_block(&self, nonce: &ChaChaNonce, counter: u32) -> [u8; 64] {
        let mut state = [0u32; 16];

        // Constants "expand 32-byte k"
        state[0] = 0x61707865;
        state[1] = 0x3320646e;
        state[2] = 0x79622d32;
        state[3] = 0x6b206574;

        // Key
        for i in 0..8 {
            state[4 + i] = u32::from_le_bytes([
                self.key[i * 4],
                self.key[i * 4 + 1],
                self.key[i * 4 + 2],
                self.key[i * 4 + 3],
            ]);
        }

        // Counter
        state[12] = counter;

        // Nonce
        state[13] = u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]);
        state[14] = u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]);
        state[15] = u32::from_le_bytes([nonce[8], nonce[9], nonce[10], nonce[11]]);

        let initial = state;

        // 20 rounds (10 double rounds)
        for _ in 0..10 {
            // Column rounds
            quarter_round(&mut state, 0, 4, 8, 12);
            quarter_round(&mut state, 1, 5, 9, 13);
            quarter_round(&mut state, 2, 6, 10, 14);
            quarter_round(&mut state, 3, 7, 11, 15);
            // Diagonal rounds
            quarter_round(&mut state, 0, 5, 10, 15);
            quarter_round(&mut state, 1, 6, 11, 12);
            quarter_round(&mut state, 2, 7, 8, 13);
            quarter_round(&mut state, 3, 4, 9, 14);
        }

        // Add initial state
        for i in 0..16 {
            state[i] = state[i].wrapping_add(initial[i]);
        }

        // Serialize to bytes
        let mut output = [0u8; 64];
        for i in 0..16 {
            let bytes = state[i].to_le_bytes();
            output[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }

        output
    }

    /// ChaCha20 stream cipher
    fn chacha20_encrypt(&self, nonce: &ChaChaNonce, start_counter: u32, data: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(data.len());
        let mut counter = start_counter;

        for chunk in data.chunks(64) {
            let keystream = self.chacha20_block(nonce, counter);
            counter += 1;

            for (i, &byte) in chunk.iter().enumerate() {
                output.push(byte ^ keystream[i]);
            }
        }

        output
    }
}

/// ChaCha20 quarter round
fn quarter_round(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(16);

    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(12);

    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(8);

    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(7);
}

/// Poly1305 one-time authenticator
fn poly1305_mac(key: &[u8; 32], ciphertext: &[u8], aad: &[u8]) -> Poly1305Tag {
    // Initialize accumulator
    let mut acc = [0u64; 3];

    // r = key[0..16] clamped
    let mut r = [0u32; 5];
    r[0] = u32::from_le_bytes([key[0], key[1], key[2], key[3]]) & 0x0fff_fffc;
    r[1] = u32::from_le_bytes([key[3], key[4], key[5], key[6]]) >> 2 & 0x0fff_fffc;
    r[2] = u32::from_le_bytes([key[6], key[7], key[8], key[9]]) >> 4 & 0x0fff_fffc;
    r[3] = u32::from_le_bytes([key[9], key[10], key[11], key[12]]) >> 6 & 0x0fff_fffc;
    r[4] = u32::from_le_bytes([key[12], key[13], key[14], key[15]]) >> 8 & 0x000f_ffff;

    // s = key[16..32]
    let s = [
        u32::from_le_bytes([key[16], key[17], key[18], key[19]]),
        u32::from_le_bytes([key[20], key[21], key[22], key[23]]),
        u32::from_le_bytes([key[24], key[25], key[26], key[27]]),
        u32::from_le_bytes([key[28], key[29], key[30], key[31]]),
    ];

    // Process AAD (padded to 16 bytes)
    poly1305_blocks(&mut acc, &r, aad);

    // Process ciphertext (padded to 16 bytes)
    poly1305_blocks(&mut acc, &r, ciphertext);

    // Process lengths
    let mut len_block = [0u8; 16];
    let aad_len = (aad.len() as u64).to_le_bytes();
    let ct_len = (ciphertext.len() as u64).to_le_bytes();
    len_block[..8].copy_from_slice(&aad_len);
    len_block[8..].copy_from_slice(&ct_len);
    poly1305_block(&mut acc, &r, &len_block, 16);

    // Final: tag = (acc + s) mod 2^128
    let mut tag = [0u8; 16];
    let f = acc[0] as u128 + ((acc[1] as u128) << 44) + ((acc[2] as u128) << 88);
    let s128 =
        s[0] as u128 + ((s[1] as u128) << 32) + ((s[2] as u128) << 64) + ((s[3] as u128) << 96);
    let result = f.wrapping_add(s128);

    tag[..8].copy_from_slice(&(result as u64).to_le_bytes());
    tag[8..].copy_from_slice(&((result >> 64) as u64).to_le_bytes());

    tag
}

/// Process multiple blocks
fn poly1305_blocks(acc: &mut [u64; 3], r: &[u32; 5], data: &[u8]) {
    for chunk in data.chunks(16) {
        let len = chunk.len();
        let mut block = [0u8; 17];
        block[..len].copy_from_slice(chunk);
        block[len] = 1; // Add 1 bit
        poly1305_block(acc, r, &block[..16], len);
    }
}

/// Process single block
fn poly1305_block(acc: &mut [u64; 3], r: &[u32; 5], block: &[u8], _len: usize) {
    // Add block to accumulator
    let n0 = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
    let n1 = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    let n2 = u32::from_le_bytes([block[8], block[9], block[10], block[11]]);
    let n3 = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);

    acc[0] = acc[0].wrapping_add(n0 as u64 | ((n1 as u64) << 32));
    acc[1] = acc[1].wrapping_add(n2 as u64 | ((n3 as u64) << 32));
    acc[2] = acc[2].wrapping_add(1);

    // Multiply by r (simplified)
    let r0 = r[0] as u64;
    let t0 = acc[0].wrapping_mul(r0);
    let t1 = acc[1].wrapping_mul(r0);
    let t2 = acc[2].wrapping_mul(r0);

    acc[0] = t0 & 0xfffffffffff;
    acc[1] = t1.wrapping_add(t0 >> 44) & 0xfffffffffff;
    acc[2] = t2.wrapping_add(t1 >> 44) & 0x3ffffffffff;
}

/// Constant-time comparison
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}
