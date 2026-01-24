//! SSH Transport Layer (RFC 4253)
//!
//! Handles:
//! - Protocol version exchange
//! - Binary packet protocol
//! - Key exchange initiation
//! - Encryption/decryption after key exchange

use alloc::string::String;
use alloc::vec::Vec;
use libc::socket::{recv, send};
use libc::*;

use crate::crypto::{SshCipher, host_public_key};
use crate::kex::{KexState, perform_key_exchange};

/// SSH version string
pub const SSH_VERSION: &[u8] = b"SSH-2.0-OXIDE_SSHD_1.0\r\n";

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
    // Diffie-Hellman
    pub const KEXDH_INIT: u8 = 30;
    pub const KEXDH_REPLY: u8 = 31;
    // ECDH
    pub const KEX_ECDH_INIT: u8 = 30;
    pub const KEX_ECDH_REPLY: u8 = 31;
    // User authentication
    pub const USERAUTH_REQUEST: u8 = 50;
    pub const USERAUTH_FAILURE: u8 = 51;
    pub const USERAUTH_SUCCESS: u8 = 52;
    pub const USERAUTH_BANNER: u8 = 53;
    // Channel
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

/// Disconnect reason codes
pub mod disconnect {
    pub const HOST_NOT_ALLOWED_TO_CONNECT: u32 = 1;
    pub const PROTOCOL_ERROR: u32 = 2;
    pub const KEY_EXCHANGE_FAILED: u32 = 3;
    pub const RESERVED: u32 = 4;
    pub const MAC_ERROR: u32 = 5;
    pub const COMPRESSION_ERROR: u32 = 6;
    pub const SERVICE_NOT_AVAILABLE: u32 = 7;
    pub const PROTOCOL_VERSION_NOT_SUPPORTED: u32 = 8;
    pub const HOST_KEY_NOT_VERIFIABLE: u32 = 9;
    pub const CONNECTION_LOST: u32 = 10;
    pub const BY_APPLICATION: u32 = 11;
    pub const TOO_MANY_CONNECTIONS: u32 = 12;
    pub const AUTH_CANCELLED_BY_USER: u32 = 13;
    pub const NO_MORE_AUTH_METHODS_AVAILABLE: u32 = 14;
    pub const ILLEGAL_USER_NAME: u32 = 15;
}

/// SSH transport error
#[derive(Debug)]
pub enum TransportError {
    /// I/O error
    Io,
    /// Protocol error
    Protocol,
    /// Invalid packet
    InvalidPacket,
    /// Decryption error
    Decryption,
    /// Key exchange failed
    KeyExchange,
    /// Connection closed
    Closed,
}

pub type TransportResult<T> = Result<T, TransportError>;

/// SSH Transport state
pub struct SshTransport {
    /// Socket file descriptor
    fd: i32,
    /// Client version string
    client_version: Vec<u8>,
    /// Server version string (our version)
    server_version: Vec<u8>,
    /// Sequence number for outgoing packets
    send_seq: u32,
    /// Sequence number for incoming packets
    recv_seq: u32,
    /// Encryption cipher (after key exchange)
    send_cipher: Option<SshCipher>,
    /// Decryption cipher (after key exchange)
    recv_cipher: Option<SshCipher>,
    /// Session ID (hash of first key exchange)
    session_id: Option<[u8; 32]>,
    /// Key exchange state
    kex: KexState,
}

impl SshTransport {
    /// Create new transport for a connected socket
    pub fn new(fd: i32) -> TransportResult<Self> {
        Ok(SshTransport {
            fd,
            client_version: Vec::new(),
            server_version: SSH_VERSION[..SSH_VERSION.len() - 2].to_vec(), // Strip \r\n
            send_seq: 0,
            recv_seq: 0,
            send_cipher: None,
            recv_cipher: None,
            session_id: None,
            kex: KexState::new(),
        })
    }

    /// Perform SSH version exchange
    pub fn version_exchange(&mut self) -> TransportResult<()> {
        // Send our version string
        self.send_raw(SSH_VERSION)?;

        // Read client version string
        let mut version = Vec::with_capacity(256);
        loop {
            let mut buf = [0u8; 1];
            let n = recv(self.fd, &mut buf, 0);
            if n <= 0 {
                return Err(TransportError::Io);
            }

            version.push(buf[0]);

            // Look for \r\n or \n
            if version.len() >= 2
                && version[version.len() - 2] == b'\r'
                && version[version.len() - 1] == b'\n'
            {
                version.truncate(version.len() - 2);
                break;
            }
            if version.len() >= 1 && version[version.len() - 1] == b'\n' {
                version.truncate(version.len() - 1);
                break;
            }

            if version.len() > 255 {
                return Err(TransportError::Protocol);
            }
        }

        // Verify it starts with SSH-2.0-
        if version.len() < 8 || &version[..8] != b"SSH-2.0-" {
            return Err(TransportError::Protocol);
        }

        self.client_version = version;
        Ok(())
    }

    /// Perform key exchange
    pub fn key_exchange(&mut self) -> TransportResult<()> {
        perform_key_exchange(self)
    }

    /// Send raw bytes (before encryption)
    pub fn send_raw(&mut self, data: &[u8]) -> TransportResult<()> {
        let mut sent = 0;
        while sent < data.len() {
            let n = send(self.fd, &data[sent..], 0);
            if n <= 0 {
                return Err(TransportError::Io);
            }
            sent += n as usize;
        }
        Ok(())
    }

    /// Receive raw bytes
    pub fn recv_raw(&mut self, buf: &mut [u8]) -> TransportResult<usize> {
        let mut received = 0;
        while received < buf.len() {
            let n = recv(self.fd, &mut buf[received..], 0);
            if n < 0 {
                return Err(TransportError::Io);
            }
            if n == 0 {
                if received == 0 {
                    return Err(TransportError::Closed);
                }
                break;
            }
            received += n as usize;
        }
        Ok(received)
    }

    /// Receive exactly n bytes
    pub fn recv_exact(&mut self, buf: &mut [u8]) -> TransportResult<()> {
        let mut received = 0;
        while received < buf.len() {
            let n = recv(self.fd, &mut buf[received..], 0);
            if n <= 0 {
                return Err(TransportError::Io);
            }
            received += n as usize;
        }
        Ok(())
    }

    /// Send an SSH packet
    pub fn send_packet(&mut self, payload: &[u8]) -> TransportResult<()> {
        if let Some(ref mut cipher) = self.send_cipher {
            // Encrypted packet
            let packet = cipher.encrypt_packet(payload, self.send_seq)?;
            self.send_raw(&packet)?;
        } else {
            // Unencrypted packet
            let packet = build_unencrypted_packet(payload);
            self.send_raw(&packet)?;
        }
        self.send_seq = self.send_seq.wrapping_add(1);
        Ok(())
    }

    /// Receive an SSH packet
    pub fn recv_packet(&mut self) -> TransportResult<Vec<u8>> {
        let payload = if let Some(ref mut cipher) = self.recv_cipher {
            // Encrypted packet
            cipher.decrypt_packet(self.fd, self.recv_seq)?
        } else {
            // Unencrypted packet
            recv_unencrypted_packet(self.fd)?
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

    /// Set session ID (first exchange hash)
    pub fn set_session_id(&mut self, id: [u8; 32]) {
        if self.session_id.is_none() {
            self.session_id = Some(id);
        }
    }

    /// Get session ID
    pub fn session_id(&self) -> Option<&[u8; 32]> {
        self.session_id.as_ref()
    }

    /// Enable encryption with derived keys
    pub fn enable_encryption(&mut self, send_cipher: SshCipher, recv_cipher: SshCipher) {
        self.send_cipher = Some(send_cipher);
        self.recv_cipher = Some(recv_cipher);
    }

    /// Get socket FD
    pub fn fd(&self) -> i32 {
        self.fd
    }

    /// Access kex state
    pub fn kex(&mut self) -> &mut KexState {
        &mut self.kex
    }

    /// Get kex state immutably
    pub fn kex_ref(&self) -> &KexState {
        &self.kex
    }

    /// Get current receive sequence number
    pub fn recv_sequence(&self) -> u32 {
        self.recv_seq
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

    // Packet length (4 bytes, big-endian)
    packet.extend_from_slice(&(packet_len as u32).to_be_bytes());

    // Padding length (1 byte)
    packet.push(padding_len as u8);

    // Payload
    packet.extend_from_slice(payload);

    // Padding (random bytes, but we use zeros for simplicity)
    for _ in 0..padding_len {
        packet.push(0);
    }

    packet
}

/// Receive an unencrypted SSH packet
fn recv_unencrypted_packet(fd: i32) -> TransportResult<Vec<u8>> {
    // Read packet length (4 bytes)
    let mut len_buf = [0u8; 4];
    let mut received = 0;
    while received < 4 {
        let n = recv(fd, &mut len_buf[received..], 0);
        if n <= 0 {
            return Err(TransportError::Io);
        }
        received += n as usize;
    }

    let packet_len = u32::from_be_bytes(len_buf) as usize;
    if packet_len > MAX_PACKET_SIZE || packet_len < 2 {
        return Err(TransportError::InvalidPacket);
    }

    // Read rest of packet
    let mut packet = alloc::vec![0u8; packet_len];
    received = 0;
    while received < packet_len {
        let n = recv(fd, &mut packet[received..], 0);
        if n <= 0 {
            return Err(TransportError::Io);
        }
        received += n as usize;
    }

    // Extract payload
    let padding_len = packet[0] as usize;
    if padding_len >= packet_len {
        return Err(TransportError::InvalidPacket);
    }

    let payload_len = packet_len - 1 - padding_len;
    Ok(packet[1..1 + payload_len].to_vec())
}

/// SSH string encoding helpers
pub fn encode_string(s: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(4 + s.len());
    result.extend_from_slice(&(s.len() as u32).to_be_bytes());
    result.extend_from_slice(s);
    result
}

pub fn encode_name_list(names: &[&str]) -> Vec<u8> {
    let joined: Vec<u8> = names
        .iter()
        .map(|s| s.as_bytes())
        .collect::<Vec<_>>()
        .join(&b","[..]);
    encode_string(&joined)
}

pub fn decode_string(data: &[u8], offset: &mut usize) -> TransportResult<Vec<u8>> {
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

pub fn decode_u32(data: &[u8], offset: &mut usize) -> TransportResult<u32> {
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

pub fn decode_u8(data: &[u8], offset: &mut usize) -> TransportResult<u8> {
    if *offset >= data.len() {
        return Err(TransportError::InvalidPacket);
    }
    let val = data[*offset];
    *offset += 1;
    Ok(val)
}
