//! TLS Session Management

use super::record::{RecordType, TlsRecord};
use super::{
    KeyMaterial, TLS_VERSION_1_2, compute_verify_data, decrypt_record, derive_key_material,
    derive_master_secret, encrypt_record,
};
use alloc::vec::Vec;
use crypto::sha256;
use rdp_traits::{RdpError, RdpResult};

/// TLS handshake state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsState {
    /// Initial state
    Initial,
    /// Waiting for ClientHello
    WaitClientHello,
    /// Sent ServerHello, waiting for client key exchange
    WaitClientKeyExchange,
    /// Waiting for ChangeCipherSpec
    WaitChangeCipherSpec,
    /// Waiting for Finished
    WaitFinished,
    /// Handshake complete, secure connection established
    Established,
    /// Error state
    Error,
}

/// TLS session configuration
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Server certificate (DER encoded)
    pub certificate: Vec<u8>,
    /// Server private key
    pub private_key: Vec<u8>,
    /// Verify client certificate
    pub verify_client: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            certificate: Vec::new(),
            private_key: Vec::new(),
            verify_client: false,
        }
    }
}

/// TLS session
pub struct TlsSession {
    /// Current state
    state: TlsState,
    /// Configuration
    config: TlsConfig,
    /// Client random (32 bytes)
    client_random: [u8; 32],
    /// Server random (32 bytes)
    server_random: [u8; 32],
    /// Master secret (48 bytes)
    master_secret: [u8; 48],
    /// Key material
    keys: Option<KeyMaterial>,
    /// Client write sequence number
    client_seq: u64,
    /// Server write sequence number
    server_seq: u64,
    /// Handshake messages hash
    handshake_hash: Vec<u8>,
    /// Is encryption active for reading
    read_encrypted: bool,
    /// Is encryption active for writing
    write_encrypted: bool,
}

impl TlsSession {
    /// Create a new TLS session
    pub fn new(config: TlsConfig) -> Self {
        Self {
            state: TlsState::Initial,
            config,
            client_random: [0u8; 32],
            server_random: [0u8; 32],
            master_secret: [0u8; 48],
            keys: None,
            client_seq: 0,
            server_seq: 0,
            handshake_hash: Vec::new(),
            read_encrypted: false,
            write_encrypted: false,
        }
    }

    /// Get current state
    pub fn state(&self) -> TlsState {
        self.state
    }

    /// Check if connection is established
    pub fn is_established(&self) -> bool {
        self.state == TlsState::Established
    }

    /// Set server random
    pub fn set_server_random(&mut self, random: [u8; 32]) {
        self.server_random = random;
    }

    /// Set client random
    pub fn set_client_random(&mut self, random: [u8; 32]) {
        self.client_random = random;
    }

    /// Initialize key material from premaster secret
    pub fn initialize_keys(&mut self, premaster_secret: &[u8]) {
        self.master_secret =
            derive_master_secret(premaster_secret, &self.client_random, &self.server_random);

        self.keys = Some(derive_key_material(
            &self.master_secret,
            &self.client_random,
            &self.server_random,
        ));
    }

    /// Add handshake message to hash
    pub fn add_handshake_message(&mut self, data: &[u8]) {
        self.handshake_hash.extend_from_slice(data);
    }

    /// Get handshake hash
    pub fn get_handshake_hash(&self) -> [u8; 32] {
        sha256(&self.handshake_hash)
    }

    /// Compute server Finished verify data
    pub fn compute_server_verify_data(&self) -> [u8; 12] {
        let hash = self.get_handshake_hash();
        compute_verify_data(&self.master_secret, b"server finished", &hash)
    }

    /// Compute expected client Finished verify data
    pub fn compute_client_verify_data(&self) -> [u8; 12] {
        let hash = self.get_handshake_hash();
        compute_verify_data(&self.master_secret, b"client finished", &hash)
    }

    /// Enable encryption for reading (after receiving ChangeCipherSpec)
    pub fn enable_read_encryption(&mut self) {
        self.read_encrypted = true;
        self.client_seq = 0;
    }

    /// Enable encryption for writing (before sending ChangeCipherSpec)
    pub fn enable_write_encryption(&mut self) {
        self.write_encrypted = true;
        self.server_seq = 0;
    }

    /// Transition to next state
    pub fn transition(&mut self, new_state: TlsState) {
        self.state = new_state;
    }

    /// Encrypt application data
    pub fn encrypt(&mut self, plaintext: &[u8]) -> RdpResult<Vec<u8>> {
        if !self.write_encrypted {
            return Err(RdpError::InvalidState);
        }

        let keys = self.keys.as_ref().ok_or(RdpError::CryptoError)?;
        let ciphertext = encrypt_record(
            &keys.server_write_key,
            &keys.server_write_iv,
            self.server_seq,
            RecordType::ApplicationData as u8,
            plaintext,
        );

        self.server_seq += 1;

        Ok(ciphertext)
    }

    /// Decrypt application data
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> RdpResult<Vec<u8>> {
        if !self.read_encrypted {
            return Err(RdpError::InvalidState);
        }

        let keys = self.keys.as_ref().ok_or(RdpError::CryptoError)?;
        let plaintext = decrypt_record(
            &keys.client_write_key,
            &keys.client_write_iv,
            RecordType::ApplicationData as u8,
            self.client_seq,
            ciphertext,
        )?;

        self.client_seq += 1;

        Ok(plaintext)
    }

    /// Encrypt a TLS record
    pub fn encrypt_record(&mut self, record_type: RecordType, data: &[u8]) -> RdpResult<TlsRecord> {
        if self.write_encrypted {
            let keys = self.keys.as_ref().ok_or(RdpError::CryptoError)?;
            let ciphertext = encrypt_record(
                &keys.server_write_key,
                &keys.server_write_iv,
                self.server_seq,
                record_type as u8,
                data,
            );
            self.server_seq += 1;

            Ok(TlsRecord {
                record_type,
                version: TLS_VERSION_1_2,
                data: ciphertext,
            })
        } else {
            Ok(TlsRecord {
                record_type,
                version: TLS_VERSION_1_2,
                data: data.to_vec(),
            })
        }
    }

    /// Decrypt a TLS record
    pub fn decrypt_record(&mut self, record: &TlsRecord) -> RdpResult<Vec<u8>> {
        if self.read_encrypted {
            let keys = self.keys.as_ref().ok_or(RdpError::CryptoError)?;
            let plaintext = decrypt_record(
                &keys.client_write_key,
                &keys.client_write_iv,
                record.record_type as u8,
                self.client_seq,
                &record.data,
            )?;
            self.client_seq += 1;
            Ok(plaintext)
        } else {
            Ok(record.data.clone())
        }
    }
}
