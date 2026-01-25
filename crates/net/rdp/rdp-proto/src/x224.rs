//! X.224 (ISO 8073) - Connection-oriented transport protocol
//!
//! X.224 provides connection establishment and data transfer on top of TPKT.
//! RDP uses X.224 Class 0 (simple class) with no error recovery.
//!
//! Key PDU types:
//! - Connection Request (CR) - Client initiates connection
//! - Connection Confirm (CC) - Server accepts connection
//! - Data (DT) - Data transfer
//! - Disconnect Request (DR) - Connection termination

use crate::{Cursor, Writer};
use rdp_traits::{protocol, RdpError, RdpResult};
use alloc::vec::Vec;

/// X.224 PDU type codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum X224Type {
    /// Connection Request
    ConnectionRequest = 0xE0,
    /// Connection Confirm
    ConnectionConfirm = 0xD0,
    /// Disconnect Request
    DisconnectRequest = 0x80,
    /// Data
    Data = 0xF0,
    /// Error
    Error = 0x70,
}

impl X224Type {
    /// Parse from byte (high nibble contains code)
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte & 0xF0 {
            0xE0 => Some(Self::ConnectionRequest),
            0xD0 => Some(Self::ConnectionConfirm),
            0x80 => Some(Self::DisconnectRequest),
            0xF0 => Some(Self::Data),
            0x70 => Some(Self::Error),
            _ => None,
        }
    }
}

/// X.224 TPDU header for connection request/confirm
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X224Header {
    /// Length indicator (length of header excluding this byte)
    pub length: u8,
    /// PDU type code
    pub pdu_type: X224Type,
    /// Destination reference (for CR/CC)
    pub dst_ref: u16,
    /// Source reference (for CR/CC)
    pub src_ref: u16,
    /// Class and options (for CR/CC)
    pub class_options: u8,
}

/// X.224 Connection Request
#[derive(Debug, Clone, Default)]
pub struct ConnectionRequest {
    /// Cookie (for load balancing)
    pub cookie: Option<Vec<u8>>,
    /// RDP negotiation request
    pub neg_req: Option<NegotiationRequest>,
    /// Correlation info
    pub correlation_info: Option<CorrelationInfo>,
}

/// RDP Negotiation Request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiationRequest {
    /// Requested protocols (bitfield)
    pub protocols: u32,
}

impl NegotiationRequest {
    /// Size in bytes
    pub const SIZE: usize = 8;

    /// Parse from cursor
    pub fn parse(cursor: &mut Cursor<'_>) -> RdpResult<Self> {
        let type_byte = cursor.read_u8()?;
        if type_byte != protocol::TYPE_RDP_NEG_REQ {
            return Err(RdpError::InvalidProtocol);
        }

        let _flags = cursor.read_u8()?;
        let length = cursor.read_u16_le()?;
        if length != 8 {
            return Err(RdpError::InvalidProtocol);
        }

        let protocols = cursor.read_u32_le()?;

        Ok(Self { protocols })
    }

    /// Write to buffer
    pub fn write(&self, writer: &mut Writer) {
        writer.write_u8(protocol::TYPE_RDP_NEG_REQ);
        writer.write_u8(0); // flags
        writer.write_u16_le(8); // length
        writer.write_u32_le(self.protocols);
    }

    /// Check if TLS is requested
    pub fn wants_tls(&self) -> bool {
        self.protocols & protocol::PROTOCOL_SSL != 0
    }

    /// Check if CredSSP (NLA) is requested
    pub fn wants_nla(&self) -> bool {
        self.protocols & protocol::PROTOCOL_HYBRID != 0
    }
}

/// RDP Negotiation Response
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiationResponse {
    /// Flags
    pub flags: u8,
    /// Selected protocol
    pub protocol: u32,
}

impl NegotiationResponse {
    /// Size in bytes
    pub const SIZE: usize = 8;

    /// Write to buffer
    pub fn write(&self, writer: &mut Writer) {
        writer.write_u8(protocol::TYPE_RDP_NEG_RSP);
        writer.write_u8(self.flags);
        writer.write_u16_le(8); // length
        writer.write_u32_le(self.protocol);
    }
}

/// Negotiation failure response
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiationFailure {
    /// Failure code
    pub code: u32,
}

impl NegotiationFailure {
    /// SSL required by server
    pub const SSL_REQUIRED_BY_SERVER: u32 = 0x00000001;
    /// SSL not allowed by server
    pub const SSL_NOT_ALLOWED_BY_SERVER: u32 = 0x00000002;
    /// SSL cert not on server
    pub const SSL_CERT_NOT_ON_SERVER: u32 = 0x00000003;
    /// Inconsistent flags
    pub const INCONSISTENT_FLAGS: u32 = 0x00000004;
    /// Hybrid required by server
    pub const HYBRID_REQUIRED_BY_SERVER: u32 = 0x00000005;
    /// SSL with user auth required by server
    pub const SSL_WITH_USER_AUTH_REQUIRED_BY_SERVER: u32 = 0x00000006;

    /// Write to buffer
    pub fn write(&self, writer: &mut Writer) {
        writer.write_u8(protocol::TYPE_RDP_NEG_FAILURE);
        writer.write_u8(0); // flags
        writer.write_u16_le(8); // length
        writer.write_u32_le(self.code);
    }
}

/// Correlation info for connection tracking
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrelationInfo {
    /// Correlation ID (16 bytes)
    pub id: [u8; 16],
}

impl ConnectionRequest {
    /// Parse an X.224 Connection Request from TPDU data
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.is_empty() {
            return Err(RdpError::InsufficientData);
        }

        let mut cursor = Cursor::new(data);

        // Length indicator
        let length = cursor.read_u8()?;
        if length < 6 {
            return Err(RdpError::InvalidProtocol);
        }

        // Type code
        let type_code = cursor.read_u8()?;
        if X224Type::from_byte(type_code) != Some(X224Type::ConnectionRequest) {
            return Err(RdpError::InvalidProtocol);
        }

        // Destination reference (always 0 for CR)
        let _dst_ref = cursor.read_u16_be()?;

        // Source reference
        let _src_ref = cursor.read_u16_be()?;

        // Class options (Class 0)
        let _class_options = cursor.read_u8()?;

        let mut result = ConnectionRequest::default();

        // Variable part - parse remaining data
        while cursor.remaining() > 0 {
            // Check for cookie ("Cookie: mstshash=...")
            if cursor.remaining() >= 7 {
                let peek = cursor.as_slice();
                if peek.starts_with(b"Cookie:") {
                    // Find end of cookie line (CR LF)
                    if let Some(end) = find_crlf(peek) {
                        result.cookie = Some(cursor.read_bytes(end)?.to_vec());
                        cursor.skip(2)?; // Skip CR LF
                        continue;
                    }
                }
            }

            // Check for RDP negotiation request
            if cursor.remaining() >= NegotiationRequest::SIZE {
                if cursor.peek_u8()? == protocol::TYPE_RDP_NEG_REQ {
                    result.neg_req = Some(NegotiationRequest::parse(&mut cursor)?);
                    continue;
                }
            }

            // Skip unknown data
            break;
        }

        Ok(result)
    }
}

/// X.224 Connection Confirm
#[derive(Debug, Clone, Default)]
pub struct ConnectionConfirm {
    /// RDP negotiation response
    pub neg_rsp: Option<NegotiationResponse>,
    /// Negotiation failure
    pub neg_failure: Option<NegotiationFailure>,
}

impl ConnectionConfirm {
    /// Encode as X.224 TPDU
    pub fn encode(&self, dst_ref: u16, src_ref: u16) -> Vec<u8> {
        let mut writer = Writer::with_capacity(32);

        // Reserve space for length indicator
        let len_pos = writer.len();
        writer.write_u8(0);

        // Type code
        writer.write_u8(protocol::X224_CONNECTION_CONFIRM);

        // Destination reference
        writer.write_u16_be(dst_ref);

        // Source reference
        writer.write_u16_be(src_ref);

        // Class options (Class 0)
        writer.write_u8(0x00);

        // Optional negotiation response/failure
        if let Some(ref neg_rsp) = self.neg_rsp {
            neg_rsp.write(&mut writer);
        } else if let Some(ref neg_failure) = self.neg_failure {
            neg_failure.write(&mut writer);
        }

        // Update length indicator (length of header excluding the length byte itself)
        let total_len = writer.len() - 1;
        writer.set_u8(len_pos, total_len as u8);

        writer.into_vec()
    }

    /// Create a successful response selecting TLS
    pub fn tls_response() -> Self {
        Self {
            neg_rsp: Some(NegotiationResponse {
                flags: 0,
                protocol: protocol::PROTOCOL_SSL,
            }),
            neg_failure: None,
        }
    }

    /// Create a successful response with standard RDP security
    pub fn rdp_response() -> Self {
        Self {
            neg_rsp: Some(NegotiationResponse {
                flags: 0,
                protocol: protocol::PROTOCOL_RDP,
            }),
            neg_failure: None,
        }
    }

    /// Create a failure response
    pub fn failure(code: u32) -> Self {
        Self {
            neg_rsp: None,
            neg_failure: Some(NegotiationFailure { code }),
        }
    }
}

/// X.224 Data TPDU
#[derive(Debug, Clone)]
pub struct DataTpdu {
    /// End of TSDU indicator (always true for RDP)
    pub eot: bool,
    /// Data payload
    pub data: Vec<u8>,
}

impl DataTpdu {
    /// Parse a Data TPDU from raw bytes
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.len() < 3 {
            return Err(RdpError::InsufficientData);
        }

        let mut cursor = Cursor::new(data);

        // Length indicator
        let length = cursor.read_u8()?;
        if length != 2 {
            // Data TPDU header is always 2 bytes (type + EOT)
            return Err(RdpError::InvalidProtocol);
        }

        // Type code
        let type_code = cursor.read_u8()?;
        if X224Type::from_byte(type_code) != Some(X224Type::Data) {
            return Err(RdpError::InvalidProtocol);
        }

        // EOT (End of TSDU) in the NR field
        let nr_eot = cursor.read_u8()?;
        let eot = (nr_eot & 0x80) != 0;

        // Remaining data is the payload
        let payload = cursor.as_slice().to_vec();

        Ok(Self { eot, data: payload })
    }

    /// Encode a Data TPDU
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::with_capacity(3 + self.data.len());

        // Length indicator (header length - 1)
        writer.write_u8(2);

        // Type code
        writer.write_u8(protocol::X224_DATA);

        // EOT (NR = 0, EOT = 1)
        writer.write_u8(if self.eot { 0x80 } else { 0x00 });

        // Payload
        writer.write_bytes(&self.data);

        writer.into_vec()
    }
}

/// Disconnect reason codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DisconnectReason {
    /// Not specified
    NotSpecified = 0x00,
    /// Congestion at TSAP
    CongestionAtTsap = 0x01,
    /// Session not attached to TSAP
    SessionNotAttached = 0x02,
    /// Address unknown
    AddressUnknown = 0x03,
}

/// X.224 Disconnect Request
#[derive(Debug, Clone)]
pub struct DisconnectRequest {
    /// Reason for disconnect
    pub reason: DisconnectReason,
}

impl DisconnectRequest {
    /// Encode a Disconnect Request TPDU
    pub fn encode(&self, dst_ref: u16, src_ref: u16) -> Vec<u8> {
        let mut writer = Writer::with_capacity(8);

        // Length indicator
        writer.write_u8(6);

        // Type code
        writer.write_u8(protocol::X224_DISCONNECT_REQUEST);

        // Destination reference
        writer.write_u16_be(dst_ref);

        // Source reference
        writer.write_u16_be(src_ref);

        // Reason
        writer.write_u8(self.reason as u8);

        writer.into_vec()
    }

    /// Parse a Disconnect Request
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.len() < 7 {
            return Err(RdpError::InsufficientData);
        }

        let mut cursor = Cursor::new(data);

        let _length = cursor.read_u8()?;
        let type_code = cursor.read_u8()?;

        if X224Type::from_byte(type_code) != Some(X224Type::DisconnectRequest) {
            return Err(RdpError::InvalidProtocol);
        }

        let _dst_ref = cursor.read_u16_be()?;
        let _src_ref = cursor.read_u16_be()?;
        let reason = cursor.read_u8()?;

        Ok(Self {
            reason: match reason {
                0x01 => DisconnectReason::CongestionAtTsap,
                0x02 => DisconnectReason::SessionNotAttached,
                0x03 => DisconnectReason::AddressUnknown,
                _ => DisconnectReason::NotSpecified,
            },
        })
    }
}

/// Find CRLF in byte slice
fn find_crlf(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(1) {
        if data[i] == b'\r' && data[i + 1] == b'\n' {
            return Some(i);
        }
    }
    None
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Encode data as an X.224 Data TPDU
pub fn encode_data(data: &[u8]) -> Vec<u8> {
    let tpdu = DataTpdu {
        eot: true,
        data: data.to_vec(),
    };
    tpdu.encode()
}

/// Alias types for connection.rs compatibility
pub type X224ConnectionRequest = ConnectionRequest;
pub type X224ConnectionResponse = ConnectionConfirm;
pub type X224Data = DataTpdu;

/// RDP Negotiation Response (re-export for convenience)
pub type RdpNegotiationResponse = NegotiationResponse;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_tpdu_encode_decode() {
        let tpdu = DataTpdu {
            eot: true,
            data: vec![1, 2, 3, 4, 5],
        };

        let encoded = tpdu.encode();
        assert_eq!(encoded[0], 2); // length
        assert_eq!(encoded[1], 0xF0); // type
        assert_eq!(encoded[2], 0x80); // EOT

        let decoded = DataTpdu::parse(&encoded).unwrap();
        assert_eq!(decoded.eot, true);
        assert_eq!(decoded.data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_connection_confirm() {
        let cc = ConnectionConfirm::tls_response();
        let encoded = cc.encode(0, 0x1234);

        assert!(encoded.len() > 6);
        assert_eq!(encoded[1] & 0xF0, 0xD0); // Connection Confirm type
    }
}
