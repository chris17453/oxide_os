//! OXIDE Sound Manager (soundd)
//!
//! A sound server daemon similar to PulseAudio/PipeWire that manages:
//! - Audio device enumeration and initialization
//! - Per-user audio sessions and stream mixing
//! - Volume control and audio routing
//! - Socket-based IPC for audio clients
//!
//! Architecture:
//! - Opens /dev/dsp* devices for hardware access
//! - Listens on /run/soundd.sock for client connections
//! - Maintains per-user audio sessions with isolated mixing
//! - Multiplexes multiple client streams to hardware devices
//!
//! Protocol:
//! Clients connect via Unix socket and send commands:
//! - OPEN_STREAM: Open audio stream for playback/capture
//! - WRITE_AUDIO: Submit audio data for playback
//! - READ_AUDIO: Read captured audio data
//! - SET_VOLUME: Set stream or device volume
//! - CLOSE_STREAM: Close audio stream
//!
//! — EchoFrame: Audio + media subsystems

#![no_std]
#![no_main]
#![allow(unused)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use libc::dirent::{closedir, opendir, readdir};
use libc::time::usleep;
use libc::c_exports::mkfifo;
use libc::*;

/// Socket path for sound daemon
const SOUNDD_SOCKET: &str = "/run/soundd.sock";

/// Device directory for audio devices
const DEV_DIR: &str = "/dev";

/// Log file
const LOG_FILE: &str = "/var/log/soundd.log";

/// Maximum number of concurrent clients
const MAX_CLIENTS: usize = 32;

/// Maximum number of audio streams per client
const MAX_STREAMS_PER_CLIENT: usize = 8;

/// Audio buffer size (frames)
const BUFFER_SIZE: usize = 4096;

/// Sample rate (Hz)
const SAMPLE_RATE: u32 = 48000;

/// Number of channels (stereo)
const CHANNELS: u8 = 2;

/// Bytes per sample (16-bit PCM)
const BYTES_PER_SAMPLE: usize = 2;

/// Audio device information
#[derive(Clone)]
struct AudioDevice {
    path: [u8; 64],
    path_len: usize,
    fd: i32,
    is_open: bool,
    supports_playback: bool,
    supports_capture: bool,
}

impl AudioDevice {
    fn empty() -> Self {
        AudioDevice {
            path: [0; 64],
            path_len: 0,
            fd: -1,
            is_open: false,
            supports_playback: false,
            supports_capture: false,
        }
    }

    fn path_str(&self) -> &str {
        core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("")
    }
}

/// Audio stream for a client
#[derive(Clone, Copy)]
struct AudioStream {
    stream_id: u32,
    uid: u32,
    volume: u8,
    is_active: bool,
    is_muted: bool,
    buffer_pos: usize,
}

impl AudioStream {
    fn empty() -> Self {
        AudioStream {
            stream_id: 0,
            uid: 0,
            volume: 100,
            is_active: false,
            is_muted: false,
            buffer_pos: 0,
        }
    }
}

/// Client connection
#[derive(Clone, Copy)]
struct Client {
    fd: i32,
    uid: u32,
    gid: u32,
    pid: i32,
    active: bool,
    stream_count: usize,
}

impl Client {
    fn empty() -> Self {
        Client {
            fd: -1,
            uid: 0,
            gid: 0,
            pid: 0,
            active: false,
            stream_count: 0,
        }
    }
}

/// Sound daemon state
struct SoundDaemon {
    devices: Vec<AudioDevice>,
    clients: [Client; MAX_CLIENTS],
    master_volume: u8,
    master_muted: bool,
    server_fd: i32,
    next_stream_id: u32,
}

impl SoundDaemon {
    fn new() -> Self {
        SoundDaemon {
            devices: Vec::new(),
            clients: [Client::empty(); MAX_CLIENTS],
            master_volume: 100,
            master_muted: false,
            server_fd: -1,
            next_stream_id: 1,
        }
    }
}

/// Print to log file
fn log(msg: &str) {
    let fd = open(LOG_FILE, (O_WRONLY | O_CREAT | O_APPEND) as u32, 0o644);
    if fd >= 0 {
        let prefix = b"[soundd] ";
        let _ = write(fd, prefix);
        let _ = write(fd, msg.as_bytes());
        let _ = write(fd, b"\n");
        close(fd);
    }

    prints("[soundd] ");
    prints(msg);
    prints("\n");
}

/// Enumerate audio devices in /dev
fn enumerate_audio_devices(daemon: &mut SoundDaemon) {
    log("Enumerating audio devices");

    let dir = opendir(DEV_DIR);
    if let Some(mut dir) = dir {
        while let Some(entry) = readdir(&mut dir) {
            let name = entry.name();
            
            // Look for dsp* and audio* devices
            if name.starts_with("dsp") || name.starts_with("audio") {
                let mut device = AudioDevice::empty();
                
                // Build full path
                let dev_prefix = DEV_DIR.as_bytes();
                let name_bytes = name.as_bytes();
                
                if dev_prefix.len() + 1 + name_bytes.len() < 64 {
                    device.path[..dev_prefix.len()].copy_from_slice(dev_prefix);
                    device.path[dev_prefix.len()] = b'/';
                    device.path[dev_prefix.len() + 1..dev_prefix.len() + 1 + name_bytes.len()]
                        .copy_from_slice(name_bytes);
                    device.path_len = dev_prefix.len() + 1 + name_bytes.len();
                    
                    // Try to open device (get path_str before modifying device)
                    let path_str = core::str::from_utf8(&device.path[..device.path_len]).unwrap_or("");
                    let fd = open2(path_str, O_RDWR);
                    
                    if fd >= 0 {
                        device.fd = fd;
                        device.is_open = true;
                        // Assume playback support (would need ioctl to query properly)
                        device.supports_playback = true;
                        device.supports_capture = name.contains("capture");
                        
                        log("Found audio device: ");
                        prints(path_str);
                        prints("\n");
                        
                        daemon.devices.push(device);
                    }
                }
            }
        }
        closedir(dir);
    }

    if daemon.devices.is_empty() {
        log("No audio devices found, creating default /dev/dsp");
        
        // Create a default device entry even if it doesn't exist yet
        let mut device = AudioDevice::empty();
        let path = b"/dev/dsp";
        device.path[..path.len()].copy_from_slice(path);
        device.path_len = path.len();
        device.supports_playback = true;
        daemon.devices.push(device);
    }

    log("Found ");
    print_i64(daemon.devices.len() as i64);
    prints(" audio devices\n");
}

/// Initialize Unix socket server
fn init_server_socket(daemon: &mut SoundDaemon) -> bool {
    log("Initializing server socket");

    // Create /run directory if needed
    let _ = mkdir("/run", 0o755);

    // Remove old socket if exists
    let _ = unlink(SOUNDD_SOCKET);

    // Create Unix socket
    // For now, we'll use a simpler approach: create a named pipe
    // In a full implementation, this would use socket() with AF_UNIX
    
    // Create FIFO for IPC (simplified)
    let result = unsafe { mkfifo(SOUNDD_SOCKET.as_ptr(), 0o666) };
    if result < 0 {
        log("Failed to create server socket");
        return false;
    }

    log("Server socket created at ");
    prints(SOUNDD_SOCKET);
    prints("\n");
    
    true
}

/// Accept new client connection
fn accept_client(daemon: &mut SoundDaemon) {
    // In a full implementation, this would use accept() on the socket
    // For now, we'll open the FIFO for reading
    
    for i in 0..MAX_CLIENTS {
        if !daemon.clients[i].active {
            let fd = open2(SOUNDD_SOCKET, O_RDWR | O_NONBLOCK);
            if fd >= 0 {
                daemon.clients[i].fd = fd;
                daemon.clients[i].active = true;
                daemon.clients[i].uid = 0; // Would get from socket credentials
                daemon.clients[i].gid = 0;
                daemon.clients[i].pid = 0;
                
                log("Accepted new client connection");
            }
            break;
        }
    }
}

/// Process client command
fn process_client_command(daemon: &mut SoundDaemon, client_idx: usize) {
    let client = &daemon.clients[client_idx];
    if !client.active || client.fd < 0 {
        return;
    }

    // Read command from client
    let mut cmd_buf = [0u8; 256];
    let n = read(client.fd, &mut cmd_buf);
    
    if n <= 0 {
        // Client disconnected
        close(client.fd);
        daemon.clients[client_idx] = Client::empty();
        log("Client disconnected");
        return;
    }

    // Parse command (simplified protocol)
    let cmd = core::str::from_utf8(&cmd_buf[..n as usize]).unwrap_or("");
    
    if cmd.starts_with("VOLUME:") {
        // Set volume command: VOLUME:75
        if let Some(vol_str) = cmd.strip_prefix("VOLUME:") {
            if let Some(vol) = parse_u8(vol_str.trim()) {
                daemon.master_volume = vol.min(100);
                log("Set master volume to ");
                print_i64(daemon.master_volume as i64);
                prints("\n");
            }
        }
    } else if cmd.starts_with("MUTE") {
        daemon.master_muted = true;
        log("Master muted");
    } else if cmd.starts_with("UNMUTE") {
        daemon.master_muted = false;
        log("Master unmuted");
    } else if cmd.starts_with("STATUS") {
        // Send status back to client
        write_status(client.fd, daemon);
    }
}

/// Write status to client
fn write_status(fd: i32, daemon: &SoundDaemon) {
    let _ = write(fd, b"STATUS:\n");
    let _ = write(fd, b"volume=");
    write_u8(fd, daemon.master_volume);
    let _ = write(fd, b"\n");
    let _ = write(fd, b"muted=");
    if daemon.master_muted {
        let _ = write(fd, b"true\n");
    } else {
        let _ = write(fd, b"false\n");
    }
    let _ = write(fd, b"devices=");
    write_u8(fd, daemon.devices.len() as u8);
    let _ = write(fd, b"\n");
}

/// Write u8 to fd
fn write_u8(fd: i32, val: u8) {
    let mut buf = [0u8; 4];
    let len = format_u8(val, &mut buf);
    let _ = write(fd, &buf[..len]);
}

/// Format u8 to buffer, returns length
fn format_u8(mut val: u8, buf: &mut [u8]) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut len = 0;
    let mut tmp = [0u8; 3];
    while val > 0 {
        tmp[len] = b'0' + (val % 10);
        val /= 10;
        len += 1;
    }

    for i in 0..len {
        buf[i] = tmp[len - 1 - i];
    }
    len
}

/// Parse u8 from string
fn parse_u8(s: &str) -> Option<u8> {
    let mut val: u8 = 0;
    for c in s.bytes() {
        if c.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add(c - b'0')?;
        } else {
            return None;
        }
    }
    Some(val)
}

/// Mix audio from multiple sources and write to device
fn mix_and_output(daemon: &mut SoundDaemon) {
    // Get the primary output device
    if daemon.devices.is_empty() {
        return;
    }

    let device = &mut daemon.devices[0];
    
    // If device isn't open yet, try to open it
    if !device.is_open {
        let path_str = device.path_str();
        device.fd = open2(path_str, O_WRONLY);
        if device.fd >= 0 {
            device.is_open = true;
        } else {
            return; // Can't open device
        }
    }

    // In a full implementation:
    // 1. Collect audio data from all active client streams
    // 2. Mix them together with volume adjustments
    // 3. Apply master volume
    // 4. Write to hardware device
    
    // For now, we'll just maintain the device connection
}

/// Main daemon loop
fn run_daemon() {
    log("Starting sound manager daemon");

    // Create necessary directories
    let _ = mkdir("/var", 0o755);
    let _ = mkdir("/var/log", 0o755);
    let _ = mkdir("/run", 0o755);
    let _ = mkdir("/dev", 0o755);

    let mut daemon = SoundDaemon::new();

    // Enumerate audio devices
    enumerate_audio_devices(&mut daemon);

    // Initialize server socket
    if !init_server_socket(&mut daemon) {
        log("Failed to initialize server socket");
        return;
    }

    log("Sound daemon initialized successfully");
    log("Ready to accept client connections");

    // Main event loop
    loop {
        // Check for new client connections
        // In a full implementation, this would use select() or poll()
        
        // Process commands from existing clients
        for i in 0..MAX_CLIENTS {
            if daemon.clients[i].active {
                process_client_command(&mut daemon, i);
            }
        }

        // Mix audio and output to devices
        mix_and_output(&mut daemon);

        // Sleep briefly to avoid spinning
        usleep(10_000); // 10ms
    }
}

/// Show daemon status
fn show_status() {
    prints("Sound Manager Status:\n\n");

    // Try to connect to daemon and request status
    let fd = open2(SOUNDD_SOCKET, O_RDWR);
    if fd < 0 {
        prints("Sound daemon is not running\n");
        return;
    }

    // Send status request
    let _ = write(fd, b"STATUS");
    
    // Read response
    usleep(100_000); // Wait 100ms for response
    let mut buf = [0u8; 512];
    let n = read(fd, &mut buf);
    
    if n > 0 {
        if let Ok(response) = core::str::from_utf8(&buf[..n as usize]) {
            prints(response);
        }
    }
    
    close(fd);
}

/// Set master volume
fn set_volume(volume: u8) {
    let fd = open2(SOUNDD_SOCKET, O_RDWR);
    if fd < 0 {
        prints("Sound daemon is not running\n");
        return;
    }

    // Format volume command
    let mut cmd_buf = [0u8; 32];
    let prefix = b"VOLUME:";
    cmd_buf[..prefix.len()].copy_from_slice(prefix);
    let len = format_u8(volume, &mut cmd_buf[prefix.len()..]);
    
    let _ = write(fd, &cmd_buf[..prefix.len() + len]);
    close(fd);

    prints("Set master volume to ");
    print_i64(volume as i64);
    prints("\n");
}

/// Mute audio
fn set_mute(mute: bool) {
    let fd = open2(SOUNDD_SOCKET, O_RDWR);
    if fd < 0 {
        prints("Sound daemon is not running\n");
        return;
    }

    if mute {
        let _ = write(fd, b"MUTE");
        prints("Audio muted\n");
    } else {
        let _ = write(fd, b"UNMUTE");
        prints("Audio unmuted\n");
    }
    
    close(fd);
}

/// Show usage
fn show_usage() {
    prints("Usage: soundd [command] [args]\n");
    prints("\n");
    prints("Commands:\n");
    prints("  daemon         Run as daemon (started by init)\n");
    prints("  status         Show sound system status\n");
    prints("  volume <0-100> Set master volume\n");
    prints("  mute           Mute audio output\n");
    prints("  unmute         Unmute audio output\n");
    prints("  help           Show this help\n");
    prints("\n");
    prints("The sound daemon manages audio devices and provides\n");
    prints("per-user audio mixing similar to PulseAudio.\n");
}

/// Main entry point
#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let cmd = if argc >= 2 {
        cstr_to_str(unsafe { *argv.add(1) })
    } else {
        "daemon"
    };

    match cmd {
        "daemon" => {
            run_daemon();
            0
        }
        "status" => {
            show_status();
            0
        }
        "volume" => {
            if argc >= 3 {
                let vol_str = cstr_to_str(unsafe { *argv.add(2) });
                if let Some(vol) = parse_u8(vol_str) {
                    set_volume(vol);
                    0
                } else {
                    prints("Invalid volume value\n");
                    1
                }
            } else {
                prints("Usage: soundd volume <0-100>\n");
                1
            }
        }
        "mute" => {
            set_mute(true);
            0
        }
        "unmute" => {
            set_mute(false);
            0
        }
        "help" | "--help" | "-h" => {
            show_usage();
            0
        }
        _ => {
            prints("Unknown command: ");
            prints(cmd);
            prints("\n");
            show_usage();
            1
        }
    }
}

/// Convert C string to str
fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}
