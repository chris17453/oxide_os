//! MCS (T.125) - Multipoint Communication Service
//!
//! MCS provides the multipoint communication layer for RDP, managing
//! channels and multiplexing data between the server and clients.

use crate::ber;
use crate::{Cursor, Writer};
use alloc::vec::Vec;
use rdp_traits::{RdpError, RdpResult};

/// MCS PDU types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McsPduType {
    /// Connect-Initial (BER encoded)
    ConnectInitial,
    /// Connect-Response (BER encoded)
    ConnectResponse,
    /// Erect Domain Request
    ErectDomainRequest,
    /// Disconnect Provider Ultimatum
    DisconnectProviderUltimatum,
    /// Attach User Request
    AttachUserRequest,
    /// Attach User Confirm
    AttachUserConfirm,
    /// Channel Join Request
    ChannelJoinRequest,
    /// Channel Join Confirm
    ChannelJoinConfirm,
    /// Send Data Request
    SendDataRequest,
    /// Send Data Indication
    SendDataIndication,
}

/// MCS domain parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainParameters {
    /// Maximum number of channels
    pub max_channel_ids: u32,
    /// Maximum number of users
    pub max_user_ids: u32,
    /// Maximum number of tokens
    pub max_token_ids: u32,
    /// Number of priorities
    pub num_priorities: u32,
    /// Minimum throughput
    pub min_throughput: u32,
    /// Maximum height (tree depth)
    pub max_height: u32,
    /// Maximum MCS PDU size
    pub max_mcs_pdu_size: u32,
    /// Protocol version
    pub protocol_version: u32,
}

impl Default for DomainParameters {
    fn default() -> Self {
        Self {
            max_channel_ids: 65535,
            max_user_ids: 65535,
            max_token_ids: 65535,
            num_priorities: 1,
            min_throughput: 0,
            max_height: 1,
            max_mcs_pdu_size: 65535,
            protocol_version: 2,
        }
    }
}

impl DomainParameters {
    /// Parse domain parameters from BER-encoded data
    pub fn parse(cursor: &mut Cursor<'_>) -> RdpResult<Self> {
        // Expect SEQUENCE tag
        let tag = cursor.read_u8()?;
        if tag != 0x30 {
            return Err(RdpError::InvalidProtocol);
        }

        let _length = ber::read_length(cursor)?;

        Ok(Self {
            max_channel_ids: ber::read_unsigned(cursor)?,
            max_user_ids: ber::read_unsigned(cursor)?,
            max_token_ids: ber::read_unsigned(cursor)?,
            num_priorities: ber::read_unsigned(cursor)?,
            min_throughput: ber::read_unsigned(cursor)?,
            max_height: ber::read_unsigned(cursor)?,
            max_mcs_pdu_size: ber::read_unsigned(cursor)?,
            protocol_version: ber::read_unsigned(cursor)?,
        })
    }

    /// Encode domain parameters as BER SEQUENCE
    pub fn encode(&self) -> Vec<u8> {
        let mut inner = Writer::new();
        ber::write_unsigned(&mut inner, self.max_channel_ids);
        ber::write_unsigned(&mut inner, self.max_user_ids);
        ber::write_unsigned(&mut inner, self.max_token_ids);
        ber::write_unsigned(&mut inner, self.num_priorities);
        ber::write_unsigned(&mut inner, self.min_throughput);
        ber::write_unsigned(&mut inner, self.max_height);
        ber::write_unsigned(&mut inner, self.max_mcs_pdu_size);
        ber::write_unsigned(&mut inner, self.protocol_version);

        let mut outer = Writer::new();
        outer.write_u8(0x30); // SEQUENCE
        ber::write_length(&mut outer, inner.len());
        outer.write_bytes(inner.as_slice());

        outer.into_vec()
    }
}

/// MCS Connect-Initial PDU
#[derive(Debug, Clone)]
pub struct ConnectInitial {
    /// Calling domain selector
    pub calling_domain_selector: Vec<u8>,
    /// Called domain selector
    pub called_domain_selector: Vec<u8>,
    /// Upward flag
    pub upward_flag: bool,
    /// Target parameters
    pub target_params: DomainParameters,
    /// Minimum parameters
    pub min_params: DomainParameters,
    /// Maximum parameters
    pub max_params: DomainParameters,
    /// User data (GCC Conference Create Request)
    pub user_data: Vec<u8>,
}

impl ConnectInitial {
    /// Application tag for Connect-Initial
    const TAG: u8 = 0x65; // [APPLICATION 101]

    /// Parse a Connect-Initial PDU
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        let mut cursor = Cursor::new(data);

        // Expect Application tag 101
        let tag = cursor.read_u8()?;
        if tag != Self::TAG {
            return Err(RdpError::InvalidProtocol);
        }

        let _length = ber::read_length(&mut cursor)?;

        // Calling domain selector (OCTET STRING)
        let calling_domain_selector = ber::read_octet_string(&mut cursor)?.to_vec();

        // Called domain selector (OCTET STRING)
        let called_domain_selector = ber::read_octet_string(&mut cursor)?.to_vec();

        // Upward flag (BOOLEAN)
        let upward_flag = ber::read_boolean(&mut cursor)?;

        // Target parameters
        let target_params = DomainParameters::parse(&mut cursor)?;

        // Minimum parameters
        let min_params = DomainParameters::parse(&mut cursor)?;

        // Maximum parameters
        let max_params = DomainParameters::parse(&mut cursor)?;

        // User data (OCTET STRING)
        let user_data = ber::read_octet_string(&mut cursor)?.to_vec();

        Ok(Self {
            calling_domain_selector,
            called_domain_selector,
            upward_flag,
            target_params,
            min_params,
            max_params,
            user_data,
        })
    }
}

/// MCS Connect-Response PDU
#[derive(Debug, Clone)]
pub struct ConnectResponse {
    /// Result code
    pub result: ConnectResult,
    /// Called connect ID
    pub called_connect_id: u32,
    /// Domain parameters
    pub domain_params: DomainParameters,
    /// User data (GCC Conference Create Response)
    pub user_data: Vec<u8>,
}

/// MCS Connect result codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectResult {
    Success = 0,
    DomainMerging = 1,
    DomainNotHierarchical = 2,
    NoSuchChannel = 3,
    NoSuchDomain = 4,
    NoSuchUser = 5,
    NotAdmitted = 6,
    OtherUserId = 7,
    ParametersUnacceptable = 8,
    TokenNotAvailable = 9,
    TokenNotPossessed = 10,
    TooManyChannels = 11,
    TooManyTokens = 12,
    TooManyUsers = 13,
    UnspecifiedFailure = 14,
    UserRejected = 15,
}

impl ConnectResponse {
    /// Application tag for Connect-Response
    const TAG: u8 = 0x66; // [APPLICATION 102]

    /// Encode a Connect-Response PDU
    pub fn encode(&self) -> Vec<u8> {
        // Build inner content
        let mut inner = Writer::new();

        // Result (ENUMERATED)
        ber::write_enumerated(&mut inner, self.result as u8);

        // Called connect ID (INTEGER)
        ber::write_unsigned(&mut inner, self.called_connect_id);

        // Domain parameters
        inner.write_bytes(&self.domain_params.encode());

        // User data (OCTET STRING)
        ber::write_octet_string(&mut inner, &self.user_data);

        // Wrap in Application tag
        let mut outer = Writer::new();
        outer.write_u8(Self::TAG);
        ber::write_length(&mut outer, inner.len());
        outer.write_bytes(inner.as_slice());

        outer.into_vec()
    }
}

/// MCS Erect Domain Request
#[derive(Debug, Clone, Copy)]
pub struct ErectDomainRequest {
    /// Sub-height
    pub sub_height: u32,
    /// Sub-interval
    pub sub_interval: u32,
}

impl ErectDomainRequest {
    /// PDU type byte
    const TYPE: u8 = 0x04;

    /// Parse an Erect Domain Request
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        let mut cursor = Cursor::new(data);

        // Type field (4 bits) + padding
        let type_byte = cursor.read_u8()?;
        if (type_byte >> 2) != Self::TYPE {
            return Err(RdpError::InvalidProtocol);
        }

        // Read PER encoded integers
        let sub_height = read_per_integer(&mut cursor)?;
        let sub_interval = read_per_integer(&mut cursor)?;

        Ok(Self {
            sub_height,
            sub_interval,
        })
    }
}

/// MCS Attach User Request
#[derive(Debug, Clone, Copy)]
pub struct AttachUserRequest;

impl AttachUserRequest {
    /// PDU type byte
    const TYPE: u8 = 0x0A;

    /// Parse an Attach User Request
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.is_empty() {
            return Err(RdpError::InsufficientData);
        }

        let type_byte = data[0];
        if (type_byte >> 2) != Self::TYPE {
            return Err(RdpError::InvalidProtocol);
        }

        Ok(Self)
    }
}

/// MCS Attach User Confirm
#[derive(Debug, Clone, Copy)]
pub struct AttachUserConfirm {
    /// Result code (0 = success)
    pub result: u8,
    /// Initiator (user ID)
    pub initiator: u16,
}

impl AttachUserConfirm {
    /// PDU type byte
    const TYPE: u8 = 0x0B;

    /// Encode an Attach User Confirm
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();

        // Type (4 bits) + result (4 bits)
        writer.write_u8((Self::TYPE << 2) | (self.result & 0x0F));

        // Initiator (user ID) - optional, present if result == 0
        if self.result == 0 {
            writer.write_u16_be(self.initiator);
        }

        writer.into_vec()
    }
}

/// MCS Channel Join Request
#[derive(Debug, Clone, Copy)]
pub struct ChannelJoinRequest {
    /// Initiator (user ID)
    pub initiator: u16,
    /// Channel ID to join
    pub channel_id: u16,
}

impl ChannelJoinRequest {
    /// PDU type byte
    const TYPE: u8 = 0x0E;

    /// Parse a Channel Join Request
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        let mut cursor = Cursor::new(data);

        let type_byte = cursor.read_u8()?;
        if (type_byte >> 2) != Self::TYPE {
            return Err(RdpError::InvalidProtocol);
        }

        let initiator = cursor.read_u16_be()?;
        let channel_id = cursor.read_u16_be()?;

        Ok(Self {
            initiator,
            channel_id,
        })
    }
}

/// MCS Channel Join Confirm
#[derive(Debug, Clone, Copy)]
pub struct ChannelJoinConfirm {
    /// Result code (0 = success)
    pub result: u8,
    /// Initiator (user ID)
    pub initiator: u16,
    /// Requested channel ID
    pub requested: u16,
    /// Channel ID (may differ from requested)
    pub channel_id: u16,
}

impl ChannelJoinConfirm {
    /// PDU type byte
    const TYPE: u8 = 0x0F;

    /// Encode a Channel Join Confirm
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();

        // Type (4 bits) + result (4 bits)
        writer.write_u8((Self::TYPE << 2) | (self.result & 0x0F));

        // Initiator
        writer.write_u16_be(self.initiator);

        // Requested
        writer.write_u16_be(self.requested);

        // Channel ID (only present if result == 0)
        if self.result == 0 {
            writer.write_u16_be(self.channel_id);
        }

        writer.into_vec()
    }
}

/// MCS Send Data Request
#[derive(Debug, Clone)]
pub struct SendDataRequest {
    /// Initiator (user ID)
    pub initiator: u16,
    /// Channel ID
    pub channel_id: u16,
    /// Data priority
    pub priority: DataPriority,
    /// User data
    pub data: Vec<u8>,
}

/// MCS Send Data Indication (server to client)
#[derive(Debug, Clone)]
pub struct SendDataIndication {
    /// Initiator (user ID)
    pub initiator: u16,
    /// Channel ID
    pub channel_id: u16,
    /// Data priority
    pub priority: DataPriority,
    /// User data
    pub data: Vec<u8>,
}

/// Data priority
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataPriority {
    Top = 0,
    High = 1,
    Medium = 2,
    Low = 3,
}

impl SendDataRequest {
    /// PDU type byte
    const TYPE: u8 = 0x19;

    /// Parse a Send Data Request
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        let mut cursor = Cursor::new(data);

        let type_byte = cursor.read_u8()?;
        if (type_byte >> 2) != Self::TYPE {
            return Err(RdpError::InvalidProtocol);
        }

        let initiator = cursor.read_u16_be()?;
        let channel_id = cursor.read_u16_be()?;

        // Priority (2 bits) + segmentation (2 bits) in next byte
        let flags = cursor.read_u8()?;
        let priority = match (flags >> 6) & 0x03 {
            0 => DataPriority::Top,
            1 => DataPriority::High,
            2 => DataPriority::Medium,
            _ => DataPriority::Low,
        };

        // Data length (PER encoded)
        let data_len = read_per_length(&mut cursor)?;

        // User data
        let user_data = cursor.read_bytes(data_len)?.to_vec();

        Ok(Self {
            initiator,
            channel_id,
            priority,
            data: user_data,
        })
    }
}

impl SendDataIndication {
    /// PDU type byte
    const TYPE: u8 = 0x1A;

    /// Encode a Send Data Indication
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::with_capacity(8 + self.data.len());

        // Type (6 bits)
        writer.write_u8(Self::TYPE << 2);

        // Initiator
        writer.write_u16_be(self.initiator);

        // Channel ID
        writer.write_u16_be(self.channel_id);

        // Priority (2 bits) + segmentation flags (2 bits: Begin=1, End=1)
        let flags = ((self.priority as u8) << 6) | 0x30;
        writer.write_u8(flags);

        // Data length (PER encoded)
        write_per_length(&mut writer, self.data.len());

        // User data
        writer.write_bytes(&self.data);

        writer.into_vec()
    }
}

/// MCS Disconnect Provider Ultimatum
#[derive(Debug, Clone, Copy)]
pub struct DisconnectProviderUltimatum {
    /// Reason code
    pub reason: DisconnectReason,
}

/// Disconnect reason codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DisconnectReason {
    DomainDisconnected = 0,
    ProviderInitiated = 1,
    TokenPurged = 2,
    UserRequested = 3,
    ChannelPurged = 4,
}

impl DisconnectProviderUltimatum {
    /// PDU type byte
    const TYPE: u8 = 0x08;

    /// Encode a Disconnect Provider Ultimatum
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();

        // Type (6 bits) + reason (3 bits, split across bytes)
        writer.write_u8((Self::TYPE << 2) | ((self.reason as u8) >> 1));
        writer.write_u8((self.reason as u8) << 7);

        writer.into_vec()
    }
}

/// Read a PER encoded integer (length-constrained)
fn read_per_integer(cursor: &mut Cursor<'_>) -> RdpResult<u32> {
    let first = cursor.read_u8()?;
    if first < 0x80 {
        Ok(first as u32)
    } else if first < 0xC0 {
        let second = cursor.read_u8()?;
        Ok((((first & 0x3F) as u32) << 8) | (second as u32))
    } else {
        // 4-byte form
        cursor.skip(1)?; // Skip continuation byte
        Ok(cursor.read_u32_be()?)
    }
}

/// Read a PER encoded length
fn read_per_length(cursor: &mut Cursor<'_>) -> RdpResult<usize> {
    let first = cursor.read_u8()?;
    if first < 0x80 {
        Ok(first as usize)
    } else {
        let second = cursor.read_u8()?;
        Ok((((first & 0x7F) as usize) << 8) | (second as usize))
    }
}

/// Write a PER encoded length
fn write_per_length(writer: &mut Writer, length: usize) {
    if length < 0x80 {
        writer.write_u8(length as u8);
    } else {
        writer.write_u8(0x80 | ((length >> 8) as u8 & 0x7F));
        writer.write_u8(length as u8);
    }
}

/// Well-known MCS channel IDs
pub mod channels {
    /// MCS I/O channel (always 1003)
    pub const IO_CHANNEL: u16 = 1003;

    /// User channel base (user IDs start here)
    pub const USER_CHANNEL_BASE: u16 = 1001;

    /// First static virtual channel
    pub const STATIC_CHANNEL_BASE: u16 = 1004;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_params_roundtrip() {
        let params = DomainParameters::default();
        let encoded = params.encode();

        let mut cursor = Cursor::new(&encoded);
        let decoded = DomainParameters::parse(&mut cursor).unwrap();

        assert_eq!(decoded.max_channel_ids, params.max_channel_ids);
        assert_eq!(decoded.max_mcs_pdu_size, params.max_mcs_pdu_size);
    }

    #[test]
    fn test_attach_user_confirm() {
        let confirm = AttachUserConfirm {
            result: 0,
            initiator: 1007,
        };
        let encoded = confirm.encode();
        assert_eq!(encoded.len(), 3);
        assert_eq!(encoded[0], 0x2C); // Type 0x0B << 2
    }
}
