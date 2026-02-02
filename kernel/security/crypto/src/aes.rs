//! AES-256-GCM authenticated encryption
//!
//! NIST-approved AEAD cipher.

use crate::{CryptoError, CryptoResult};
use alloc::vec::Vec;

/// AES-256 key (32 bytes)
pub type AesKey = [u8; 32];

/// GCM nonce (12 bytes)
pub type AesNonce = [u8; 12];

/// Authentication tag (16 bytes)
pub type AesTag = [u8; 16];

/// AES-256-GCM cipher
pub struct Aes256Gcm {
    /// Round keys
    round_keys: [[u8; 16]; 15],
}

impl Aes256Gcm {
    /// Create new cipher with key
    pub fn new(key: &AesKey) -> Self {
        let round_keys = key_expansion(key);
        Aes256Gcm { round_keys }
    }

    /// Encrypt plaintext with associated data
    pub fn encrypt(&self, nonce: &AesNonce, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(plaintext.len() + 16);

        // Generate counter blocks
        let mut counter = [0u8; 16];
        counter[..12].copy_from_slice(nonce);
        counter[15] = 1;

        // Encrypt each block
        let mut ciphertext = Vec::with_capacity(plaintext.len());
        for chunk in plaintext.chunks(16) {
            // Increment counter
            inc_counter(&mut counter);

            // Encrypt counter
            let keystream = self.encrypt_block(&counter);

            // XOR with plaintext
            for (i, &p) in chunk.iter().enumerate() {
                ciphertext.push(p ^ keystream[i]);
            }
        }

        // Compute authentication tag
        let tag = self.compute_tag(nonce, &ciphertext, aad);

        output.extend_from_slice(&ciphertext);
        output.extend_from_slice(&tag);
        output
    }

    /// Decrypt ciphertext with associated data
    pub fn decrypt(
        &self,
        nonce: &AesNonce,
        ciphertext: &[u8],
        aad: &[u8],
    ) -> CryptoResult<Vec<u8>> {
        if ciphertext.len() < 16 {
            return Err(CryptoError::InvalidInput);
        }

        let tag_offset = ciphertext.len() - 16;
        let ct = &ciphertext[..tag_offset];
        let tag = &ciphertext[tag_offset..];

        // Verify tag
        let computed_tag = self.compute_tag(nonce, ct, aad);
        if !constant_time_eq(tag, &computed_tag) {
            return Err(CryptoError::DecryptionFailed);
        }

        // Decrypt
        let mut counter = [0u8; 16];
        counter[..12].copy_from_slice(nonce);
        counter[15] = 1;

        let mut plaintext = Vec::with_capacity(ct.len());
        for chunk in ct.chunks(16) {
            inc_counter(&mut counter);
            let keystream = self.encrypt_block(&counter);

            for (i, &c) in chunk.iter().enumerate() {
                plaintext.push(c ^ keystream[i]);
            }
        }

        Ok(plaintext)
    }

    /// Encrypt a single AES block
    fn encrypt_block(&self, block: &[u8; 16]) -> [u8; 16] {
        let mut state = *block;

        // Initial round
        add_round_key(&mut state, &self.round_keys[0]);

        // Main rounds
        for i in 1..14 {
            sub_bytes(&mut state);
            shift_rows(&mut state);
            mix_columns(&mut state);
            add_round_key(&mut state, &self.round_keys[i]);
        }

        // Final round (no mix columns)
        sub_bytes(&mut state);
        shift_rows(&mut state);
        add_round_key(&mut state, &self.round_keys[14]);

        state
    }

    /// Compute GHASH authentication tag
    fn compute_tag(&self, nonce: &AesNonce, ciphertext: &[u8], aad: &[u8]) -> AesTag {
        // H = AES(K, 0^128)
        let h = self.encrypt_block(&[0u8; 16]);

        // Initial counter for tag
        let mut j0 = [0u8; 16];
        j0[..12].copy_from_slice(nonce);
        j0[15] = 1;

        // GHASH(H, A || C || len(A) || len(C))
        let mut ghash = [0u8; 16];

        // Process AAD
        for chunk in aad.chunks(16) {
            let mut block = [0u8; 16];
            block[..chunk.len()].copy_from_slice(chunk);
            xor_blocks(&mut ghash, &block);
            ghash = gf_mult(&ghash, &h);
        }

        // Process ciphertext
        for chunk in ciphertext.chunks(16) {
            let mut block = [0u8; 16];
            block[..chunk.len()].copy_from_slice(chunk);
            xor_blocks(&mut ghash, &block);
            ghash = gf_mult(&ghash, &h);
        }

        // Append lengths
        let mut len_block = [0u8; 16];
        let aad_bits = (aad.len() as u64) * 8;
        let ct_bits = (ciphertext.len() as u64) * 8;
        len_block[..8].copy_from_slice(&aad_bits.to_be_bytes());
        len_block[8..].copy_from_slice(&ct_bits.to_be_bytes());
        xor_blocks(&mut ghash, &len_block);
        ghash = gf_mult(&ghash, &h);

        // T = GHASH XOR AES(K, J0)
        let encrypted_j0 = self.encrypt_block(&j0);
        xor_blocks(&mut ghash, &encrypted_j0);

        ghash
    }
}

/// AES S-box
const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

/// Round constants
const RCON: [u8; 11] = [
    0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36,
];

/// Key expansion for AES-256
fn key_expansion(key: &AesKey) -> [[u8; 16]; 15] {
    let mut round_keys = [[0u8; 16]; 15];
    let mut w = [0u8; 240]; // 60 words * 4 bytes

    // Copy key
    w[..32].copy_from_slice(key);

    // Generate remaining words
    for i in 8..60 {
        let mut temp = [w[i * 4 - 4], w[i * 4 - 3], w[i * 4 - 2], w[i * 4 - 1]];

        if i % 8 == 0 {
            // Rotate
            let t = temp[0];
            temp[0] = SBOX[temp[1] as usize] ^ RCON[i / 8];
            temp[1] = SBOX[temp[2] as usize];
            temp[2] = SBOX[temp[3] as usize];
            temp[3] = SBOX[t as usize];
        } else if i % 8 == 4 {
            temp[0] = SBOX[temp[0] as usize];
            temp[1] = SBOX[temp[1] as usize];
            temp[2] = SBOX[temp[2] as usize];
            temp[3] = SBOX[temp[3] as usize];
        }

        w[i * 4] = w[i * 4 - 32] ^ temp[0];
        w[i * 4 + 1] = w[i * 4 - 31] ^ temp[1];
        w[i * 4 + 2] = w[i * 4 - 30] ^ temp[2];
        w[i * 4 + 3] = w[i * 4 - 29] ^ temp[3];
    }

    // Copy to round keys
    for i in 0..15 {
        round_keys[i].copy_from_slice(&w[i * 16..(i + 1) * 16]);
    }

    round_keys
}

/// SubBytes transformation
fn sub_bytes(state: &mut [u8; 16]) {
    for byte in state.iter_mut() {
        *byte = SBOX[*byte as usize];
    }
}

/// ShiftRows transformation
fn shift_rows(state: &mut [u8; 16]) {
    // Row 1: shift left by 1
    let temp = state[1];
    state[1] = state[5];
    state[5] = state[9];
    state[9] = state[13];
    state[13] = temp;

    // Row 2: shift left by 2
    let temp1 = state[2];
    let temp2 = state[6];
    state[2] = state[10];
    state[6] = state[14];
    state[10] = temp1;
    state[14] = temp2;

    // Row 3: shift left by 3
    let temp = state[15];
    state[15] = state[11];
    state[11] = state[7];
    state[7] = state[3];
    state[3] = temp;
}

/// MixColumns transformation
fn mix_columns(state: &mut [u8; 16]) {
    for i in 0..4 {
        let col = i * 4;
        let a = state[col];
        let b = state[col + 1];
        let c = state[col + 2];
        let d = state[col + 3];

        state[col] = gf_mul(a, 2) ^ gf_mul(b, 3) ^ c ^ d;
        state[col + 1] = a ^ gf_mul(b, 2) ^ gf_mul(c, 3) ^ d;
        state[col + 2] = a ^ b ^ gf_mul(c, 2) ^ gf_mul(d, 3);
        state[col + 3] = gf_mul(a, 3) ^ b ^ c ^ gf_mul(d, 2);
    }
}

/// AddRoundKey transformation
fn add_round_key(state: &mut [u8; 16], round_key: &[u8; 16]) {
    for i in 0..16 {
        state[i] ^= round_key[i];
    }
}

/// GF(2^8) multiplication
fn gf_mul(a: u8, b: u8) -> u8 {
    let mut p = 0u8;
    let mut hi_bit: u8;
    let mut aa = a;
    let mut bb = b;

    for _ in 0..8 {
        if bb & 1 != 0 {
            p ^= aa;
        }
        hi_bit = aa & 0x80;
        aa <<= 1;
        if hi_bit != 0 {
            aa ^= 0x1b; // Reduction polynomial
        }
        bb >>= 1;
    }

    p
}

/// GF(2^128) multiplication for GHASH
fn gf_mult(x: &[u8; 16], y: &[u8; 16]) -> [u8; 16] {
    let mut z = [0u8; 16];
    let mut v = *y;

    for i in 0..128 {
        if (x[i / 8] >> (7 - (i % 8))) & 1 != 0 {
            xor_blocks(&mut z, &v);
        }

        let lsb = v[15] & 1;
        // Shift right
        for j in (1..16).rev() {
            v[j] = (v[j] >> 1) | (v[j - 1] << 7);
        }
        v[0] >>= 1;

        if lsb != 0 {
            v[0] ^= 0xe1; // Reduction polynomial
        }
    }

    z
}

/// XOR two blocks
fn xor_blocks(a: &mut [u8; 16], b: &[u8; 16]) {
    for i in 0..16 {
        a[i] ^= b[i];
    }
}

/// Increment counter block
fn inc_counter(counter: &mut [u8; 16]) {
    for i in (12..16).rev() {
        counter[i] = counter[i].wrapping_add(1);
        if counter[i] != 0 {
            break;
        }
    }
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
