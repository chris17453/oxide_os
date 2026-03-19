//! TLS 1.3 Extensions (RFC 8446 Section 4.2)
//!
//! — ColdCipher: Extensions are how TLS 1.3 negotiates everything that
//! matters: which version, which key exchange, which signatures, and
//! most critically — the Server Name Indication (SNI) that tells the
//! server which certificate to present. Without SNI, virtual hosting
//! breaks and you get the wrong cert. — ColdCipher

extern crate alloc;
use alloc::vec::Vec;

// Extension type codes
pub const EXT_SERVER_NAME: u16 = 0;
pub const EXT_SUPPORTED_GROUPS: u16 = 10;
pub const EXT_SIGNATURE_ALGORITHMS: u16 = 13;
pub const EXT_SUPPORTED_VERSIONS: u16 = 43;
pub const EXT_KEY_SHARE: u16 = 51;

// Named groups
pub const GROUP_SECP256R1: u16 = 0x0017; // P-256
pub const GROUP_SECP384R1: u16 = 0x0018; // P-384
pub const GROUP_X25519: u16 = 0x001D;

// Signature algorithms
pub const SIG_ECDSA_SECP256R1_SHA256: u16 = 0x0403;
pub const SIG_RSA_PSS_RSAE_SHA256: u16 = 0x0804;
pub const SIG_RSA_PKCS1_SHA256: u16 = 0x0401;
pub const SIG_ECDSA_SECP384R1_SHA384: u16 = 0x0503;
pub const SIG_RSA_PSS_RSAE_SHA384: u16 = 0x0805;
pub const SIG_RSA_PKCS1_SHA384: u16 = 0x0501;
pub const SIG_ED25519: u16 = 0x0807;

/// Build SNI (Server Name Indication) extension
/// — ColdCipher: The most important extension. Without it, CDNs serve
/// the wrong certificate and the handshake fails. — ColdCipher
pub fn build_sni(hostname: &str) -> Vec<u8> {
    let name = hostname.as_bytes();
    let name_len = name.len();

    // ServerNameList: list_length(2) + name_type(1) + name_length(2) + name
    let list_len = 1 + 2 + name_len;
    let ext_data_len = 2 + list_len;

    let mut ext = Vec::with_capacity(4 + ext_data_len);
    // Extension header
    ext.push((EXT_SERVER_NAME >> 8) as u8);
    ext.push((EXT_SERVER_NAME & 0xFF) as u8);
    ext.push((ext_data_len >> 8) as u8);
    ext.push((ext_data_len & 0xFF) as u8);
    // ServerNameList length
    ext.push((list_len >> 8) as u8);
    ext.push((list_len & 0xFF) as u8);
    // HostName type = 0
    ext.push(0);
    // HostName length
    ext.push((name_len >> 8) as u8);
    ext.push((name_len & 0xFF) as u8);
    // HostName
    ext.extend_from_slice(name);
    ext
}

/// Build supported_versions extension (client)
/// — ColdCipher: We only speak TLS 1.3. No fallback. — ColdCipher
pub fn build_supported_versions() -> Vec<u8> {
    let mut ext = Vec::with_capacity(7);
    ext.push((EXT_SUPPORTED_VERSIONS >> 8) as u8);
    ext.push((EXT_SUPPORTED_VERSIONS & 0xFF) as u8);
    ext.push(0); ext.push(3); // Extension data length = 3
    ext.push(2); // List length = 2 (one version)
    ext.push(0x03); ext.push(0x04); // TLS 1.3
    ext
}

/// Build supported_groups extension — offer multiple groups like real clients
pub fn build_supported_groups() -> Vec<u8> {
    let groups = [GROUP_X25519, GROUP_SECP256R1, GROUP_SECP384R1];
    let list_len = groups.len() * 2;
    let ext_len = 2 + list_len;

    let mut ext = Vec::with_capacity(4 + ext_len);
    ext.push((EXT_SUPPORTED_GROUPS >> 8) as u8);
    ext.push((EXT_SUPPORTED_GROUPS & 0xFF) as u8);
    ext.push((ext_len >> 8) as u8);
    ext.push((ext_len & 0xFF) as u8);
    ext.push((list_len >> 8) as u8);
    ext.push((list_len & 0xFF) as u8);
    for &g in &groups {
        ext.push((g >> 8) as u8);
        ext.push((g & 0xFF) as u8);
    }
    ext
}

/// Build signature_algorithms extension
pub fn build_signature_algorithms() -> Vec<u8> {
    let algs = [
        SIG_ECDSA_SECP256R1_SHA256,
        SIG_RSA_PSS_RSAE_SHA256,
        SIG_RSA_PKCS1_SHA256,
        SIG_ECDSA_SECP384R1_SHA384,
        SIG_RSA_PSS_RSAE_SHA384,
        SIG_ED25519,
    ];
    let list_len = algs.len() * 2;
    let ext_len = 2 + list_len;

    let mut ext = Vec::with_capacity(4 + ext_len);
    ext.push((EXT_SIGNATURE_ALGORITHMS >> 8) as u8);
    ext.push((EXT_SIGNATURE_ALGORITHMS & 0xFF) as u8);
    ext.push((ext_len >> 8) as u8);
    ext.push((ext_len & 0xFF) as u8);
    ext.push((list_len >> 8) as u8);
    ext.push((list_len & 0xFF) as u8);
    for &alg in &algs {
        ext.push((alg >> 8) as u8);
        ext.push((alg & 0xFF) as u8);
    }
    ext
}

/// Build key_share extension with X25519 public key
pub fn build_key_share(pubkey: &[u8; 32]) -> Vec<u8> {
    // KeyShareEntry: group(2) + key_exchange_length(2) + key_exchange(32)
    let entry_len = 2 + 2 + 32;
    let ext_len = 2 + entry_len; // client_shares length(2) + entry

    let mut ext = Vec::with_capacity(4 + ext_len);
    ext.push((EXT_KEY_SHARE >> 8) as u8);
    ext.push((EXT_KEY_SHARE & 0xFF) as u8);
    ext.push((ext_len >> 8) as u8);
    ext.push((ext_len & 0xFF) as u8);
    // client_shares length
    ext.push((entry_len >> 8) as u8);
    ext.push((entry_len & 0xFF) as u8);
    // KeyShareEntry
    ext.push((GROUP_X25519 >> 8) as u8);
    ext.push((GROUP_X25519 & 0xFF) as u8);
    ext.push(0); ext.push(32); // key_exchange length
    ext.extend_from_slice(pubkey);
    ext
}

/// Build key_share extension with dual key shares: X25519 + P-256.
/// — ColdCipher: "Two key shares, one extension. The server picks its favorite.
///   Most will choose X25519 (smaller, faster). Some old-school servers pick P-256.
///   Either way, we're ready." — ColdCipher
pub fn build_key_share_dual(x25519_pub: &[u8; 32], p256_pub: &[u8; 65]) -> Vec<u8> {
    // X25519 entry: group(2) + key_len(2) + key(32) = 36
    // P-256 entry:  group(2) + key_len(2) + key(65) = 69
    let x25519_entry_len = 2 + 2 + 32;
    let p256_entry_len = 2 + 2 + 65;
    let shares_len = x25519_entry_len + p256_entry_len;
    let ext_len = 2 + shares_len; // client_shares length(2) + entries

    let mut ext = Vec::with_capacity(4 + ext_len);
    // Extension header
    ext.push((EXT_KEY_SHARE >> 8) as u8);
    ext.push((EXT_KEY_SHARE & 0xFF) as u8);
    ext.push((ext_len >> 8) as u8);
    ext.push((ext_len & 0xFF) as u8);
    // client_shares total length
    ext.push((shares_len >> 8) as u8);
    ext.push((shares_len & 0xFF) as u8);

    // X25519 entry (listed first — preferred)
    ext.push((GROUP_X25519 >> 8) as u8);
    ext.push((GROUP_X25519 & 0xFF) as u8);
    ext.push(0); ext.push(32);
    ext.extend_from_slice(x25519_pub);

    // P-256 entry
    ext.push((GROUP_SECP256R1 >> 8) as u8);
    ext.push((GROUP_SECP256R1 & 0xFF) as u8);
    ext.push(0); ext.push(65);
    ext.extend_from_slice(p256_pub);

    ext
}

/// Parsed server key share result
pub enum ServerKeyShare {
    X25519([u8; 32]),
    P256(Vec<u8>), // Uncompressed point (65 bytes: 0x04 || x || y)
}

/// Parse server's key_share extension
pub fn parse_server_key_share(data: &[u8]) -> Option<ServerKeyShare> {
    if data.len() < 4 {
        return None;
    }
    let group = ((data[0] as u16) << 8) | data[1] as u16;
    let key_len = ((data[2] as u16) << 8) | data[3] as u16;

    if data.len() < 4 + key_len as usize {
        return None;
    }

    match group {
        GROUP_X25519 => {
            if key_len != 32 { return None; }
            let mut key = [0u8; 32];
            key.copy_from_slice(&data[4..36]);
            Some(ServerKeyShare::X25519(key))
        }
        GROUP_SECP256R1 => {
            if key_len != 65 { return None; } // Uncompressed P-256 point
            Some(ServerKeyShare::P256(data[4..4 + 65].to_vec()))
        }
        _ => None, // Unsupported group
    }
}

/// Parse server's supported_versions extension
pub fn parse_server_supported_versions(data: &[u8]) -> Option<u16> {
    if data.len() < 2 {
        return None;
    }
    Some(((data[0] as u16) << 8) | data[1] as u16)
}

/// Parse extensions from a buffer. Returns list of (type, data).
pub fn parse_extensions(data: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos + 4 <= data.len() {
        let ext_type = ((data[pos] as u16) << 8) | data[pos + 1] as u16;
        let ext_len = ((data[pos + 2] as u16) << 8) | data[pos + 3] as u16;
        pos += 4;
        if pos + ext_len as usize > data.len() {
            break;
        }
        result.push((ext_type, data[pos..pos + ext_len as usize].to_vec()));
        pos += ext_len as usize;
    }

    result
}
