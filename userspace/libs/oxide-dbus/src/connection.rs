//! D-Bus Connection Management
//!
//! — PatchBay: A connection wraps an AF_UNIX socket to the bus daemon.
//! It handles SASL authentication, serial number allocation, and message
//! send/receive. GTK's GDBusConnection ultimately calls into this.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::message::Message;
use crate::MessageType;

/// D-Bus connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// SASL authentication in progress
    Authenticating,
    /// Authenticated, message mode active
    Connected,
}

/// A D-Bus connection to the message bus.
pub struct Connection {
    /// The AF_UNIX socket fd
    pub fd: i32,
    /// Connection state
    pub state: ConnectionState,
    /// Our unique bus name (e.g., ":1.42")
    pub unique_name: Option<String>,
    /// Server GUID
    pub guid: Option<String>,
    /// Next message serial number
    next_serial: AtomicU32,
    /// Receive buffer for partial reads
    recv_buf: Vec<u8>,
}

impl Connection {
    /// Create a new disconnected connection.
    pub fn new() -> Self {
        Connection {
            fd: -1,
            state: ConnectionState::Disconnected,
            unique_name: None,
            guid: None,
            next_serial: AtomicU32::new(1),
            recv_buf: Vec::new(),
        }
    }

    /// Allocate the next serial number for an outgoing message.
    pub fn next_serial(&self) -> u32 {
        self.next_serial.fetch_add(1, Ordering::Relaxed)
    }

    /// Append raw bytes from the socket to our receive buffer.
    pub fn feed_data(&mut self, data: &[u8]) {
        self.recv_buf.extend_from_slice(data);
    }

    /// Try to parse the next complete message from the receive buffer.
    pub fn try_recv_message(&mut self) -> Option<Message> {
        if self.recv_buf.len() < 16 {
            return None;
        }

        match Message::parse(&self.recv_buf) {
            Some((msg, consumed)) => {
                // Remove consumed bytes from buffer
                self.recv_buf = self.recv_buf[consumed..].to_vec();
                Some(msg)
            }
            None => None,
        }
    }

    /// Create the initial Hello method call (required first message after auth).
    /// The bus daemon assigns us a unique name in response.
    pub fn create_hello_message(&self) -> Message {
        Message::method_call(
            self.next_serial(),
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
        )
    }

    /// Create a RequestName method call.
    pub fn create_request_name(&self, name: &str, flags: u32) -> Message {
        let serial = self.next_serial();
        let mut msg = Message::method_call(
            serial,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
        );
        msg.signature = Some(String::from("su"));

        // Marshal body: STRING name + UINT32 flags
        let mut body = crate::wire::Marshaller::new_le();
        body.write_string(name);
        body.write_u32(flags);
        msg.body = body.data;

        msg
    }

    /// Process a Hello reply — extract our unique name.
    pub fn handle_hello_reply(&mut self, msg: &Message) -> bool {
        if msg.msg_type != MessageType::MethodReturn {
            return false;
        }
        // Body contains a single STRING (our unique name)
        let mut u = crate::wire::Unmarshaller::new(&msg.body, b'l');
        if let Some(name) = u.read_string() {
            self.unique_name = Some(name);
            true
        } else {
            false
        }
    }
}
