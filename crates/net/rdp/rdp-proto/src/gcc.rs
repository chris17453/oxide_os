//! GCC (T.124) - Generic Conference Control
//!
//! GCC provides conference establishment for RDP. The conference data
//! is embedded in MCS Connect-Initial/Response user data fields.

use crate::{Cursor, Writer};
use alloc::string::String;
use alloc::vec::Vec;
use rdp_traits::{RdpError, RdpResult};

/// GCC Conference Create Request
#[derive(Debug, Clone)]
pub struct ConferenceCreateRequest {
    /// Conference name (usually "1")
    pub conference_name: String,
    /// User data blocks
    pub user_data: Vec<UserDataBlock>,
}

/// GCC Conference Create Response
#[derive(Debug, Clone)]
pub struct ConferenceCreateResponse {
    /// User data blocks
    pub user_data: Vec<UserDataBlock>,
}

/// User data block types
#[derive(Debug, Clone)]
pub enum UserDataBlock {
    /// Client core data
    ClientCore(ClientCoreData),
    /// Client security data
    ClientSecurity(ClientSecurityData),
    /// Client network data (channel list)
    ClientNetwork(ClientNetworkData),
    /// Client cluster data
    ClientCluster(ClientClusterData),
    /// Server core data
    ServerCore(ServerCoreData),
    /// Server security data
    ServerSecurity(ServerSecurityData),
    /// Server network data
    ServerNetwork(ServerNetworkData),
    /// Unknown block
    Unknown { type_id: u16, data: Vec<u8> },
}

/// User data block type IDs
pub mod block_types {
    // Client to server
    pub const CS_CORE: u16 = 0xC001;
    pub const CS_SECURITY: u16 = 0xC002;
    pub const CS_NET: u16 = 0xC003;
    pub const CS_CLUSTER: u16 = 0xC004;
    pub const CS_MONITOR: u16 = 0xC005;
    pub const CS_MCS_MSGCHANNEL: u16 = 0xC006;
    pub const CS_MONITOR_EX: u16 = 0xC008;
    pub const CS_MULTITRANSPORT: u16 = 0xC00A;

    // Server to client
    pub const SC_CORE: u16 = 0x0C01;
    pub const SC_SECURITY: u16 = 0x0C02;
    pub const SC_NET: u16 = 0x0C03;
    pub const SC_MCS_MSGCHANNEL: u16 = 0x0C04;
    pub const SC_MULTITRANSPORT: u16 = 0x0C08;
}

/// Client core data
#[derive(Debug, Clone)]
pub struct ClientCoreData {
    /// RDP version
    pub version: u32,
    /// Desktop width
    pub desktop_width: u16,
    /// Desktop height
    pub desktop_height: u16,
    /// Color depth
    pub color_depth: ColorDepth,
    /// SAS sequence
    pub sas_sequence: u16,
    /// Keyboard layout
    pub keyboard_layout: u32,
    /// Client build
    pub client_build: u32,
    /// Client name (up to 15 chars)
    pub client_name: String,
    /// Keyboard type
    pub keyboard_type: u32,
    /// Keyboard subtype
    pub keyboard_sub_type: u32,
    /// Keyboard function key count
    pub keyboard_fn_keys: u32,
    /// IME file name
    pub ime_file_name: String,
    /// Post-Beta2 color depth
    pub post_beta2_color_depth: Option<ColorDepth>,
    /// Client product ID
    pub client_product_id: Option<u16>,
    /// Serial number
    pub serial_number: Option<u32>,
    /// High color depth
    pub high_color_depth: Option<HighColorDepth>,
    /// Supported color depths
    pub supported_color_depths: Option<u16>,
    /// Early capability flags
    pub early_capability_flags: Option<u16>,
    /// Client dig product ID
    pub client_dig_product_id: Option<String>,
    /// Connection type
    pub connection_type: Option<u8>,
    /// Server selected protocol
    pub server_selected_protocol: Option<u32>,
}

/// Color depth values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ColorDepth {
    Bpp4 = 0xCA00,
    Bpp8 = 0xCA01,
}

/// High color depth values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum HighColorDepth {
    Bpp4 = 0x0004,
    Bpp8 = 0x0008,
    Bpp15 = 0x000F,
    Bpp16 = 0x0010,
    Bpp24 = 0x0018,
}

impl ClientCoreData {
    /// Parse client core data
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.len() < 132 {
            return Err(RdpError::InsufficientData);
        }

        let mut cursor = Cursor::new(data);

        let version = cursor.read_u32_le()?;
        let desktop_width = cursor.read_u16_le()?;
        let desktop_height = cursor.read_u16_le()?;
        let color_depth_raw = cursor.read_u16_le()?;
        let color_depth = match color_depth_raw {
            0xCA00 => ColorDepth::Bpp4,
            _ => ColorDepth::Bpp8,
        };
        let sas_sequence = cursor.read_u16_le()?;
        let keyboard_layout = cursor.read_u32_le()?;
        let client_build = cursor.read_u32_le()?;

        // Client name (32 bytes, UTF-16LE, null-terminated)
        let name_bytes = cursor.read_bytes(32)?;
        let client_name = decode_utf16le_string(name_bytes);

        let keyboard_type = cursor.read_u32_le()?;
        let keyboard_sub_type = cursor.read_u32_le()?;
        let keyboard_fn_keys = cursor.read_u32_le()?;

        // IME file name (64 bytes)
        let ime_bytes = cursor.read_bytes(64)?;
        let ime_file_name = decode_utf16le_string(ime_bytes);

        // Optional fields
        let post_beta2_color_depth = if cursor.remaining() >= 2 {
            let val = cursor.read_u16_le()?;
            Some(match val {
                0xCA00 => ColorDepth::Bpp4,
                _ => ColorDepth::Bpp8,
            })
        } else {
            None
        };

        let client_product_id = if cursor.remaining() >= 2 {
            Some(cursor.read_u16_le()?)
        } else {
            None
        };

        let serial_number = if cursor.remaining() >= 4 {
            Some(cursor.read_u32_le()?)
        } else {
            None
        };

        let high_color_depth = if cursor.remaining() >= 2 {
            let val = cursor.read_u16_le()?;
            Some(match val {
                0x0004 => HighColorDepth::Bpp4,
                0x0008 => HighColorDepth::Bpp8,
                0x000F => HighColorDepth::Bpp15,
                0x0010 => HighColorDepth::Bpp16,
                _ => HighColorDepth::Bpp24,
            })
        } else {
            None
        };

        let supported_color_depths = if cursor.remaining() >= 2 {
            Some(cursor.read_u16_le()?)
        } else {
            None
        };

        let early_capability_flags = if cursor.remaining() >= 2 {
            Some(cursor.read_u16_le()?)
        } else {
            None
        };

        // Skip client dig product ID (64 bytes) and connection type
        let client_dig_product_id = if cursor.remaining() >= 64 {
            let bytes = cursor.read_bytes(64)?;
            Some(decode_utf16le_string(bytes))
        } else {
            None
        };

        let connection_type = if cursor.remaining() >= 1 {
            Some(cursor.read_u8()?)
        } else {
            None
        };

        // Skip pad
        if cursor.remaining() >= 1 {
            cursor.skip(1)?;
        }

        let server_selected_protocol = if cursor.remaining() >= 4 {
            Some(cursor.read_u32_le()?)
        } else {
            None
        };

        Ok(Self {
            version,
            desktop_width,
            desktop_height,
            color_depth,
            sas_sequence,
            keyboard_layout,
            client_build,
            client_name,
            keyboard_type,
            keyboard_sub_type,
            keyboard_fn_keys,
            ime_file_name,
            post_beta2_color_depth,
            client_product_id,
            serial_number,
            high_color_depth,
            supported_color_depths,
            early_capability_flags,
            client_dig_product_id,
            connection_type,
            server_selected_protocol,
        })
    }
}

/// Client security data
#[derive(Debug, Clone, Copy)]
pub struct ClientSecurityData {
    /// Encryption methods supported
    pub encryption_methods: u32,
    /// External encryption methods
    pub ext_encryption_methods: u32,
}

impl ClientSecurityData {
    /// Parse client security data
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.len() < 8 {
            return Err(RdpError::InsufficientData);
        }

        let mut cursor = Cursor::new(data);
        let encryption_methods = cursor.read_u32_le()?;
        let ext_encryption_methods = cursor.read_u32_le()?;

        Ok(Self {
            encryption_methods,
            ext_encryption_methods,
        })
    }
}

/// Client network data (virtual channel list)
#[derive(Debug, Clone)]
pub struct ClientNetworkData {
    /// Requested channels
    pub channels: Vec<ChannelDef>,
}

/// Channel definition
#[derive(Debug, Clone)]
pub struct ChannelDef {
    /// Channel name (up to 8 chars)
    pub name: String,
    /// Channel options
    pub options: u32,
}

impl ClientNetworkData {
    /// Parse client network data
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.len() < 4 {
            return Err(RdpError::InsufficientData);
        }

        let mut cursor = Cursor::new(data);
        let channel_count = cursor.read_u32_le()? as usize;

        let mut channels = Vec::with_capacity(channel_count);
        for _ in 0..channel_count {
            if cursor.remaining() < 12 {
                break;
            }

            // Channel name (8 bytes, null-terminated ASCII)
            let name_bytes = cursor.read_bytes(8)?;
            let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
            let name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();

            let options = cursor.read_u32_le()?;

            channels.push(ChannelDef { name, options });
        }

        Ok(Self { channels })
    }
}

/// Client cluster data
#[derive(Debug, Clone, Copy)]
pub struct ClientClusterData {
    /// Flags
    pub flags: u32,
    /// Redirected session ID
    pub redirected_session_id: u32,
}

impl ClientClusterData {
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.len() < 8 {
            return Err(RdpError::InsufficientData);
        }

        let mut cursor = Cursor::new(data);
        let flags = cursor.read_u32_le()?;
        let redirected_session_id = cursor.read_u32_le()?;

        Ok(Self {
            flags,
            redirected_session_id,
        })
    }
}

/// Server core data
#[derive(Debug, Clone)]
pub struct ServerCoreData {
    /// RDP version
    pub version: u32,
    /// Client requested protocols
    pub client_requested_protocols: Option<u32>,
    /// Early capability flags
    pub early_capability_flags: Option<u32>,
}

impl ServerCoreData {
    /// Encode server core data
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();

        // Header
        writer.write_u16_le(block_types::SC_CORE);
        let len_pos = writer.len();
        writer.write_u16_le(0); // Placeholder for length

        // Version
        writer.write_u32_le(self.version);

        // Optional fields
        if let Some(protocols) = self.client_requested_protocols {
            writer.write_u32_le(protocols);
        }

        if let Some(flags) = self.early_capability_flags {
            writer.write_u32_le(flags);
        }

        // Update length
        let len = writer.len() as u16;
        writer.set_u16_be(len_pos, len.swap_bytes());

        writer.into_vec()
    }
}

/// Server security data
#[derive(Debug, Clone)]
pub struct ServerSecurityData {
    /// Encryption method
    pub encryption_method: u32,
    /// Encryption level
    pub encryption_level: u32,
    /// Server random (32 bytes, if encryption enabled)
    pub server_random: Option<[u8; 32]>,
    /// Server certificate (if encryption enabled)
    pub server_certificate: Option<Vec<u8>>,
}

impl ServerSecurityData {
    /// Encode server security data (no encryption)
    pub fn encode_no_encryption() -> Vec<u8> {
        let mut writer = Writer::new();

        // Header
        writer.write_u16_le(block_types::SC_SECURITY);
        writer.write_u16_le(12); // Length

        // Encryption method: None
        writer.write_u32_le(0);

        // Encryption level: None
        writer.write_u32_le(0);

        writer.into_vec()
    }

    /// Encode with encryption info
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();

        // Header
        writer.write_u16_le(block_types::SC_SECURITY);
        let len_pos = writer.len();
        writer.write_u16_le(0); // Placeholder

        writer.write_u32_le(self.encryption_method);
        writer.write_u32_le(self.encryption_level);

        if let (Some(random), Some(cert)) = (&self.server_random, &self.server_certificate) {
            // Server random length
            writer.write_u32_le(32);
            // Server certificate length
            writer.write_u32_le(cert.len() as u32);
            // Server random
            writer.write_bytes(random);
            // Server certificate
            writer.write_bytes(cert);
        }

        // Update length
        let len = writer.len() as u16;
        writer.set_u16_be(len_pos, len.swap_bytes());

        writer.into_vec()
    }
}

/// Server network data
#[derive(Debug, Clone)]
pub struct ServerNetworkData {
    /// I/O channel ID
    pub io_channel: u16,
    /// Channel IDs for virtual channels
    pub channel_ids: Vec<u16>,
}

impl ServerNetworkData {
    /// Encode server network data
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();

        // Header
        writer.write_u16_le(block_types::SC_NET);
        let channel_count = self.channel_ids.len() as u16;
        let len = 8 + channel_count * 2 + (channel_count % 2) * 2; // Pad to 4-byte boundary
        writer.write_u16_le(len);

        // MCS channel ID (I/O channel)
        writer.write_u16_le(self.io_channel);

        // Channel count
        writer.write_u16_le(channel_count);

        // Channel IDs
        for &id in &self.channel_ids {
            writer.write_u16_le(id);
        }

        // Padding
        if channel_count % 2 != 0 {
            writer.write_u16_le(0);
        }

        writer.into_vec()
    }
}

impl ConferenceCreateRequest {
    /// Parse a GCC Conference Create Request from MCS user data
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.len() < 23 {
            return Err(RdpError::InsufficientData);
        }

        let mut cursor = Cursor::new(data);

        // Skip GCC header (21 bytes of boilerplate)
        // Object identifier, connect PDU tag, etc.
        cursor.skip(21)?;

        // Conference name length (1 byte)
        let name_len = cursor.read_u8()? as usize;
        if name_len == 0 || cursor.remaining() < name_len + 2 {
            return Err(RdpError::InvalidProtocol);
        }

        // Conference name (BMP string)
        let name_bytes = cursor.read_bytes(name_len)?;
        let conference_name = String::from_utf8_lossy(name_bytes).into_owned();

        // Skip padding and user data length
        cursor.skip(2)?;

        // Parse user data blocks
        let mut user_data = Vec::new();

        while cursor.remaining() >= 4 {
            let type_id = cursor.read_u16_le()?;
            let block_len = cursor.read_u16_le()? as usize;

            if block_len < 4 || cursor.remaining() < block_len - 4 {
                break;
            }

            let block_data = cursor.read_bytes(block_len - 4)?;

            let block = match type_id {
                block_types::CS_CORE => {
                    UserDataBlock::ClientCore(ClientCoreData::parse(block_data)?)
                }
                block_types::CS_SECURITY => {
                    UserDataBlock::ClientSecurity(ClientSecurityData::parse(block_data)?)
                }
                block_types::CS_NET => {
                    UserDataBlock::ClientNetwork(ClientNetworkData::parse(block_data)?)
                }
                block_types::CS_CLUSTER => {
                    UserDataBlock::ClientCluster(ClientClusterData::parse(block_data)?)
                }
                _ => UserDataBlock::Unknown {
                    type_id,
                    data: block_data.to_vec(),
                },
            };

            user_data.push(block);
        }

        Ok(Self {
            conference_name,
            user_data,
        })
    }

    /// Get client core data if present
    pub fn client_core(&self) -> Option<&ClientCoreData> {
        self.user_data.iter().find_map(|block| {
            if let UserDataBlock::ClientCore(core) = block {
                Some(core)
            } else {
                None
            }
        })
    }

    /// Get client network data if present
    pub fn client_network(&self) -> Option<&ClientNetworkData> {
        self.user_data.iter().find_map(|block| {
            if let UserDataBlock::ClientNetwork(net) = block {
                Some(net)
            } else {
                None
            }
        })
    }
}

impl ConferenceCreateResponse {
    /// Encode a GCC Conference Create Response
    pub fn encode(&self) -> Vec<u8> {
        // Collect user data blocks
        let mut blocks = Writer::new();
        for block in &self.user_data {
            match block {
                UserDataBlock::ServerCore(core) => blocks.write_bytes(&core.encode()),
                UserDataBlock::ServerSecurity(sec) => blocks.write_bytes(&sec.encode()),
                UserDataBlock::ServerNetwork(net) => blocks.write_bytes(&net.encode()),
                _ => {}
            }
        }

        let blocks_data = blocks.into_vec();

        // Build GCC packet
        let mut writer = Writer::with_capacity(32 + blocks_data.len());

        // PER encoded header
        // Object identifier for T.124
        writer.write_bytes(&[0x00, 0x05, 0x00, 0x14]);

        // Connect PDU tag and length
        writer.write_u8(0x7C);

        // Length of remaining data (PER)
        let remaining = 2 + 2 + 2 + blocks_data.len();
        if remaining < 0x80 {
            writer.write_u8(remaining as u8);
        } else {
            writer.write_u8(0x80 | ((remaining >> 8) as u8 & 0x7F));
            writer.write_u8(remaining as u8);
        }

        // Result (success = 0)
        writer.write_u16_be(0x0000);

        // Connect ID
        writer.write_u16_be(0x0000);

        // User data length
        let ud_len = blocks_data.len();
        if ud_len < 0x80 {
            writer.write_u8(ud_len as u8);
        } else {
            writer.write_u8(0x80 | ((ud_len >> 8) as u8 & 0x7F));
            writer.write_u8(ud_len as u8);
        }

        // User data blocks
        writer.write_bytes(&blocks_data);

        writer.into_vec()
    }
}

/// Decode a UTF-16LE string from bytes
fn decode_utf16le_string(bytes: &[u8]) -> String {
    let u16_len = bytes.len() / 2;
    let mut chars = Vec::with_capacity(u16_len);

    for i in 0..u16_len {
        let code_unit = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
        if code_unit == 0 {
            break;
        }
        chars.push(code_unit);
    }

    String::from_utf16_lossy(&chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_network_encode() {
        let net = ServerNetworkData {
            io_channel: 1003,
            channel_ids: vec![1004, 1005],
        };

        let encoded = net.encode();
        assert!(encoded.len() >= 12);

        // Check header
        assert_eq!(u16::from_le_bytes([encoded[0], encoded[1]]), block_types::SC_NET);
    }
}
