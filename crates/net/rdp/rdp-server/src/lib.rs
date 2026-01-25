//! RDP Server Implementation
//!
//! Full RDP server for OXIDE OS supporting:
//! - TLS encrypted connections
//! - Screen sharing via system framebuffer
//! - Keyboard and mouse input
//! - Multiple concurrent clients (console sharing)

#![no_std]
#![allow(unused)]

extern crate alloc;

mod capabilities;
mod connection;
mod server;
mod session;

pub use capabilities::ServerCapabilities;
pub use connection::{ConnectionHandler, ConnectionState};
pub use server::{RdpServer, ServerConfig};
pub use session::{RdpSession, SessionManager};

use alloc::sync::Arc;
use rdp_traits::RdpResult;
use spin::Mutex;

/// Default RDP port
pub const RDP_PORT: u16 = 3389;

/// Maximum concurrent sessions
pub const MAX_SESSIONS: usize = 10;

/// Frame rate limit (ms between frames)
pub const FRAME_INTERVAL_MS: u64 = 33; // ~30 FPS

/// Initialize the RDP server
pub fn init_rdp_server() -> RdpResult<Arc<Mutex<RdpServer>>> {
    let config = ServerConfig::default();
    let server = RdpServer::new(config)?;
    Ok(Arc::new(Mutex::new(server)))
}
