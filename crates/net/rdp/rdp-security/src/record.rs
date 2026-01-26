//! TLS Record Layer

use super::TLS_VERSION_1_2;
use alloc::vec;
use alloc::vec::Vec;
use rdp_traits::{RdpError, RdpResult};

/// TLS record types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecordType {
    /// Change Cipher Spec
    ChangeCipherSpec = 20,
    /// Alert
    Alert = 21,
    /// Handshake
    Handshake = 22,
    /// Application Data
    ApplicationData = 23,
}

impl RecordType {
    /// Parse from byte
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            20 => Some(Self::ChangeCipherSpec),
            21 => Some(Self::Alert),
            22 => Some(Self::Handshake),
            23 => Some(Self::ApplicationData),
            _ => None,
        }
    }
}

/// TLS record
#[derive(Debug, Clone)]
pub struct TlsRecord {
    /// Record type
    pub record_type: RecordType,
    /// Protocol version
    pub version: u16,
    /// Record data
    pub data: Vec<u8>,
}

impl TlsRecord {
    /// Header size
    pub const HEADER_SIZE: usize = 5;

    /// Parse a TLS record from bytes
    ///
    /// Returns `None` if more data is needed.
    pub fn parse(data: &[u8]) -> RdpResult<Option<(Self, usize)>> {
        if data.len() < Self::HEADER_SIZE {
            return Ok(None);
        }

        let record_type = RecordType::from_byte(data[0]).ok_or(RdpError::InvalidProtocol)?;
        let version = u16::from_be_bytes([data[1], data[2]]);
        let length = u16::from_be_bytes([data[3], data[4]]) as usize;

        let total_len = Self::HEADER_SIZE + length;
        if data.len() < total_len {
            return Ok(None);
        }

        let record_data = data[Self::HEADER_SIZE..total_len].to_vec();

        Ok(Some((
            Self {
                record_type,
                version,
                data: record_data,
            },
            total_len,
        )))
    }

    /// Encode a TLS record
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(Self::HEADER_SIZE + self.data.len());
        output.push(self.record_type as u8);
        output.extend_from_slice(&self.version.to_be_bytes());
        output.extend_from_slice(&(self.data.len() as u16).to_be_bytes());
        output.extend_from_slice(&self.data);
        output
    }

    /// Create a ChangeCipherSpec record
    pub fn change_cipher_spec() -> Self {
        Self {
            record_type: RecordType::ChangeCipherSpec,
            version: TLS_VERSION_1_2,
            data: vec![1], // CCS message is always a single byte 0x01
        }
    }

    /// Create an alert record
    pub fn alert(level: AlertLevel, description: AlertDescription) -> Self {
        Self {
            record_type: RecordType::Alert,
            version: TLS_VERSION_1_2,
            data: vec![level as u8, description as u8],
        }
    }
}

/// TLS alert levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AlertLevel {
    /// Warning
    Warning = 1,
    /// Fatal
    Fatal = 2,
}

/// TLS alert descriptions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AlertDescription {
    /// Close notify
    CloseNotify = 0,
    /// Unexpected message
    UnexpectedMessage = 10,
    /// Bad record MAC
    BadRecordMac = 20,
    /// Decryption failed
    DecryptionFailed = 21,
    /// Record overflow
    RecordOverflow = 22,
    /// Decompression failure
    DecompressionFailure = 30,
    /// Handshake failure
    HandshakeFailure = 40,
    /// Bad certificate
    BadCertificate = 42,
    /// Unsupported certificate
    UnsupportedCertificate = 43,
    /// Certificate revoked
    CertificateRevoked = 44,
    /// Certificate expired
    CertificateExpired = 45,
    /// Certificate unknown
    CertificateUnknown = 46,
    /// Illegal parameter
    IllegalParameter = 47,
    /// Unknown CA
    UnknownCa = 48,
    /// Access denied
    AccessDenied = 49,
    /// Decode error
    DecodeError = 50,
    /// Decrypt error
    DecryptError = 51,
    /// Protocol version
    ProtocolVersion = 70,
    /// Insufficient security
    InsufficientSecurity = 71,
    /// Internal error
    InternalError = 80,
    /// User canceled
    UserCanceled = 90,
    /// No renegotiation
    NoRenegotiation = 100,
}

/// Handshake message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HandshakeType {
    /// Hello Request
    HelloRequest = 0,
    /// Client Hello
    ClientHello = 1,
    /// Server Hello
    ServerHello = 2,
    /// Certificate
    Certificate = 11,
    /// Server Key Exchange
    ServerKeyExchange = 12,
    /// Certificate Request
    CertificateRequest = 13,
    /// Server Hello Done
    ServerHelloDone = 14,
    /// Certificate Verify
    CertificateVerify = 15,
    /// Client Key Exchange
    ClientKeyExchange = 16,
    /// Finished
    Finished = 20,
}

/// Handshake message
#[derive(Debug, Clone)]
pub struct HandshakeMessage {
    /// Message type
    pub msg_type: HandshakeType,
    /// Message data
    pub data: Vec<u8>,
}

impl HandshakeMessage {
    /// Parse a handshake message
    pub fn parse(data: &[u8]) -> RdpResult<Option<(Self, usize)>> {
        if data.len() < 4 {
            return Ok(None);
        }

        let msg_type = data[0];
        let length = ((data[1] as usize) << 16) | ((data[2] as usize) << 8) | (data[3] as usize);

        let total_len = 4 + length;
        if data.len() < total_len {
            return Ok(None);
        }

        let msg_type = match msg_type {
            0 => HandshakeType::HelloRequest,
            1 => HandshakeType::ClientHello,
            2 => HandshakeType::ServerHello,
            11 => HandshakeType::Certificate,
            12 => HandshakeType::ServerKeyExchange,
            13 => HandshakeType::CertificateRequest,
            14 => HandshakeType::ServerHelloDone,
            15 => HandshakeType::CertificateVerify,
            16 => HandshakeType::ClientKeyExchange,
            20 => HandshakeType::Finished,
            _ => return Err(RdpError::InvalidProtocol),
        };

        let message_data = data[4..total_len].to_vec();

        Ok(Some((
            Self {
                msg_type,
                data: message_data,
            },
            total_len,
        )))
    }

    /// Encode a handshake message
    pub fn encode(&self) -> Vec<u8> {
        let length = self.data.len();
        let mut output = Vec::with_capacity(4 + length);
        output.push(self.msg_type as u8);
        output.push((length >> 16) as u8);
        output.push((length >> 8) as u8);
        output.push(length as u8);
        output.extend_from_slice(&self.data);
        output
    }
}
