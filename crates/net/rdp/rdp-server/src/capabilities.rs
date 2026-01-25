//! Server Capabilities
//!
//! Defines the capability sets advertised by the RDP server during
//! the Demand Active / Confirm Active exchange.

use alloc::vec;
use alloc::vec::Vec;
use rdp_proto::pdu::CapabilitySet;
use rdp_traits::CapabilityType;

/// Server capabilities configuration
pub struct ServerCapabilities {
    /// Desktop width
    pub desktop_width: u16,
    /// Desktop height
    pub desktop_height: u16,
    /// Color depth (bits per pixel)
    pub color_depth: u16,
    /// Support for FastPath output
    pub fast_path_output: bool,
    /// Support for surface commands
    pub surface_commands: bool,
    /// Support for large pointers
    pub large_pointers: bool,
    /// Support for desktop composition
    pub desktop_composition: bool,
}

impl ServerCapabilities {
    /// Create new server capabilities with default settings
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            desktop_width: width,
            desktop_height: height,
            color_depth: 32,
            fast_path_output: true,
            surface_commands: false,
            large_pointers: true,
            desktop_composition: false,
        }
    }

    /// Get the number of capability sets
    pub fn count(&self) -> usize {
        // General, Bitmap, Order, BitmapCache, Pointer, Input, VirtualChannel, Sound, Font
        9
    }

    /// Convert to capability set list for Demand Active PDU
    pub fn to_capability_sets(&self) -> Vec<CapabilitySet> {
        vec![
            self.build_general_capability(),
            self.build_bitmap_capability(),
            self.build_order_capability(),
            self.build_bitmap_cache_capability(),
            self.build_pointer_capability(),
            self.build_input_capability(),
            self.build_virtual_channel_capability(),
            self.build_sound_capability(),
            self.build_font_capability(),
        ]
    }

    /// Build General Capability Set (TS_GENERAL_CAPABILITYSET)
    fn build_general_capability(&self) -> CapabilitySet {
        let mut data = Vec::with_capacity(24);

        // osMajorType (2 bytes) - OSMAJORTYPE_WINDOWS
        data.extend_from_slice(&0x0001u16.to_le_bytes());
        // osMinorType (2 bytes) - OSMINORTYPE_WINDOWS_NT
        data.extend_from_slice(&0x0003u16.to_le_bytes());
        // protocolVersion (2 bytes) - must be TS_CAPS_PROTOCOLVERSION (0x0200)
        data.extend_from_slice(&0x0200u16.to_le_bytes());
        // pad2octetsA (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // generalCompressionTypes (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // extraFlags (2 bytes)
        let mut extra_flags: u16 = 0;
        extra_flags |= 0x0001; // FASTPATH_OUTPUT_SUPPORTED
        extra_flags |= 0x0400; // NO_BITMAP_COMPRESSION_HDR
        extra_flags |= 0x0004; // AUTORECONNECT_SUPPORTED
        if self.fast_path_output {
            extra_flags |= 0x0001;
        }
        data.extend_from_slice(&extra_flags.to_le_bytes());
        // updateCapabilityFlag (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // remoteUnshareFlag (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // generalCompressionLevel (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // refreshRectSupport (1 byte)
        data.push(1);
        // suppressOutputSupport (1 byte)
        data.push(1);

        CapabilitySet {
            cap_type: CapabilityType::General as u16,
            data,
        }
    }

    /// Build Bitmap Capability Set (TS_BITMAP_CAPABILITYSET)
    fn build_bitmap_capability(&self) -> CapabilitySet {
        let mut data = Vec::with_capacity(28);

        // preferredBitsPerPixel (2 bytes)
        data.extend_from_slice(&self.color_depth.to_le_bytes());
        // receive1BitPerPixel (2 bytes)
        data.extend_from_slice(&1u16.to_le_bytes());
        // receive4BitsPerPixel (2 bytes)
        data.extend_from_slice(&1u16.to_le_bytes());
        // receive8BitsPerPixel (2 bytes)
        data.extend_from_slice(&1u16.to_le_bytes());
        // desktopWidth (2 bytes)
        data.extend_from_slice(&self.desktop_width.to_le_bytes());
        // desktopHeight (2 bytes)
        data.extend_from_slice(&self.desktop_height.to_le_bytes());
        // pad2octets (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // desktopResizeFlag (2 bytes)
        data.extend_from_slice(&1u16.to_le_bytes());
        // bitmapCompressionFlag (2 bytes)
        data.extend_from_slice(&1u16.to_le_bytes());
        // highColorFlags (1 byte)
        data.push(0);
        // drawingFlags (1 byte)
        let mut drawing_flags: u8 = 0;
        drawing_flags |= 0x08; // DRAW_ALLOW_DYNAMIC_COLOR_FIDELITY
        drawing_flags |= 0x02; // DRAW_ALLOW_COLOR_SUBSAMPLING
        drawing_flags |= 0x20; // DRAW_ALLOW_SKIP_ALPHA
        data.push(drawing_flags);
        // multipleRectangleSupport (2 bytes)
        data.extend_from_slice(&1u16.to_le_bytes());
        // pad2octetsB (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());

        CapabilitySet {
            cap_type: CapabilityType::Bitmap as u16,
            data,
        }
    }

    /// Build Order Capability Set (TS_ORDER_CAPABILITYSET)
    fn build_order_capability(&self) -> CapabilitySet {
        let mut data = Vec::with_capacity(88);

        // terminalDescriptor (16 bytes)
        data.extend_from_slice(&[0u8; 16]);
        // pad4octetsA (4 bytes)
        data.extend_from_slice(&0u32.to_le_bytes());
        // desktopSaveXGranularity (2 bytes)
        data.extend_from_slice(&1u16.to_le_bytes());
        // desktopSaveYGranularity (2 bytes)
        data.extend_from_slice(&20u16.to_le_bytes());
        // pad2octetsA (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // maximumOrderLevel (2 bytes)
        data.extend_from_slice(&1u16.to_le_bytes());
        // numberFonts (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // orderFlags (2 bytes)
        let order_flags: u16 = 0x0002 | 0x0008 | 0x0020; // NEGOTIATEORDERSUPPORT | ZEROBOUNDSDELTASSUPPORT | COLORINDEXSUPPORT
        data.extend_from_slice(&order_flags.to_le_bytes());
        // orderSupport (32 bytes) - bitmap of supported drawing orders
        let mut order_support = [0u8; 32];
        // Enable basic orders
        order_support[0] = 1; // TS_NEG_DSTBLT_INDEX
        order_support[1] = 1; // TS_NEG_PATBLT_INDEX
        order_support[2] = 1; // TS_NEG_SCRBLT_INDEX
        order_support[3] = 1; // TS_NEG_MEMBLT_INDEX
        order_support[4] = 1; // TS_NEG_MEM3BLT_INDEX
        order_support[8] = 1; // TS_NEG_LINETO_INDEX
        order_support[20] = 1; // TS_NEG_MULTI_DSTBLT_INDEX
        order_support[21] = 1; // TS_NEG_MULTI_PATBLT_INDEX
        order_support[22] = 1; // TS_NEG_MULTI_SCRBLT_INDEX
        order_support[27] = 1; // TS_NEG_GLYPH_INDEX
        data.extend_from_slice(&order_support);
        // textFlags (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // orderSupportExFlags (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // pad4octetsB (4 bytes)
        data.extend_from_slice(&0u32.to_le_bytes());
        // desktopSaveSize (4 bytes)
        data.extend_from_slice(&(480 * 480u32).to_le_bytes());
        // pad2octetsC (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // pad2octetsD (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // textANSICodePage (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // pad2octetsE (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());

        CapabilitySet {
            cap_type: CapabilityType::Order as u16,
            data,
        }
    }

    /// Build Bitmap Cache Capability Set (TS_BITMAPCACHE_CAPABILITYSET_REV2)
    fn build_bitmap_cache_capability(&self) -> CapabilitySet {
        let mut data = Vec::with_capacity(40);

        // cacheFlags (2 bytes)
        let cache_flags: u16 = 0x0001; // PERSISTENT_KEYS_EXPECTED_FLAG (disabled)
        data.extend_from_slice(&cache_flags.to_le_bytes());
        // pad2 (1 byte)
        data.push(0);
        // numCellCaches (1 byte)
        data.push(3);
        // bitmapCache0CellInfo (4 bytes) - 120 entries, not persistent
        data.extend_from_slice(&((120 & 0x7FFFFFFF) as u32).to_le_bytes());
        // bitmapCache1CellInfo (4 bytes) - 120 entries
        data.extend_from_slice(&((120 & 0x7FFFFFFF) as u32).to_le_bytes());
        // bitmapCache2CellInfo (4 bytes) - 2048 entries
        data.extend_from_slice(&((2048 & 0x7FFFFFFF) as u32).to_le_bytes());
        // bitmapCache3CellInfo (4 bytes)
        data.extend_from_slice(&0u32.to_le_bytes());
        // bitmapCache4CellInfo (4 bytes)
        data.extend_from_slice(&0u32.to_le_bytes());
        // pad3 (12 bytes)
        data.extend_from_slice(&[0u8; 12]);

        CapabilitySet {
            cap_type: CapabilityType::BitmapCacheV2 as u16,
            data,
        }
    }

    /// Build Pointer Capability Set (TS_POINTER_CAPABILITYSET)
    fn build_pointer_capability(&self) -> CapabilitySet {
        let mut data = Vec::with_capacity(10);

        // colorPointerFlag (2 bytes)
        data.extend_from_slice(&1u16.to_le_bytes());
        // colorPointerCacheSize (2 bytes)
        data.extend_from_slice(&25u16.to_le_bytes());
        // pointerCacheSize (2 bytes) - large pointer support
        if self.large_pointers {
            data.extend_from_slice(&25u16.to_le_bytes());
        } else {
            data.extend_from_slice(&0u16.to_le_bytes());
        }

        CapabilitySet {
            cap_type: CapabilityType::Pointer as u16,
            data,
        }
    }

    /// Build Input Capability Set (TS_INPUT_CAPABILITYSET)
    fn build_input_capability(&self) -> CapabilitySet {
        let mut data = Vec::with_capacity(88);

        // inputFlags (2 bytes)
        let mut input_flags: u16 = 0;
        input_flags |= 0x0001; // INPUT_FLAG_SCANCODES
        input_flags |= 0x0004; // INPUT_FLAG_MOUSEX
        input_flags |= 0x0010; // INPUT_FLAG_UNICODE
        input_flags |= 0x0020; // INPUT_FLAG_FASTPATH_INPUT
        input_flags |= 0x0080; // INPUT_FLAG_FASTPATH_INPUT2
        data.extend_from_slice(&input_flags.to_le_bytes());
        // pad2octetsA (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());
        // keyboardLayout (4 bytes) - US keyboard
        data.extend_from_slice(&0x00000409u32.to_le_bytes());
        // keyboardType (4 bytes) - IBM enhanced (101/102)
        data.extend_from_slice(&4u32.to_le_bytes());
        // keyboardSubType (4 bytes)
        data.extend_from_slice(&0u32.to_le_bytes());
        // keyboardFunctionKey (4 bytes) - 12 function keys
        data.extend_from_slice(&12u32.to_le_bytes());
        // imeFileName (64 bytes)
        data.extend_from_slice(&[0u8; 64]);

        CapabilitySet {
            cap_type: CapabilityType::Input as u16,
            data,
        }
    }

    /// Build Virtual Channel Capability Set
    fn build_virtual_channel_capability(&self) -> CapabilitySet {
        let mut data = Vec::with_capacity(8);

        // flags (4 bytes)
        let flags: u32 = 0x00000001; // VCCAPS_COMPR_SC - server-to-client compression
        data.extend_from_slice(&flags.to_le_bytes());
        // VCChunkSize (4 bytes) - optional
        // Using default chunk size

        CapabilitySet {
            cap_type: CapabilityType::VirtualChannel as u16,
            data,
        }
    }

    /// Build Sound Capability Set
    fn build_sound_capability(&self) -> CapabilitySet {
        let mut data = Vec::with_capacity(4);

        // soundFlags (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes()); // No sound support
        // pad2octetsA (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());

        CapabilitySet {
            cap_type: CapabilityType::Sound as u16,
            data,
        }
    }

    /// Build Font Capability Set
    fn build_font_capability(&self) -> CapabilitySet {
        let mut data = Vec::with_capacity(4);

        // fontSupportFlags (2 bytes)
        data.extend_from_slice(&0x0001u16.to_le_bytes()); // FONTSUPPORT_FONTLIST
        // pad2octets (2 bytes)
        data.extend_from_slice(&0u16.to_le_bytes());

        CapabilitySet {
            cap_type: CapabilityType::Font as u16,
            data,
        }
    }
}
