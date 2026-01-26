//! RDP Connection State Machine
//!
//! Manages the connection lifecycle from initial TCP connection through
//! X.224, MCS negotiation, capability exchange, and active session.

use alloc::vec::Vec;
use rdp_proto::gcc::ConferenceCreateRequest;
use rdp_proto::mcs;
use rdp_proto::pdu::ConfirmActivePdu;
use rdp_proto::x224::{ConnectionConfirm, ConnectionRequest};
use rdp_proto::{tpkt, x224};
use rdp_security::TlsState;
use rdp_traits::{RdpError, RdpResult};

use super::session::RdpSession;

/// Connection state for RDP protocol negotiation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state - waiting for X.224 Connection Request
    Initial,
    /// Received X.224 CR, sent X.224 CC - waiting for TLS handshake
    X224Connected,
    /// TLS handshake in progress
    TlsHandshaking,
    /// TLS established - waiting for MCS Connect Initial
    TlsEstablished,
    /// Received MCS Connect Initial, sent MCS Connect Response
    McsConnectInitial,
    /// MCS Erect Domain Request received
    McsErectDomain,
    /// MCS Attach User Request received, sent Attach User Confirm
    McsAttachUser,
    /// MCS Channel Join in progress
    McsChannelJoin,
    /// All channels joined - waiting for Client Info PDU
    ChannelsJoined,
    /// Client Info received - sent License PDU
    ClientInfoReceived,
    /// License exchange complete - sent Demand Active PDU
    LicenseComplete,
    /// Received Confirm Active PDU
    CapabilitiesExchanged,
    /// Received Synchronize PDU
    Synchronized,
    /// Received Control Cooperate PDU
    ControlCooperate,
    /// Received Control Request Control PDU
    ControlGranted,
    /// Received Font List PDU - connection complete
    Active,
    /// Disconnecting
    Disconnecting,
    /// Disconnected
    Disconnected,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Initial
    }
}

/// Connection handler for managing RDP protocol state
pub struct ConnectionHandler {
    /// Current connection state
    state: ConnectionState,
    /// Channels to join (IO channel + virtual channels)
    channels_to_join: Vec<u16>,
    /// Channels already joined
    channels_joined: Vec<u16>,
    /// Pending data to send
    pending_send: Vec<u8>,
}

impl ConnectionHandler {
    /// Create a new connection handler
    pub fn new() -> Self {
        Self {
            state: ConnectionState::Initial,
            channels_to_join: Vec::new(),
            channels_joined: Vec::new(),
            pending_send: Vec::with_capacity(8 * 1024),
        }
    }

    /// Get current connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Set connection state
    pub fn set_state(&mut self, state: ConnectionState) {
        self.state = state;
    }

    /// Check if connection is active
    pub fn is_active(&self) -> bool {
        self.state == ConnectionState::Active
    }

    /// Check if connection is disconnected
    pub fn is_disconnected(&self) -> bool {
        matches!(
            self.state,
            ConnectionState::Disconnecting | ConnectionState::Disconnected
        )
    }

    /// Process X.224 Connection Request
    pub fn process_x224_cr(
        &mut self,
        _session: &mut RdpSession,
        _request: &ConnectionRequest,
    ) -> RdpResult<Vec<u8>> {
        if self.state != ConnectionState::Initial {
            return Err(RdpError::InvalidProtocol);
        }

        // Build X.224 Connection Confirm with TLS negotiation
        let response = ConnectionConfirm::tls_response();
        let x224_data = response.encode(0, 0x1234);
        let tpkt_packet = tpkt::encode(&x224_data);

        self.state = ConnectionState::X224Connected;
        Ok(tpkt_packet)
    }

    /// Process TLS handshake completion notification
    ///
    /// The actual TLS handshake is handled at a lower level by rdp-security.
    /// This method updates connection state based on the TLS session state.
    pub fn process_tls_state(&mut self, session: &RdpSession) -> RdpResult<()> {
        let tls = session.tls().ok_or(RdpError::TlsError)?;

        match tls.state() {
            TlsState::Initial | TlsState::WaitClientHello => {
                self.state = ConnectionState::X224Connected;
            }
            TlsState::WaitClientKeyExchange
            | TlsState::WaitChangeCipherSpec
            | TlsState::WaitFinished => {
                self.state = ConnectionState::TlsHandshaking;
            }
            TlsState::Established => {
                self.state = ConnectionState::TlsEstablished;
            }
            TlsState::Error => {
                return Err(RdpError::TlsError);
            }
        }

        Ok(())
    }

    /// Mark TLS as established
    pub fn tls_established(&mut self) {
        self.state = ConnectionState::TlsEstablished;
    }

    /// Process MCS Connect Initial
    pub fn process_mcs_connect_initial(
        &mut self,
        session: &mut RdpSession,
        gcc: &ConferenceCreateRequest,
    ) -> RdpResult<Vec<u8>> {
        if self.state != ConnectionState::TlsEstablished {
            return Err(RdpError::InvalidProtocol);
        }

        // Process client info from GCC
        session.process_client_info(gcc);

        // Build MCS Connect Response with GCC Conference Create Response
        let gcc_response = session.build_gcc_response();
        let mcs_response = mcs::build_connect_response(&gcc_response);

        // Wrap in X.224 Data
        let x224_data = x224::encode_data(&mcs_response);
        let tpkt_packet = tpkt::encode(&x224_data);

        // Setup channels to join
        self.channels_to_join.clear();
        self.channels_to_join.push(session.io_channel_id());
        // Add any virtual channels here

        self.state = ConnectionState::McsConnectInitial;
        Ok(tpkt_packet)
    }

    /// Process MCS Erect Domain Request
    pub fn process_erect_domain(&mut self) -> RdpResult<()> {
        if self.state != ConnectionState::McsConnectInitial {
            return Err(RdpError::InvalidProtocol);
        }

        self.state = ConnectionState::McsErectDomain;
        Ok(())
    }

    /// Process MCS Attach User Request
    pub fn process_attach_user(&mut self, session: &mut RdpSession) -> RdpResult<Vec<u8>> {
        if self.state != ConnectionState::McsErectDomain {
            return Err(RdpError::InvalidProtocol);
        }

        // Build Attach User Confirm
        let user_id = session.user_channel_id();
        let response = mcs::build_attach_user_confirm(user_id);

        let x224_data = x224::encode_data(&response);
        let tpkt_packet = tpkt::encode(&x224_data);

        self.state = ConnectionState::McsAttachUser;
        Ok(tpkt_packet)
    }

    /// Process MCS Channel Join Request
    pub fn process_channel_join(
        &mut self,
        session: &RdpSession,
        channel_id: u16,
    ) -> RdpResult<Vec<u8>> {
        if !matches!(
            self.state,
            ConnectionState::McsAttachUser | ConnectionState::McsChannelJoin
        ) {
            return Err(RdpError::InvalidProtocol);
        }

        // Build Channel Join Confirm
        let user_id = session.user_channel_id();
        let response = mcs::build_channel_join_confirm(user_id, channel_id);

        let x224_data = x224::encode_data(&response);
        let tpkt_packet = tpkt::encode(&x224_data);

        // Track joined channels
        if !self.channels_joined.contains(&channel_id) {
            self.channels_joined.push(channel_id);
        }

        // Check if all channels are joined
        if self.all_channels_joined() {
            self.state = ConnectionState::ChannelsJoined;
        } else {
            self.state = ConnectionState::McsChannelJoin;
        }

        Ok(tpkt_packet)
    }

    /// Check if all required channels have been joined
    fn all_channels_joined(&self) -> bool {
        self.channels_to_join
            .iter()
            .all(|ch| self.channels_joined.contains(ch))
    }

    /// Process Client Info PDU
    pub fn process_client_info(&mut self) -> RdpResult<Vec<u8>> {
        if self.state != ConnectionState::ChannelsJoined {
            return Err(RdpError::InvalidProtocol);
        }

        // Send licensing PDU (skip licensing for now)
        // In a full implementation, we'd send a valid license error
        let license_pdu = build_license_error_pdu();

        self.state = ConnectionState::ClientInfoReceived;
        Ok(license_pdu)
    }

    /// Process after licensing - send Demand Active
    pub fn send_demand_active(&mut self, session: &RdpSession) -> RdpResult<Vec<u8>> {
        if self.state != ConnectionState::ClientInfoReceived {
            return Err(RdpError::InvalidProtocol);
        }

        let demand_active = session.build_demand_active();
        // The encode method includes the share control header
        let pdu_data = demand_active.encode(session.user_channel_id());

        let mcs_data = mcs::build_send_data_indication(
            session.user_channel_id(),
            session.io_channel_id(),
            &pdu_data,
        );

        let x224_data = x224::encode_data(&mcs_data);
        let tpkt_packet = tpkt::encode(&x224_data);

        self.state = ConnectionState::LicenseComplete;
        Ok(tpkt_packet)
    }

    /// Process Confirm Active PDU
    pub fn process_confirm_active(
        &mut self,
        session: &mut RdpSession,
        confirm: &ConfirmActivePdu,
    ) -> RdpResult<()> {
        if self.state != ConnectionState::LicenseComplete {
            return Err(RdpError::InvalidProtocol);
        }

        session.process_confirm_active(confirm)?;
        self.state = ConnectionState::CapabilitiesExchanged;
        Ok(())
    }

    /// Process Synchronize PDU
    pub fn process_synchronize(&mut self) -> RdpResult<Vec<u8>> {
        if self.state != ConnectionState::CapabilitiesExchanged {
            return Err(RdpError::InvalidProtocol);
        }

        // Send Synchronize PDU back
        let sync_pdu = build_synchronize_pdu();

        self.state = ConnectionState::Synchronized;
        Ok(sync_pdu)
    }

    /// Process Control Cooperate PDU
    pub fn process_control_cooperate(&mut self) -> RdpResult<Vec<u8>> {
        if self.state != ConnectionState::Synchronized {
            return Err(RdpError::InvalidProtocol);
        }

        // Send Control Cooperate back
        let cooperate_pdu = build_control_cooperate_pdu();

        self.state = ConnectionState::ControlCooperate;
        Ok(cooperate_pdu)
    }

    /// Process Control Request Control PDU
    pub fn process_control_request(&mut self) -> RdpResult<Vec<u8>> {
        if self.state != ConnectionState::ControlCooperate {
            return Err(RdpError::InvalidProtocol);
        }

        // Send Control Granted
        let granted_pdu = build_control_granted_pdu();

        self.state = ConnectionState::ControlGranted;
        Ok(granted_pdu)
    }

    /// Process Font List PDU - finalize connection
    pub fn process_font_list(&mut self) -> RdpResult<Vec<u8>> {
        if self.state != ConnectionState::ControlGranted {
            return Err(RdpError::InvalidProtocol);
        }

        // Send Font Map PDU
        let font_map_pdu = build_font_map_pdu();

        self.state = ConnectionState::Active;
        Ok(font_map_pdu)
    }

    /// Take pending send data
    pub fn take_pending_send(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.pending_send)
    }

    /// Initiate disconnection
    pub fn disconnect(&mut self) -> RdpResult<Vec<u8>> {
        self.state = ConnectionState::Disconnecting;

        // Send MCS Disconnect Provider Ultimatum
        let disconnect = mcs::build_disconnect_provider_ultimatum();
        let x224_data = x224::encode_data(&disconnect);
        let tpkt_packet = tpkt::encode(&x224_data);

        self.state = ConnectionState::Disconnected;
        Ok(tpkt_packet)
    }
}

impl Default for ConnectionHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Share Control PDU types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ShareControlPduType {
    DemandActivePdu = 0x0001,
    ConfirmActivePdu = 0x0003,
    DeactivateAllPdu = 0x0006,
    DataPdu = 0x0007,
    ServerRedirectionPdu = 0x000A,
}

/// Build share control PDU wrapper
fn build_share_control_pdu(_share_id: u32, pdu_type: ShareControlPduType, data: &[u8]) -> Vec<u8> {
    let total_length = 6 + data.len(); // header + data

    let mut pdu = Vec::with_capacity(total_length);

    // totalLength (2 bytes)
    pdu.extend_from_slice(&(total_length as u16).to_le_bytes());
    // pduType (2 bytes) - includes version in high bits
    let pdu_type_with_version = (pdu_type as u16) | 0x0010; // Version 1
    pdu.extend_from_slice(&pdu_type_with_version.to_le_bytes());
    // pduSource (2 bytes) - user channel
    pdu.extend_from_slice(&1007u16.to_le_bytes());
    // PDU data
    pdu.extend_from_slice(data);

    pdu
}

/// Build license error PDU (to skip licensing)
fn build_license_error_pdu() -> Vec<u8> {
    // LICENSE_ERROR_MESSAGE with STATUS_VALID_CLIENT
    let mut pdu = Vec::with_capacity(32);

    // Security header (basic)
    pdu.extend_from_slice(&[0x80, 0x00]); // SEC_LICENSE_PKT

    // preamble
    pdu.push(0xFF); // ERROR_ALERT
    pdu.push(0x03); // PREAMBLE_VERSION_3_0
    pdu.extend_from_slice(&20u16.to_le_bytes()); // wMsgSize

    // validClientMessage
    pdu.extend_from_slice(&0x00000007u32.to_le_bytes()); // dwErrorCode: STATUS_VALID_CLIENT
    pdu.extend_from_slice(&0x00000002u32.to_le_bytes()); // dwStateTransition: ST_NO_TRANSITION
    // blob
    pdu.extend_from_slice(&0x00BBu16.to_le_bytes()); // wBlobType: BB_ERROR_BLOB
    pdu.extend_from_slice(&0u16.to_le_bytes()); // wBlobLen: 0

    pdu
}

/// Build Synchronize PDU
fn build_synchronize_pdu() -> Vec<u8> {
    let mut pdu = Vec::with_capacity(8);

    // messageType (2 bytes)
    pdu.extend_from_slice(&1u16.to_le_bytes()); // SYNCMSGTYPE_SYNC
    // targetUser (2 bytes)
    pdu.extend_from_slice(&1007u16.to_le_bytes());

    pdu
}

/// Build Control Cooperate PDU
fn build_control_cooperate_pdu() -> Vec<u8> {
    let mut pdu = Vec::with_capacity(12);

    // action (2 bytes)
    pdu.extend_from_slice(&0x0004u16.to_le_bytes()); // CTRLACTION_COOPERATE
    // grantId (2 bytes)
    pdu.extend_from_slice(&0u16.to_le_bytes());
    // controlId (4 bytes)
    pdu.extend_from_slice(&0u32.to_le_bytes());

    pdu
}

/// Build Control Granted PDU
fn build_control_granted_pdu() -> Vec<u8> {
    let mut pdu = Vec::with_capacity(12);

    // action (2 bytes)
    pdu.extend_from_slice(&0x0002u16.to_le_bytes()); // CTRLACTION_GRANTED_CONTROL
    // grantId (2 bytes)
    pdu.extend_from_slice(&1007u16.to_le_bytes());
    // controlId (4 bytes)
    pdu.extend_from_slice(&0x00030001u32.to_le_bytes());

    pdu
}

/// Build Font Map PDU
fn build_font_map_pdu() -> Vec<u8> {
    let mut pdu = Vec::with_capacity(8);

    // numberEntries (2 bytes)
    pdu.extend_from_slice(&0u16.to_le_bytes());
    // totalNumEntries (2 bytes)
    pdu.extend_from_slice(&0u16.to_le_bytes());
    // mapFlags (2 bytes)
    pdu.extend_from_slice(&0x0003u16.to_le_bytes()); // FONTMAP_FIRST | FONTMAP_LAST
    // entrySize (2 bytes)
    pdu.extend_from_slice(&4u16.to_le_bytes());

    pdu
}
