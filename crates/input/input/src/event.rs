//! Input Event Types

/// Timestamp for events
#[derive(Debug, Clone, Copy, Default)]
pub struct Timestamp {
    /// Seconds
    pub sec: u64,
    /// Microseconds
    pub usec: u64,
}

/// Input event (Linux evdev compatible)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InputEvent {
    /// Timestamp
    pub time: Timestamp,
    /// Event type
    pub type_: u16,
    /// Event code
    pub code: u16,
    /// Event value
    pub value: i32,
}

impl InputEvent {
    /// Create a new input event
    pub fn new(type_: EventType, code: u16, value: i32) -> Self {
        InputEvent {
            time: Timestamp::default(),
            type_: type_ as u16,
            code,
            value,
        }
    }

    /// Create a key event
    pub fn key(code: u16, value: KeyValue) -> Self {
        Self::new(EventType::Key, code, value as i32)
    }

    /// Create a relative movement event
    pub fn rel(code: u16, value: i32) -> Self {
        Self::new(EventType::Rel, code, value)
    }

    /// Create an absolute movement event
    pub fn abs(code: u16, value: i32) -> Self {
        Self::new(EventType::Abs, code, value)
    }

    /// Create a synchronization event
    pub fn sync() -> Self {
        Self::new(EventType::Syn, SynCode::Report as u16, 0)
    }

    /// Get event type
    pub fn event_type(&self) -> EventType {
        EventType::from(self.type_)
    }
}

/// Event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum EventType {
    /// Synchronization events
    Syn = 0x00,
    /// Key/button events
    Key = 0x01,
    /// Relative axes
    Rel = 0x02,
    /// Absolute axes
    Abs = 0x03,
    /// Miscellaneous events
    Msc = 0x04,
    /// Switch events
    Sw = 0x05,
    /// LEDs
    Led = 0x11,
    /// Sound effects
    Snd = 0x12,
    /// Repeat settings
    Rep = 0x14,
    /// Force feedback
    Ff = 0x15,
    /// Power events
    Pwr = 0x16,
    /// Force feedback status
    FfStatus = 0x17,
    /// Unknown
    Unknown = 0xFF,
}

impl From<u16> for EventType {
    fn from(value: u16) -> Self {
        match value {
            0x00 => EventType::Syn,
            0x01 => EventType::Key,
            0x02 => EventType::Rel,
            0x03 => EventType::Abs,
            0x04 => EventType::Msc,
            0x05 => EventType::Sw,
            0x11 => EventType::Led,
            0x12 => EventType::Snd,
            0x14 => EventType::Rep,
            0x15 => EventType::Ff,
            0x16 => EventType::Pwr,
            0x17 => EventType::FfStatus,
            _ => EventType::Unknown,
        }
    }
}

/// Synchronization event codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum SynCode {
    /// Report event batch end
    Report = 0,
    /// Configuration change
    Config = 1,
    /// MT slot sync
    MtReport = 2,
    /// Device dropped events
    Dropped = 3,
}

/// Key event values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum KeyValue {
    /// Key released
    Released = 0,
    /// Key pressed
    Pressed = 1,
    /// Key repeat (auto-repeat)
    Repeat = 2,
}

impl From<i32> for KeyValue {
    fn from(value: i32) -> Self {
        match value {
            0 => KeyValue::Released,
            1 => KeyValue::Pressed,
            2 => KeyValue::Repeat,
            _ => KeyValue::Released,
        }
    }
}

/// Relative axis codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum RelCode {
    /// X axis
    X = 0x00,
    /// Y axis
    Y = 0x01,
    /// Z axis
    Z = 0x02,
    /// Horizontal wheel
    HWheel = 0x06,
    /// Dial
    Dial = 0x07,
    /// Vertical wheel
    Wheel = 0x08,
    /// Miscellaneous
    Misc = 0x09,
    /// High-resolution vertical wheel (120 units per notch)
    WheelHiRes = 0x0B,
    /// High-resolution horizontal wheel (120 units per notch)
    HWheelHiRes = 0x0C,
}

/// Absolute axis codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum AbsCode {
    /// X axis
    X = 0x00,
    /// Y axis
    Y = 0x01,
    /// Z axis
    Z = 0x02,
    /// Rx axis
    Rx = 0x03,
    /// Ry axis
    Ry = 0x04,
    /// Rz axis
    Rz = 0x05,
    /// Throttle
    Throttle = 0x06,
    /// Rudder
    Rudder = 0x07,
    /// Wheel
    Wheel = 0x08,
    /// Gas
    Gas = 0x09,
    /// Brake
    Brake = 0x0a,
    /// Hat0 X
    Hat0X = 0x10,
    /// Hat0 Y
    Hat0Y = 0x11,
    /// Pressure
    Pressure = 0x18,
    /// Multi-touch slot
    MtSlot = 0x2f,
    /// MT touch major
    MtTouchMajor = 0x30,
    /// MT position X
    MtPositionX = 0x35,
    /// MT position Y
    MtPositionY = 0x36,
    /// MT tracking ID
    MtTrackingId = 0x39,
}

/// Button codes (used with EV_KEY)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum BtnCode {
    /// Mouse buttons
    Left = 0x110,
    Right = 0x111,
    Middle = 0x112,
    Side = 0x113,
    Extra = 0x114,
    Forward = 0x115,
    Back = 0x116,
    Task = 0x117,
    /// Touchscreen
    Touch = 0x14a,
    /// Stylus
    Stylus = 0x14b,
    Stylus2 = 0x14c,
    /// Tool types
    ToolPen = 0x140,
    ToolRubber = 0x141,
    ToolFinger = 0x145,
}

/// LED codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum LedCode {
    /// Num Lock
    NumLock = 0x00,
    /// Caps Lock
    CapsLock = 0x01,
    /// Scroll Lock
    ScrollLock = 0x02,
}
