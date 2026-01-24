//! SSH Session Management (Client Side)
//!
//! Handles:
//! - Password authentication
//! - Channel management
//! - PTY requests
//! - Interactive shell sessions

use alloc::vec::Vec;
use libc::poll::{PollFd, events, poll};
use libc::*;

use crate::transport::{
    SshTransport, TransportError, TransportResult, decode_string, decode_u32, encode_string, msg,
};

/// Request ssh-userauth service
pub fn request_userauth_service(transport: &mut SshTransport) -> TransportResult<()> {
    // SERVICE_REQUEST
    let mut request = Vec::with_capacity(20);
    request.push(msg::SERVICE_REQUEST);
    request.extend_from_slice(&encode_string(b"ssh-userauth"));
    transport.send_packet(&request)?;

    // Wait for SERVICE_ACCEPT
    let response = transport.recv_packet()?;
    if response.is_empty() || response[0] != msg::SERVICE_ACCEPT {
        return Err(TransportError::Protocol);
    }

    Ok(())
}

/// Authenticate with password
pub fn authenticate_password(
    transport: &mut SshTransport,
    username: &[u8],
    password: &[u8],
) -> TransportResult<()> {
    // USERAUTH_REQUEST with password
    let mut request = Vec::with_capacity(64 + username.len() + password.len());
    request.push(msg::USERAUTH_REQUEST);
    request.extend_from_slice(&encode_string(username));
    request.extend_from_slice(&encode_string(b"ssh-connection"));
    request.extend_from_slice(&encode_string(b"password"));
    request.push(0); // FALSE (not changing password)
    request.extend_from_slice(&encode_string(password));

    transport.send_packet(&request)?;

    // Wait for response
    let response = transport.recv_packet()?;
    if response.is_empty() {
        return Err(TransportError::Protocol);
    }

    match response[0] {
        msg::USERAUTH_SUCCESS => Ok(()),
        msg::USERAUTH_FAILURE => Err(TransportError::AuthFailed),
        msg::USERAUTH_BANNER => {
            // Skip banner and wait for real response
            let response = transport.recv_packet()?;
            if response.is_empty() || response[0] != msg::USERAUTH_SUCCESS {
                return Err(TransportError::AuthFailed);
            }
            Ok(())
        }
        _ => Err(TransportError::Protocol),
    }
}

/// SSH Channel
pub struct SshChannel {
    /// Local channel number
    local_channel: u32,
    /// Remote channel number
    remote_channel: u32,
    /// Remote window size
    remote_window: u32,
    /// Remote max packet size
    remote_max_packet: u32,
}

impl SshChannel {
    /// Open a session channel
    pub fn open_session(transport: &mut SshTransport) -> TransportResult<Self> {
        let local_channel: u32 = 0; // First channel
        let initial_window: u32 = 1024 * 1024; // 1MB window
        let max_packet: u32 = 32768; // 32KB max packet

        // CHANNEL_OPEN
        let mut request = Vec::with_capacity(32);
        request.push(msg::CHANNEL_OPEN);
        request.extend_from_slice(&encode_string(b"session"));
        request.extend_from_slice(&local_channel.to_be_bytes());
        request.extend_from_slice(&initial_window.to_be_bytes());
        request.extend_from_slice(&max_packet.to_be_bytes());

        transport.send_packet(&request)?;

        // Wait for CHANNEL_OPEN_CONFIRMATION
        let response = transport.recv_packet()?;
        if response.is_empty() {
            return Err(TransportError::Protocol);
        }

        match response[0] {
            msg::CHANNEL_OPEN_CONFIRMATION => {
                let mut offset = 1;
                let recipient = decode_u32(&response, &mut offset)?;
                let remote_channel = decode_u32(&response, &mut offset)?;
                let remote_window = decode_u32(&response, &mut offset)?;
                let remote_max_packet = decode_u32(&response, &mut offset)?;

                if recipient != local_channel {
                    return Err(TransportError::Protocol);
                }

                Ok(SshChannel {
                    local_channel,
                    remote_channel,
                    remote_window,
                    remote_max_packet,
                })
            }
            msg::CHANNEL_OPEN_FAILURE => Err(TransportError::Protocol),
            _ => Err(TransportError::Protocol),
        }
    }

    /// Request a PTY
    pub fn request_pty(&self, transport: &mut SshTransport) -> TransportResult<()> {
        // Get terminal size
        let (cols, rows) = get_terminal_size();

        // CHANNEL_REQUEST for pty-req
        let mut request = Vec::with_capacity(64);
        request.push(msg::CHANNEL_REQUEST);
        request.extend_from_slice(&self.remote_channel.to_be_bytes());
        request.extend_from_slice(&encode_string(b"pty-req"));
        request.push(1); // want_reply

        // Terminal type
        request.extend_from_slice(&encode_string(b"xterm-256color"));

        // Terminal dimensions
        request.extend_from_slice(&cols.to_be_bytes()); // width chars
        request.extend_from_slice(&rows.to_be_bytes()); // height chars
        request.extend_from_slice(&0u32.to_be_bytes()); // width pixels
        request.extend_from_slice(&0u32.to_be_bytes()); // height pixels

        // Terminal modes (empty for now)
        request.extend_from_slice(&encode_string(b""));

        transport.send_packet(&request)?;

        // Wait for response
        let response = transport.recv_packet()?;
        if response.is_empty() {
            return Err(TransportError::Protocol);
        }

        match response[0] {
            msg::CHANNEL_SUCCESS => Ok(()),
            msg::CHANNEL_FAILURE => Err(TransportError::Protocol),
            _ => Err(TransportError::Protocol),
        }
    }

    /// Request shell
    pub fn request_shell(&self, transport: &mut SshTransport) -> TransportResult<()> {
        // CHANNEL_REQUEST for shell
        let mut request = Vec::with_capacity(32);
        request.push(msg::CHANNEL_REQUEST);
        request.extend_from_slice(&self.remote_channel.to_be_bytes());
        request.extend_from_slice(&encode_string(b"shell"));
        request.push(1); // want_reply

        transport.send_packet(&request)?;

        // Wait for response
        let response = transport.recv_packet()?;
        if response.is_empty() {
            return Err(TransportError::Protocol);
        }

        match response[0] {
            msg::CHANNEL_SUCCESS => Ok(()),
            msg::CHANNEL_FAILURE => Err(TransportError::Protocol),
            _ => Err(TransportError::Protocol),
        }
    }

    /// Send data to the channel
    pub fn send_data(&mut self, transport: &mut SshTransport, data: &[u8]) -> TransportResult<()> {
        let mut offset = 0;
        while offset < data.len() {
            let chunk_size = (data.len() - offset)
                .min(self.remote_max_packet as usize)
                .min(self.remote_window as usize);

            if chunk_size == 0 {
                // Window is full, need to wait for window adjust
                break;
            }

            let mut packet = Vec::with_capacity(9 + chunk_size);
            packet.push(msg::CHANNEL_DATA);
            packet.extend_from_slice(&self.remote_channel.to_be_bytes());
            packet.extend_from_slice(&(chunk_size as u32).to_be_bytes());
            packet.extend_from_slice(&data[offset..offset + chunk_size]);

            transport.send_packet(&packet)?;

            self.remote_window = self.remote_window.saturating_sub(chunk_size as u32);
            offset += chunk_size;
        }

        Ok(())
    }

    /// Process incoming packets
    pub fn process_packet(&mut self, packet: &[u8]) -> TransportResult<Option<Vec<u8>>> {
        if packet.is_empty() {
            return Err(TransportError::Protocol);
        }

        match packet[0] {
            msg::CHANNEL_DATA => {
                let mut offset = 1;
                let channel = decode_u32(packet, &mut offset)?;
                if channel != self.local_channel {
                    return Ok(None);
                }
                let data = decode_string(packet, &mut offset)?;
                Ok(Some(data))
            }
            msg::CHANNEL_EXTENDED_DATA => {
                let mut offset = 1;
                let channel = decode_u32(packet, &mut offset)?;
                if channel != self.local_channel {
                    return Ok(None);
                }
                let _data_type = decode_u32(packet, &mut offset)?;
                let data = decode_string(packet, &mut offset)?;
                // Extended data (stderr) - treat same as regular data for now
                Ok(Some(data))
            }
            msg::CHANNEL_WINDOW_ADJUST => {
                let mut offset = 1;
                let channel = decode_u32(packet, &mut offset)?;
                if channel != self.local_channel {
                    return Ok(None);
                }
                let bytes_to_add = decode_u32(packet, &mut offset)?;
                self.remote_window = self.remote_window.saturating_add(bytes_to_add);
                Ok(None)
            }
            msg::CHANNEL_EOF => {
                // Remote side closed write end
                Ok(None)
            }
            msg::CHANNEL_CLOSE => {
                // Channel closed
                Err(TransportError::Closed)
            }
            msg::CHANNEL_REQUEST => {
                // Server sending a request (e.g., exit-status)
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Send window adjust
    pub fn send_window_adjust(
        &self,
        transport: &mut SshTransport,
        bytes: u32,
    ) -> TransportResult<()> {
        let mut packet = Vec::with_capacity(9);
        packet.push(msg::CHANNEL_WINDOW_ADJUST);
        packet.extend_from_slice(&self.remote_channel.to_be_bytes());
        packet.extend_from_slice(&bytes.to_be_bytes());
        transport.send_packet(&packet)
    }

    /// Send channel close
    pub fn close(&self, transport: &mut SshTransport) -> TransportResult<()> {
        let mut packet = Vec::with_capacity(5);
        packet.push(msg::CHANNEL_CLOSE);
        packet.extend_from_slice(&self.remote_channel.to_be_bytes());
        transport.send_packet(&packet)
    }
}

/// Get terminal size
fn get_terminal_size() -> (u32, u32) {
    // Default to 80x24
    (80, 24)
}

/// Run interactive session with proper I/O multiplexing
pub fn run_session(transport: &mut SshTransport, channel: &mut SshChannel) -> TransportResult<()> {
    let stdin_fd = 0;
    let socket_fd = transport.fd();

    // Set terminal to raw mode for proper character handling
    set_raw_mode(stdin_fd);

    let mut local_window = 1024 * 1024u32;
    let mut stdin_buf = [0u8; 256];

    loop {
        // Use poll to wait for either stdin or socket data
        let mut pollfds = [
            PollFd {
                fd: stdin_fd,
                events: events::POLLIN,
                revents: 0,
            },
            PollFd {
                fd: socket_fd,
                events: events::POLLIN,
                revents: 0,
            },
        ];

        let ready = poll(&mut pollfds, 100); // 100ms timeout
        if ready < 0 {
            // Error
            break;
        }

        // Check stdin
        if pollfds[0].revents & events::POLLIN != 0 {
            let n = read(stdin_fd, &mut stdin_buf);
            if n > 0 {
                // Check for escape sequence (e.g., ~. to disconnect)
                if n == 2 && stdin_buf[0] == b'~' && stdin_buf[1] == b'.' {
                    // Disconnect
                    channel.close(transport)?;
                    break;
                }
                channel.send_data(transport, &stdin_buf[..n as usize])?;
            } else if n == 0 {
                // EOF on stdin
                channel.close(transport)?;
                break;
            }
        }

        // Check socket
        if pollfds[1].revents & events::POLLIN != 0 {
            match transport.recv_packet() {
                Ok(packet) => {
                    match channel.process_packet(&packet) {
                        Ok(Some(data)) => {
                            // Write to stdout
                            let _ = write(1, &data);
                            local_window = local_window.saturating_sub(data.len() as u32);

                            // Send window adjust if needed
                            if local_window < 512 * 1024 {
                                let adjust = 1024 * 1024 - local_window;
                                channel.send_window_adjust(transport, adjust)?;
                                local_window += adjust;
                            }
                        }
                        Ok(None) => {}
                        Err(TransportError::Closed) => {
                            break;
                        }
                        Err(_) => {}
                    }
                }
                Err(TransportError::Closed) => {
                    break;
                }
                Err(_) => {
                    // Might be timeout or other error
                }
            }
        }

        // Check for hangup
        if pollfds[1].revents & events::POLLHUP != 0 {
            break;
        }
    }

    // Restore terminal mode
    restore_terminal_mode(stdin_fd);

    Ok(())
}

/// Set terminal to raw mode
fn set_raw_mode(_fd: i32) {
    // In OXIDE, implement terminal mode changes
    // For now, this is a placeholder
}

/// Restore terminal to normal mode
fn restore_terminal_mode(_fd: i32) {
    // In OXIDE, implement terminal mode restoration
    // For now, this is a placeholder
}
