//! OXIDE D-Bus — Custom D-Bus Wire Protocol Implementation
//!
//! — PatchBay: Our own D-Bus. Not a port of libdbus, not a wrapper around sd-bus.
//! Pure Rust implementation of the D-Bus wire protocol with a libdbus-1 compatible
//! C API surface so GTK/glib link against us without knowing the difference.
//!
//! The D-Bus wire protocol is simpler than people think:
//! - Fixed 16-byte header (endianness, type, flags, version, body_length, serial)
//! - Array of header fields (typed variants with alignment)
//! - Body with typed marshalling (BYTE through DICT_ENTRY)
//!
//! We implement just enough for GTK's GDBusConnection to work:
//! - METHOD_CALL, METHOD_RETURN, ERROR, SIGNAL message types
//! - SASL EXTERNAL authentication over AF_UNIX
//! - org.freedesktop.DBus interface basics (Hello, RequestName, etc.)

#![no_std]

extern crate alloc;

pub mod wire;
pub mod message;
pub mod connection;
pub mod auth;
pub mod c_api;

/// D-Bus protocol version (always 1)
pub const DBUS_PROTOCOL_VERSION: u8 = 1;

/// Maximum message size (128MB — same as reference implementation)
pub const DBUS_MAX_MESSAGE_SIZE: usize = 128 * 1024 * 1024;

/// Maximum array length (64MB)
pub const DBUS_MAX_ARRAY_LENGTH: usize = 64 * 1024 * 1024;

/// D-Bus message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Invalid = 0,
    MethodCall = 1,
    MethodReturn = 2,
    Error = 3,
    Signal = 4,
}

impl From<u8> for MessageType {
    fn from(v: u8) -> Self {
        match v {
            1 => MessageType::MethodCall,
            2 => MessageType::MethodReturn,
            3 => MessageType::Error,
            4 => MessageType::Signal,
            _ => MessageType::Invalid,
        }
    }
}

/// D-Bus header field codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HeaderField {
    Invalid = 0,
    Path = 1,
    Interface = 2,
    Member = 3,
    ErrorName = 4,
    ReplySerial = 5,
    Destination = 6,
    Sender = 7,
    Signature = 8,
    UnixFds = 9,
}

/// D-Bus type signature characters
pub mod types {
    pub const BYTE: u8 = b'y';
    pub const BOOLEAN: u8 = b'b';
    pub const INT16: u8 = b'n';
    pub const UINT16: u8 = b'q';
    pub const INT32: u8 = b'i';
    pub const UINT32: u8 = b'u';
    pub const INT64: u8 = b'x';
    pub const UINT64: u8 = b't';
    pub const DOUBLE: u8 = b'd';
    pub const STRING: u8 = b's';
    pub const OBJECT_PATH: u8 = b'o';
    pub const SIGNATURE: u8 = b'g';
    pub const ARRAY: u8 = b'a';
    pub const STRUCT_BEGIN: u8 = b'(';
    pub const STRUCT_END: u8 = b')';
    pub const VARIANT: u8 = b'v';
    pub const DICT_ENTRY_BEGIN: u8 = b'{';
    pub const DICT_ENTRY_END: u8 = b'}';
    pub const UNIX_FD: u8 = b'h';

    /// Get alignment for a D-Bus type
    pub fn alignment(sig_char: u8) -> usize {
        match sig_char {
            BYTE | SIGNATURE => 1,
            INT16 | UINT16 => 2,
            BOOLEAN | INT32 | UINT32 | STRING | OBJECT_PATH | ARRAY | UNIX_FD => 4,
            INT64 | UINT64 | DOUBLE | STRUCT_BEGIN | DICT_ENTRY_BEGIN => 8,
            _ => 1,
        }
    }
}

/// D-Bus bus types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum BusType {
    Session = 0,
    System = 1,
    Starter = 2,
}

/// Error result for D-Bus operations
#[derive(Debug, Clone)]
pub struct DbusError {
    pub name: alloc::string::String,
    pub message: alloc::string::String,
}

impl DbusError {
    pub fn new(name: &str, message: &str) -> Self {
        DbusError {
            name: alloc::string::String::from(name),
            message: alloc::string::String::from(message),
        }
    }
}
