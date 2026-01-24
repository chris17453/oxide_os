//! SSH Channel Layer (RFC 4254)
//!
//! Handles channel multiplexing and window management.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::transport::{decode_string, decode_u32, msg, SshTransport, TransportError, TransportResult};

/// Maximum window size
const MAX_WINDOW_SIZE: u32 = 2 * 1024 * 1024;

/// Maximum packet size
const MAX_PACKET_SIZE: u32 = 32768;

/// Channel state
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    Opening,
    Open,
    Closing,
    Closed,
}

/// A single SSH channel
pub struct Channel {
    /// Local channel ID
    pub local_id: u32,
    /// Remote channel ID
    pub remote_id: u32,
    /// Channel state
    pub state: ChannelState,
    /// Local window size
    pub local_window: u32,
    /// Remote window size
    pub remote_window: u32,
    /// Maximum packet size
    pub max_packet: u32,
    /// PTY file descriptor (if allocated)
    pub pty_master: Option<i32>,
    /// Shell PID (if running)
    pub shell_pid: Option<i32>,
}

impl Channel {
    pub fn new(local_id: u32, remote_id: u32, remote_window: u32, max_packet: u32) -> Self {
        Channel {
            local_id,
            remote_id,
            state: ChannelState::Open,
            local_window: MAX_WINDOW_SIZE,
            remote_window,
            max_packet: max_packet.min(MAX_PACKET_SIZE),
            pty_master: None,
            shell_pid: None,
        }
    }
}

/// Channel manager
pub struct ChannelManager {
    /// Channels by local ID
    channels: BTreeMap<u32, Channel>,
    /// Next channel ID (public for iteration)
    pub next_id: u32,
}

impl ChannelManager {
    pub fn new() -> Self {
        ChannelManager {
            channels: BTreeMap::new(),
            next_id: 0,
        }
    }

    /// Handle channel open request
    pub fn handle_channel_open(
        &mut self,
        transport: &mut SshTransport,
        payload: &[u8],
    ) -> TransportResult<u32> {
        let mut offset = 1; // Skip message type

        let channel_type = decode_string(payload, &mut offset)?;
        let sender_channel = decode_u32(payload, &mut offset)?;
        let initial_window = decode_u32(payload, &mut offset)?;
        let max_packet = decode_u32(payload, &mut offset)?;

        // We only support "session" channels
        if &channel_type != b"session" {
            // Send channel open failure
            let mut failure = Vec::with_capacity(32);
            failure.push(msg::CHANNEL_OPEN_FAILURE);
            failure.extend_from_slice(&sender_channel.to_be_bytes());
            failure.extend_from_slice(&3u32.to_be_bytes()); // SSH_OPEN_UNKNOWN_CHANNEL_TYPE
            failure.extend_from_slice(&0u32.to_be_bytes()); // description (empty)
            failure.extend_from_slice(&0u32.to_be_bytes()); // language (empty)
            transport.send_packet(&failure)?;
            return Err(TransportError::Protocol);
        }

        // Allocate local channel
        let local_id = self.next_id;
        self.next_id += 1;

        let channel = Channel::new(local_id, sender_channel, initial_window, max_packet);
        self.channels.insert(local_id, channel);

        // Send channel open confirmation
        let mut confirm = Vec::with_capacity(24);
        confirm.push(msg::CHANNEL_OPEN_CONFIRMATION);
        confirm.extend_from_slice(&sender_channel.to_be_bytes());
        confirm.extend_from_slice(&local_id.to_be_bytes());
        confirm.extend_from_slice(&MAX_WINDOW_SIZE.to_be_bytes());
        confirm.extend_from_slice(&MAX_PACKET_SIZE.to_be_bytes());
        transport.send_packet(&confirm)?;

        Ok(local_id)
    }

    /// Handle channel data
    pub fn handle_channel_data(
        &mut self,
        transport: &mut SshTransport,
        payload: &[u8],
    ) -> TransportResult<(u32, Vec<u8>)> {
        let mut offset = 1;
        let recipient = decode_u32(payload, &mut offset)?;
        let data = decode_string(payload, &mut offset)?;

        // Update window
        if let Some(channel) = self.channels.get_mut(&recipient) {
            channel.local_window = channel.local_window.saturating_sub(data.len() as u32);

            // Send window adjust if needed
            if channel.local_window < MAX_WINDOW_SIZE / 2 {
                let adjust = MAX_WINDOW_SIZE - channel.local_window;
                channel.local_window = MAX_WINDOW_SIZE;

                let mut msg = Vec::with_capacity(12);
                msg.push(msg::CHANNEL_WINDOW_ADJUST);
                msg.extend_from_slice(&channel.remote_id.to_be_bytes());
                msg.extend_from_slice(&adjust.to_be_bytes());
                transport.send_packet(&msg)?;
            }

            Ok((recipient, data))
        } else {
            Err(TransportError::Protocol)
        }
    }

    /// Send channel data
    pub fn send_channel_data(
        &mut self,
        transport: &mut SshTransport,
        local_id: u32,
        data: &[u8],
    ) -> TransportResult<()> {
        let channel = self.channels.get(&local_id).ok_or(TransportError::Protocol)?;

        // Check window
        if channel.remote_window < data.len() as u32 {
            // Would block - for now just send what we can
            // In production, we'd queue the data
        }

        // Build and send data message
        let mut msg = Vec::with_capacity(9 + data.len());
        msg.push(msg::CHANNEL_DATA);
        msg.extend_from_slice(&channel.remote_id.to_be_bytes());
        msg.extend_from_slice(&(data.len() as u32).to_be_bytes());
        msg.extend_from_slice(data);
        transport.send_packet(&msg)?;

        // Update window
        if let Some(channel) = self.channels.get_mut(&local_id) {
            channel.remote_window = channel.remote_window.saturating_sub(data.len() as u32);
        }

        Ok(())
    }

    /// Handle window adjust
    pub fn handle_window_adjust(&mut self, payload: &[u8]) -> TransportResult<()> {
        let mut offset = 1;
        let recipient = decode_u32(payload, &mut offset)?;
        let bytes_to_add = decode_u32(payload, &mut offset)?;

        if let Some(channel) = self.channels.get_mut(&recipient) {
            channel.remote_window = channel.remote_window.saturating_add(bytes_to_add);
        }

        Ok(())
    }

    /// Handle channel close
    pub fn handle_channel_close(
        &mut self,
        transport: &mut SshTransport,
        payload: &[u8],
    ) -> TransportResult<()> {
        let mut offset = 1;
        let recipient = decode_u32(payload, &mut offset)?;

        if let Some(channel) = self.channels.get_mut(&recipient) {
            if channel.state != ChannelState::Closing {
                // Send close reply
                let mut msg = Vec::with_capacity(8);
                msg.push(msg::CHANNEL_CLOSE);
                msg.extend_from_slice(&channel.remote_id.to_be_bytes());
                transport.send_packet(&msg)?;
            }
            channel.state = ChannelState::Closed;
        }

        Ok(())
    }

    /// Handle channel EOF
    pub fn handle_channel_eof(&mut self, payload: &[u8]) -> TransportResult<()> {
        let mut offset = 1;
        let _recipient = decode_u32(payload, &mut offset)?;
        // Just note that client sent EOF
        Ok(())
    }

    /// Send channel EOF
    pub fn send_channel_eof(&mut self, transport: &mut SshTransport, local_id: u32) -> TransportResult<()> {
        let channel = self.channels.get(&local_id).ok_or(TransportError::Protocol)?;

        let mut msg = Vec::with_capacity(8);
        msg.push(msg::CHANNEL_EOF);
        msg.extend_from_slice(&channel.remote_id.to_be_bytes());
        transport.send_packet(&msg)
    }

    /// Send channel close
    pub fn send_channel_close(&mut self, transport: &mut SshTransport, local_id: u32) -> TransportResult<()> {
        if let Some(channel) = self.channels.get_mut(&local_id) {
            channel.state = ChannelState::Closing;

            let mut msg = Vec::with_capacity(8);
            msg.push(msg::CHANNEL_CLOSE);
            msg.extend_from_slice(&channel.remote_id.to_be_bytes());
            transport.send_packet(&msg)?;
        }
        Ok(())
    }

    /// Get channel by local ID
    pub fn get(&self, local_id: u32) -> Option<&Channel> {
        self.channels.get(&local_id)
    }

    /// Get mutable channel by local ID
    pub fn get_mut(&mut self, local_id: u32) -> Option<&mut Channel> {
        self.channels.get_mut(&local_id)
    }

    /// Remove channel
    pub fn remove(&mut self, local_id: u32) {
        self.channels.remove(&local_id);
    }

    /// Check if any channels are open
    pub fn has_open_channels(&self) -> bool {
        self.channels.values().any(|c| c.state == ChannelState::Open)
    }
}

/// Send channel request success
pub fn send_channel_success(transport: &mut SshTransport, remote_id: u32) -> TransportResult<()> {
    let mut msg = Vec::with_capacity(8);
    msg.push(msg::CHANNEL_SUCCESS);
    msg.extend_from_slice(&remote_id.to_be_bytes());
    transport.send_packet(&msg)
}

/// Send channel request failure
pub fn send_channel_failure(transport: &mut SshTransport, remote_id: u32) -> TransportResult<()> {
    let mut msg = Vec::with_capacity(8);
    msg.push(msg::CHANNEL_FAILURE);
    msg.extend_from_slice(&remote_id.to_be_bytes());
    transport.send_packet(&msg)
}
