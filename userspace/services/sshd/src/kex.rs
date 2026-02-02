//! SSH Key Exchange (RFC 4253, RFC 8731)
//!
//! Implements curve25519-sha256 key exchange.

use alloc::vec::Vec;
use libc::*;

use crate::crypto::{
    SshCipher, derive_keys, encode_host_public_key, encode_signature, sign_with_host_key, x25519,
    x25519_keypair,
};
use crate::transport::{
    SshTransport, TransportError, TransportResult, decode_string, encode_name_list, encode_string,
    msg,
};

/// Key exchange state
pub struct KexState {
    /// Our KEXINIT payload
    pub our_kexinit: Vec<u8>,
    /// Client's KEXINIT payload
    pub client_kexinit: Vec<u8>,
    /// Our ephemeral X25519 private key
    pub our_private: [u8; 32],
    /// Our ephemeral X25519 public key
    pub our_public: [u8; 32],
    /// Client's ephemeral public key
    pub client_public: [u8; 32],
    /// Shared secret K
    pub shared_secret: [u8; 32],
    /// Exchange hash H
    pub exchange_hash: [u8; 32],
}

impl KexState {
    pub fn new() -> Self {
        KexState {
            our_kexinit: Vec::new(),
            client_kexinit: Vec::new(),
            our_private: [0; 32],
            our_public: [0; 32],
            client_public: [0; 32],
            shared_secret: [0; 32],
            exchange_hash: [0; 32],
        }
    }
}

/// Algorithms we support
const KEX_ALGORITHMS: &[&str] = &["curve25519-sha256", "curve25519-sha256@libssh.org"];
const HOST_KEY_ALGORITHMS: &[&str] = &["ssh-ed25519"];
const ENCRYPTION_ALGORITHMS: &[&str] = &["chacha20-poly1305@openssh.com"];
const MAC_ALGORITHMS: &[&str] = &["none"]; // Implicit with AEAD
const COMPRESSION_ALGORITHMS: &[&str] = &["none"];

/// Perform key exchange
pub fn perform_key_exchange(transport: &mut SshTransport) -> TransportResult<()> {
    // Generate and send our KEXINIT
    let our_kexinit = build_kexinit()?;
    transport.kex().our_kexinit = our_kexinit.clone();
    transport.send_packet(&our_kexinit)?;

    // Receive client's KEXINIT
    let client_kexinit = transport.recv_packet()?;
    if client_kexinit.is_empty() || client_kexinit[0] != msg::KEXINIT {
        return Err(TransportError::Protocol);
    }
    transport.kex().client_kexinit = client_kexinit.clone();

    // Parse client's algorithm preferences (just validate it's a KEXINIT)
    parse_kexinit(&client_kexinit)?;

    // Receive ECDH_INIT (client's ephemeral public key)
    let ecdh_init = transport.recv_packet()?;
    if ecdh_init.is_empty() || ecdh_init[0] != msg::KEX_ECDH_INIT {
        return Err(TransportError::Protocol);
    }

    // Parse client's public key
    let mut offset = 1;
    let client_public_vec = decode_string(&ecdh_init, &mut offset)?;
    if client_public_vec.len() != 32 {
        return Err(TransportError::Protocol);
    }
    let mut client_public = [0u8; 32];
    client_public.copy_from_slice(&client_public_vec);
    transport.kex().client_public = client_public;

    // Generate our ephemeral key pair
    let random = generate_random_32();
    let (our_private, our_public) = x25519_keypair(&random);
    transport.kex().our_private = our_private;
    transport.kex().our_public = our_public;

    // Compute shared secret
    let shared_secret = x25519(&our_private, &client_public);
    transport.kex().shared_secret = shared_secret;

    // Compute exchange hash H
    let exchange_hash = compute_exchange_hash(transport)?;
    transport.kex().exchange_hash = exchange_hash;

    // Set session ID (first exchange hash)
    transport.set_session_id(exchange_hash);

    // Sign the exchange hash with our host key
    let signature = sign_with_host_key(&exchange_hash);
    let encoded_signature = encode_signature(&signature);

    // Build ECDH_REPLY
    let host_key = encode_host_public_key();
    let mut reply = Vec::new();
    reply.push(msg::KEX_ECDH_REPLY);

    // K_S (host key blob)
    reply.extend_from_slice(&(host_key.len() as u32).to_be_bytes());
    reply.extend_from_slice(&host_key);

    // Q_S (server ephemeral public key)
    reply.extend_from_slice(&(32u32).to_be_bytes());
    reply.extend_from_slice(&our_public);

    // Signature of H
    reply.extend_from_slice(&(encoded_signature.len() as u32).to_be_bytes());
    reply.extend_from_slice(&encoded_signature);

    transport.send_packet(&reply)?;

    // Send NEWKEYS
    transport.send_packet(&[msg::NEWKEYS])?;

    // Receive NEWKEYS
    let newkeys = transport.recv_packet()?;
    if newkeys.is_empty() || newkeys[0] != msg::NEWKEYS {
        return Err(TransportError::Protocol);
    }

    // Derive encryption keys
    let session_id = transport.session_id().ok_or(TransportError::Protocol)?;
    let (iv_c2s, iv_s2c, enc_c2s, enc_s2c) =
        derive_keys(&shared_secret, &exchange_hash, session_id);

    // Enable encryption
    let send_cipher = SshCipher::new(enc_s2c, iv_s2c);
    let recv_cipher = SshCipher::new(enc_c2s, iv_c2s);
    transport.enable_encryption(send_cipher, recv_cipher);

    Ok(())
}

/// Build KEXINIT message
fn build_kexinit() -> TransportResult<Vec<u8>> {
    let mut msg = Vec::with_capacity(256);

    // Message type
    msg.push(msg::KEXINIT);

    // Cookie (16 random bytes)
    let cookie = generate_random_16();
    msg.extend_from_slice(&cookie);

    // Algorithm lists
    msg.extend_from_slice(&encode_name_list(KEX_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(HOST_KEY_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(ENCRYPTION_ALGORITHMS)); // enc c2s
    msg.extend_from_slice(&encode_name_list(ENCRYPTION_ALGORITHMS)); // enc s2c
    msg.extend_from_slice(&encode_name_list(MAC_ALGORITHMS)); // mac c2s
    msg.extend_from_slice(&encode_name_list(MAC_ALGORITHMS)); // mac s2c
    msg.extend_from_slice(&encode_name_list(COMPRESSION_ALGORITHMS)); // comp c2s
    msg.extend_from_slice(&encode_name_list(COMPRESSION_ALGORITHMS)); // comp s2c
    msg.extend_from_slice(&encode_name_list(&[])); // languages c2s
    msg.extend_from_slice(&encode_name_list(&[])); // languages s2c

    // first_kex_packet_follows
    msg.push(0);

    // reserved (uint32)
    msg.extend_from_slice(&0u32.to_be_bytes());

    Ok(msg)
}

/// Parse KEXINIT message (basic validation)
fn parse_kexinit(data: &[u8]) -> TransportResult<()> {
    if data.len() < 17 {
        return Err(TransportError::InvalidPacket);
    }
    // Skip msg type and cookie
    let mut offset = 17;

    // Parse algorithm lists (just skip for now)
    for _ in 0..10 {
        let _ = decode_string(data, &mut offset)?;
    }

    // first_kex_packet_follows
    if offset >= data.len() {
        return Err(TransportError::InvalidPacket);
    }

    Ok(())
}

/// Compute exchange hash H
fn compute_exchange_hash(transport: &SshTransport) -> TransportResult<[u8; 32]> {
    // H = SHA256(V_C || V_S || I_C || I_S || K_S || Q_C || Q_S || K)
    // Where:
    // V_C = client version string
    // V_S = server version string
    // I_C = client KEXINIT payload
    // I_S = server KEXINIT payload
    // K_S = host key blob
    // Q_C = client ephemeral public key
    // Q_S = server ephemeral public key
    // K = shared secret (as mpint)

    let kex = transport.kex_ref();
    let mut hash_input = Vec::with_capacity(512);

    // V_C (client version string)
    hash_input.extend_from_slice(&encode_string(transport.client_version()));

    // V_S (server version string)
    hash_input.extend_from_slice(&encode_string(transport.server_version()));

    // I_C (client KEXINIT - without packet overhead)
    hash_input.extend_from_slice(&encode_string(&kex.client_kexinit));

    // I_S (server KEXINIT - without packet overhead)
    hash_input.extend_from_slice(&encode_string(&kex.our_kexinit));

    // K_S (host key blob)
    let host_key = encode_host_public_key();
    hash_input.extend_from_slice(&encode_string(&host_key));

    // Q_C (client ephemeral public key)
    hash_input.extend_from_slice(&encode_string(&kex.client_public));

    // Q_S (server ephemeral public key)
    hash_input.extend_from_slice(&encode_string(&kex.our_public));

    // K (shared secret as mpint)
    hash_input.extend_from_slice(&encode_mpint(&kex.shared_secret));

    // Compute SHA-256 hash
    let hash = sha256(&hash_input);
    Ok(hash)
}

/// Encode as SSH mpint (signed, big-endian)
fn encode_mpint(data: &[u8; 32]) -> Vec<u8> {
    let mut result = Vec::with_capacity(37);

    // Skip leading zeros
    let mut start = 0;
    while start < 32 && data[start] == 0 {
        start += 1;
    }

    if start == 32 {
        // Zero value
        result.extend_from_slice(&0u32.to_be_bytes());
    } else {
        // Check if high bit is set (would indicate negative)
        let needs_pad = (data[start] & 0x80) != 0;
        let len = 32 - start + if needs_pad { 1 } else { 0 };

        result.extend_from_slice(&(len as u32).to_be_bytes());
        if needs_pad {
            result.push(0);
        }
        result.extend_from_slice(&data[start..]);
    }

    result
}

/// SHA-256 hash
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Padding
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process blocks
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
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

    let mut result = [0u8; 32];
    for (i, &val) in h.iter().enumerate() {
        result[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

/// Generate 16 random bytes
fn generate_random_16() -> [u8; 16] {
    let mut buf = [0u8; 16];
    let fd = open2("/dev/random", O_RDONLY);
    if fd >= 0 {
        let _ = read(fd, &mut buf);
        close(fd);
    } else {
        // Fallback: simple PRNG
        let seed = getpid() as u64;
        for i in 0..16 {
            buf[i] = ((seed >> (i % 8 * 8)) ^ (i as u64 * 13)) as u8;
        }
    }
    buf
}

/// Generate 32 random bytes
fn generate_random_32() -> [u8; 32] {
    let mut buf = [0u8; 32];
    let fd = open2("/dev/random", O_RDONLY);
    if fd >= 0 {
        let _ = read(fd, &mut buf);
        close(fd);
    } else {
        // Fallback
        let seed = getpid() as u64;
        for i in 0..32 {
            buf[i] = ((seed >> (i % 8 * 8)) ^ (i as u64 * 17)) as u8;
        }
    }
    buf
}
