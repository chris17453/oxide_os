//! HDA Codec Verbs and Widget Discovery
//!
//! Implements the HDA codec command protocol (Intel HDA Spec §7).
//! Codec verbs are 32-bit commands sent via CORB, responses come back via RIRB.
//! — EchoFrame: interrogating silicon for its audio topology

#![allow(dead_code)]

// ============================================================================
// Get Parameter Verb (§7.3.4.6)
// Format: 0xF0000 | parameter_id
// ============================================================================

/// Base verb for "Get Parameter"
pub const GET_PARAM: u32 = 0xF0000;

/// Parameter IDs
pub const PARAM_VENDOR_ID: u32 = 0x00;
pub const PARAM_REVISION_ID: u32 = 0x02;
pub const PARAM_SUB_NODE_COUNT: u32 = 0x04;
pub const PARAM_FN_GROUP_TYPE: u32 = 0x05;
pub const PARAM_AUDIO_CAPS: u32 = 0x09;
pub const PARAM_PIN_CAPS: u32 = 0x0C;
pub const PARAM_AMP_IN_CAPS: u32 = 0x0D;
pub const PARAM_CONN_LIST_LEN: u32 = 0x0E;
pub const PARAM_POWER_STATES: u32 = 0x0F;
pub const PARAM_GPIO_COUNT: u32 = 0x11;
pub const PARAM_AMP_OUT_CAPS: u32 = 0x12;
pub const PARAM_VOL_KNOB_CAPS: u32 = 0x13;

// ============================================================================
// Set/Get Verbs
// ============================================================================

/// Get Connection Select (§7.3.3.1)
pub const GET_CONN_SELECT: u32 = 0xF0100;

/// Set Connection Select (§7.3.3.2)
pub const SET_CONN_SELECT: u32 = 0x70100;

/// Get Connection List Entry (§7.3.3.3)
/// Lower 8 bits = starting offset
pub const GET_CONN_LIST: u32 = 0xF0200;

/// Pin Widget Control — Get/Set (§7.3.3.13)
pub const GET_PIN_CTL: u32 = 0xF0700;
pub const SET_PIN_CTL: u32 = 0x70700;

/// Amplifier Gain/Mute — Get (§7.3.3.7)
/// Bit 15: Left channel, Bit 13: Output amp
/// Bits [3:0]: Index
pub const GET_AMP_GAIN: u32 = 0xB0000;

/// Amplifier Gain/Mute — Set (§7.3.3.8)
/// Bit 15: Set left, Bit 14: Set right, Bit 13: Set output, Bit 12: Set input
/// Bit 7: Mute
/// Bits [6:0]: Gain
pub const SET_AMP_GAIN: u32 = 0x30000;

/// Power State — Get/Set (§7.3.3.10)
pub const GET_POWER_STATE: u32 = 0xF0500;
pub const SET_POWER_STATE: u32 = 0x70500;

/// Converter Stream/Channel — Get/Set (§7.3.3.11)
/// Bits [7:4]: Stream tag
/// Bits [3:0]: Channel ID
pub const GET_STREAM_CHAN: u32 = 0xF0600;
pub const SET_STREAM_CHAN: u32 = 0x70600;

/// Converter Format — Set (§7.3.3.8)
pub const SET_CONV_FMT: u32 = 0x20000;

/// Converter Format — Get
pub const GET_CONV_FMT: u32 = 0xA0000;

/// EAPD/BTL Enable — Get/Set
pub const GET_EAPD: u32 = 0xF0C00;
pub const SET_EAPD: u32 = 0x70C00;

/// Pin Configuration Default — Get
pub const GET_CONFIG_DEFAULT: u32 = 0xF1C00;

// ============================================================================
// Pin Widget Control Bits
// ============================================================================

/// Pin output enable (headphone/speaker active)
pub const PIN_CTL_OUT_EN: u8 = 0x40;

/// Pin input enable (microphone active)
pub const PIN_CTL_IN_EN: u8 = 0x20;

/// Headphone amplifier enable
pub const PIN_CTL_HP_EN: u8 = 0x80;

// ============================================================================
// Amplifier Gain/Mute bits for SET_AMP_GAIN
// ============================================================================

/// Set output amp
pub const AMP_SET_OUTPUT: u16 = 1 << 13;

/// Set input amp
pub const AMP_SET_INPUT: u16 = 1 << 12;

/// Set left channel
pub const AMP_SET_LEFT: u16 = 1 << 13;

/// Set right channel
pub const AMP_SET_RIGHT: u16 = 1 << 12;

/// Mute bit
pub const AMP_MUTE: u16 = 1 << 7;

// ============================================================================
// Widget Type IDs (from Audio Widget Capabilities parameter, bits [23:20])
// ============================================================================

/// Audio Output converter (DAC)
pub const WIDGET_AUDIO_OUT: u8 = 0x0;

/// Audio Input converter (ADC)
pub const WIDGET_AUDIO_IN: u8 = 0x1;

/// Audio Mixer
pub const WIDGET_AUDIO_MIXER: u8 = 0x2;

/// Audio Selector
pub const WIDGET_AUDIO_SELECTOR: u8 = 0x3;

/// Pin Complex
pub const WIDGET_PIN_COMPLEX: u8 = 0x4;

/// Power Widget
pub const WIDGET_POWER: u8 = 0x5;

/// Volume Knob
pub const WIDGET_VOL_KNOB: u8 = 0x6;

/// Beep Generator
pub const WIDGET_BEEP_GEN: u8 = 0x7;

/// Vendor-defined
pub const WIDGET_VENDOR: u8 = 0xF;

// ============================================================================
// Function Group Type IDs
// ============================================================================

/// Audio function group
pub const FN_GROUP_AUDIO: u8 = 0x01;

/// Modem function group
pub const FN_GROUP_MODEM: u8 = 0x02;

// ============================================================================
// Pin Configuration Default (GET_CONFIG_DEFAULT) field extraction
// ============================================================================

/// Extract default device type from pin config default
/// Bits [23:20]
pub fn pin_default_device(config: u32) -> u8 {
    ((config >> 20) & 0xF) as u8
}

/// Extract connectivity from pin config default
/// Bits [31:30]
pub fn pin_connectivity(config: u32) -> u8 {
    ((config >> 30) & 0x3) as u8
}

/// Default device values
pub const PIN_DEV_LINE_OUT: u8 = 0x0;
pub const PIN_DEV_SPEAKER: u8 = 0x1;
pub const PIN_DEV_HP_OUT: u8 = 0x2;
pub const PIN_DEV_CD: u8 = 0x3;
pub const PIN_DEV_SPDIF_OUT: u8 = 0x4;
pub const PIN_DEV_LINE_IN: u8 = 0x8;
pub const PIN_DEV_MIC_IN: u8 = 0xA;

/// Connectivity values
pub const PIN_CONN_JACK: u8 = 0x0;
pub const PIN_CONN_NONE: u8 = 0x1;
pub const PIN_CONN_FIXED: u8 = 0x2;
pub const PIN_CONN_BOTH: u8 = 0x3;

// ============================================================================
// Power State Values
// ============================================================================

/// D0 — fully on
pub const POWER_D0: u32 = 0x00;

/// D1 — low power
pub const POWER_D1: u32 = 0x01;

/// D2 — lower power
pub const POWER_D2: u32 = 0x02;

/// D3 — off
pub const POWER_D3: u32 = 0x03;

// ============================================================================
// Audio Widget Capabilities Parsing
// ============================================================================

/// Extract widget type from Audio Widget Capabilities parameter
/// Bits [23:20]
pub fn widget_type(caps: u32) -> u8 {
    ((caps >> 20) & 0xF) as u8
}

/// Check if widget has connection list
/// Bit 8
pub fn has_conn_list(caps: u32) -> bool {
    caps & (1 << 8) != 0
}

/// Check if widget has input amplifier
/// Bit 1
pub fn has_in_amp(caps: u32) -> bool {
    caps & (1 << 1) != 0
}

/// Check if widget has output amplifier
/// Bit 2
pub fn has_out_amp(caps: u32) -> bool {
    caps & (1 << 2) != 0
}

/// Check if widget has power control
/// Bit 10
pub fn has_power_ctl(caps: u32) -> bool {
    caps & (1 << 10) != 0
}
