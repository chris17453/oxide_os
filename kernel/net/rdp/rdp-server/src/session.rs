//! RDP Session Management

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use rdp_input::RdpInputHandler;
use rdp_proto::fast_path::FastPathInputEvent;
use rdp_proto::gcc::{
    ConferenceCreateRequest, ConferenceCreateResponse, ServerCoreData, ServerNetworkData,
    ServerSecurityData, UserDataBlock,
};
use rdp_proto::mcs;
use rdp_proto::pdu::{ConfirmActivePdu, DemandActivePdu};
use rdp_security::{TlsConfig, TlsSession};
use rdp_traits::{ClientInfo, RdpError, RdpResult, ScreenCaptureProvider, SessionId, SessionState};
use spin::Mutex;

use super::capabilities::ServerCapabilities;

/// RDP Session
pub struct RdpSession {
    /// Session ID
    id: SessionId,
    /// Current state
    state: SessionState,
    /// Client information
    client_info: ClientInfo,
    /// Negotiated desktop width
    desktop_width: u16,
    /// Negotiated desktop height
    desktop_height: u16,
    /// Share ID
    share_id: u32,
    /// User channel ID
    user_channel_id: u16,
    /// I/O channel ID
    io_channel_id: u16,
    /// Virtual channel IDs
    channel_ids: Vec<u16>,
    /// TLS session
    tls: Option<TlsSession>,
    /// Server capabilities
    capabilities: ServerCapabilities,
    /// Screen capture provider (shared)
    capture_provider: Option<Arc<Mutex<dyn ScreenCaptureProvider>>>,
    /// Input handler (shared)
    input_handler: Option<Arc<Mutex<RdpInputHandler>>>,
    /// Send buffer
    send_buffer: Vec<u8>,
    /// Receive buffer
    recv_buffer: Vec<u8>,
    /// Frame sequence number
    frame_seq: u64,
}

impl RdpSession {
    /// Create a new session
    pub fn new(
        id: SessionId,
        width: u16,
        height: u16,
        capture_provider: Option<Arc<Mutex<dyn ScreenCaptureProvider>>>,
        input_handler: Option<Arc<Mutex<RdpInputHandler>>>,
    ) -> Self {
        Self {
            id,
            state: SessionState::Initial,
            client_info: ClientInfo::default(),
            desktop_width: width,
            desktop_height: height,
            share_id: 0x00010000 + id.0,
            user_channel_id: 1007,
            io_channel_id: mcs::channels::IO_CHANNEL,
            channel_ids: Vec::new(),
            tls: None,
            capabilities: ServerCapabilities::new(width, height),
            capture_provider,
            input_handler,
            send_buffer: Vec::with_capacity(64 * 1024),
            recv_buffer: Vec::with_capacity(64 * 1024),
            frame_seq: 0,
        }
    }

    /// Get session ID
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Get current state
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Set state
    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
    }

    /// Get share ID
    pub fn share_id(&self) -> u32 {
        self.share_id
    }

    /// Get user channel ID
    pub fn user_channel_id(&self) -> u16 {
        self.user_channel_id
    }

    /// Set user channel ID
    pub fn set_user_channel_id(&mut self, id: u16) {
        self.user_channel_id = id;
    }

    /// Get I/O channel ID
    pub fn io_channel_id(&self) -> u16 {
        self.io_channel_id
    }

    /// Get desktop dimensions
    pub fn desktop_dimensions(&self) -> (u16, u16) {
        (self.desktop_width, self.desktop_height)
    }

    /// Initialize TLS session
    pub fn init_tls(&mut self, config: TlsConfig) {
        self.tls = Some(TlsSession::new(config));
    }

    /// Get TLS session reference
    pub fn tls(&self) -> Option<&TlsSession> {
        self.tls.as_ref()
    }

    /// Get mutable TLS session reference
    pub fn tls_mut(&mut self) -> Option<&mut TlsSession> {
        self.tls.as_mut()
    }

    /// Check if TLS is established
    pub fn is_tls_established(&self) -> bool {
        self.tls.as_ref().map_or(false, |t| t.is_established())
    }

    /// Process client info from GCC
    pub fn process_client_info(&mut self, gcc: &ConferenceCreateRequest) {
        if let Some(core) = gcc.client_core() {
            self.client_info.computer_name = core.client_name.clone();
            self.client_info.desktop_width = core.desktop_width;
            self.client_info.desktop_height = core.desktop_height;
            self.client_info.client_build = core.client_build;

            // Negotiate desktop size
            if core.desktop_width > 0 && core.desktop_height > 0 {
                self.desktop_width = core.desktop_width;
                self.desktop_height = core.desktop_height;
            }
        }

        if let Some(network) = gcc.client_network() {
            // Store requested channel names
            for (i, _channel) in network.channels.iter().enumerate() {
                let channel_id = mcs::channels::STATIC_CHANNEL_BASE + i as u16;
                self.channel_ids.push(channel_id);
            }
        }
    }

    /// Build GCC Conference Create Response
    pub fn build_gcc_response(&self) -> ConferenceCreateResponse {
        use rdp_traits::protocol;

        let mut user_data = Vec::new();

        // Server Core
        user_data.push(UserDataBlock::ServerCore(ServerCoreData {
            version: protocol::RDP_VERSION_5_PLUS,
            client_requested_protocols: Some(protocol::PROTOCOL_SSL),
            early_capability_flags: Some(0x00000001), // RNS_UD_SC_DYNAMIC_DST_SUPPORTED
        }));

        // Server Security (no RDP security, using TLS)
        user_data.push(UserDataBlock::ServerSecurity(ServerSecurityData {
            encryption_method: 0,
            encryption_level: 0,
            server_random: None,
            server_certificate: None,
        }));

        // Server Network
        user_data.push(UserDataBlock::ServerNetwork(ServerNetworkData {
            io_channel: self.io_channel_id,
            channel_ids: self.channel_ids.clone(),
        }));

        ConferenceCreateResponse { user_data }
    }

    /// Build Demand Active PDU
    pub fn build_demand_active(&self) -> DemandActivePdu {
        DemandActivePdu {
            share_id: self.share_id,
            source_descriptor_len: 5,
            capabilities_len: 0, // Will be calculated during encoding
            source_descriptor: String::from("RDP\0"),
            num_capabilities: self.capabilities.count() as u16,
            capabilities: self.capabilities.to_capability_sets(),
            session_id: 0,
        }
    }

    /// Process Confirm Active PDU
    pub fn process_confirm_active(&mut self, confirm: &ConfirmActivePdu) -> RdpResult<()> {
        if confirm.share_id != self.share_id {
            return Err(RdpError::InvalidProtocol);
        }

        // Store client capabilities for reference
        // In a full implementation, we'd parse each capability set
        // and adjust our behavior accordingly

        self.state = SessionState::Connected;
        Ok(())
    }

    /// Process input event
    pub fn process_input(&self, event: &FastPathInputEvent) -> RdpResult<()> {
        if let Some(ref handler) = self.input_handler {
            handler.lock().process_fast_path_event(event)
        } else {
            Ok(())
        }
    }

    /// Queue data to send
    pub fn queue_send(&mut self, data: &[u8]) {
        self.send_buffer.extend_from_slice(data);
    }

    /// Get and clear send buffer
    pub fn take_send_buffer(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.send_buffer)
    }

    /// Add received data
    pub fn add_received(&mut self, data: &[u8]) {
        self.recv_buffer.extend_from_slice(data);
    }

    /// Get receive buffer
    pub fn recv_buffer(&self) -> &[u8] {
        &self.recv_buffer
    }

    /// Consume bytes from receive buffer
    pub fn consume_received(&mut self, count: usize) {
        if count >= self.recv_buffer.len() {
            self.recv_buffer.clear();
        } else {
            self.recv_buffer.drain(..count);
        }
    }

    /// Get next frame sequence number
    pub fn next_frame_seq(&mut self) -> u64 {
        let seq = self.frame_seq;
        self.frame_seq += 1;
        seq
    }
}

/// Session manager
pub struct SessionManager {
    sessions: BTreeMap<SessionId, RdpSession>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
        }
    }

    /// Add a session
    pub fn add(&mut self, id: SessionId, session: RdpSession) {
        self.sessions.insert(id, session);
    }

    /// Remove a session
    pub fn remove(&mut self, id: SessionId) -> Option<RdpSession> {
        self.sessions.remove(&id)
    }

    /// Get a session
    pub fn get(&self, id: SessionId) -> Option<&RdpSession> {
        self.sessions.get(&id)
    }

    /// Get a mutable session
    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut RdpSession> {
        self.sessions.get_mut(&id)
    }

    /// Get session count
    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    /// Disconnect all sessions
    pub fn disconnect_all(&mut self) {
        for session in self.sessions.values_mut() {
            session.set_state(SessionState::Disconnected);
        }
        self.sessions.clear();
    }

    /// Get all session IDs
    pub fn session_ids(&self) -> Vec<SessionId> {
        self.sessions.keys().copied().collect()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
