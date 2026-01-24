//! SSH Transport Layer (RFC 4253)
//!
//! Handles protocol version exchange, binary packet protocol,
//! and encryption after key exchange using oxide-std abstractions.

use alloc::string::String;
use alloc::vec::Vec;
use oxide_std::io::{self, Read, Write};
use oxide_std::net::TcpStream;

use crate::crypto::SshCipher;
use crate::kex::KexState;

/// SSH client version string
pub const SSH_CLIENT_VERSION: &[u8] = b"SSH-2.0-OXIDE_SSH_2.0\r\n";

/// Maximum packet size (256KB)
const MAX_PACKET_SIZE: usize = 262144;

/// SSH message types
pub mod msg {
    pub const DISCONNECT: u8 = 1;
    pub const IGNORE: u8 = 2;
    pub const UNIMPLEMENTED: u8 = 3;
    pub const DEBUG: u8 = 4;
    pub const SERVICE_REQUEST: u8 = 5;
    pub const SERVICE_ACCEPT: u8 = 6;
    pub const KEXINIT: u8 = 20;
    pub const NEWKEYS: u8 = 21;
    pub const KEX_ECDH_INIT: u8 = 30;
    pub const KEX_ECDH_REPLY: u8 = 31;
    pub const USERAUTH_REQUEST: u8 = 50;
    pub const USERAUTH_FAILURE: u8 = 51;
    pub const USERAUTH_SUCCESS: u8 = 52;
    pub const USERAUTH_BANNER: u8 = 53;
    pub const CHANNEL_OPEN: u8 = 90;
    pub const CHANNEL_OPEN_CONFIRMATION: u8 = 91;
    pub const CHANNEL_OPEN_FAILURE: u8 = 92;
    pub const CHANNEL_WINDOW_ADJUST: u8 = 93;
    pub const CHANNEL_DATA: u8 = 94;
    pub const CHANNEL_EXTENDED_DATA: u8 = 95;
    pub const CHANNEL_EOF: u8 = 96;
    pub const CHANNEL_CLOSE: u8 = 97;
    pub const CHANNEL_REQUEST: u8 = 98;
    pub const CHANNEL_SUCCESS: u8 = 99;
    pub const CHANNEL_FAILURE: u8 = 100;
}

/// SSH transport error
#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    Protocol,
    InvalidPacket,
    Decryption,
    KeyExchange,
    Closed,
    HostKeyVerification,
    AuthFailed,
}

impl From<io::Error> for TransportError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            TransportError::Closed
        } else {
            TransportError::Io(e)
        }
    }
}

pub type Result<T> = core::result::Result<T, TransportError>;

/// SSH Transport state
pub struct SshTransport {
    /// TCP stream
    stream: TcpStream,
    /// Client version string
    client_version: Vec<u8>,
    /// Server version string
    server_version: Vec<u8>,
    /// Sequence number for outgoing packets
    send_seq: u32,
    /// Sequence number for incoming packets
    recv_seq: u32,
    /// Encryption cipher
    send_cipher: Option<SshCipher>,
    /// Decryption cipher
    recv_cipher: Option<SshCipher>,
    /// Session ID (hash of first key exchange)
    session_id: Option<[u8; 32]>,
    /// Key exchange state
    kex: KexState,
}

impl SshTransport {
    /// Create new transport from a connected TCP stream
    pub fn new(stream: TcpStream) -> Self {
        SshTransport {
            stream,
            client_version: SSH_CLIENT_VERSION[..SSH_CLIENT_VERSION.len() - 2].to_vec(),
            server_version: Vec::new(),
            send_seq: 0,
            recv_seq: 0,
            send_cipher: None,
            recv_cipher: None,
            session_id: None,
            kex: KexState::new(),
        }
    }

    /// Perform SSH version exchange
    pub fn version_exchange(&mut self) -> Result<()> {
        // Send our version string
        self.stream.write_all(SSH_CLIENT_VERSION)?;

        // Read server version string
        let mut version = Vec::with_capacity(256);
        let mut retry_count = 0;
        const MAX_RETRIES: i32 = 1000;

        loop {
            let mut buf = [0u8; 1];
            match self.stream.read(&mut buf) {
                Ok(0) => return Err(TransportError::Closed),
                Ok(_) => {
                    retry_count = 0;
                    version.push(buf[0]);

                    // Look for line ending
                    if version.ends_with(b"\r\n") {
                        version.truncate(version.len() - 2);
                        break;
                    }
                    if version.ends_with(b"\n") {
                        version.truncate(version.len() - 1);
                        break;
                    }
                    if version.len() > 255 {
                        return Err(TransportError::Protocol);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    retry_count += 1;
                    if retry_count > MAX_RETRIES {
                        return Err(TransportError::Io(e));
                    }
                    libc::sched_yield();
                    continue;
                }
                Err(e) => return Err(TransportError::Io(e)),
            }
        }

        // Verify SSH-2.0 prefix
        if version.len() < 8 || &version[..8] != b"SSH-2.0-" {
            return Err(TransportError::Protocol);
        }

        self.server_version = version;
        Ok(())
    }

    /// Send an SSH packet
    pub fn send_packet(&mut self, payload: &[u8]) -> Result<()> {
        if let Some(ref mut cipher) = self.send_cipher {
            let packet = cipher.encrypt_packet(payload, self.send_seq)?;
            self.stream.write_all(&packet)?;
        } else {
            let packet = build_unencrypted_packet(payload);
            self.stream.write_all(&packet)?;
        }
        self.send_seq = self.send_seq.wrapping_add(1);
        Ok(())
    }

    /// Receive an SSH packet
    pub fn recv_packet(&mut self) -> Result<Vec<u8>> {
        let payload = if let Some(ref mut cipher) = self.recv_cipher {
            cipher.decrypt_packet(&mut self.stream, self.recv_seq)?
        } else {
            recv_unencrypted_packet(&mut self.stream)?
        };
        self.recv_seq = self.recv_seq.wrapping_add(1);
        Ok(payload)
    }

    /// Get client version string
    pub fn client_version(&self) -> &[u8] {
        &self.client_version
    }

    /// Get server version string
    pub fn server_version(&self) -> &[u8] {
        &self.server_version
    }

    /// Set session ID
    pub fn set_session_id(&mut self, id: [u8; 32]) {
        if self.session_id.is_none() {
            self.session_id = Some(id);
        }
    }

    /// Get session ID
    pub fn session_id(&self) -> Option<&[u8; 32]> {
        self.session_id.as_ref()
    }

    /// Enable encryption
    pub fn enable_encryption(&mut self, send_cipher: SshCipher, recv_cipher: SshCipher) {
        self.send_cipher = Some(send_cipher);
        self.recv_cipher = Some(recv_cipher);
    }

    /// Get raw file descriptor for polling
    pub fn as_raw_fd(&self) -> i32 {
        self.stream.as_raw_fd()
    }

    /// Access kex state mutably
    pub fn kex(&mut self) -> &mut KexState {
        &mut self.kex
    }

    /// Access kex state immutably
    pub fn kex_ref(&self) -> &KexState {
        &self.kex
    }
}

/// Build an unencrypted SSH packet
fn build_unencrypted_packet(payload: &[u8]) -> Vec<u8> {
    let payload_len = payload.len();
    let padding_len = 8 - ((payload_len + 5) % 8);
    let padding_len = if padding_len < 4 {
        padding_len + 8
    } else {
        padding_len
    };
    let packet_len = 1 + payload_len + padding_len;

    let mut packet = Vec::with_capacity(4 + packet_len);
    packet.extend_from_slice(&(packet_len as u32).to_be_bytes());
    packet.push(padding_len as u8);
    packet.extend_from_slice(payload);
    packet.extend(core::iter::repeat(0).take(padding_len));
    packet
}

/// Receive an unencrypted SSH packet
fn recv_unencrypted_packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    // Read packet length
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;

    let packet_len = u32::from_be_bytes(len_buf) as usize;
    if packet_len > MAX_PACKET_SIZE || packet_len < 2 {
        return Err(TransportError::InvalidPacket);
    }

    // Read rest of packet
    let mut packet = alloc::vec![0u8; packet_len];
    stream.read_exact(&mut packet)?;

    // Extract payload
    let padding_len = packet[0] as usize;
    if padding_len >= packet_len {
        return Err(TransportError::InvalidPacket);
    }

    let payload_len = packet_len - 1 - padding_len;
    Ok(packet[1..1 + payload_len].to_vec())
}

// ============================================================================
// SSH String Encoding Helpers
// ============================================================================

/// Encode a string with length prefix
pub fn encode_string(s: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(4 + s.len());
    result.extend_from_slice(&(s.len() as u32).to_be_bytes());
    result.extend_from_slice(s);
    result
}

/// Encode a name list (comma-separated strings)
pub fn encode_name_list(names: &[&str]) -> Vec<u8> {
    let joined: Vec<u8> = names
        .iter()
        .map(|s| s.as_bytes())
        .collect::<Vec<_>>()
        .join(&b","[..]);
    encode_string(&joined)
}

/// Decode a length-prefixed string
pub fn decode_string(data: &[u8], offset: &mut usize) -> Result<Vec<u8>> {
    if *offset + 4 > data.len() {
        return Err(TransportError::InvalidPacket);
    }
    let len = u32::from_be_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]) as usize;
    *offset += 4;

    if *offset + len > data.len() {
        return Err(TransportError::InvalidPacket);
    }
    let s = data[*offset..*offset + len].to_vec();
    *offset += len;
    Ok(s)
}

/// Decode a u32 from big-endian bytes
pub fn decode_u32(data: &[u8], offset: &mut usize) -> Result<u32> {
    if *offset + 4 > data.len() {
        return Err(TransportError::InvalidPacket);
    }
    let val = u32::from_be_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    Ok(val)
}
