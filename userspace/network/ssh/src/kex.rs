//! SSH Key Exchange (RFC 4253, RFC 8731)
//!
//! Implements curve25519-sha256 key exchange.

use alloc::vec::Vec;

use crate::crypto::{
    SshCipher, derive_keys, ed25519_verify, generate_random_16, generate_random_32, sha256, x25519,
    x25519_keypair,
};
use crate::transport::{
    Result, SshTransport, TransportError, decode_string, encode_name_list, encode_string, msg,
};

/// Key exchange state
pub struct KexState {
    pub our_kexinit: Vec<u8>,
    pub server_kexinit: Vec<u8>,
    pub our_private: [u8; 32],
    pub our_public: [u8; 32],
    pub server_public: [u8; 32],
    pub server_host_key: [u8; 32],
    pub shared_secret: [u8; 32],
    pub exchange_hash: [u8; 32],
}

impl KexState {
    pub fn new() -> Self {
        KexState {
            our_kexinit: Vec::new(),
            server_kexinit: Vec::new(),
            our_private: [0; 32],
            our_public: [0; 32],
            server_public: [0; 32],
            server_host_key: [0; 32],
            shared_secret: [0; 32],
            exchange_hash: [0; 32],
        }
    }
}

impl Default for KexState {
    fn default() -> Self {
        Self::new()
    }
}

// Supported algorithms
const KEX_ALGORITHMS: &[&str] = &["curve25519-sha256", "curve25519-sha256@libssh.org"];
const HOST_KEY_ALGORITHMS: &[&str] = &["ssh-ed25519"];
const ENCRYPTION_ALGORITHMS: &[&str] = &["chacha20-poly1305@openssh.com"];
const MAC_ALGORITHMS: &[&str] = &["none"];
const COMPRESSION_ALGORITHMS: &[&str] = &["none"];

/// Perform key exchange
pub fn perform_key_exchange(transport: &mut SshTransport) -> Result<()> {
    // Send our KEXINIT
    let our_kexinit = build_kexinit()?;
    transport.kex().our_kexinit = our_kexinit.clone();
    transport.send_packet(&our_kexinit)?;

    // Receive server's KEXINIT
    let server_kexinit = transport.recv_packet()?;
    if server_kexinit.is_empty() || server_kexinit[0] != msg::KEXINIT {
        return Err(TransportError::Protocol);
    }
    transport.kex().server_kexinit = server_kexinit.clone();

    // Validate KEXINIT
    parse_kexinit(&server_kexinit)?;

    // Generate ephemeral key pair
    let random = generate_random_32();
    let (our_private, our_public) = x25519_keypair(&random);
    transport.kex().our_private = our_private;
    transport.kex().our_public = our_public;

    // Send ECDH_INIT
    let mut ecdh_init = Vec::with_capacity(37);
    ecdh_init.push(msg::KEX_ECDH_INIT);
    ecdh_init.extend_from_slice(&(32u32).to_be_bytes());
    ecdh_init.extend_from_slice(&our_public);
    transport.send_packet(&ecdh_init)?;

    // Receive ECDH_REPLY
    let ecdh_reply = transport.recv_packet()?;
    if ecdh_reply.is_empty() || ecdh_reply[0] != msg::KEX_ECDH_REPLY {
        return Err(TransportError::Protocol);
    }

    // Parse ECDH_REPLY
    let mut offset = 1;

    // K_S (host key blob)
    let host_key_blob = decode_string(&ecdh_reply, &mut offset)?;
    let server_host_key = parse_host_key(&host_key_blob)?;
    transport.kex().server_host_key = server_host_key;

    // Q_S (server ephemeral public key)
    let server_public_vec = decode_string(&ecdh_reply, &mut offset)?;
    if server_public_vec.len() != 32 {
        return Err(TransportError::Protocol);
    }
    let mut server_public = [0u8; 32];
    server_public.copy_from_slice(&server_public_vec);
    transport.kex().server_public = server_public;

    // Signature
    let signature_blob = decode_string(&ecdh_reply, &mut offset)?;
    let signature = parse_signature(&signature_blob)?;

    // Compute shared secret
    let shared_secret = x25519(&our_private, &server_public);
    transport.kex().shared_secret = shared_secret;

    // Compute exchange hash
    let exchange_hash = compute_exchange_hash(transport, &host_key_blob)?;
    transport.kex().exchange_hash = exchange_hash;

    // Set session ID
    transport.set_session_id(exchange_hash);

    // Verify signature
    if !ed25519_verify(&exchange_hash, &signature, &server_host_key) {
        return Err(TransportError::HostKeyVerification);
    }

    // Receive NEWKEYS
    let newkeys = transport.recv_packet()?;
    if newkeys.is_empty() || newkeys[0] != msg::NEWKEYS {
        return Err(TransportError::Protocol);
    }

    // Send NEWKEYS
    transport.send_packet(&[msg::NEWKEYS])?;

    // Derive and enable encryption
    let session_id = transport.session_id().ok_or(TransportError::Protocol)?;
    let (iv_c2s, iv_s2c, enc_c2s, enc_s2c) =
        derive_keys(&shared_secret, &exchange_hash, session_id);

    let send_cipher = SshCipher::new(enc_c2s, iv_c2s);
    let recv_cipher = SshCipher::new(enc_s2c, iv_s2c);
    transport.enable_encryption(send_cipher, recv_cipher);

    Ok(())
}

/// Build KEXINIT message
fn build_kexinit() -> Result<Vec<u8>> {
    let mut msg = Vec::with_capacity(256);

    msg.push(msg::KEXINIT);
    msg.extend_from_slice(&generate_random_16());
    msg.extend_from_slice(&encode_name_list(KEX_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(HOST_KEY_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(ENCRYPTION_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(ENCRYPTION_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(MAC_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(MAC_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(COMPRESSION_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(COMPRESSION_ALGORITHMS));
    msg.extend_from_slice(&encode_name_list(&[]));
    msg.extend_from_slice(&encode_name_list(&[]));
    msg.push(0); // first_kex_packet_follows
    msg.extend_from_slice(&0u32.to_be_bytes());

    Ok(msg)
}

/// Parse KEXINIT message
fn parse_kexinit(data: &[u8]) -> Result<()> {
    if data.len() < 17 {
        return Err(TransportError::InvalidPacket);
    }
    let mut offset = 17; // Skip msg type and cookie

    for _ in 0..10 {
        let _ = decode_string(data, &mut offset)?;
    }

    if offset >= data.len() {
        return Err(TransportError::InvalidPacket);
    }

    Ok(())
}

/// Parse host key blob
fn parse_host_key(blob: &[u8]) -> Result<[u8; 32]> {
    let mut offset = 0;

    let key_type = decode_string(blob, &mut offset)?;
    if key_type != b"ssh-ed25519" {
        return Err(TransportError::Protocol);
    }

    let public_key = decode_string(blob, &mut offset)?;
    if public_key.len() != 32 {
        return Err(TransportError::Protocol);
    }

    let mut result = [0u8; 32];
    result.copy_from_slice(&public_key);
    Ok(result)
}

/// Parse signature blob
fn parse_signature(blob: &[u8]) -> Result<[u8; 64]> {
    let mut offset = 0;

    let sig_type = decode_string(blob, &mut offset)?;
    if sig_type != b"ssh-ed25519" {
        return Err(TransportError::Protocol);
    }

    let sig_bytes = decode_string(blob, &mut offset)?;
    if sig_bytes.len() != 64 {
        return Err(TransportError::Protocol);
    }

    let mut result = [0u8; 64];
    result.copy_from_slice(&sig_bytes);
    Ok(result)
}

/// Compute exchange hash
fn compute_exchange_hash(transport: &SshTransport, host_key_blob: &[u8]) -> Result<[u8; 32]> {
    let kex = transport.kex_ref();
    let mut hash_input = Vec::with_capacity(512);

    hash_input.extend_from_slice(&encode_string(transport.client_version()));
    hash_input.extend_from_slice(&encode_string(transport.server_version()));
    hash_input.extend_from_slice(&encode_string(&kex.our_kexinit));
    hash_input.extend_from_slice(&encode_string(&kex.server_kexinit));
    hash_input.extend_from_slice(&encode_string(host_key_blob));
    hash_input.extend_from_slice(&encode_string(&kex.our_public));
    hash_input.extend_from_slice(&encode_string(&kex.server_public));
    hash_input.extend_from_slice(&encode_mpint(&kex.shared_secret));

    Ok(sha256(&hash_input))
}

/// Encode as SSH mpint
fn encode_mpint(data: &[u8; 32]) -> Vec<u8> {
    let mut result = Vec::with_capacity(37);

    let mut start = 0;
    while start < 32 && data[start] == 0 {
        start += 1;
    }

    if start == 32 {
        result.extend_from_slice(&0u32.to_be_bytes());
    } else {
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
