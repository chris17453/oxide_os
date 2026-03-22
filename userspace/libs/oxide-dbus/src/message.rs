//! D-Bus Message — The atomic unit of bus communication
//!
//! — PatchBay: Every D-Bus interaction is a message. Method calls get replies,
//! signals are fire-and-forget, errors carry failure details. The header tells
//! you what it is, who sent it, where it's going, and what's in the body.
//!
//! Wire format (little-endian):
//! [0]   endian      'l' or 'B'
//! [1]   type        1=METHOD_CALL, 2=METHOD_RETURN, 3=ERROR, 4=SIGNAL
//! [2]   flags       0x01=NO_REPLY_EXPECTED, 0x02=NO_AUTO_START
//! [3]   version     always 1
//! [4-7] body_len    u32 — body size in bytes
//! [8-11] serial     u32 — message serial number (unique per connection)
//! [12-15] fields_len u32 — header fields array length
//! [16+] fields      array of (byte, variant) header field entries
//! [pad to 8]
//! [body] body data matching the Signature header field

use alloc::string::String;
use alloc::vec::Vec;

use crate::{MessageType, HeaderField};
use crate::wire::{Marshaller, Unmarshaller, align_to};

/// A parsed D-Bus message.
#[derive(Debug, Clone)]
pub struct Message {
    pub msg_type: MessageType,
    pub flags: u8,
    pub serial: u32,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub reply_serial: Option<u32>,
    pub destination: Option<String>,
    pub sender: Option<String>,
    pub signature: Option<String>,
    pub unix_fds: Option<u32>,
    pub body: Vec<u8>,
}

impl Message {
    pub fn new(msg_type: MessageType, serial: u32) -> Self {
        Message {
            msg_type,
            flags: 0,
            serial,
            path: None,
            interface: None,
            member: None,
            error_name: None,
            reply_serial: None,
            destination: None,
            sender: None,
            signature: None,
            unix_fds: None,
            body: Vec::new(),
        }
    }

    /// Create a METHOD_CALL message.
    pub fn method_call(serial: u32, dest: &str, path: &str, iface: &str, member: &str) -> Self {
        let mut msg = Message::new(MessageType::MethodCall, serial);
        msg.destination = Some(String::from(dest));
        msg.path = Some(String::from(path));
        msg.interface = Some(String::from(iface));
        msg.member = Some(String::from(member));
        msg
    }

    /// Create a METHOD_RETURN message.
    pub fn method_return(serial: u32, reply_to: u32) -> Self {
        let mut msg = Message::new(MessageType::MethodReturn, serial);
        msg.reply_serial = Some(reply_to);
        msg
    }

    /// Create an ERROR message.
    pub fn error(serial: u32, reply_to: u32, error_name: &str) -> Self {
        let mut msg = Message::new(MessageType::Error, serial);
        msg.reply_serial = Some(reply_to);
        msg.error_name = Some(String::from(error_name));
        msg
    }

    /// Create a SIGNAL message.
    pub fn signal(serial: u32, path: &str, iface: &str, member: &str) -> Self {
        let mut msg = Message::new(MessageType::Signal, serial);
        msg.path = Some(String::from(path));
        msg.interface = Some(String::from(iface));
        msg.member = Some(String::from(member));
        msg
    }

    /// Serialize this message to bytes (D-Bus wire format).
    pub fn serialize(&self) -> Vec<u8> {
        // Build header fields
        let mut fields = Marshaller::new_le();
        self.marshal_header_fields(&mut fields);

        // Build the full message
        let fields_data = &fields.data;
        let body_len = self.body.len() as u32;
        let fields_len = fields_data.len() as u32;

        let mut msg = Marshaller::new_le();

        // Fixed header (12 bytes before fields_len)
        msg.write_byte(b'l'); // Little-endian
        msg.write_byte(self.msg_type as u8);
        msg.write_byte(self.flags);
        msg.write_byte(crate::DBUS_PROTOCOL_VERSION);
        msg.write_u32(body_len);
        msg.write_u32(self.serial);
        msg.write_u32(fields_len);

        // Header fields
        msg.data.extend_from_slice(fields_data);

        // Pad header to 8-byte boundary before body
        let padded = align_to(msg.data.len(), 8);
        while msg.data.len() < padded {
            msg.data.push(0);
        }

        // Body
        msg.data.extend_from_slice(&self.body);

        msg.data
    }

    fn marshal_header_fields(&self, m: &mut Marshaller) {
        // Each header field is a STRUCT(BYTE, VARIANT) — 8-byte aligned
        if let Some(ref path) = self.path {
            m.pad_to(8);
            m.write_byte(HeaderField::Path as u8);
            m.write_signature("o");
            m.write_string(path);
        }
        if let Some(ref iface) = self.interface {
            m.pad_to(8);
            m.write_byte(HeaderField::Interface as u8);
            m.write_signature("s");
            m.write_string(iface);
        }
        if let Some(ref member) = self.member {
            m.pad_to(8);
            m.write_byte(HeaderField::Member as u8);
            m.write_signature("s");
            m.write_string(member);
        }
        if let Some(ref error_name) = self.error_name {
            m.pad_to(8);
            m.write_byte(HeaderField::ErrorName as u8);
            m.write_signature("s");
            m.write_string(error_name);
        }
        if let Some(reply_serial) = self.reply_serial {
            m.pad_to(8);
            m.write_byte(HeaderField::ReplySerial as u8);
            m.write_signature("u");
            m.write_u32(reply_serial);
        }
        if let Some(ref dest) = self.destination {
            m.pad_to(8);
            m.write_byte(HeaderField::Destination as u8);
            m.write_signature("s");
            m.write_string(dest);
        }
        if let Some(ref sender) = self.sender {
            m.pad_to(8);
            m.write_byte(HeaderField::Sender as u8);
            m.write_signature("s");
            m.write_string(sender);
        }
        if let Some(ref sig) = self.signature {
            m.pad_to(8);
            m.write_byte(HeaderField::Signature as u8);
            m.write_signature("g");
            m.write_signature(sig);
        }
        if let Some(fds) = self.unix_fds {
            m.pad_to(8);
            m.write_byte(HeaderField::UnixFds as u8);
            m.write_signature("u");
            m.write_u32(fds);
        }
    }

    /// Parse a D-Bus message from bytes.
    pub fn parse(data: &[u8]) -> Option<(Message, usize)> {
        if data.len() < 16 {
            return None;
        }

        let endian = data[0];
        if endian != b'l' && endian != b'B' {
            return None;
        }

        let msg_type = MessageType::from(data[1]);
        let flags = data[2];
        let _version = data[3];

        let mut u = Unmarshaller::new(data, endian);
        u.pos = 4;

        let body_len = u.read_u32()? as usize;
        let serial = u.read_u32()?;
        let fields_len = u.read_u32()? as usize;

        let fields_end = u.pos + fields_len;
        if fields_end > data.len() {
            return None;
        }

        let mut msg = Message::new(msg_type, serial);
        msg.flags = flags;

        // Parse header fields
        while u.pos < fields_end {
            u.pad_to(8);
            if u.pos >= fields_end {
                break;
            }
            let field_code = u.read_byte()?;
            let _sig = u.read_signature()?;

            match field_code {
                1 => msg.path = u.read_string(),           // PATH
                2 => msg.interface = u.read_string(),      // INTERFACE
                3 => msg.member = u.read_string(),         // MEMBER
                4 => msg.error_name = u.read_string(),     // ERROR_NAME
                5 => msg.reply_serial = u.read_u32(),      // REPLY_SERIAL
                6 => msg.destination = u.read_string(),    // DESTINATION
                7 => msg.sender = u.read_string(),         // SENDER
                8 => msg.signature = u.read_signature(),   // SIGNATURE
                9 => msg.unix_fds = u.read_u32(),          // UNIX_FDS
                _ => {
                    // Skip unknown field — we can't know the size without
                    // parsing the variant signature, so bail on unknowns
                    break;
                }
            }
        }

        // Skip to body (8-byte aligned after header)
        let header_end = 12 + 4 + fields_len; // fixed(12) + fields_len(4) + fields
        let body_start = align_to(header_end, 8);
        let total_len = body_start + body_len;

        if total_len > data.len() {
            return None; // Incomplete message
        }

        if body_len > 0 && body_start < data.len() {
            msg.body = data[body_start..body_start + body_len].to_vec();
        }

        Some((msg, total_len))
    }
}
