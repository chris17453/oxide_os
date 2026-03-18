//! TLS 1.3 Alert Protocol (RFC 8446 Section 6)
//!
//! — ColdCipher: Every error has a code. Every code tells a story.
//! Most of them end badly for someone's connection.

#![allow(dead_code)]

/// TLS alert level
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum AlertLevel {
    Warning = 1,
    Fatal = 2,
}

/// TLS alert description
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum AlertDescription {
    CloseNotify = 0,
    UnexpectedMessage = 10,
    BadRecordMac = 20,
    RecordOverflow = 22,
    HandshakeFailure = 40,
    BadCertificate = 42,
    UnsupportedCertificate = 43,
    CertificateRevoked = 44,
    CertificateExpired = 45,
    CertificateUnknown = 46,
    IllegalParameter = 47,
    UnknownCa = 48,
    AccessDenied = 49,
    DecodeError = 50,
    DecryptError = 51,
    ProtocolVersion = 70,
    InsufficientSecurity = 71,
    InternalError = 80,
    InappropriateFallback = 86,
    UserCanceled = 90,
    MissingExtension = 109,
    UnsupportedExtension = 110,
    UnrecognizedName = 112,
    BadCertificateStatusResponse = 113,
    UnknownPskIdentity = 115,
    CertificateRequired = 116,
    NoApplicationProtocol = 120,
}

/// — ColdCipher: A TLS alert. Two bytes of diplomatic failure. — ColdCipher
#[derive(Debug, Clone, Copy)]
pub struct Alert {
    pub level: AlertLevel,
    pub description: AlertDescription,
}

impl Alert {
    pub fn fatal(desc: AlertDescription) -> Self {
        Alert { level: AlertLevel::Fatal, description: desc }
    }

    pub fn warning(desc: AlertDescription) -> Self {
        Alert { level: AlertLevel::Warning, description: desc }
    }

    pub fn encode(&self) -> [u8; 2] {
        [self.level as u8, self.description as u8]
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 2 { return None; }
        let level = match data[0] {
            1 => AlertLevel::Warning,
            2 => AlertLevel::Fatal,
            _ => return None,
        };
        let description = match data[1] {
            0 => AlertDescription::CloseNotify,
            10 => AlertDescription::UnexpectedMessage,
            20 => AlertDescription::BadRecordMac,
            40 => AlertDescription::HandshakeFailure,
            42 => AlertDescription::BadCertificate,
            48 => AlertDescription::UnknownCa,
            50 => AlertDescription::DecodeError,
            51 => AlertDescription::DecryptError,
            70 => AlertDescription::ProtocolVersion,
            80 => AlertDescription::InternalError,
            _ => AlertDescription::InternalError,
        };
        Some(Alert { level, description })
    }
}
