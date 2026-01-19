//! X.509 Certificate Handling for EFFLUX OS
//!
//! Parsing and validation of X.509 certificates.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// X.509 error types
#[derive(Debug, Clone)]
pub enum X509Error {
    /// Invalid certificate format
    InvalidFormat,
    /// Unsupported version
    UnsupportedVersion,
    /// Signature verification failed
    SignatureInvalid,
    /// Certificate expired
    Expired,
    /// Certificate not yet valid
    NotYetValid,
    /// Chain validation failed
    ChainInvalid,
    /// Revoked certificate
    Revoked,
    /// Parse error
    ParseError(String),
}

impl core::fmt::Display for X509Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidFormat => write!(f, "invalid certificate format"),
            Self::UnsupportedVersion => write!(f, "unsupported X.509 version"),
            Self::SignatureInvalid => write!(f, "signature verification failed"),
            Self::Expired => write!(f, "certificate has expired"),
            Self::NotYetValid => write!(f, "certificate is not yet valid"),
            Self::ChainInvalid => write!(f, "certificate chain validation failed"),
            Self::Revoked => write!(f, "certificate has been revoked"),
            Self::ParseError(s) => write!(f, "parse error: {}", s),
        }
    }
}

/// Result type for X.509 operations
pub type X509Result<T> = Result<T, X509Error>;

/// X.509 certificate version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    V1,
    V2,
    V3,
}

/// Distinguished Name
#[derive(Debug, Clone)]
pub struct Name {
    /// Common Name (CN)
    pub common_name: Option<String>,
    /// Organization (O)
    pub organization: Option<String>,
    /// Organizational Unit (OU)
    pub organizational_unit: Option<String>,
    /// Country (C)
    pub country: Option<String>,
    /// State/Province (ST)
    pub state: Option<String>,
    /// Locality (L)
    pub locality: Option<String>,
}

impl Name {
    /// Create empty name
    pub fn new() -> Self {
        Name {
            common_name: None,
            organization: None,
            organizational_unit: None,
            country: None,
            state: None,
            locality: None,
        }
    }

    /// Get display string
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ref cn) = self.common_name {
            parts.push(alloc::format!("CN={}", cn));
        }
        if let Some(ref o) = self.organization {
            parts.push(alloc::format!("O={}", o));
        }
        if let Some(ref c) = self.country {
            parts.push(alloc::format!("C={}", c));
        }
        parts.join(", ")
    }
}

impl Default for Name {
    fn default() -> Self {
        Self::new()
    }
}

/// Timestamp for certificate validity
#[derive(Debug, Clone, Copy)]
pub struct Timestamp {
    /// Year
    pub year: u16,
    /// Month (1-12)
    pub month: u8,
    /// Day (1-31)
    pub day: u8,
    /// Hour (0-23)
    pub hour: u8,
    /// Minute (0-59)
    pub minute: u8,
    /// Second (0-59)
    pub second: u8,
}

impl Timestamp {
    /// Create from Unix timestamp
    pub fn from_unix(_timestamp: u64) -> Self {
        // Simplified - would do proper conversion
        Timestamp {
            year: 2025,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }

    /// Check if before another timestamp
    pub fn before(&self, other: &Timestamp) -> bool {
        if self.year != other.year {
            return self.year < other.year;
        }
        if self.month != other.month {
            return self.month < other.month;
        }
        if self.day != other.day {
            return self.day < other.day;
        }
        if self.hour != other.hour {
            return self.hour < other.hour;
        }
        if self.minute != other.minute {
            return self.minute < other.minute;
        }
        self.second < other.second
    }
}

/// X.509 extension
#[derive(Debug, Clone)]
pub struct Extension {
    /// OID
    pub oid: Vec<u32>,
    /// Critical flag
    pub critical: bool,
    /// Value
    pub value: Vec<u8>,
}

/// Public key algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAlgorithm {
    Rsa,
    Ecdsa,
    Ed25519,
    Unknown,
}

/// X.509 certificate
#[derive(Debug, Clone)]
pub struct Certificate {
    /// Version
    pub version: Version,
    /// Serial number
    pub serial: Vec<u8>,
    /// Issuer
    pub issuer: Name,
    /// Subject
    pub subject: Name,
    /// Not valid before
    pub not_before: Timestamp,
    /// Not valid after
    pub not_after: Timestamp,
    /// Public key algorithm
    pub key_algorithm: KeyAlgorithm,
    /// Public key bytes
    pub public_key: Vec<u8>,
    /// Extensions
    pub extensions: Vec<Extension>,
    /// Signature algorithm
    pub signature_algorithm: KeyAlgorithm,
    /// Signature
    pub signature: Vec<u8>,
    /// Raw TBS (to-be-signed) data
    pub tbs_raw: Vec<u8>,
}

impl Certificate {
    /// Parse certificate from DER bytes
    pub fn from_der(der: &[u8]) -> X509Result<Self> {
        // Simplified DER parser
        if der.len() < 10 {
            return Err(X509Error::InvalidFormat);
        }

        // Check for SEQUENCE tag
        if der[0] != 0x30 {
            return Err(X509Error::InvalidFormat);
        }

        // Placeholder certificate
        Ok(Certificate {
            version: Version::V3,
            serial: Vec::new(),
            issuer: Name::new(),
            subject: Name::new(),
            not_before: Timestamp::from_unix(0),
            not_after: Timestamp::from_unix(u64::MAX),
            key_algorithm: KeyAlgorithm::Unknown,
            public_key: Vec::new(),
            extensions: Vec::new(),
            signature_algorithm: KeyAlgorithm::Unknown,
            signature: Vec::new(),
            tbs_raw: der.to_vec(),
        })
    }

    /// Parse certificate from PEM string
    pub fn from_pem(pem: &str) -> X509Result<Self> {
        // Find base64 content
        let start = pem
            .find("-----BEGIN CERTIFICATE-----")
            .ok_or(X509Error::InvalidFormat)?;
        let end = pem
            .find("-----END CERTIFICATE-----")
            .ok_or(X509Error::InvalidFormat)?;

        let b64_start = start + "-----BEGIN CERTIFICATE-----".len();
        let b64_content = &pem[b64_start..end].trim();

        // Decode base64 (simplified)
        let der = base64_decode(b64_content)?;
        Self::from_der(&der)
    }

    /// Check if certificate is valid at given time
    pub fn is_valid_at(&self, time: &Timestamp) -> bool {
        self.not_before.before(time) && time.before(&self.not_after)
    }

    /// Check if self-signed
    pub fn is_self_signed(&self) -> bool {
        self.issuer.display() == self.subject.display()
    }

    /// Get subject common name
    pub fn subject_cn(&self) -> Option<&str> {
        self.subject.common_name.as_deref()
    }

    /// Get issuer common name
    pub fn issuer_cn(&self) -> Option<&str> {
        self.issuer.common_name.as_deref()
    }
}

/// Verify certificate chain
pub fn verify_chain(
    cert: &Certificate,
    intermediates: &[Certificate],
    trust_anchors: &[Certificate],
) -> X509Result<()> {
    // Find issuer
    let issuer = if cert.is_self_signed() {
        // Check against trust anchors
        if trust_anchors.iter().any(|ta| {
            ta.subject.display() == cert.subject.display()
        }) {
            return Ok(());
        }
        return Err(X509Error::ChainInvalid);
    } else {
        // Find in intermediates or trust anchors
        let issuer_name = cert.issuer.display();
        intermediates
            .iter()
            .chain(trust_anchors.iter())
            .find(|c| c.subject.display() == issuer_name)
    };

    match issuer {
        Some(issuer_cert) => {
            // Recursively verify
            verify_chain(issuer_cert, intermediates, trust_anchors)
        }
        None => Err(X509Error::ChainInvalid),
    }
}

/// Certificate Revocation List
#[derive(Debug, Clone)]
pub struct Crl {
    /// Issuer
    pub issuer: Name,
    /// This update
    pub this_update: Timestamp,
    /// Next update
    pub next_update: Option<Timestamp>,
    /// Revoked certificates (serial numbers)
    pub revoked: Vec<Vec<u8>>,
}

impl Crl {
    /// Check if certificate is revoked
    pub fn is_revoked(&self, cert: &Certificate) -> bool {
        self.revoked.iter().any(|serial| *serial == cert.serial)
    }
}

/// Simple base64 decode
fn base64_decode(input: &str) -> X509Result<Vec<u8>> {
    let chars: Vec<char> = input.chars().filter(|c| !c.is_whitespace()).collect();
    let mut output = Vec::with_capacity(chars.len() * 3 / 4);

    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let decode_char = |c: char| -> X509Result<u8> {
        if c == '=' {
            return Ok(0);
        }
        TABLE
            .iter()
            .position(|&x| x == c as u8)
            .map(|p| p as u8)
            .ok_or(X509Error::ParseError(String::from("invalid base64")))
    };

    for chunk in chars.chunks(4) {
        if chunk.len() < 4 {
            break;
        }

        let a = decode_char(chunk[0])?;
        let b = decode_char(chunk[1])?;
        let c = decode_char(chunk[2])?;
        let d = decode_char(chunk[3])?;

        output.push((a << 2) | (b >> 4));
        if chunk[2] != '=' {
            output.push((b << 4) | (c >> 2));
        }
        if chunk[3] != '=' {
            output.push((c << 6) | d);
        }
    }

    Ok(output)
}
