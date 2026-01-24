//! SSH Session Management
//!
//! Handles:
//! - Channel requests (pty-req, shell, exec)
//! - PTY allocation
//! - Shell execution
//! - I/O multiplexing between client and shell

use alloc::vec::Vec;
use libc::socket::{msg as sock_msg, recv};
use libc::*;

use crate::auth::authenticated_user;
use crate::channel::{ChannelManager, ChannelState, send_channel_failure, send_channel_success};
use crate::transport::{
    SshTransport, TransportError, TransportResult, decode_string, decode_u8, decode_u32, msg,
};

/// Run the session after authentication
pub fn run_session(transport: &mut SshTransport) -> TransportResult<()> {
    let mut channels = ChannelManager::new();

    loop {
        // Receive next message
        let payload = match transport.recv_packet() {
            Ok(p) => p,
            Err(TransportError::Closed) => break,
            Err(e) => return Err(e),
        };

        if payload.is_empty() {
            continue;
        }

        match payload[0] {
            msg::CHANNEL_OPEN => {
                let _ = channels.handle_channel_open(transport, &payload)?;
            }

            msg::CHANNEL_REQUEST => {
                handle_channel_request(transport, &mut channels, &payload)?;
            }

            msg::CHANNEL_DATA => {
                let (local_id, data) = channels.handle_channel_data(transport, &payload)?;

                // Forward data to PTY
                if let Some(channel) = channels.get(local_id) {
                    if let Some(pty_fd) = channel.pty_master {
                        let _ = write(pty_fd, &data);
                    }
                }
            }

            msg::CHANNEL_WINDOW_ADJUST => {
                channels.handle_window_adjust(&payload)?;
            }

            msg::CHANNEL_EOF => {
                channels.handle_channel_eof(&payload)?;
            }

            msg::CHANNEL_CLOSE => {
                channels.handle_channel_close(transport, &payload)?;

                // Check if all channels closed
                if !channels.has_open_channels() {
                    break;
                }
            }

            msg::DISCONNECT => {
                break;
            }

            msg::IGNORE | msg::DEBUG => {
                // Ignore these
            }

            _ => {
                // Unknown message - send unimplemented
                let mut unimpl = Vec::with_capacity(8);
                unimpl.push(msg::UNIMPLEMENTED);
                unimpl.extend_from_slice(&transport.recv_sequence().to_be_bytes());
                let _ = transport.send_packet(&unimpl);
            }
        }

        // Poll PTYs for output
        poll_pty_output(transport, &mut channels)?;
    }

    Ok(())
}

/// Handle channel request
fn handle_channel_request(
    transport: &mut SshTransport,
    channels: &mut ChannelManager,
    payload: &[u8],
) -> TransportResult<()> {
    let mut offset = 1;
    let recipient = decode_u32(payload, &mut offset)?;
    let request_type = decode_string(payload, &mut offset)?;
    let want_reply = decode_u8(payload, &mut offset)?;

    let channel = channels
        .get_mut(recipient)
        .ok_or(TransportError::Protocol)?;
    let remote_id = channel.remote_id;

    let success = match request_type.as_slice() {
        b"pty-req" => {
            // PTY request
            let _term = decode_string(payload, &mut offset)?;
            let width = decode_u32(payload, &mut offset)?;
            let height = decode_u32(payload, &mut offset)?;
            let _pixel_width = decode_u32(payload, &mut offset)?;
            let _pixel_height = decode_u32(payload, &mut offset)?;
            let _modes = decode_string(payload, &mut offset)?;

            // Allocate PTY
            allocate_pty(channel, width, height)
        }

        b"shell" => {
            // Shell request
            start_shell(channel)
        }

        b"exec" => {
            // Exec request
            let command = decode_string(payload, &mut offset)?;
            let cmd_str = core::str::from_utf8(&command).unwrap_or("/bin/esh");
            start_command(channel, cmd_str)
        }

        b"env" => {
            // Environment variable - ignore for now
            true
        }

        b"window-change" => {
            // Window size change
            let width = decode_u32(payload, &mut offset)?;
            let height = decode_u32(payload, &mut offset)?;
            resize_pty(channel, width, height)
        }

        _ => {
            // Unknown request type
            false
        }
    };

    if want_reply != 0 {
        if success {
            send_channel_success(transport, remote_id)?;
        } else {
            send_channel_failure(transport, remote_id)?;
        }
    }

    Ok(())
}

/// Allocate a PTY for the channel
fn allocate_pty(channel: &mut crate::channel::Channel, width: u32, height: u32) -> bool {
    // Open PTY master
    let master = open2("/dev/ptmx", O_RDWR | O_NOCTTY);
    if master < 0 {
        return false;
    }

    // Get slave name and open it
    // In a real implementation, we'd use grantpt/unlockpt/ptsname
    // For now, assume slave is /dev/pts/N where N is derived from master

    // Set window size
    let ws = Winsize {
        ws_row: height as u16,
        ws_col: width as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let _ = ioctl_tiocswinsz(master, &ws);

    channel.pty_master = Some(master);
    true
}

/// Start a shell on the channel
fn start_shell(channel: &mut crate::channel::Channel) -> bool {
    let user = match authenticated_user() {
        Some(u) => u,
        None => return false,
    };

    let pty_master = match channel.pty_master {
        Some(fd) => fd,
        None => return false,
    };

    // Fork
    let pid = fork();
    if pid < 0 {
        return false;
    }

    if pid == 0 {
        // Child process

        // Create new session
        let _ = setsid();

        // Open PTY slave
        // In a real impl, we'd get the slave path from ptsname
        let slave = open2("/dev/pts/0", O_RDWR);
        if slave < 0 {
            _exit(1);
        }

        // Set as controlling terminal
        let _ = ioctl_tiocsctty(slave, 0);

        // Dup to stdin/stdout/stderr
        dup2(slave, 0);
        dup2(slave, 1);
        dup2(slave, 2);

        if slave > 2 {
            close(slave);
        }

        // Set environment
        setenv("HOME", &user.home);
        setenv("USER", &user.username);
        setenv("SHELL", &user.shell);
        setenv("TERM", "xterm-256color");

        // Change to home directory
        let _ = chdir(&user.home);

        // Drop privileges
        let _ = setgid(user.gid);
        let _ = setuid(user.uid);

        // Exec shell
        exec(&user.shell);
        _exit(1);
    } else {
        // Parent
        channel.shell_pid = Some(pid);
    }

    true
}

/// Start a command on the channel
fn start_command(channel: &mut crate::channel::Channel, command: &str) -> bool {
    let user = match authenticated_user() {
        Some(u) => u,
        None => return false,
    };

    let pty_master = match channel.pty_master {
        Some(fd) => fd,
        None => return false,
    };

    let pid = fork();
    if pid < 0 {
        return false;
    }

    if pid == 0 {
        // Child
        let _ = setsid();

        let slave = open2("/dev/pts/0", O_RDWR);
        if slave >= 0 {
            let _ = ioctl_tiocsctty(slave, 0);
            dup2(slave, 0);
            dup2(slave, 1);
            dup2(slave, 2);
            if slave > 2 {
                close(slave);
            }
        }

        setenv("HOME", &user.home);
        setenv("USER", &user.username);
        setenv("SHELL", &user.shell);
        setenv("TERM", "xterm-256color");

        let _ = chdir(&user.home);
        let _ = setgid(user.gid);
        let _ = setuid(user.uid);

        // Execute command through shell
        execv(&user.shell, &["-c", command]);
        _exit(1);
    } else {
        channel.shell_pid = Some(pid);
    }

    true
}

/// Resize PTY window
fn resize_pty(channel: &mut crate::channel::Channel, width: u32, height: u32) -> bool {
    if let Some(pty_fd) = channel.pty_master {
        let ws = Winsize {
            ws_row: height as u16,
            ws_col: width as u16,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        ioctl_tiocswinsz(pty_fd, &ws) >= 0
    } else {
        false
    }
}

/// Poll PTY for output and send to client
fn poll_pty_output(
    transport: &mut SshTransport,
    channels: &mut ChannelManager,
) -> TransportResult<()> {
    // This is a simplified version - in production, we'd use poll/select
    // Collect channel info first to avoid borrow conflicts
    let channel_ids: Vec<u32> = (0..channels.next_id).collect();

    for local_id in channel_ids {
        // Get channel info without holding mutable borrow
        let (pty_fd, shell_pid, is_open) = {
            match channels.get(local_id) {
                Some(channel) if channel.state == ChannelState::Open => {
                    (channel.pty_master, channel.shell_pid, true)
                }
                _ => (None, None, false),
            }
        };

        if !is_open {
            continue;
        }

        if let Some(pty_fd) = pty_fd {
            let mut buf = [0u8; 4096];

            // Non-blocking read
            let n = read_nonblock(pty_fd, &mut buf);
            if n > 0 {
                let data = buf[..n as usize].to_vec();
                channels.send_channel_data(transport, local_id, &data)?;
            }

            // Check if shell exited
            if let Some(pid) = shell_pid {
                let mut status = 0;
                let result = waitpid(pid, &mut status, WNOHANG);
                if result > 0 {
                    // Shell exited - clear the pid
                    if let Some(channel) = channels.get_mut(local_id) {
                        channel.shell_pid = None;
                    }

                    // Send remaining output
                    loop {
                        let n = read_nonblock(pty_fd, &mut buf);
                        if n <= 0 {
                            break;
                        }
                        let data = buf[..n as usize].to_vec();
                        channels.send_channel_data(transport, local_id, &data)?;
                    }

                    // Send EOF and close
                    channels.send_channel_eof(transport, local_id)?;
                    channels.send_channel_close(transport, local_id)?;
                }
            }
        }
    }

    Ok(())
}

/// Winsize struct for ioctl
#[repr(C)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

// Placeholder functions - these would be in libc
fn ioctl_tiocswinsz(_fd: i32, _ws: &Winsize) -> i32 {
    0 // Stub
}

fn ioctl_tiocsctty(_fd: i32, _arg: i32) -> i32 {
    0 // Stub
}

fn read_nonblock(fd: i32, buf: &mut [u8]) -> isize {
    // In a real implementation, use fcntl to set O_NONBLOCK first
    // or use poll() to check for data
    recv(fd, buf, sock_msg::DONTWAIT)
}

fn setsid() -> i32 {
    // Stub - would be a syscall
    0
}

fn execv(_path: &str, _args: &[&str]) {
    // Stub - would parse args and exec
    exec(_path);
}
