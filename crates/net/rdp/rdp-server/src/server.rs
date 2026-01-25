//! RDP Server

use alloc::string::String;
use alloc::sync::Arc;
use rdp_graphics::{create_capture_provider, GraphicsConfig, GraphicsEncoder};
use rdp_input::RdpInputHandler;
use rdp_security::{SelfSignedCert, TlsConfig};
use rdp_traits::{RdpError, RdpResult, ScreenCaptureProvider, SessionId};
use spin::Mutex;

use super::session::{RdpSession, SessionManager};
use super::{MAX_SESSIONS, RDP_PORT};

/// RDP server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Server hostname
    pub hostname: String,
    /// Listening port
    pub port: u16,
    /// Maximum concurrent sessions
    pub max_sessions: usize,
    /// Require TLS
    pub require_tls: bool,
    /// Graphics configuration
    pub graphics: GraphicsConfig,
    /// Frame rate limit (FPS)
    pub max_fps: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            hostname: String::from("OXIDE"),
            port: RDP_PORT,
            max_sessions: MAX_SESSIONS,
            require_tls: true,
            graphics: GraphicsConfig::default(),
            max_fps: 30,
        }
    }
}

/// RDP Server
pub struct RdpServer {
    /// Server configuration
    config: ServerConfig,
    /// Session manager
    sessions: SessionManager,
    /// Screen capture provider (shared by all sessions)
    capture_provider: Option<Arc<Mutex<dyn ScreenCaptureProvider>>>,
    /// Input handler (shared by all sessions)
    input_handler: Option<Arc<Mutex<RdpInputHandler>>>,
    /// TLS certificate
    certificate: SelfSignedCert,
    /// Next session ID
    next_session_id: u32,
    /// Server running flag
    running: bool,
}

impl RdpServer {
    /// Create a new RDP server
    pub fn new(config: ServerConfig) -> RdpResult<Self> {
        // Generate self-signed certificate
        let certificate = SelfSignedCert::generate(&config.hostname);

        // Create capture provider from system framebuffer
        let capture_provider = create_capture_provider();

        // Create input handler
        let input_handler = if let Some(ref cap) = capture_provider {
            let (width, height) = cap.lock().dimensions();
            Some(Arc::new(Mutex::new(RdpInputHandler::new(width, height))))
        } else {
            None
        };

        Ok(Self {
            config,
            sessions: SessionManager::new(),
            capture_provider,
            input_handler,
            certificate,
            next_session_id: 1,
            running: false,
        })
    }

    /// Get server configuration
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Get certificate fingerprint
    pub fn certificate_fingerprint(&self) -> [u8; 32] {
        self.certificate.fingerprint()
    }

    /// Get certificate bytes
    pub fn certificate_bytes(&self) -> &[u8] {
        &self.certificate.certificate
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Start the server
    pub fn start(&mut self) -> RdpResult<()> {
        if self.capture_provider.is_none() {
            return Err(RdpError::Internal);
        }

        self.running = true;
        Ok(())
    }

    /// Stop the server
    pub fn stop(&mut self) {
        self.running = false;
        self.sessions.disconnect_all();
    }

    /// Create a new session for an incoming connection
    pub fn create_session(&mut self) -> RdpResult<SessionId> {
        if self.sessions.count() >= self.config.max_sessions {
            return Err(RdpError::ResourceExhausted);
        }

        let id = SessionId::new(self.next_session_id);
        self.next_session_id += 1;

        // Get screen dimensions
        let (width, height) = if let Some(ref cap) = self.capture_provider {
            cap.lock().dimensions()
        } else {
            (1024, 768)
        };

        // Create session
        let session = RdpSession::new(
            id,
            width as u16,
            height as u16,
            self.capture_provider.clone(),
            self.input_handler.clone(),
        );

        self.sessions.add(id, session);

        Ok(id)
    }

    /// Remove a session
    pub fn remove_session(&mut self, id: SessionId) {
        self.sessions.remove(id);
    }

    /// Get a session by ID
    pub fn get_session(&self, id: SessionId) -> Option<&RdpSession> {
        self.sessions.get(id)
    }

    /// Get a mutable session by ID
    pub fn get_session_mut(&mut self, id: SessionId) -> Option<&mut RdpSession> {
        self.sessions.get_mut(id)
    }

    /// Get active session count
    pub fn session_count(&self) -> usize {
        self.sessions.count()
    }

    /// Get TLS configuration
    pub fn tls_config(&self) -> TlsConfig {
        TlsConfig {
            certificate: self.certificate.certificate.clone(),
            private_key: self.certificate.private_key.clone(),
            verify_client: false,
        }
    }

    /// Get screen dimensions
    pub fn screen_dimensions(&self) -> (u32, u32) {
        if let Some(ref cap) = self.capture_provider {
            cap.lock().dimensions()
        } else {
            (1024, 768)
        }
    }

    /// Get graphics encoder for a session
    pub fn create_graphics_encoder(&self) -> Option<GraphicsEncoder> {
        let capture = self.capture_provider.clone()?;
        Some(GraphicsEncoder::new(capture, self.config.graphics.clone()))
    }
}
