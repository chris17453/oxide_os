//! SSH Session Management
//!
//! Handles authentication, channels, PTY requests, and interactive sessions.

use alloc::vec::Vec;
use libc::poll::{PollFd, events, poll};

use crate::transport::{
    Result, SshTransport, TransportError, decode_string, decode_u32, encode_string, msg,
};

/// Request ssh-userauth service
pub fn request_userauth_service(transport: &mut SshTransport) -> Result<()> {
    let mut request = Vec::with_capacity(20);
    request.push(msg::SERVICE_REQUEST);
    request.extend_from_slice(&encode_string(b"ssh-userauth"));
    transport.send_packet(&request)?;

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
) -> Result<()> {
    let mut request = Vec::with_capacity(64 + username.len() + password.len());
    request.push(msg::USERAUTH_REQUEST);
    request.extend_from_slice(&encode_string(username));
    request.extend_from_slice(&encode_string(b"ssh-connection"));
    request.extend_from_slice(&encode_string(b"password"));
    request.push(0); // FALSE
    request.extend_from_slice(&encode_string(password));

    transport.send_packet(&request)?;

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
    local_channel: u32,
    remote_channel: u32,
    remote_window: u32,
    remote_max_packet: u32,
}

impl SshChannel {
    /// Open a session channel
    pub fn open_session(transport: &mut SshTransport) -> Result<Self> {
        let local_channel: u32 = 0;
        let initial_window: u32 = 1024 * 1024;
        let max_packet: u32 = 32768;

        let mut request = Vec::with_capacity(32);
        request.push(msg::CHANNEL_OPEN);
        request.extend_from_slice(&encode_string(b"session"));
        request.extend_from_slice(&local_channel.to_be_bytes());
        request.extend_from_slice(&initial_window.to_be_bytes());
        request.extend_from_slice(&max_packet.to_be_bytes());

        transport.send_packet(&request)?;

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
    pub fn request_pty(&self, transport: &mut SshTransport) -> Result<()> {
        let (cols, rows) = get_terminal_size();

        let mut request = Vec::with_capacity(64);
        request.push(msg::CHANNEL_REQUEST);
        request.extend_from_slice(&self.remote_channel.to_be_bytes());
        request.extend_from_slice(&encode_string(b"pty-req"));
        request.push(1); // want_reply
        request.extend_from_slice(&encode_string(b"xterm-256color"));
        request.extend_from_slice(&cols.to_be_bytes());
        request.extend_from_slice(&rows.to_be_bytes());
        request.extend_from_slice(&0u32.to_be_bytes());
        request.extend_from_slice(&0u32.to_be_bytes());
        request.extend_from_slice(&encode_string(b""));

        transport.send_packet(&request)?;

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
    pub fn request_shell(&self, transport: &mut SshTransport) -> Result<()> {
        let mut request = Vec::with_capacity(32);
        request.push(msg::CHANNEL_REQUEST);
        request.extend_from_slice(&self.remote_channel.to_be_bytes());
        request.extend_from_slice(&encode_string(b"shell"));
        request.push(1); // want_reply

        transport.send_packet(&request)?;

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
    pub fn send_data(&mut self, transport: &mut SshTransport, data: &[u8]) -> Result<()> {
        let mut offset = 0;
        while offset < data.len() {
            let chunk_size = (data.len() - offset)
                .min(self.remote_max_packet as usize)
                .min(self.remote_window as usize);

            if chunk_size == 0 {
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

    /// Process incoming packet
    pub fn process_packet(&mut self, packet: &[u8]) -> Result<Option<Vec<u8>>> {
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
            msg::CHANNEL_EOF => Ok(None),
            msg::CHANNEL_CLOSE => Err(TransportError::Closed),
            msg::CHANNEL_REQUEST => Ok(None),
            _ => Ok(None),
        }
    }

    /// Send window adjust
    pub fn send_window_adjust(&self, transport: &mut SshTransport, bytes: u32) -> Result<()> {
        let mut packet = Vec::with_capacity(9);
        packet.push(msg::CHANNEL_WINDOW_ADJUST);
        packet.extend_from_slice(&self.remote_channel.to_be_bytes());
        packet.extend_from_slice(&bytes.to_be_bytes());
        transport.send_packet(&packet)
    }

    /// Close the channel
    pub fn close(&self, transport: &mut SshTransport) -> Result<()> {
        let mut packet = Vec::with_capacity(5);
        packet.push(msg::CHANNEL_CLOSE);
        packet.extend_from_slice(&self.remote_channel.to_be_bytes());
        transport.send_packet(&packet)
    }
}

/// Get terminal size
fn get_terminal_size() -> (u32, u32) {
    (80, 24)
}

/// Run interactive session
pub fn run_session(transport: &mut SshTransport, channel: &mut SshChannel) -> Result<()> {
    let stdin_fd = 0;
    let socket_fd = transport.as_raw_fd();

    set_raw_mode(stdin_fd);

    let mut local_window = 1024 * 1024u32;
    let mut stdin_buf = [0u8; 256];

    loop {
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

        let ready = poll(&mut pollfds, 100);
        if ready < 0 {
            break;
        }

        // Check stdin
        if pollfds[0].revents & events::POLLIN != 0 {
            let n = libc::read(stdin_fd, &mut stdin_buf);
            if n > 0 {
                // Check for disconnect escape sequence
                if n == 2 && stdin_buf[0] == b'~' && stdin_buf[1] == b'.' {
                    channel.close(transport)?;
                    break;
                }
                channel.send_data(transport, &stdin_buf[..n as usize])?;
            } else if n == 0 {
                channel.close(transport)?;
                break;
            }
        }

        // Check socket
        if pollfds[1].revents & events::POLLIN != 0 {
            match transport.recv_packet() {
                Ok(packet) => match channel.process_packet(&packet) {
                    Ok(Some(data)) => {
                        let _ = libc::write(1, &data);
                        local_window = local_window.saturating_sub(data.len() as u32);

                        if local_window < 512 * 1024 {
                            let adjust = 1024 * 1024 - local_window;
                            channel.send_window_adjust(transport, adjust)?;
                            local_window += adjust;
                        }
                    }
                    Ok(None) => {}
                    Err(TransportError::Closed) => break,
                    Err(_) => {}
                },
                Err(TransportError::Closed) => break,
                Err(_) => {}
            }
        }

        if pollfds[1].revents & events::POLLHUP != 0 {
            break;
        }
    }

    restore_terminal_mode(stdin_fd);

    Ok(())
}

fn set_raw_mode(_fd: i32) {
    // Terminal mode configuration placeholder
}

fn restore_terminal_mode(_fd: i32) {
    // Terminal mode restoration placeholder
}
