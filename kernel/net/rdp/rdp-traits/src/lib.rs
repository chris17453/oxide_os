//! RDP Traits - Core traits and types for OXIDE OS RDP implementation
//!
//! This crate defines the foundational types, traits, and error handling
//! for the RDP (Remote Desktop Protocol) server implementation.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

// ============================================================================
// Error Types
// ============================================================================

/// RDP error type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RdpError {
    /// Invalid protocol data
    InvalidProtocol,
    /// Insufficient data to parse
    InsufficientData,
    /// Unsupported protocol version
    UnsupportedVersion,
    /// Unsupported encryption method
    UnsupportedEncryption,
    /// Authentication failed
    AuthenticationFailed,
    /// Invalid credentials
    InvalidCredentials,
    /// Connection refused
    ConnectionRefused,
    /// Connection closed by peer
    ConnectionClosed,
    /// TLS handshake failure
    TlsError,
    /// Encryption/decryption error
    CryptoError,
    /// Capability negotiation failed
    CapabilityNegotiationFailed,
    /// License error
    LicenseError,
    /// Channel error
    ChannelError,
    /// Graphics encoding error
    GraphicsError,
    /// Input injection error
    InputError,
    /// Resource exhausted
    ResourceExhausted,
    /// Internal error
    Internal,
    /// I/O error
    IoError,
    /// Invalid state transition
    InvalidState,
    /// Timeout
    Timeout,
    /// Not implemented
    NotImplemented,
}

impl fmt::Display for RdpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RdpError::InvalidProtocol => write!(f, "invalid protocol data"),
            RdpError::InsufficientData => write!(f, "insufficient data"),
            RdpError::UnsupportedVersion => write!(f, "unsupported protocol version"),
            RdpError::UnsupportedEncryption => write!(f, "unsupported encryption method"),
            RdpError::AuthenticationFailed => write!(f, "authentication failed"),
            RdpError::InvalidCredentials => write!(f, "invalid credentials"),
            RdpError::ConnectionRefused => write!(f, "connection refused"),
            RdpError::ConnectionClosed => write!(f, "connection closed"),
            RdpError::TlsError => write!(f, "TLS error"),
            RdpError::CryptoError => write!(f, "cryptographic error"),
            RdpError::CapabilityNegotiationFailed => write!(f, "capability negotiation failed"),
            RdpError::LicenseError => write!(f, "license error"),
            RdpError::ChannelError => write!(f, "channel error"),
            RdpError::GraphicsError => write!(f, "graphics error"),
            RdpError::InputError => write!(f, "input error"),
            RdpError::ResourceExhausted => write!(f, "resource exhausted"),
            RdpError::Internal => write!(f, "internal error"),
            RdpError::IoError => write!(f, "I/O error"),
            RdpError::InvalidState => write!(f, "invalid state"),
            RdpError::Timeout => write!(f, "timeout"),
            RdpError::NotImplemented => write!(f, "not implemented"),
        }
    }
}

/// RDP result type
pub type RdpResult<T> = Result<T, RdpError>;

// ============================================================================
// Pixel Format
// ============================================================================

/// Pixel format for RDP graphics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 32-bit BGRA (B at lowest address) - RDP native wire format
    Bgra8888,
    /// 32-bit RGBA
    Rgba8888,
    /// 24-bit BGR
    Bgr888,
    /// 24-bit RGB
    Rgb888,
    /// 16-bit RGB565
    Rgb565,
    /// 15-bit RGB555
    Rgb555,
    /// 8-bit indexed color
    Indexed8,
}

impl PixelFormat {
    /// Bytes per pixel for this format
    pub const fn bytes_per_pixel(&self) -> u32 {
        match self {
            PixelFormat::Bgra8888 | PixelFormat::Rgba8888 => 4,
            PixelFormat::Bgr888 | PixelFormat::Rgb888 => 3,
            PixelFormat::Rgb565 | PixelFormat::Rgb555 => 2,
            PixelFormat::Indexed8 => 1,
        }
    }

    /// Bits per pixel for this format
    pub const fn bits_per_pixel(&self) -> u32 {
        self.bytes_per_pixel() * 8
    }
}

// ============================================================================
// Dirty Region
// ============================================================================

/// A rectangular region that has been modified
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRegion {
    /// X coordinate (left edge)
    pub x: u32,
    /// Y coordinate (top edge)
    pub y: u32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
}

impl DirtyRegion {
    /// Create a new dirty region
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if this region is empty
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Calculate the area in pixels
    pub const fn area(&self) -> u32 {
        self.width * self.height
    }

    /// Check if this region intersects with another
    pub const fn intersects(&self, other: &DirtyRegion) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    /// Merge two regions into a bounding box
    pub fn merge(&self, other: &DirtyRegion) -> DirtyRegion {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let x2 = (self.x + self.width).max(other.x + other.width);
        let y2 = (self.y + self.height).max(other.y + other.height);
        DirtyRegion {
            x,
            y,
            width: x2 - x,
            height: y2 - y,
        }
    }
}

// ============================================================================
// Screen Capture Provider Trait
// ============================================================================

/// Trait for providing screen capture functionality
///
/// Implementors wrap the system framebuffer and provide efficient
/// methods for capturing screen content and detecting changes.
pub trait ScreenCaptureProvider: Send + Sync {
    /// Get screen dimensions (width, height)
    fn dimensions(&self) -> (u32, u32);

    /// Get the pixel format
    fn pixel_format(&self) -> PixelFormat;

    /// Get stride (bytes per row)
    fn stride(&self) -> u32;

    /// Capture the full screen into the provided buffer
    ///
    /// Buffer must be at least `height * stride` bytes.
    /// Returns error if buffer is too small.
    fn capture_full(&self, buffer: &mut [u8]) -> RdpResult<()>;

    /// Capture a rectangular region into the provided buffer
    ///
    /// Buffer must be at least `height * width * bytes_per_pixel` bytes
    /// for packed output (no row padding).
    fn capture_region(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        buffer: &mut [u8],
    ) -> RdpResult<()>;

    /// Get list of dirty regions since last call
    ///
    /// Each call clears the internal dirty tracking, so regions
    /// are only reported once.
    fn get_dirty_regions(&mut self) -> Vec<DirtyRegion>;

    /// Mark the entire screen as dirty
    ///
    /// Useful after initial connection or mode change.
    fn mark_all_dirty(&mut self);
}

// ============================================================================
// Input Injection
// ============================================================================

/// Keyboard input flags from RDP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardFlags(pub u16);

impl KeyboardFlags {
    /// Key is extended (e.g., right control, numpad enter)
    pub const EXTENDED: u16 = 0x0100;
    /// Key is extended with additional E1 prefix
    pub const EXTENDED1: u16 = 0x0200;
    /// Key release event (vs key press)
    pub const RELEASE: u16 = 0x8000;

    /// Create new flags
    pub const fn new(flags: u16) -> Self {
        Self(flags)
    }

    /// Check if key is being released
    pub const fn is_release(&self) -> bool {
        self.0 & Self::RELEASE != 0
    }

    /// Check if key is extended
    pub const fn is_extended(&self) -> bool {
        self.0 & Self::EXTENDED != 0
    }

    /// Check if key is extended1 (E1 prefix)
    pub const fn is_extended1(&self) -> bool {
        self.0 & Self::EXTENDED1 != 0
    }
}

/// Mouse button flags from RDP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseFlags(pub u16);

impl MouseFlags {
    /// Movement event
    pub const MOVE: u16 = 0x0800;
    /// Left button down
    pub const LEFT_DOWN: u16 = 0x1000;
    /// Left button up
    pub const LEFT_UP: u16 = 0x2000;
    /// Right button down
    pub const RIGHT_DOWN: u16 = 0x4000;
    /// Right button up
    pub const RIGHT_UP: u16 = 0x8000;
    /// Middle button down
    pub const MIDDLE_DOWN: u16 = 0x0001;
    /// Middle button up
    pub const MIDDLE_UP: u16 = 0x0002;

    pub const fn new(flags: u16) -> Self {
        Self(flags)
    }

    pub const fn is_move(&self) -> bool {
        self.0 & Self::MOVE != 0
    }

    pub const fn is_left_down(&self) -> bool {
        self.0 & Self::LEFT_DOWN != 0
    }

    pub const fn is_left_up(&self) -> bool {
        self.0 & Self::LEFT_UP != 0
    }

    pub const fn is_right_down(&self) -> bool {
        self.0 & Self::RIGHT_DOWN != 0
    }

    pub const fn is_right_up(&self) -> bool {
        self.0 & Self::RIGHT_UP != 0
    }

    pub const fn is_middle_down(&self) -> bool {
        self.0 & Self::MIDDLE_DOWN != 0
    }

    pub const fn is_middle_up(&self) -> bool {
        self.0 & Self::MIDDLE_UP != 0
    }
}

/// Extended mouse flags for wheel and extra buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtendedMouseFlags(pub u16);

impl ExtendedMouseFlags {
    /// Vertical wheel rotation
    pub const WHEEL: u16 = 0x0200;
    /// Horizontal wheel (tilt)
    pub const HWHEEL: u16 = 0x0400;
    /// Wheel rotation is negative (down/left)
    pub const WHEEL_NEGATIVE: u16 = 0x0100;
    /// X button 1 down
    pub const XBUTTON1_DOWN: u16 = 0x0001;
    /// X button 1 up
    pub const XBUTTON1_UP: u16 = 0x0002;
    /// X button 2 down
    pub const XBUTTON2_DOWN: u16 = 0x0004;
    /// X button 2 up
    pub const XBUTTON2_UP: u16 = 0x0008;

    pub const fn new(flags: u16) -> Self {
        Self(flags)
    }

    pub const fn is_wheel(&self) -> bool {
        self.0 & Self::WHEEL != 0
    }

    pub const fn is_hwheel(&self) -> bool {
        self.0 & Self::HWHEEL != 0
    }

    pub const fn is_wheel_negative(&self) -> bool {
        self.0 & Self::WHEEL_NEGATIVE != 0
    }
}

/// Trait for injecting input events into the system
pub trait InputInjector: Send + Sync {
    /// Inject a keyboard event
    ///
    /// `scancode` is the RDP scancode (Set 1 compatible)
    fn inject_keyboard(&self, scancode: u16, flags: KeyboardFlags) -> RdpResult<()>;

    /// Inject a mouse event
    ///
    /// Coordinates are absolute screen position.
    fn inject_mouse(&self, x: u16, y: u16, flags: MouseFlags) -> RdpResult<()>;

    /// Inject an extended mouse event (wheel, extra buttons)
    ///
    /// `delta` is the wheel rotation amount (positive = up/right).
    fn inject_mouse_extended(
        &self,
        x: u16,
        y: u16,
        flags: ExtendedMouseFlags,
        delta: i16,
    ) -> RdpResult<()>;

    /// Inject a Unicode keyboard event
    ///
    /// Used when the client sends Unicode characters directly.
    fn inject_unicode(&self, code_point: u16, is_release: bool) -> RdpResult<()>;
}

// ============================================================================
// Virtual Channel
// ============================================================================

/// Trait for RDP virtual channels
///
/// Virtual channels provide extensibility for clipboard, audio,
/// drive redirection, and other features.
pub trait VirtualChannel: Send + Sync {
    /// Get the channel name (e.g., "cliprdr", "rdpsnd")
    fn name(&self) -> &str;

    /// Handle data received from the client
    fn on_receive(&mut self, data: &[u8]) -> RdpResult<()>;

    /// Poll for data to send to the client
    ///
    /// Returns `None` if no data is pending.
    fn poll_send(&mut self) -> Option<Vec<u8>>;

    /// Called when the channel is connected
    fn on_connect(&mut self);

    /// Called when the channel is disconnected
    fn on_disconnect(&mut self);

    /// Channel options flags for MCS
    fn options(&self) -> u32 {
        // Default: initialized, compress supported
        0xC0000000
    }
}

// ============================================================================
// Session Types
// ============================================================================

/// Unique session identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u32);

impl SessionId {
    /// Create a new session ID
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Session({})", self.0)
    }
}

/// Session state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Initial state, waiting for connection
    Initial,
    /// X.224 connection request received
    ConnectionRequest,
    /// X.224 connection confirmed
    ConnectionConfirm,
    /// MCS connect-initial received
    McsConnectInitial,
    /// MCS connect-response sent
    McsConnectResponse,
    /// MCS erect domain request received
    McsErectDomain,
    /// MCS attach user request received
    McsAttachUser,
    /// MCS channels joined
    McsChannelJoin,
    /// Security exchange (TLS/encryption setup)
    SecurityExchange,
    /// Client info received
    ClientInfo,
    /// License negotiation
    Licensing,
    /// Capability exchange (demand active/confirm active)
    CapabilityExchange,
    /// Connection finalization
    Finalization,
    /// Fully connected, data exchange
    Connected,
    /// Disconnect initiated
    Disconnecting,
    /// Session ended
    Disconnected,
}

impl SessionState {
    /// Check if the session is in a connected state (capable of data exchange)
    pub const fn is_connected(&self) -> bool {
        matches!(self, SessionState::Connected)
    }

    /// Check if the session is still active (not disconnected)
    pub const fn is_active(&self) -> bool {
        !matches!(self, SessionState::Disconnected)
    }
}

// ============================================================================
// Connection Info
// ============================================================================

/// Client connection information
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// Client machine name
    pub computer_name: String,
    /// Requested username
    pub username: String,
    /// Domain name
    pub domain: String,
    /// Requested desktop width
    pub desktop_width: u16,
    /// Requested desktop height
    pub desktop_height: u16,
    /// Requested color depth (bits per pixel)
    pub color_depth: u16,
    /// Client build number
    pub client_build: u32,
    /// Client product ID
    pub client_product_id: u16,
    /// Performance flags
    pub performance_flags: u32,
    /// Auto-reconnect cookie (if any)
    pub auto_reconnect_cookie: Option<[u8; 28]>,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self {
            computer_name: String::new(),
            username: String::new(),
            domain: String::new(),
            desktop_width: 1024,
            desktop_height: 768,
            color_depth: 32,
            client_build: 0,
            client_product_id: 0,
            performance_flags: 0,
            auto_reconnect_cookie: None,
        }
    }
}

// ============================================================================
// Server Configuration
// ============================================================================

/// RDP server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Server hostname for certificate
    pub hostname: String,
    /// Listening port (default 3389)
    pub port: u16,
    /// Maximum concurrent sessions
    pub max_sessions: u32,
    /// Frame rate limit (FPS)
    pub max_fps: u32,
    /// Enable TLS encryption
    pub require_tls: bool,
    /// Enable NLA (Network Level Authentication)
    pub require_nla: bool,
    /// Allowed authentication methods
    pub auth_methods: AuthMethods,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            hostname: String::from("OXIDE"),
            port: 3389,
            max_sessions: 10,
            max_fps: 30,
            require_tls: true,
            require_nla: false,
            auth_methods: AuthMethods(AuthMethods::PASSWORD),
        }
    }
}

/// Authentication methods supported
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthMethods(pub u32);

impl AuthMethods {
    pub const NONE: u32 = 0x0001;
    pub const PASSWORD: u32 = 0x0002;
    pub const SMARTCARD: u32 = 0x0004;
    pub const KERBEROS: u32 = 0x0008;

    pub const fn new(methods: u32) -> Self {
        Self(methods)
    }

    pub const fn allows_none(&self) -> bool {
        self.0 & Self::NONE != 0
    }

    pub const fn allows_password(&self) -> bool {
        self.0 & Self::PASSWORD != 0
    }
}

// ============================================================================
// Protocol Constants
// ============================================================================

/// RDP protocol constants
pub mod protocol {
    /// Default RDP port
    pub const DEFAULT_PORT: u16 = 3389;

    /// TPKT header size
    pub const TPKT_HEADER_SIZE: usize = 4;

    /// TPKT version
    pub const TPKT_VERSION: u8 = 3;

    /// X.224 connection request code
    pub const X224_CONNECTION_REQUEST: u8 = 0xE0;

    /// X.224 connection confirm code
    pub const X224_CONNECTION_CONFIRM: u8 = 0xD0;

    /// X.224 data code
    pub const X224_DATA: u8 = 0xF0;

    /// X.224 disconnect request code
    pub const X224_DISCONNECT_REQUEST: u8 = 0x80;

    /// MCS connect-initial tag
    pub const MCS_CONNECT_INITIAL: u8 = 0x7F;

    /// MCS connect-response tag
    pub const MCS_CONNECT_RESPONSE: u8 = 0x7F;

    /// MCS domain parameters tag
    pub const MCS_DOMAIN_PARAMS: u8 = 0x30;

    /// Security protocol: Standard RDP
    pub const PROTOCOL_RDP: u32 = 0x00000000;

    /// Security protocol: TLS 1.0
    pub const PROTOCOL_SSL: u32 = 0x00000001;

    /// Security protocol: CredSSP (NLA)
    pub const PROTOCOL_HYBRID: u32 = 0x00000002;

    /// RDP negotiation request type
    pub const TYPE_RDP_NEG_REQ: u8 = 0x01;

    /// RDP negotiation response type
    pub const TYPE_RDP_NEG_RSP: u8 = 0x02;

    /// RDP negotiation failure type
    pub const TYPE_RDP_NEG_FAILURE: u8 = 0x03;

    /// RDP version 5.0+
    pub const RDP_VERSION_5_PLUS: u32 = 0x00080004;

    /// Encryption method: 40-bit
    pub const ENCRYPTION_40BIT: u32 = 0x00000001;

    /// Encryption method: 128-bit
    pub const ENCRYPTION_128BIT: u32 = 0x00000002;

    /// Encryption method: 56-bit
    pub const ENCRYPTION_56BIT: u32 = 0x00000008;

    /// Encryption method: FIPS
    pub const ENCRYPTION_FIPS: u32 = 0x00000010;

    /// Encryption level: None
    pub const ENCRYPTION_LEVEL_NONE: u32 = 0x00000000;

    /// Encryption level: Low
    pub const ENCRYPTION_LEVEL_LOW: u32 = 0x00000001;

    /// Encryption level: Client compatible
    pub const ENCRYPTION_LEVEL_CLIENT: u32 = 0x00000002;

    /// Encryption level: High
    pub const ENCRYPTION_LEVEL_HIGH: u32 = 0x00000003;

    /// Encryption level: FIPS
    pub const ENCRYPTION_LEVEL_FIPS: u32 = 0x00000004;
}

// ============================================================================
// PDU Types
// ============================================================================

/// RDP PDU types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum PduType {
    /// Demand Active PDU
    DemandActive = 0x0001,
    /// Confirm Active PDU
    ConfirmActive = 0x0003,
    /// Deactivate All PDU
    DeactivateAll = 0x0006,
    /// Data PDU
    Data = 0x0007,
    /// Server Redirection PDU
    ServerRedirection = 0x000A,
}

/// Data PDU types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataPduType {
    /// Update PDU
    Update = 0x02,
    /// Control PDU
    Control = 0x14,
    /// Pointer PDU
    Pointer = 0x1B,
    /// Input PDU
    Input = 0x1C,
    /// Synchronize PDU
    Synchronize = 0x1F,
    /// Refresh Rectangle PDU
    RefreshRect = 0x21,
    /// Play Sound PDU
    PlaySound = 0x22,
    /// Suppress Output PDU
    SuppressOutput = 0x23,
    /// Shutdown Request PDU
    ShutdownRequest = 0x24,
    /// Shutdown Denied PDU
    ShutdownDenied = 0x25,
    /// Save Session Info PDU
    SaveSessionInfo = 0x26,
    /// Font List PDU
    FontList = 0x27,
    /// Font Map PDU
    FontMap = 0x28,
    /// Set Error Info PDU
    SetErrorInfo = 0x2F,
    /// Set Keyboard Indicators PDU
    SetKeyboardIndicators = 0x30,
    /// Set Keyboard IME Status PDU
    SetKeyboardImeStatus = 0x31,
    /// Bitmap Cache Persistent List PDU
    BitmapCachePersistentList = 0x2B,
}

/// Update types in Update PDU
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum UpdateType {
    /// Orders
    Orders = 0x0000,
    /// Bitmap
    Bitmap = 0x0001,
    /// Palette
    Palette = 0x0002,
    /// Synchronize
    Synchronize = 0x0003,
}

// ============================================================================
// Capability Types
// ============================================================================

/// Capability set types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum CapabilityType {
    /// General capability
    General = 0x0001,
    /// Bitmap capability
    Bitmap = 0x0002,
    /// Order capability
    Order = 0x0003,
    /// Bitmap cache capability
    BitmapCache = 0x0004,
    /// Control capability
    Control = 0x0005,
    /// Window activation capability
    Activation = 0x0007,
    /// Pointer capability
    Pointer = 0x0008,
    /// Share capability
    Share = 0x0009,
    /// Color cache capability
    ColorCache = 0x000A,
    /// Sound capability
    Sound = 0x000C,
    /// Input capability
    Input = 0x000D,
    /// Font capability
    Font = 0x000E,
    /// Brush capability
    Brush = 0x000F,
    /// Glyph cache capability
    GlyphCache = 0x0010,
    /// Offscreen bitmap cache capability
    OffscreenCache = 0x0011,
    /// Bitmap cache host support capability
    BitmapCacheHostSupport = 0x0012,
    /// Bitmap cache v2 capability
    BitmapCacheV2 = 0x0013,
    /// Virtual channel capability
    VirtualChannel = 0x0014,
    /// Draw nine grid cache capability
    DrawNineGridCache = 0x0015,
    /// Draw GDI+ capability
    DrawGdiPlus = 0x0016,
    /// Rail capability
    Rail = 0x0017,
    /// Window capability
    Window = 0x0018,
    /// Comp desk capability
    CompDesk = 0x0019,
    /// Multifragment update capability
    MultiFragmentUpdate = 0x001A,
    /// Large pointer capability
    LargePointer = 0x001B,
    /// Surface commands capability
    SurfaceCommands = 0x001C,
    /// Bitmap codecs capability
    BitmapCodecs = 0x001D,
    /// Frame acknowledge capability
    FrameAcknowledge = 0x001E,
}
