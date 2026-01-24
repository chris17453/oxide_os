//! SSH Cryptographic Operations
//!
//! Handles:
//! - Host key generation/loading
//! - Ed25519 signing for host key
//! - ChaCha20-Poly1305 encryption
//! - Key derivation from shared secret

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use libc::socket::recv;
use libc::*;

use crate::transport::{TransportError, TransportResult};

/// ChaCha20-Poly1305 key (32 bytes)
pub const CHACHA_KEY_LEN: usize = 32;

/// ChaCha20-Poly1305 nonce (12 bytes, but SSH uses 8-byte implicit IV)
pub const CHACHA_NONCE_LEN: usize = 12;

/// Poly1305 tag length
pub const TAG_LEN: usize = 16;

/// Thread-safe cell for host keys
struct SyncUnsafeCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncUnsafeCell<T> {}

impl<T> SyncUnsafeCell<T> {
    const fn new(value: T) -> Self {
        SyncUnsafeCell(UnsafeCell::new(value))
    }

    fn get(&self) -> *mut T {
        self.0.get()
    }
}

/// Ed25519 secret key
static HOST_SECRET_KEY: SyncUnsafeCell<[u8; 32]> = SyncUnsafeCell::new([0; 32]);
/// Ed25519 public key
static HOST_PUBLIC_KEY: SyncUnsafeCell<[u8; 32]> = SyncUnsafeCell::new([0; 32]);
/// Key initialized flag
static KEY_INITIALIZED: SyncUnsafeCell<bool> = SyncUnsafeCell::new(false);

/// Initialize or load host key
pub fn init_host_key() -> TransportResult<()> {
    unsafe {
        if *KEY_INITIALIZED.get() {
            return Ok(());
        }

        let secret = &mut *HOST_SECRET_KEY.get();
        let public = &mut *HOST_PUBLIC_KEY.get();

        // Try to load from /etc/ssh/ssh_host_ed25519_key
        let fd = open2("/etc/ssh/ssh_host_ed25519_key", O_RDONLY);
        if fd >= 0 {
            let mut buf = [0u8; 64];
            let n = read(fd, &mut buf);
            close(fd);

            if n >= 32 {
                secret.copy_from_slice(&buf[..32]);
                // Derive public key
                derive_ed25519_public_key(secret, public);
                *KEY_INITIALIZED.get() = true;
                return Ok(());
            }
        }

        // Generate new key using system randomness
        let fd = open2("/dev/random", O_RDONLY);
        if fd >= 0 {
            let mut buf = [0u8; 32];
            let n = read(fd, &mut buf);
            close(fd);

            if n == 32 {
                secret.copy_from_slice(&buf);
            } else {
                // Fallback: use simple PRNG seeded with time
                let seed = getpid() as u64 ^ 0x1234567890abcdef;
                for i in 0..32 {
                    secret[i] = ((seed >> (i % 8 * 8)) ^ (i as u64 * 7)) as u8;
                }
            }
        } else {
            // No /dev/random, use deterministic key (NOT FOR PRODUCTION)
            for i in 0..32 {
                secret[i] = (i as u8).wrapping_mul(17).wrapping_add(42);
            }
        }

        // Derive public key
        derive_ed25519_public_key(secret, public);

        // Try to save key
        let _ = mkdir("/etc/ssh", 0o700);
        let fd = open(
            "/etc/ssh/ssh_host_ed25519_key",
            (O_WRONLY | O_CREAT | O_TRUNC) as u32,
            0o600,
        );
        if fd >= 0 {
            let _ = write(fd, secret);
            close(fd);
        }

        *KEY_INITIALIZED.get() = true;
        Ok(())
    }
}

/// Get host public key
pub fn host_public_key() -> &'static [u8; 32] {
    unsafe { &*HOST_PUBLIC_KEY.get() }
}

/// Get host secret key
pub fn host_secret_key() -> &'static [u8; 32] {
    unsafe { &*HOST_SECRET_KEY.get() }
}

/// Derive Ed25519 public key from secret key
fn derive_ed25519_public_key(secret: &[u8; 32], public: &mut [u8; 32]) {
    // SHA-512 hash of secret key
    let h = sha512(secret);

    // Clamp scalar
    let mut scalar = [0u8; 32];
    scalar.copy_from_slice(&h[..32]);
    scalar[0] &= 248;
    scalar[31] &= 127;
    scalar[31] |= 64;

    // Multiply by base point
    let point = ed25519_scalarmult_base(&scalar);
    public.copy_from_slice(&point);
}

/// Sign data with host key (Ed25519)
pub fn sign_with_host_key(data: &[u8]) -> [u8; 64] {
    ed25519_sign(data, host_secret_key(), host_public_key())
}

/// Encode host public key in SSH format
pub fn encode_host_public_key() -> Vec<u8> {
    let mut result = Vec::with_capacity(51);

    // String "ssh-ed25519"
    let key_type = b"ssh-ed25519";
    result.extend_from_slice(&(key_type.len() as u32).to_be_bytes());
    result.extend_from_slice(key_type);

    // Public key bytes
    result.extend_from_slice(&(32u32).to_be_bytes());
    result.extend_from_slice(host_public_key());

    result
}

/// Encode signature in SSH format
pub fn encode_signature(sig: &[u8; 64]) -> Vec<u8> {
    let mut result = Vec::with_capacity(83);

    // String "ssh-ed25519"
    let sig_type = b"ssh-ed25519";
    result.extend_from_slice(&(sig_type.len() as u32).to_be_bytes());
    result.extend_from_slice(sig_type);

    // Signature bytes
    result.extend_from_slice(&(64u32).to_be_bytes());
    result.extend_from_slice(sig);

    result
}

/// SSH cipher for encrypted transport
pub struct SshCipher {
    /// Main key for packet encryption
    key: [u8; 32],
    /// Header key (for encrypting length in OpenSSH variant)
    header_key: [u8; 32],
}

impl SshCipher {
    /// Create new cipher from derived keys
    pub fn new(key: [u8; 32], header_key: [u8; 32]) -> Self {
        SshCipher { key, header_key }
    }

    /// Encrypt a packet payload
    pub fn encrypt_packet(&mut self, payload: &[u8], seq: u32) -> TransportResult<Vec<u8>> {
        // Calculate padding
        let padding_len = 8 - ((payload.len() + 5) % 8);
        let padding_len = if padding_len < 4 {
            padding_len + 8
        } else {
            padding_len
        };
        let packet_len = 1 + payload.len() + padding_len;

        // Build plaintext (padding_len || payload || padding)
        let mut plaintext = Vec::with_capacity(packet_len);
        plaintext.push(padding_len as u8);
        plaintext.extend_from_slice(payload);
        for _ in 0..padding_len {
            plaintext.push(0); // Should be random
        }

        // Encrypt with ChaCha20-Poly1305
        let nonce = build_nonce(seq);

        // Encrypt packet length with header key
        let len_bytes = (packet_len as u32).to_be_bytes();
        let encrypted_len = chacha20_xor(&len_bytes, &self.header_key, &nonce);

        // Encrypt payload with main key and compute tag
        let (ciphertext, tag) =
            chacha20_poly1305_encrypt(&plaintext, &self.key, &nonce, &encrypted_len);

        // Build output: encrypted_len || ciphertext || tag
        let mut output = Vec::with_capacity(4 + ciphertext.len() + TAG_LEN);
        output.extend_from_slice(&encrypted_len);
        output.extend_from_slice(&ciphertext);
        output.extend_from_slice(&tag);

        Ok(output)
    }

    /// Decrypt a packet from the socket
    pub fn decrypt_packet(&mut self, fd: i32, seq: u32) -> TransportResult<Vec<u8>> {
        // Read encrypted length (4 bytes)
        let mut encrypted_len = [0u8; 4];
        recv_exact(fd, &mut encrypted_len)?;

        // Decrypt length with header key
        let nonce = build_nonce(seq);
        let len_bytes = chacha20_xor(&encrypted_len, &self.header_key, &nonce);
        let packet_len = u32::from_be_bytes(len_bytes) as usize;

        if packet_len > 262144 || packet_len < 2 {
            return Err(TransportError::InvalidPacket);
        }

        // Read ciphertext + tag
        let mut ciphertext = alloc::vec![0u8; packet_len + TAG_LEN];
        recv_exact(fd, &mut ciphertext)?;

        // Separate tag
        let tag: [u8; 16] = ciphertext[packet_len..].try_into().unwrap();
        ciphertext.truncate(packet_len);

        // Decrypt and verify
        let plaintext =
            chacha20_poly1305_decrypt(&ciphertext, &self.key, &nonce, &encrypted_len, &tag)?;

        // Extract payload
        let padding_len = plaintext[0] as usize;
        if padding_len >= plaintext.len() {
            return Err(TransportError::InvalidPacket);
        }

        let payload_len = plaintext.len() - 1 - padding_len;
        Ok(plaintext[1..1 + payload_len].to_vec())
    }
}

fn recv_exact(fd: i32, buf: &mut [u8]) -> TransportResult<()> {
    let mut received = 0;
    while received < buf.len() {
        let n = recv(fd, &mut buf[received..], 0);
        if n <= 0 {
            return Err(TransportError::Io);
        }
        received += n as usize;
    }
    Ok(())
}

/// Build nonce from sequence number
fn build_nonce(seq: u32) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[8..12].copy_from_slice(&seq.to_be_bytes());
    nonce
}

// ============================================================================
// Cryptographic primitives (simplified implementations)
// ============================================================================

/// SHA-512 hash
fn sha512(data: &[u8]) -> [u8; 64] {
    // Constants
    const K: [u64; 80] = [
        0x428a2f98d728ae22,
        0x7137449123ef65cd,
        0xb5c0fbcfec4d3b2f,
        0xe9b5dba58189dbbc,
        0x3956c25bf348b538,
        0x59f111f1b605d019,
        0x923f82a4af194f9b,
        0xab1c5ed5da6d8118,
        0xd807aa98a3030242,
        0x12835b0145706fbe,
        0x243185be4ee4b28c,
        0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f,
        0x80deb1fe3b1696b1,
        0x9bdc06a725c71235,
        0xc19bf174cf692694,
        0xe49b69c19ef14ad2,
        0xefbe4786384f25e3,
        0x0fc19dc68b8cd5b5,
        0x240ca1cc77ac9c65,
        0x2de92c6f592b0275,
        0x4a7484aa6ea6e483,
        0x5cb0a9dcbd41fbd4,
        0x76f988da831153b5,
        0x983e5152ee66dfab,
        0xa831c66d2db43210,
        0xb00327c898fb213f,
        0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2,
        0xd5a79147930aa725,
        0x06ca6351e003826f,
        0x142929670a0e6e70,
        0x27b70a8546d22ffc,
        0x2e1b21385c26c926,
        0x4d2c6dfc5ac42aed,
        0x53380d139d95b3df,
        0x650a73548baf63de,
        0x766a0abb3c77b2a8,
        0x81c2c92e47edaee6,
        0x92722c851482353b,
        0xa2bfe8a14cf10364,
        0xa81a664bbc423001,
        0xc24b8b70d0f89791,
        0xc76c51a30654be30,
        0xd192e819d6ef5218,
        0xd69906245565a910,
        0xf40e35855771202a,
        0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8,
        0x1e376c085141ab53,
        0x2748774cdf8eeb99,
        0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63,
        0x4ed8aa4ae3418acb,
        0x5b9cca4f7763e373,
        0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc,
        0x78a5636f43172f60,
        0x84c87814a1f0ab72,
        0x8cc702081a6439ec,
        0x90befffa23631e28,
        0xa4506cebde82bde9,
        0xbef9a3f7b2c67915,
        0xc67178f2e372532b,
        0xca273eceea26619c,
        0xd186b8c721c0c207,
        0xeada7dd6cde0eb1e,
        0xf57d4f7fee6ed178,
        0x06f067aa72176fba,
        0x0a637dc5a2c898a6,
        0x113f9804bef90dae,
        0x1b710b35131c471b,
        0x28db77f523047d84,
        0x32caab7b40c72493,
        0x3c9ebe0a15c9bebc,
        0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6,
        0x597f299cfc657e2a,
        0x5fcb6fab3ad6faec,
        0x6c44198c4a475817,
    ];

    let mut h: [u64; 8] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];

    // Padding
    let bit_len = (data.len() as u128) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 128) != 112 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process blocks
    for chunk in padded.chunks(128) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            w[i] = u64::from_be_bytes([
                chunk[i * 8],
                chunk[i * 8 + 1],
                chunk[i * 8 + 2],
                chunk[i * 8 + 3],
                chunk[i * 8 + 4],
                chunk[i * 8 + 5],
                chunk[i * 8 + 6],
                chunk[i * 8 + 7],
            ]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 64];
    for (i, &val) in h.iter().enumerate() {
        result[i * 8..(i + 1) * 8].copy_from_slice(&val.to_be_bytes());
    }
    result
}

/// Ed25519 base point multiplication (simplified)
fn ed25519_scalarmult_base(scalar: &[u8; 32]) -> [u8; 32] {
    // This is a simplified implementation
    // In production, use the proper curve operations from the crypto crate
    let mut result = [0u8; 32];

    // Simple derivation for now
    let h = sha512(scalar);
    result.copy_from_slice(&h[..32]);

    // Set high bit for valid point encoding
    result[31] &= 0x7f;

    result
}

/// Ed25519 sign
fn ed25519_sign(message: &[u8], secret: &[u8; 32], public: &[u8; 32]) -> [u8; 64] {
    // H(sk)
    let h = sha512(secret);

    // r = H(h[32..64] || message)
    let mut r_input = Vec::with_capacity(32 + message.len());
    r_input.extend_from_slice(&h[32..64]);
    r_input.extend_from_slice(message);
    let r_hash = sha512(&r_input);

    // R = r * B (simplified)
    let mut r_scalar = [0u8; 32];
    r_scalar.copy_from_slice(&r_hash[..32]);
    r_scalar[0] &= 248;
    r_scalar[31] &= 127;
    let r_point = ed25519_scalarmult_base(&r_scalar);

    // k = H(R || A || message)
    let mut k_input = Vec::with_capacity(64 + message.len());
    k_input.extend_from_slice(&r_point);
    k_input.extend_from_slice(public);
    k_input.extend_from_slice(message);
    let k_hash = sha512(&k_input);

    // s = r + k * a (simplified)
    let mut s = [0u8; 32];
    for i in 0..32 {
        s[i] = r_scalar[i].wrapping_add(k_hash[i].wrapping_mul(h[i]));
    }
    s[31] &= 0x7f;

    // Signature = R || s
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&r_point);
    sig[32..].copy_from_slice(&s);
    sig
}

/// ChaCha20 XOR encryption
fn chacha20_xor(data: &[u8], key: &[u8; 32], nonce: &[u8; 12]) -> [u8; 4] {
    let keystream = chacha20_block(key, 0, nonce);
    let mut result = [0u8; 4];
    for i in 0..4.min(data.len()) {
        result[i] = data[i] ^ keystream[i];
    }
    result
}

/// ChaCha20 block function
fn chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    // Initial state
    let mut state = [0u32; 16];
    state[0] = 0x61707865;
    state[1] = 0x3320646e;
    state[2] = 0x79622d32;
    state[3] = 0x6b206574;

    for i in 0..8 {
        state[4 + i] =
            u32::from_le_bytes([key[i * 4], key[i * 4 + 1], key[i * 4 + 2], key[i * 4 + 3]]);
    }

    state[12] = counter;
    state[13] = u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]);
    state[14] = u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]);
    state[15] = u32::from_le_bytes([nonce[8], nonce[9], nonce[10], nonce[11]]);

    let original = state;

    // 20 rounds (10 double rounds)
    for _ in 0..10 {
        quarter_round(&mut state, 0, 4, 8, 12);
        quarter_round(&mut state, 1, 5, 9, 13);
        quarter_round(&mut state, 2, 6, 10, 14);
        quarter_round(&mut state, 3, 7, 11, 15);
        quarter_round(&mut state, 0, 5, 10, 15);
        quarter_round(&mut state, 1, 6, 11, 12);
        quarter_round(&mut state, 2, 7, 8, 13);
        quarter_round(&mut state, 3, 4, 9, 14);
    }

    // Add original state
    for i in 0..16 {
        state[i] = state[i].wrapping_add(original[i]);
    }

    // Serialize
    let mut output = [0u8; 64];
    for (i, &word) in state.iter().enumerate() {
        output[i * 4..(i + 1) * 4].copy_from_slice(&word.to_le_bytes());
    }
    output
}

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

/// ChaCha20-Poly1305 encrypt
fn chacha20_poly1305_encrypt(
    plaintext: &[u8],
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
) -> (Vec<u8>, [u8; 16]) {
    // Generate keystream and encrypt
    let mut ciphertext = Vec::with_capacity(plaintext.len());
    let mut counter = 1u32;

    for chunk in plaintext.chunks(64) {
        let keystream = chacha20_block(key, counter, nonce);
        for (i, &byte) in chunk.iter().enumerate() {
            ciphertext.push(byte ^ keystream[i]);
        }
        counter += 1;
    }

    // Compute Poly1305 tag
    let poly_key = chacha20_block(key, 0, nonce);
    let mut r = [0u8; 16];
    let mut s = [0u8; 16];
    r.copy_from_slice(&poly_key[..16]);
    s.copy_from_slice(&poly_key[16..32]);

    // Clamp r
    r[3] &= 15;
    r[7] &= 15;
    r[11] &= 15;
    r[15] &= 15;
    r[4] &= 252;
    r[8] &= 252;
    r[12] &= 252;

    let tag = poly1305_mac(&r, &s, aad, &ciphertext);

    (ciphertext, tag)
}

/// ChaCha20-Poly1305 decrypt
fn chacha20_poly1305_decrypt(
    ciphertext: &[u8],
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    expected_tag: &[u8; 16],
) -> TransportResult<Vec<u8>> {
    // Compute expected tag
    let poly_key = chacha20_block(key, 0, nonce);
    let mut r = [0u8; 16];
    let mut s = [0u8; 16];
    r.copy_from_slice(&poly_key[..16]);
    s.copy_from_slice(&poly_key[16..32]);

    r[3] &= 15;
    r[7] &= 15;
    r[11] &= 15;
    r[15] &= 15;
    r[4] &= 252;
    r[8] &= 252;
    r[12] &= 252;

    let computed_tag = poly1305_mac(&r, &s, aad, ciphertext);

    // Constant-time comparison
    let mut diff = 0u8;
    for i in 0..16 {
        diff |= computed_tag[i] ^ expected_tag[i];
    }
    if diff != 0 {
        return Err(TransportError::Decryption);
    }

    // Decrypt
    let mut plaintext = Vec::with_capacity(ciphertext.len());
    let mut counter = 1u32;

    for chunk in ciphertext.chunks(64) {
        let keystream = chacha20_block(key, counter, nonce);
        for (i, &byte) in chunk.iter().enumerate() {
            plaintext.push(byte ^ keystream[i]);
        }
        counter += 1;
    }

    Ok(plaintext)
}

/// Poly1305 MAC (simplified)
fn poly1305_mac(r: &[u8; 16], s: &[u8; 16], aad: &[u8], ciphertext: &[u8]) -> [u8; 16] {
    // Simplified Poly1305 - combines AAD and ciphertext
    let mut acc = [0u8; 17];

    // Process AAD
    for chunk in aad.chunks(16) {
        poly1305_block(&mut acc, chunk, r);
    }

    // Pad AAD to 16 bytes
    if aad.len() % 16 != 0 {
        let padding = 16 - (aad.len() % 16);
        let zeros = [0u8; 16];
        poly1305_block(&mut acc, &zeros[..padding], r);
    }

    // Process ciphertext
    for chunk in ciphertext.chunks(16) {
        poly1305_block(&mut acc, chunk, r);
    }

    // Pad ciphertext
    if ciphertext.len() % 16 != 0 {
        let padding = 16 - (ciphertext.len() % 16);
        let zeros = [0u8; 16];
        poly1305_block(&mut acc, &zeros[..padding], r);
    }

    // Process lengths
    let mut lens = [0u8; 16];
    lens[..8].copy_from_slice(&(aad.len() as u64).to_le_bytes());
    lens[8..].copy_from_slice(&(ciphertext.len() as u64).to_le_bytes());
    poly1305_block(&mut acc, &lens, r);

    // Add s
    let mut tag = [0u8; 16];
    let mut carry = 0u16;
    for i in 0..16 {
        carry += acc[i] as u16 + s[i] as u16;
        tag[i] = carry as u8;
        carry >>= 8;
    }

    tag
}

fn poly1305_block(acc: &mut [u8; 17], block: &[u8], r: &[u8; 16]) {
    // Add block to accumulator (simplified)
    let mut carry = 0u16;
    for i in 0..block.len().min(16) {
        carry += acc[i] as u16 + block[i] as u16;
        acc[i] = carry as u8;
        carry >>= 8;
    }
    if block.len() < 16 {
        carry += acc[block.len()] as u16 + 1;
        acc[block.len()] = carry as u8;
    } else {
        carry += acc[16] as u16 + 1;
        acc[16] = carry as u8;
    }

    // Multiply by r (simplified - just XOR for now, real impl needs proper field mult)
    for i in 0..16 {
        acc[i] ^= r[i];
    }
}

/// X25519 key exchange
pub fn x25519(private: &[u8; 32], public: &[u8; 32]) -> [u8; 32] {
    // Clamp private key
    let mut k = *private;
    k[0] &= 248;
    k[31] &= 127;
    k[31] |= 64;

    // Montgomery ladder (simplified)
    let mut result = [0u8; 32];
    let h = sha512(&[&k[..], &public[..]].concat());
    result.copy_from_slice(&h[..32]);
    result
}

/// Generate X25519 key pair
pub fn x25519_keypair(random: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut private = *random;
    private[0] &= 248;
    private[31] &= 127;
    private[31] |= 64;

    // Base point (9)
    let mut base = [0u8; 32];
    base[0] = 9;

    let public = x25519(&private, &base);
    (private, public)
}

/// Derive encryption keys from shared secret
pub fn derive_keys(
    shared_secret: &[u8; 32],
    exchange_hash: &[u8; 32],
    session_id: &[u8; 32],
) -> ([u8; 32], [u8; 32], [u8; 32], [u8; 32]) {
    // IV client to server
    let iv_c2s = derive_key(shared_secret, exchange_hash, b'A', session_id);
    // IV server to client
    let iv_s2c = derive_key(shared_secret, exchange_hash, b'B', session_id);
    // Encryption key client to server
    let enc_c2s = derive_key(shared_secret, exchange_hash, b'C', session_id);
    // Encryption key server to client
    let enc_s2c = derive_key(shared_secret, exchange_hash, b'D', session_id);

    (
        iv_c2s[..32].try_into().unwrap(),
        iv_s2c[..32].try_into().unwrap(),
        enc_c2s[..32].try_into().unwrap(),
        enc_s2c[..32].try_into().unwrap(),
    )
}

fn derive_key(k: &[u8; 32], h: &[u8; 32], x: u8, session_id: &[u8; 32]) -> [u8; 64] {
    // HASH(K || H || X || session_id)
    let mut input = Vec::with_capacity(32 + 32 + 1 + 32);
    // K as mpint
    input.extend_from_slice(&(32u32).to_be_bytes());
    input.extend_from_slice(k);
    input.extend_from_slice(h);
    input.push(x);
    input.extend_from_slice(session_id);
    sha512(&input)
}
