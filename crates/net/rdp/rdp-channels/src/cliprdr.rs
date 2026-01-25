//! Clipboard Redirection (CLIPRDR) Channel
//!
//! Implements the MS-RDPECLIP protocol for bidirectional clipboard sharing
//! between the RDP client and server.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use rdp_traits::{RdpError, RdpResult, VirtualChannel};

/// Clipboard channel name
pub const CLIPRDR_CHANNEL_NAME: &str = "cliprdr";

/// CLIPRDR message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ClipMsgType {
    /// Clipboard Capabilities
    MonitorReady = 0x0001,
    /// Format List (announce available formats)
    FormatList = 0x0002,
    /// Format List Response
    FormatListResponse = 0x0003,
    /// Format Data Request
    FormatDataRequest = 0x0004,
    /// Format Data Response
    FormatDataResponse = 0x0005,
    /// Temporary Directory
    TempDirectory = 0x0006,
    /// Capabilities
    Capabilities = 0x0007,
    /// File Contents Request
    FileContentsRequest = 0x0008,
    /// File Contents Response
    FileContentsResponse = 0x0009,
    /// Lock Clipboard Data
    LockClipboardData = 0x000A,
    /// Unlock Clipboard Data
    UnlockClipboardData = 0x000B,
}

/// CLIPRDR message flags
pub mod msg_flags {
    pub const CB_RESPONSE_OK: u16 = 0x0001;
    pub const CB_RESPONSE_FAIL: u16 = 0x0002;
    pub const CB_ASCII_NAMES: u16 = 0x0004;
}

/// CLIPRDR capability flags
pub mod cap_flags {
    /// Use long format names
    pub const CB_USE_LONG_FORMAT_NAMES: u32 = 0x00000002;
    /// Stream file clip supported
    pub const CB_STREAM_FILECLIP_ENABLED: u32 = 0x00000004;
    /// File clip no file paths
    pub const CB_FILECLIP_NO_FILE_PATHS: u32 = 0x00000008;
    /// Can lock clipboard data
    pub const CB_CAN_LOCK_CLIPDATA: u32 = 0x00000010;
    /// Huge file support enabled
    pub const CB_HUGE_FILE_SUPPORT_ENABLED: u32 = 0x00000020;
}

/// Standard clipboard formats
pub mod formats {
    /// Plain text (CF_TEXT)
    pub const CF_TEXT: u32 = 1;
    /// Bitmap (CF_BITMAP)
    pub const CF_BITMAP: u32 = 2;
    /// Metafile (CF_METAFILEPICT)
    pub const CF_METAFILEPICT: u32 = 3;
    /// Unicode text (CF_UNICODETEXT)
    pub const CF_UNICODETEXT: u32 = 13;
    /// HTML Format
    pub const CF_HTML: u32 = 49353;
}

/// CLIPRDR PDU header
#[derive(Debug, Clone)]
pub struct ClipPduHeader {
    /// Message type
    pub msg_type: u16,
    /// Message flags
    pub msg_flags: u16,
    /// Data length (excluding header)
    pub data_len: u32,
}

impl ClipPduHeader {
    /// Header size
    pub const SIZE: usize = 8;

    /// Parse from bytes
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        if data.len() < Self::SIZE {
            return Err(RdpError::InsufficientData);
        }

        let msg_type = u16::from_le_bytes([data[0], data[1]]);
        let msg_flags = u16::from_le_bytes([data[2], data[3]]);
        let data_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

        Ok(Self {
            msg_type,
            msg_flags,
            data_len,
        })
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(Self::SIZE);
        data.extend_from_slice(&self.msg_type.to_le_bytes());
        data.extend_from_slice(&self.msg_flags.to_le_bytes());
        data.extend_from_slice(&self.data_len.to_le_bytes());
        data
    }
}

/// Clipboard format entry
#[derive(Debug, Clone)]
pub struct ClipboardFormat {
    /// Format ID
    pub format_id: u32,
    /// Format name (for custom formats)
    pub format_name: Option<String>,
}

/// Clipboard channel state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardState {
    /// Initial state
    Initial,
    /// Capabilities exchanged
    Ready,
    /// Waiting for format data
    WaitingForData,
}

/// Clipboard channel implementation
pub struct ClipboardChannel {
    /// Channel state
    state: ClipboardState,
    /// Server capabilities
    capabilities: u32,
    /// Client capabilities
    client_capabilities: u32,
    /// Available formats (local)
    local_formats: Vec<ClipboardFormat>,
    /// Remote formats (from client)
    remote_formats: Vec<ClipboardFormat>,
    /// Pending clipboard data
    pending_data: Option<Vec<u8>>,
    /// Pending format request
    pending_format_request: Option<u32>,
    /// Outgoing PDU queue
    outgoing: Vec<Vec<u8>>,
}

impl ClipboardChannel {
    /// Create a new clipboard channel
    pub fn new() -> Self {
        Self {
            state: ClipboardState::Initial,
            capabilities: cap_flags::CB_USE_LONG_FORMAT_NAMES,
            client_capabilities: 0,
            local_formats: Vec::new(),
            remote_formats: Vec::new(),
            pending_data: None,
            pending_format_request: None,
            outgoing: Vec::new(),
        }
    }

    /// Get current state
    pub fn state(&self) -> ClipboardState {
        self.state
    }

    /// Set local clipboard content (text)
    pub fn set_clipboard_text(&mut self, text: &str) {
        // Store as Unicode text
        let text_bytes = encode_utf16le(text);
        self.pending_data = Some(text_bytes);

        // Set available formats
        self.local_formats = vec![
            ClipboardFormat {
                format_id: formats::CF_UNICODETEXT,
                format_name: None,
            },
            ClipboardFormat {
                format_id: formats::CF_TEXT,
                format_name: None,
            },
        ];

        // Send format list if ready
        if self.state == ClipboardState::Ready {
            self.send_format_list();
        }
    }

    /// Get clipboard text (if available)
    pub fn get_clipboard_text(&self) -> Option<String> {
        self.pending_data
            .as_ref()
            .map(|data| decode_utf16le(data))
    }

    /// Request clipboard data from client
    pub fn request_clipboard_data(&mut self, format_id: u32) {
        if self.state != ClipboardState::Ready {
            return;
        }

        self.pending_format_request = Some(format_id);
        self.send_format_data_request(format_id);
        self.state = ClipboardState::WaitingForData;
    }

    /// Send format list to client
    fn send_format_list(&mut self) {
        let mut data = Vec::new();

        for format in &self.local_formats {
            // Format ID
            data.extend_from_slice(&format.format_id.to_le_bytes());

            // Format name (null-terminated UTF-16LE)
            if let Some(ref name) = format.format_name {
                let name_bytes = encode_utf16le(name);
                data.extend_from_slice(&name_bytes);
            }
            // Null terminator (2 bytes for Unicode)
            data.extend_from_slice(&[0u8, 0]);
        }

        let header = ClipPduHeader {
            msg_type: ClipMsgType::FormatList as u16,
            msg_flags: 0,
            data_len: data.len() as u32,
        };

        let mut pdu = header.encode();
        pdu.extend_from_slice(&data);
        self.outgoing.push(pdu);
    }

    /// Send format list response
    fn send_format_list_response(&mut self, success: bool) {
        let header = ClipPduHeader {
            msg_type: ClipMsgType::FormatListResponse as u16,
            msg_flags: if success {
                msg_flags::CB_RESPONSE_OK
            } else {
                msg_flags::CB_RESPONSE_FAIL
            },
            data_len: 0,
        };

        self.outgoing.push(header.encode());
    }

    /// Send format data request
    fn send_format_data_request(&mut self, format_id: u32) {
        let header = ClipPduHeader {
            msg_type: ClipMsgType::FormatDataRequest as u16,
            msg_flags: 0,
            data_len: 4,
        };

        let mut pdu = header.encode();
        pdu.extend_from_slice(&format_id.to_le_bytes());
        self.outgoing.push(pdu);
    }

    /// Send format data response
    fn send_format_data_response(&mut self, format_id: u32) {
        let data = match format_id {
            formats::CF_UNICODETEXT => self.pending_data.clone().unwrap_or_default(),
            formats::CF_TEXT => {
                // Convert to ASCII
                self.pending_data
                    .as_ref()
                    .map(|d| decode_utf16le(d).into_bytes())
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };

        let header = ClipPduHeader {
            msg_type: ClipMsgType::FormatDataResponse as u16,
            msg_flags: if !data.is_empty() {
                msg_flags::CB_RESPONSE_OK
            } else {
                msg_flags::CB_RESPONSE_FAIL
            },
            data_len: data.len() as u32,
        };

        let mut pdu = header.encode();
        pdu.extend_from_slice(&data);
        self.outgoing.push(pdu);
    }

    /// Send server capabilities
    fn send_capabilities(&mut self) {
        // Capability set
        let mut caps = Vec::new();

        // CB_CAPSTYPE_GENERAL
        caps.extend_from_slice(&1u16.to_le_bytes()); // capabilitySetType
        caps.extend_from_slice(&12u16.to_le_bytes()); // lengthCapability
        caps.extend_from_slice(&2u32.to_le_bytes()); // version (CB_CAPS_VERSION_2)
        caps.extend_from_slice(&self.capabilities.to_le_bytes()); // generalFlags

        // Header for capabilities
        let header = ClipPduHeader {
            msg_type: ClipMsgType::Capabilities as u16,
            msg_flags: 0,
            data_len: (2 + caps.len()) as u32, // cCapabilitiesSets + caps
        };

        let mut pdu = header.encode();
        pdu.extend_from_slice(&1u16.to_le_bytes()); // cCapabilitiesSets
        pdu.extend_from_slice(&0u16.to_le_bytes()); // pad1
        pdu.extend_from_slice(&caps);

        self.outgoing.push(pdu);
    }

    /// Send monitor ready
    fn send_monitor_ready(&mut self) {
        let header = ClipPduHeader {
            msg_type: ClipMsgType::MonitorReady as u16,
            msg_flags: 0,
            data_len: 0,
        };

        self.outgoing.push(header.encode());
    }

    /// Process incoming monitor ready
    fn process_monitor_ready(&mut self, _data: &[u8]) -> RdpResult<()> {
        self.state = ClipboardState::Ready;

        // Send our format list if we have data
        if !self.local_formats.is_empty() {
            self.send_format_list();
        }

        Ok(())
    }

    /// Process incoming capabilities
    fn process_capabilities(&mut self, data: &[u8]) -> RdpResult<()> {
        if data.len() < 4 {
            return Ok(());
        }

        let _num_caps = u16::from_le_bytes([data[0], data[1]]);
        // Skip pad1

        // Parse capability sets (simplified - just extract general flags)
        if data.len() >= 16 {
            self.client_capabilities = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        }

        Ok(())
    }

    /// Process incoming format list
    fn process_format_list(&mut self, data: &[u8]) -> RdpResult<()> {
        self.remote_formats.clear();

        let mut offset = 0;
        while offset + 4 <= data.len() {
            let format_id = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            offset += 4;

            // Read format name (null-terminated UTF-16LE string)
            let name_start = offset;
            while offset + 2 <= data.len() {
                if data[offset] == 0 && data[offset + 1] == 0 {
                    break;
                }
                offset += 2;
            }

            let format_name = if offset > name_start {
                Some(decode_utf16le(&data[name_start..offset]))
            } else {
                None
            };

            // Skip null terminator
            offset += 2;

            self.remote_formats.push(ClipboardFormat {
                format_id,
                format_name,
            });
        }

        // Send response
        self.send_format_list_response(true);

        Ok(())
    }

    /// Process incoming format data request
    fn process_format_data_request(&mut self, data: &[u8]) -> RdpResult<()> {
        if data.len() < 4 {
            return Err(RdpError::InsufficientData);
        }

        let format_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

        // Send the data
        self.send_format_data_response(format_id);

        Ok(())
    }

    /// Process incoming format data response
    fn process_format_data_response(&mut self, data: &[u8], success: bool) -> RdpResult<()> {
        if success {
            // Store the received data
            self.pending_data = Some(data.to_vec());
        }

        self.state = ClipboardState::Ready;
        self.pending_format_request = None;

        Ok(())
    }
}

impl Default for ClipboardChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualChannel for ClipboardChannel {
    fn name(&self) -> &str {
        CLIPRDR_CHANNEL_NAME
    }

    fn on_receive(&mut self, data: &[u8]) -> RdpResult<()> {
        let header = ClipPduHeader::parse(data)?;
        let payload = &data[ClipPduHeader::SIZE..];

        match header.msg_type {
            x if x == ClipMsgType::MonitorReady as u16 => {
                self.process_monitor_ready(payload)
            }
            x if x == ClipMsgType::Capabilities as u16 => {
                self.process_capabilities(payload)
            }
            x if x == ClipMsgType::FormatList as u16 => {
                self.process_format_list(payload)
            }
            x if x == ClipMsgType::FormatListResponse as u16 => {
                // Acknowledged
                Ok(())
            }
            x if x == ClipMsgType::FormatDataRequest as u16 => {
                self.process_format_data_request(payload)
            }
            x if x == ClipMsgType::FormatDataResponse as u16 => {
                let success = header.msg_flags & msg_flags::CB_RESPONSE_OK != 0;
                self.process_format_data_response(payload, success)
            }
            _ => Ok(()), // Ignore unknown messages
        }
    }

    fn poll_send(&mut self) -> Option<Vec<u8>> {
        if !self.outgoing.is_empty() {
            Some(self.outgoing.remove(0))
        } else {
            None
        }
    }

    fn on_connect(&mut self) {
        self.state = ClipboardState::Initial;
        self.outgoing.clear();

        // Send server capabilities
        self.send_capabilities();

        // Send monitor ready
        self.send_monitor_ready();
    }

    fn on_disconnect(&mut self) {
        self.state = ClipboardState::Initial;
        self.outgoing.clear();
        self.remote_formats.clear();
        self.pending_data = None;
        self.pending_format_request = None;
    }
}

/// Encode a string as UTF-16LE
fn encode_utf16le(s: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(s.len() * 2 + 2);
    for c in s.encode_utf16() {
        result.extend_from_slice(&c.to_le_bytes());
    }
    // Add null terminator
    result.extend_from_slice(&[0, 0]);
    result
}

/// Decode a UTF-16LE string
fn decode_utf16le(data: &[u8]) -> String {
    let mut chars = Vec::with_capacity(data.len() / 2);

    for i in (0..data.len()).step_by(2) {
        if i + 1 >= data.len() {
            break;
        }

        let code_unit = u16::from_le_bytes([data[i], data[i + 1]]);
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
    fn test_utf16_encode_decode() {
        let original = "Hello, World!";
        let encoded = encode_utf16le(original);
        let decoded = decode_utf16le(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_clipboard_header() {
        let header = ClipPduHeader {
            msg_type: ClipMsgType::FormatList as u16,
            msg_flags: msg_flags::CB_ASCII_NAMES,
            data_len: 100,
        };

        let encoded = header.encode();
        let decoded = ClipPduHeader::parse(&encoded).unwrap();

        assert_eq!(decoded.msg_type, header.msg_type);
        assert_eq!(decoded.msg_flags, header.msg_flags);
        assert_eq!(decoded.data_len, header.data_len);
    }
}
