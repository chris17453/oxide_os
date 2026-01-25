//! TLS Key Material

use crypto::AesKey;

/// Key material derived from master secret
#[derive(Clone)]
pub struct KeyMaterial {
    /// Client write key (32 bytes for AES-256)
    pub client_write_key: AesKey,
    /// Server write key (32 bytes for AES-256)
    pub server_write_key: AesKey,
    /// Client write IV (4 bytes implicit nonce)
    pub client_write_iv: [u8; 4],
    /// Server write IV (4 bytes implicit nonce)
    pub server_write_iv: [u8; 4],
}

impl KeyMaterial {
    /// Create key material from raw bytes
    pub fn from_raw(
        client_key: &[u8],
        server_key: &[u8],
        client_iv: &[u8],
        server_iv: &[u8],
    ) -> Option<Self> {
        if client_key.len() != 32 || server_key.len() != 32 {
            return None;
        }
        if client_iv.len() != 4 || server_iv.len() != 4 {
            return None;
        }

        let mut ck = [0u8; 32];
        let mut sk = [0u8; 32];
        let mut ci = [0u8; 4];
        let mut si = [0u8; 4];

        ck.copy_from_slice(client_key);
        sk.copy_from_slice(server_key);
        ci.copy_from_slice(client_iv);
        si.copy_from_slice(server_iv);

        Some(Self {
            client_write_key: ck,
            server_write_key: sk,
            client_write_iv: ci,
            server_write_iv: si,
        })
    }
}
