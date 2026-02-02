//! RDP PDU (Protocol Data Unit) Definitions
//!
//! Application-layer messages for RDP connection finalization,
//! capability exchange, and data transfer.

use crate::{Cursor, Writer};
use alloc::string::String;
use alloc::vec::Vec;
use rdp_traits::{CapabilityType, DataPduType, PduType, RdpError, RdpResult, UpdateType};

/// Share control header
#[derive(Debug, Clone, Copy)]
pub struct ShareControlHeader {
    /// Total length including header
    pub total_length: u16,
    /// PDU type
    pub pdu_type: u16,
    /// PDU source (user ID + 1001)
    pub pdu_source: u16,
}

impl ShareControlHeader {
    /// Header size
    pub const SIZE: usize = 6;

    /// Parse share control header
    pub fn parse(cursor: &mut Cursor<'_>) -> RdpResult<Self> {
        let total_length = cursor.read_u16_le()?;
        let pdu_type = cursor.read_u16_le()?;
        let pdu_source = cursor.read_u16_le()?;

        Ok(Self {
            total_length,
            pdu_type,
            pdu_source,
        })
    }

    /// Write share control header
    pub fn write(&self, writer: &mut Writer) {
        writer.write_u16_le(self.total_length);
        writer.write_u16_le(self.pdu_type);
        writer.write_u16_le(self.pdu_source);
    }
}

/// Share data header
#[derive(Debug, Clone, Copy)]
pub struct ShareDataHeader {
    /// Share ID
    pub share_id: u32,
    /// Padding
    pub pad1: u8,
    /// Stream ID
    pub stream_id: u8,
    /// Uncompressed length
    pub uncompressed_length: u16,
    /// PDU type
    pub pdu_type: u8,
    /// Compression type
    pub compression_type: u8,
    /// Compressed length
    pub compressed_length: u16,
}

impl ShareDataHeader {
    /// Header size
    pub const SIZE: usize = 12;

    /// Stream ID for low priority
    pub const STREAM_LOW: u8 = 0x01;
    /// Stream ID for medium priority
    pub const STREAM_MED: u8 = 0x02;
    /// Stream ID for high priority
    pub const STREAM_HI: u8 = 0x04;

    /// Parse share data header
    pub fn parse(cursor: &mut Cursor<'_>) -> RdpResult<Self> {
        let share_id = cursor.read_u32_le()?;
        let pad1 = cursor.read_u8()?;
        let stream_id = cursor.read_u8()?;
        let uncompressed_length = cursor.read_u16_le()?;
        let pdu_type = cursor.read_u8()?;
        let compression_type = cursor.read_u8()?;
        let compressed_length = cursor.read_u16_le()?;

        Ok(Self {
            share_id,
            pad1,
            stream_id,
            uncompressed_length,
            pdu_type,
            compression_type,
            compressed_length,
        })
    }

    /// Write share data header
    pub fn write(&self, writer: &mut Writer) {
        writer.write_u32_le(self.share_id);
        writer.write_u8(self.pad1);
        writer.write_u8(self.stream_id);
        writer.write_u16_le(self.uncompressed_length);
        writer.write_u8(self.pdu_type);
        writer.write_u8(self.compression_type);
        writer.write_u16_le(self.compressed_length);
    }
}

/// Demand Active PDU
#[derive(Debug, Clone)]
pub struct DemandActivePdu {
    /// Share ID
    pub share_id: u32,
    /// Length of source descriptor
    pub source_descriptor_len: u16,
    /// Combined length of capability sets
    pub capabilities_len: u16,
    /// Source descriptor
    pub source_descriptor: String,
    /// Number of capability sets
    pub num_capabilities: u16,
    /// Capability sets
    pub capabilities: Vec<CapabilitySet>,
    /// Session ID
    pub session_id: u32,
}

impl DemandActivePdu {
    /// Encode a Demand Active PDU
    pub fn encode(&self, source: u16) -> Vec<u8> {
        // Build capability data
        let mut caps_data = Writer::new();
        for cap in &self.capabilities {
            cap.write(&mut caps_data);
        }
        let caps_bytes = caps_data.into_vec();

        // Source descriptor as bytes
        let source_desc = self.source_descriptor.as_bytes();

        // Calculate payload size
        let payload_size = 4 + // share ID
            2 + // source descriptor length
            2 + // capabilities length
            source_desc.len() + 1 + // source descriptor + null
            2 + 2 + // num capabilities + padding
            caps_bytes.len() +
            4; // session ID

        // Build full PDU
        let total_len = ShareControlHeader::SIZE + payload_size;
        let mut writer = Writer::with_capacity(total_len);

        // Share control header
        let header = ShareControlHeader {
            total_length: total_len as u16,
            pdu_type: PduType::DemandActive as u16,
            pdu_source: source,
        };
        header.write(&mut writer);

        // Demand Active payload
        writer.write_u32_le(self.share_id);
        writer.write_u16_le((source_desc.len() + 1) as u16);
        writer.write_u16_le(caps_bytes.len() as u16);
        writer.write_bytes(source_desc);
        writer.write_u8(0); // null terminator
        writer.write_u16_le(self.num_capabilities);
        writer.write_u16_le(0); // padding
        writer.write_bytes(&caps_bytes);
        writer.write_u32_le(self.session_id);

        writer.into_vec()
    }
}

/// Confirm Active PDU (received from client)
#[derive(Debug, Clone)]
pub struct ConfirmActivePdu {
    /// Share ID
    pub share_id: u32,
    /// Originator ID
    pub originator_id: u16,
    /// Source descriptor
    pub source_descriptor: String,
    /// Capability sets
    pub capabilities: Vec<CapabilitySet>,
}

impl ConfirmActivePdu {
    /// Parse a Confirm Active PDU
    pub fn parse(data: &[u8]) -> RdpResult<Self> {
        let mut cursor = Cursor::new(data);

        // Skip share control header (already parsed)
        // We expect data to start at the payload

        let share_id = cursor.read_u32_le()?;
        let originator_id = cursor.read_u16_le()?;
        let source_desc_len = cursor.read_u16_le()? as usize;
        let _caps_len = cursor.read_u16_le()? as usize;

        // Source descriptor
        let source_bytes = cursor.read_bytes(source_desc_len)?;
        let source_descriptor = String::from_utf8_lossy(source_bytes).into_owned();

        // Number of capabilities
        let num_caps = cursor.read_u16_le()?;
        let _pad = cursor.read_u16_le()?;

        // Parse capabilities
        let mut capabilities = Vec::with_capacity(num_caps as usize);
        for _ in 0..num_caps {
            if cursor.remaining() < 4 {
                break;
            }
            capabilities.push(CapabilitySet::parse(&mut cursor)?);
        }

        Ok(Self {
            share_id,
            originator_id,
            source_descriptor,
            capabilities,
        })
    }
}

/// Capability set
#[derive(Debug, Clone)]
pub struct CapabilitySet {
    /// Capability type
    pub cap_type: u16,
    /// Capability data
    pub data: Vec<u8>,
}

impl CapabilitySet {
    /// Parse a capability set
    pub fn parse(cursor: &mut Cursor<'_>) -> RdpResult<Self> {
        let cap_type = cursor.read_u16_le()?;
        let length = cursor.read_u16_le()? as usize;

        if length < 4 {
            return Err(RdpError::InvalidProtocol);
        }

        let data_len = length - 4;
        let data = cursor.read_bytes(data_len)?.to_vec();

        Ok(Self { cap_type, data })
    }

    /// Write a capability set
    pub fn write(&self, writer: &mut Writer) {
        writer.write_u16_le(self.cap_type);
        writer.write_u16_le((4 + self.data.len()) as u16);
        writer.write_bytes(&self.data);
    }
}

/// General capability set
#[derive(Debug, Clone)]
pub struct GeneralCapability {
    /// OS major type
    pub os_major_type: u16,
    /// OS minor type
    pub os_minor_type: u16,
    /// Protocol version
    pub protocol_version: u16,
    /// General compression types
    pub general_compression_types: u16,
    /// Extra flags
    pub extra_flags: u16,
    /// Update capability flags
    pub update_capability_flags: u16,
    /// Remote unshare
    pub remote_unshare: u16,
    /// General compression level
    pub general_compression_level: u16,
    /// Refresh rect support
    pub refresh_rect_support: u8,
    /// Suppress output support
    pub suppress_output_support: u8,
}

impl GeneralCapability {
    /// OS major type: Windows
    pub const OS_MAJOR_TYPE_WINDOWS: u16 = 1;
    /// OS minor type: Windows NT
    pub const OS_MINOR_TYPE_WINDOWS_NT: u16 = 3;

    /// Create default server general capability
    pub fn default_server() -> Self {
        Self {
            os_major_type: Self::OS_MAJOR_TYPE_WINDOWS,
            os_minor_type: Self::OS_MINOR_TYPE_WINDOWS_NT,
            protocol_version: 0x0200,
            general_compression_types: 0,
            extra_flags: 0x0001 | 0x0004 | 0x0100, // FASTPATH_OUTPUT_SUPPORTED | NO_BITMAP_COMPRESSION_HDR | LONG_CREDENTIALS
            update_capability_flags: 0,
            remote_unshare: 0,
            general_compression_level: 0,
            refresh_rect_support: 1,
            suppress_output_support: 1,
        }
    }

    /// Encode to capability set
    pub fn encode(&self) -> CapabilitySet {
        let mut writer = Writer::with_capacity(24);

        writer.write_u16_le(self.os_major_type);
        writer.write_u16_le(self.os_minor_type);
        writer.write_u16_le(self.protocol_version);
        writer.write_u16_le(0); // padding
        writer.write_u16_le(self.general_compression_types);
        writer.write_u16_le(self.extra_flags);
        writer.write_u16_le(self.update_capability_flags);
        writer.write_u16_le(self.remote_unshare);
        writer.write_u16_le(self.general_compression_level);
        writer.write_u8(self.refresh_rect_support);
        writer.write_u8(self.suppress_output_support);

        CapabilitySet {
            cap_type: CapabilityType::General as u16,
            data: writer.into_vec(),
        }
    }
}

/// Bitmap capability set
#[derive(Debug, Clone)]
pub struct BitmapCapability {
    /// Preferred bits per pixel
    pub preferred_bpp: u16,
    /// Receive 1 BPP
    pub receive_1bpp: u16,
    /// Receive 4 BPP
    pub receive_4bpp: u16,
    /// Receive 8 BPP
    pub receive_8bpp: u16,
    /// Desktop width
    pub desktop_width: u16,
    /// Desktop height
    pub desktop_height: u16,
    /// Desktop resize
    pub desktop_resize: u16,
    /// Bitmap compression
    pub bitmap_compression: u16,
    /// High color flags
    pub high_color_flags: u8,
    /// Drawing flags
    pub drawing_flags: u8,
    /// Multiple rectangle support
    pub multiple_rect_support: u16,
}

impl BitmapCapability {
    /// Create bitmap capability for given resolution
    pub fn new(width: u16, height: u16, bpp: u16) -> Self {
        Self {
            preferred_bpp: bpp,
            receive_1bpp: 1,
            receive_4bpp: 1,
            receive_8bpp: 1,
            desktop_width: width,
            desktop_height: height,
            desktop_resize: 1,
            bitmap_compression: 1,
            high_color_flags: 0,
            drawing_flags: 0x08 | 0x20, // ALLOW_SKIP_ALPHA | ALLOW_DYNAMIC_COLOR_FIDELITY
            multiple_rect_support: 1,
        }
    }

    /// Encode to capability set
    pub fn encode(&self) -> CapabilitySet {
        let mut writer = Writer::with_capacity(28);

        writer.write_u16_le(self.preferred_bpp);
        writer.write_u16_le(self.receive_1bpp);
        writer.write_u16_le(self.receive_4bpp);
        writer.write_u16_le(self.receive_8bpp);
        writer.write_u16_le(self.desktop_width);
        writer.write_u16_le(self.desktop_height);
        writer.write_u16_le(0); // padding
        writer.write_u16_le(self.desktop_resize);
        writer.write_u16_le(self.bitmap_compression);
        writer.write_u8(self.high_color_flags);
        writer.write_u8(self.drawing_flags);
        writer.write_u16_le(self.multiple_rect_support);
        writer.write_u16_le(0); // padding

        CapabilitySet {
            cap_type: CapabilityType::Bitmap as u16,
            data: writer.into_vec(),
        }
    }
}

/// Order capability set
#[derive(Debug, Clone)]
pub struct OrderCapability {
    /// Terminal descriptor
    pub terminal_descriptor: [u8; 16],
    /// Desktop save X granularity
    pub desktop_save_x_granularity: u32,
    /// Desktop save Y granularity
    pub desktop_save_y_granularity: u32,
    /// Maximum order level
    pub max_order_level: u16,
    /// Number of fonts
    pub number_fonts: u16,
    /// Order flags
    pub order_flags: u16,
    /// Order support (32 bytes bitmap)
    pub order_support: [u8; 32],
    /// Text flags
    pub text_flags: u16,
    /// Order support extra flags
    pub order_support_extra_flags: u16,
    /// Desktop save size
    pub desktop_save_size: u32,
    /// Text ANSI code page
    pub text_ansi_code_page: u16,
}

impl OrderCapability {
    /// Create default (no drawing orders - use bitmap updates)
    pub fn default_server() -> Self {
        Self {
            terminal_descriptor: [0; 16],
            desktop_save_x_granularity: 1,
            desktop_save_y_granularity: 20,
            max_order_level: 1,
            number_fonts: 0,
            order_flags: 0x0022, // NEGOTIATE_ORDER_SUPPORT | ZERO_BOUNDS_DELTAS
            order_support: [0; 32], // No orders supported
            text_flags: 0,
            order_support_extra_flags: 0,
            desktop_save_size: 0,
            text_ansi_code_page: 0,
        }
    }

    /// Encode to capability set
    pub fn encode(&self) -> CapabilitySet {
        let mut writer = Writer::with_capacity(88);

        writer.write_bytes(&self.terminal_descriptor);
        writer.write_u32_le(0); // padding
        writer.write_u16_le(1); // desktop save X
        writer.write_u16_le(20); // desktop save Y
        writer.write_u16_le(0); // padding
        writer.write_u16_le(self.max_order_level);
        writer.write_u16_le(self.number_fonts);
        writer.write_u16_le(self.order_flags);
        writer.write_bytes(&self.order_support);
        writer.write_u16_le(self.text_flags);
        writer.write_u16_le(self.order_support_extra_flags);
        writer.write_u32_le(0); // padding
        writer.write_u32_le(self.desktop_save_size);
        writer.write_u16_le(0); // padding
        writer.write_u16_le(0); // padding
        writer.write_u16_le(self.text_ansi_code_page);
        writer.write_u16_le(0); // padding

        CapabilitySet {
            cap_type: CapabilityType::Order as u16,
            data: writer.into_vec(),
        }
    }
}

/// Pointer capability set
#[derive(Debug, Clone)]
pub struct PointerCapability {
    /// Color pointer flag
    pub color_pointer_flag: u16,
    /// Color pointer cache size
    pub color_pointer_cache_size: u16,
    /// Pointer cache size
    pub pointer_cache_size: u16,
}

impl PointerCapability {
    /// Create default pointer capability
    pub fn default_server() -> Self {
        Self {
            color_pointer_flag: 1,
            color_pointer_cache_size: 25,
            pointer_cache_size: 25,
        }
    }

    /// Encode to capability set
    pub fn encode(&self) -> CapabilitySet {
        let mut writer = Writer::with_capacity(8);

        writer.write_u16_le(self.color_pointer_flag);
        writer.write_u16_le(self.color_pointer_cache_size);
        writer.write_u16_le(self.pointer_cache_size);

        CapabilitySet {
            cap_type: CapabilityType::Pointer as u16,
            data: writer.into_vec(),
        }
    }
}

/// Input capability set
#[derive(Debug, Clone)]
pub struct InputCapability {
    /// Input flags
    pub input_flags: u16,
    /// Keyboard layout
    pub keyboard_layout: u32,
    /// Keyboard type
    pub keyboard_type: u32,
    /// Keyboard subtype
    pub keyboard_sub_type: u32,
    /// Keyboard function key
    pub keyboard_fn_keys: u32,
    /// IME file name
    pub ime_file_name: [u8; 64],
}

impl InputCapability {
    /// Create default input capability
    pub fn default_server() -> Self {
        Self {
            input_flags: 0x0001 | 0x0004 | 0x0010 | 0x0020, // SCANCODES | MOUSEX | FASTPATH_INPUT | UNICODE
            keyboard_layout: 0,
            keyboard_type: 4, // Enhanced 101/102 key
            keyboard_sub_type: 0,
            keyboard_fn_keys: 12,
            ime_file_name: [0; 64],
        }
    }

    /// Encode to capability set
    pub fn encode(&self) -> CapabilitySet {
        let mut writer = Writer::with_capacity(88);

        writer.write_u16_le(self.input_flags);
        writer.write_u16_le(0); // padding
        writer.write_u32_le(self.keyboard_layout);
        writer.write_u32_le(self.keyboard_type);
        writer.write_u32_le(self.keyboard_sub_type);
        writer.write_u32_le(self.keyboard_fn_keys);
        writer.write_bytes(&self.ime_file_name);

        CapabilitySet {
            cap_type: CapabilityType::Input as u16,
            data: writer.into_vec(),
        }
    }
}

/// Virtual channel capability
#[derive(Debug, Clone)]
pub struct VirtualChannelCapability {
    /// Flags
    pub flags: u32,
    /// VCChunk size (optional)
    pub vc_chunk_size: Option<u32>,
}

impl VirtualChannelCapability {
    /// Create default virtual channel capability
    pub fn default_server() -> Self {
        Self {
            flags: 0, // No compression
            vc_chunk_size: Some(1600),
        }
    }

    /// Encode to capability set
    pub fn encode(&self) -> CapabilitySet {
        let mut writer = Writer::with_capacity(8);

        writer.write_u32_le(self.flags);
        if let Some(chunk_size) = self.vc_chunk_size {
            writer.write_u32_le(chunk_size);
        }

        CapabilitySet {
            cap_type: CapabilityType::VirtualChannel as u16,
            data: writer.into_vec(),
        }
    }
}

/// Share capability
#[derive(Debug, Clone)]
pub struct ShareCapability {
    /// Node ID
    pub node_id: u16,
}

impl ShareCapability {
    pub fn encode(&self) -> CapabilitySet {
        let mut writer = Writer::with_capacity(4);
        writer.write_u16_le(self.node_id);
        writer.write_u16_le(0); // padding

        CapabilitySet {
            cap_type: CapabilityType::Share as u16,
            data: writer.into_vec(),
        }
    }
}

/// Synchronize PDU
#[derive(Debug, Clone)]
pub struct SynchronizePdu {
    /// Message type (always 1)
    pub message_type: u16,
    /// Target user
    pub target_user: u16,
}

impl SynchronizePdu {
    /// Encode synchronize PDU
    pub fn encode(&self, share_id: u32, source: u16) -> Vec<u8> {
        let payload_len = 4;
        let data_len = ShareDataHeader::SIZE + payload_len;
        let total_len = ShareControlHeader::SIZE + data_len;

        let mut writer = Writer::with_capacity(total_len);

        // Share control header
        let ctrl_header = ShareControlHeader {
            total_length: total_len as u16,
            pdu_type: PduType::Data as u16,
            pdu_source: source,
        };
        ctrl_header.write(&mut writer);

        // Share data header
        let data_header = ShareDataHeader {
            share_id,
            pad1: 0,
            stream_id: ShareDataHeader::STREAM_LOW,
            uncompressed_length: payload_len as u16,
            pdu_type: DataPduType::Synchronize as u8,
            compression_type: 0,
            compressed_length: 0,
        };
        data_header.write(&mut writer);

        // Payload
        writer.write_u16_le(self.message_type);
        writer.write_u16_le(self.target_user);

        writer.into_vec()
    }
}

/// Control PDU
#[derive(Debug, Clone)]
pub struct ControlPdu {
    /// Action
    pub action: u16,
    /// Grant ID
    pub grant_id: u16,
    /// Control ID
    pub control_id: u32,
}

impl ControlPdu {
    /// Cooperate action
    pub const ACTION_COOPERATE: u16 = 0x0004;
    /// Granted control action
    pub const ACTION_GRANTED_CONTROL: u16 = 0x0002;

    /// Encode control PDU
    pub fn encode(&self, share_id: u32, source: u16) -> Vec<u8> {
        let payload_len = 8;
        let data_len = ShareDataHeader::SIZE + payload_len;
        let total_len = ShareControlHeader::SIZE + data_len;

        let mut writer = Writer::with_capacity(total_len);

        // Share control header
        let ctrl_header = ShareControlHeader {
            total_length: total_len as u16,
            pdu_type: PduType::Data as u16,
            pdu_source: source,
        };
        ctrl_header.write(&mut writer);

        // Share data header
        let data_header = ShareDataHeader {
            share_id,
            pad1: 0,
            stream_id: ShareDataHeader::STREAM_MED,
            uncompressed_length: payload_len as u16,
            pdu_type: DataPduType::Control as u8,
            compression_type: 0,
            compressed_length: 0,
        };
        data_header.write(&mut writer);

        // Payload
        writer.write_u16_le(self.action);
        writer.write_u16_le(self.grant_id);
        writer.write_u32_le(self.control_id);

        writer.into_vec()
    }
}

/// Font List PDU
#[derive(Debug, Clone)]
pub struct FontListPdu {
    /// Number of fonts
    pub num_fonts: u16,
    /// Total number of fonts
    pub total_fonts: u16,
    /// List flags
    pub list_flags: u16,
    /// Entry size
    pub entry_size: u16,
}

/// Font Map PDU (server response to font list)
#[derive(Debug, Clone)]
pub struct FontMapPdu {
    /// Number of entries
    pub num_entries: u16,
    /// Total number of entries
    pub total_entries: u16,
    /// Map flags
    pub map_flags: u16,
    /// Entry size
    pub entry_size: u16,
}

impl FontMapPdu {
    /// Encode font map PDU
    pub fn encode(&self, share_id: u32, source: u16) -> Vec<u8> {
        let payload_len = 8;
        let data_len = ShareDataHeader::SIZE + payload_len;
        let total_len = ShareControlHeader::SIZE + data_len;

        let mut writer = Writer::with_capacity(total_len);

        // Share control header
        let ctrl_header = ShareControlHeader {
            total_length: total_len as u16,
            pdu_type: PduType::Data as u16,
            pdu_source: source,
        };
        ctrl_header.write(&mut writer);

        // Share data header
        let data_header = ShareDataHeader {
            share_id,
            pad1: 0,
            stream_id: ShareDataHeader::STREAM_LOW,
            uncompressed_length: payload_len as u16,
            pdu_type: DataPduType::FontMap as u8,
            compression_type: 0,
            compressed_length: 0,
        };
        data_header.write(&mut writer);

        // Payload
        writer.write_u16_le(self.num_entries);
        writer.write_u16_le(self.total_entries);
        writer.write_u16_le(self.map_flags);
        writer.write_u16_le(self.entry_size);

        writer.into_vec()
    }
}

/// Bitmap update
#[derive(Debug, Clone)]
pub struct BitmapUpdate {
    /// Update rectangles
    pub rectangles: Vec<BitmapRectangle>,
}

/// Bitmap rectangle data
#[derive(Debug, Clone)]
pub struct BitmapRectangle {
    /// Destination left
    pub dest_left: u16,
    /// Destination top
    pub dest_top: u16,
    /// Destination right
    pub dest_right: u16,
    /// Destination bottom
    pub dest_bottom: u16,
    /// Width
    pub width: u16,
    /// Height
    pub height: u16,
    /// Bits per pixel
    pub bpp: u16,
    /// Compression flags
    pub flags: u16,
    /// Bitmap data
    pub data: Vec<u8>,
}

impl BitmapRectangle {
    /// Compressed flag
    pub const BITMAP_COMPRESSION: u16 = 0x0001;
    /// No bitmap compression header
    pub const NO_BITMAP_COMPRESSION_HDR: u16 = 0x0400;
}

impl BitmapUpdate {
    /// Encode bitmap update as a slow-path update PDU
    pub fn encode(&self, share_id: u32, source: u16) -> Vec<u8> {
        // First encode the bitmap data
        let mut bitmap_data = Writer::new();
        bitmap_data.write_u16_le(self.rectangles.len() as u16);

        for rect in &self.rectangles {
            bitmap_data.write_u16_le(rect.dest_left);
            bitmap_data.write_u16_le(rect.dest_top);
            bitmap_data.write_u16_le(rect.dest_right);
            bitmap_data.write_u16_le(rect.dest_bottom);
            bitmap_data.write_u16_le(rect.width);
            bitmap_data.write_u16_le(rect.height);
            bitmap_data.write_u16_le(rect.bpp);
            bitmap_data.write_u16_le(rect.flags);
            bitmap_data.write_u16_le(rect.data.len() as u16);
            bitmap_data.write_bytes(&rect.data);
        }

        let bitmap_bytes = bitmap_data.into_vec();

        // Update PDU header
        let update_len = 2 + bitmap_bytes.len(); // update type + bitmap data
        let payload_len = update_len;
        let data_len = ShareDataHeader::SIZE + payload_len;
        let total_len = ShareControlHeader::SIZE + data_len;

        let mut writer = Writer::with_capacity(total_len);

        // Share control header
        let ctrl_header = ShareControlHeader {
            total_length: total_len as u16,
            pdu_type: PduType::Data as u16,
            pdu_source: source,
        };
        ctrl_header.write(&mut writer);

        // Share data header
        let data_header = ShareDataHeader {
            share_id,
            pad1: 0,
            stream_id: ShareDataHeader::STREAM_HI,
            uncompressed_length: payload_len as u16,
            pdu_type: DataPduType::Update as u8,
            compression_type: 0,
            compressed_length: 0,
        };
        data_header.write(&mut writer);

        // Update type
        writer.write_u16_le(UpdateType::Bitmap as u16);

        // Bitmap data
        writer.write_bytes(&bitmap_bytes);

        writer.into_vec()
    }
}

/// Error info PDU
#[derive(Debug, Clone)]
pub struct SetErrorInfoPdu {
    /// Error code
    pub error_info: u32,
}

impl SetErrorInfoPdu {
    /// No error
    pub const ERROR_NONE: u32 = 0x00000000;
    /// Disconnected by admin
    pub const DISCONNECT_BY_ADMIN: u32 = 0x0000000B;

    /// Encode error info PDU
    pub fn encode(&self, share_id: u32, source: u16) -> Vec<u8> {
        let payload_len = 4;
        let data_len = ShareDataHeader::SIZE + payload_len;
        let total_len = ShareControlHeader::SIZE + data_len;

        let mut writer = Writer::with_capacity(total_len);

        // Share control header
        let ctrl_header = ShareControlHeader {
            total_length: total_len as u16,
            pdu_type: PduType::Data as u16,
            pdu_source: source,
        };
        ctrl_header.write(&mut writer);

        // Share data header
        let data_header = ShareDataHeader {
            share_id,
            pad1: 0,
            stream_id: ShareDataHeader::STREAM_LOW,
            uncompressed_length: payload_len as u16,
            pdu_type: DataPduType::SetErrorInfo as u8,
            compression_type: 0,
            compressed_length: 0,
        };
        data_header.write(&mut writer);

        // Error code
        writer.write_u32_le(self.error_info);

        writer.into_vec()
    }
}

/// Deactivate All PDU
#[derive(Debug, Clone)]
pub struct DeactivateAllPdu {
    /// Share ID
    pub share_id: u32,
    /// Length of source descriptor
    pub source_descriptor_len: u16,
}

impl DeactivateAllPdu {
    /// Encode deactivate all PDU
    pub fn encode(&self, source: u16) -> Vec<u8> {
        let payload_len = 6;
        let total_len = ShareControlHeader::SIZE + payload_len;

        let mut writer = Writer::with_capacity(total_len);

        // Share control header
        let ctrl_header = ShareControlHeader {
            total_length: total_len as u16,
            pdu_type: PduType::DeactivateAll as u16,
            pdu_source: source,
        };
        ctrl_header.write(&mut writer);

        // Payload
        writer.write_u32_le(self.share_id);
        writer.write_u16_le(self.source_descriptor_len);

        writer.into_vec()
    }
}
