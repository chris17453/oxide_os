//! Sound system interface
//!
//! Connects to the soundd daemon for audio output.
//! -- EchoFrame: Audio + media subsystems integration

use crate::game::Game;
use libc::socket::{SockAddrIn, SockAddrUn, af, connect, sock, socket};
use libc::{close, read, write};

/// Sound system state
pub struct SoundSystem {
    socket_fd: i32,
    connected: bool,
}

impl SoundSystem {
    /// Create a new sound system and connect to soundd
    /// -- EchoFrame: Socket connection to soundd daemon
    pub fn new() -> Self {
        let socket_fd = socket(af::UNIX, sock::STREAM, 0);
        if socket_fd < 0 {
            return SoundSystem {
                socket_fd: -1,
                connected: false,
            };
        }

        // Connect to soundd Unix socket
        let mut addr = SockAddrUn {
            sun_family: af::UNIX as u16,
            sun_path: [0; 108],
        };

        let socket_path = b"/run/soundd.sock";
        let path_len = socket_path.len().min(108);
        for i in 0..path_len {
            addr.sun_path[i] = socket_path[i];
        }

        // For Unix sockets, we need to use a generic sockaddr cast
        // The connect function in libc expects SockAddrIn but we can transmute
        let connected = unsafe {
            let addr_generic = core::mem::transmute::<&SockAddrUn, &SockAddrIn>(&addr);
            connect(
                socket_fd,
                addr_generic,
                core::mem::size_of::<SockAddrUn>() as u32,
            ) >= 0
        };

        if !connected {
            close(socket_fd);
            return SoundSystem {
                socket_fd: -1,
                connected: false,
            };
        }

        // Set initial volume
        let _ = write(socket_fd, b"VOLUME:70");

        SoundSystem {
            socket_fd,
            connected: true,
        }
    }

    /// Check if connected to sound system
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Update sound system (play sounds based on game state)
    /// -- EchoFrame: Sound triggering logic - reactions to game events
    pub fn update(&mut self, _game: &Game) {
        if !self.connected {
            return;
        }

        // TODO: Check game state for sound events
        // - Player firing weapon
        // - Player taking damage
        // - Doors opening
        // - Monster sounds
        // etc.

        // For now, just ensure connection is alive
        let mut buf = [0u8; 128];
        let _ = read(self.socket_fd, &mut buf);
    }

    /// Play a sound effect (stub for now)
    #[allow(dead_code)]
    fn play_sound(&mut self, _sound_id: u32) {
        if !self.connected {
            return;
        }

        // TODO: Send sound playback command to soundd
        // This would require extending soundd protocol to support
        // sound effect playback with sound ID
    }

    /// Set volume
    #[allow(dead_code)]
    pub fn set_volume(&mut self, volume: u8) {
        if !self.connected {
            return;
        }

        let vol = volume.min(100);
        let mut cmd = [0u8; 32];
        let vol_str = format_volume(vol);

        let cmd_str = b"VOLUME:";
        for i in 0..cmd_str.len() {
            cmd[i] = cmd_str[i];
        }

        let mut offset = cmd_str.len();
        for &b in vol_str.iter() {
            if b == 0 {
                break;
            }
            cmd[offset] = b;
            offset += 1;
        }

        write(self.socket_fd, &cmd[..offset]);
    }
}

impl Drop for SoundSystem {
    fn drop(&mut self) {
        if self.socket_fd >= 0 {
            close(self.socket_fd);
        }
    }
}

/// Format volume as string (no std formatting)
fn format_volume(vol: u8) -> [u8; 4] {
    let mut buf = [0u8; 4];
    let mut v = vol;
    let mut i = 0;

    if v == 0 {
        buf[0] = b'0';
        return buf;
    }

    let mut temp = [0u8; 3];
    let mut j = 0;
    while v > 0 {
        temp[j] = (v % 10) as u8 + b'0';
        v /= 10;
        j += 1;
    }

    while j > 0 {
        j -= 1;
        buf[i] = temp[j];
        i += 1;
    }

    buf
}
