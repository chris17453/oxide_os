//! Trust sharing (export/import)

use alloc::string::String;
use alloc::vec::Vec;
use crypto::PublicKey;
use crate::key::TrustedKey;

/// Export format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// PEM encoded
    Pem,
    /// DER binary
    Der,
    /// JSON
    Json,
    /// QR code data
    QrCode,
}

/// Trust export
#[derive(Debug, Clone)]
pub enum TrustExport {
    /// QR code (up to ~2KB)
    QrCode {
        /// Public key
        public_key: PublicKey,
        /// Name
        name: String,
        /// Fingerprint
        fingerprint: [u8; 32],
    },

    /// NFC (NDEF record)
    Nfc {
        /// Public key
        public_key: PublicKey,
        /// Name
        name: String,
    },

    /// File export
    File {
        /// Keys to export
        keys: Vec<TrustedKey>,
        /// Export format
        format: ExportFormat,
    },
}

impl TrustExport {
    /// Create QR code export
    pub fn qr_code(key: &TrustedKey) -> Self {
        TrustExport::QrCode {
            public_key: key.public_key.clone(),
            name: key.name.clone(),
            fingerprint: key.fingerprint,
        }
    }

    /// Create NFC export
    pub fn nfc(key: &TrustedKey) -> Self {
        TrustExport::Nfc {
            public_key: key.public_key.clone(),
            name: key.name.clone(),
        }
    }

    /// Create file export
    pub fn file(keys: Vec<TrustedKey>, format: ExportFormat) -> Self {
        TrustExport::File { keys, format }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            TrustExport::QrCode { public_key, name, fingerprint } => {
                let mut data = Vec::new();
                data.push(0x01); // Type: QR
                data.extend_from_slice(public_key.as_bytes());
                data.push(name.len() as u8);
                data.extend_from_slice(name.as_bytes());
                data.extend_from_slice(fingerprint);
                data
            }
            TrustExport::Nfc { public_key, name } => {
                let mut data = Vec::new();
                data.push(0x02); // Type: NFC
                data.extend_from_slice(public_key.as_bytes());
                data.push(name.len() as u8);
                data.extend_from_slice(name.as_bytes());
                data
            }
            TrustExport::File { keys, format } => {
                let mut data = Vec::new();
                data.push(0x03); // Type: File
                data.push(*format as u8);
                data.push(keys.len() as u8);
                for key in keys {
                    data.extend_from_slice(key.public_key.as_bytes());
                    data.push(key.name.len() as u8);
                    data.extend_from_slice(key.name.as_bytes());
                }
                data
            }
        }
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        match data[0] {
            0x01 => {
                // QR code
                if data.len() < 34 {
                    return None;
                }
                let public_key = PublicKey::from_bytes(&data[1..33]).ok()?;
                let name_len = data[33] as usize;
                if data.len() < 34 + name_len + 32 {
                    return None;
                }
                let name = String::from_utf8(data[34..34 + name_len].to_vec()).ok()?;
                let mut fingerprint = [0u8; 32];
                fingerprint.copy_from_slice(&data[34 + name_len..66 + name_len]);
                Some(TrustExport::QrCode {
                    public_key,
                    name,
                    fingerprint,
                })
            }
            0x02 => {
                // NFC
                if data.len() < 34 {
                    return None;
                }
                let public_key = PublicKey::from_bytes(&data[1..33]).ok()?;
                let name_len = data[33] as usize;
                if data.len() < 34 + name_len {
                    return None;
                }
                let name = String::from_utf8(data[34..34 + name_len].to_vec()).ok()?;
                Some(TrustExport::Nfc { public_key, name })
            }
            _ => None,
        }
    }
}
