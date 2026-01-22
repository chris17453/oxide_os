//! Cryptographic random number generation
//!
//! CSPRNG for key generation and nonces.

use crate::{CryptoError, CryptoResult};
use spin::Mutex;

/// Global CSPRNG state
static RNG: Mutex<ChaChaRng> = Mutex::new(ChaChaRng::new());

/// ChaCha-based CSPRNG
struct ChaChaRng {
    state: [u32; 16],
    buffer: [u8; 64],
    index: usize,
}

impl ChaChaRng {
    /// Create new RNG (unseeded)
    const fn new() -> Self {
        ChaChaRng {
            state: [
                0x61707865, 0x3320646e, 0x79622d32, 0x6b206574, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            buffer: [0; 64],
            index: 64,
        }
    }

    /// Seed from entropy
    fn seed(&mut self, entropy: &[u8; 32]) {
        for i in 0..8 {
            self.state[4 + i] = u32::from_le_bytes([
                entropy[i * 4],
                entropy[i * 4 + 1],
                entropy[i * 4 + 2],
                entropy[i * 4 + 3],
            ]);
        }
        self.index = 64;
    }

    /// Add entropy (XOR with state)
    fn reseed(&mut self, entropy: &[u8]) {
        for (i, chunk) in entropy.chunks(4).enumerate() {
            if i >= 8 {
                break;
            }
            let mut bytes = [0u8; 4];
            bytes[..chunk.len()].copy_from_slice(chunk);
            self.state[4 + i] ^= u32::from_le_bytes(bytes);
        }
        self.index = 64;
    }

    /// Generate random bytes
    fn fill(&mut self, dest: &mut [u8]) {
        for byte in dest.iter_mut() {
            if self.index >= 64 {
                self.refill();
            }
            *byte = self.buffer[self.index];
            self.index += 1;
        }
    }

    /// Refill buffer with ChaCha20 block
    fn refill(&mut self) {
        let mut working = self.state;

        // 20 rounds
        for _ in 0..10 {
            // Column rounds
            quarter_round(&mut working, 0, 4, 8, 12);
            quarter_round(&mut working, 1, 5, 9, 13);
            quarter_round(&mut working, 2, 6, 10, 14);
            quarter_round(&mut working, 3, 7, 11, 15);
            // Diagonal rounds
            quarter_round(&mut working, 0, 5, 10, 15);
            quarter_round(&mut working, 1, 6, 11, 12);
            quarter_round(&mut working, 2, 7, 8, 13);
            quarter_round(&mut working, 3, 4, 9, 14);
        }

        // Add original state
        for i in 0..16 {
            working[i] = working[i].wrapping_add(self.state[i]);
        }

        // Serialize to buffer
        for i in 0..16 {
            let bytes = working[i].to_le_bytes();
            self.buffer[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }

        // Increment counter
        self.state[12] = self.state[12].wrapping_add(1);
        if self.state[12] == 0 {
            self.state[13] = self.state[13].wrapping_add(1);
        }

        self.index = 0;
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

/// Seed the global RNG
pub fn seed(entropy: &[u8; 32]) {
    RNG.lock().seed(entropy);
}

/// Add entropy to the global RNG
pub fn reseed(entropy: &[u8]) {
    RNG.lock().reseed(entropy);
}

/// Fill buffer with random bytes
pub fn fill_bytes(dest: &mut [u8]) {
    RNG.lock().fill(dest);
}

/// Generate random bytes
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    fill_bytes(&mut bytes);
    bytes
}

/// Generate random u64
pub fn random_u64() -> u64 {
    let bytes: [u8; 8] = random_bytes();
    u64::from_le_bytes(bytes)
}

/// Generate random u32
pub fn random_u32() -> u32 {
    let bytes: [u8; 4] = random_bytes();
    u32::from_le_bytes(bytes)
}

/// Generate a secure random 256-bit key
pub fn generate_key() -> CryptoResult<[u8; 32]> {
    Ok(random_bytes())
}

/// Generate a secure random 96-bit nonce
pub fn generate_nonce() -> CryptoResult<[u8; 12]> {
    Ok(random_bytes())
}
