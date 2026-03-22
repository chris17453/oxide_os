//! Ancillary Data (Control Messages) for Unix Sockets
//!
//! — ColdCipher: The secret weapon of Unix IPC. While read/write handle bytes,
//! sendmsg/recvmsg carry "ancillary data" — metadata that rides alongside the
//! payload. Two critical types:
//!
//! SCM_RIGHTS: Pass file descriptors between processes. The kernel clones the
//! fd from sender's table and installs it in receiver's table. This is how
//! Wayland clients share GPU buffers, how systemd passes sockets, how DBus
//! sends fds. Without this, AF_UNIX is just a fancy pipe.
//!
//! SCM_CREDENTIALS: Prove who you are. The kernel stamps each message with
//! the sender's (pid, uid, gid). No forgery possible — the kernel fills these
//! in, not userspace. DBus uses this for authentication.

use alloc::vec::Vec;

/// Linux constants for ancillary data
pub const SOL_SOCKET: i32 = 1;
pub const SCM_RIGHTS: i32 = 1;
pub const SCM_CREDENTIALS: i32 = 2;

/// Ancillary data types carried through Unix sockets.
/// — ColdCipher: Each variant maps to a specific cmsg_type value.
#[derive(Debug, Clone)]
pub enum CmsgData {
    /// SCM_RIGHTS: file descriptors to transfer.
    /// — ColdCipher: These are the RAW fd numbers from the sender's table.
    /// The syscall layer must translate them: look up Arc<File> in sender's
    /// FdTable, then install in receiver's FdTable to get new fd numbers.
    /// The Vec<i32> here stores the SENDER's fd numbers pre-translation.
    Rights(Vec<i32>),

    /// SCM_CREDENTIALS: process credentials.
    /// — ColdCipher: Kernel-verified. Even if userspace tries to lie,
    /// we overwrite with the real credentials from the sending task.
    Credentials {
        pid: u32,
        uid: u32,
        gid: u32,
    },
}

/// Linux struct cmsghdr layout (for parsing userspace buffers)
/// — ColdCipher: This is the wire format. Must match Linux exactly or every
/// library in existence breaks.
///
/// ```c
/// struct cmsghdr {
///     size_t cmsg_len;    // Length including header (8-byte aligned on x86_64)
///     int    cmsg_level;  // SOL_SOCKET
///     int    cmsg_type;   // SCM_RIGHTS or SCM_CREDENTIALS
///     // Followed by: data (fds or ucred)
/// };
/// ```
pub const CMSGHDR_SIZE: usize = 16; // size_t(8) + int(4) + int(4) on x86_64

/// Align to CMSG alignment boundary (8 bytes on x86_64).
/// — ColdCipher: Linux uses sizeof(size_t) alignment. On x86_64 that's 8.
pub fn cmsg_align(len: usize) -> usize {
    (len + 7) & !7
}

/// Calculate total cmsg length including header.
pub fn cmsg_len(data_len: usize) -> usize {
    CMSGHDR_SIZE + data_len
}

/// Calculate aligned total cmsg space.
pub fn cmsg_space(data_len: usize) -> usize {
    cmsg_align(cmsg_len(data_len))
}

/// Parse ancillary data from a userspace msg_control buffer.
/// — ColdCipher: Walks the cmsg chain, extracting SCM_RIGHTS and SCM_CREDENTIALS.
/// Returns the parsed control messages.
///
/// SAFETY: `control_buf` must be a valid slice copied from userspace.
pub fn parse_cmsg(control_buf: &[u8]) -> Vec<CmsgData> {
    let mut result = Vec::new();
    let mut offset = 0;

    while offset + CMSGHDR_SIZE <= control_buf.len() {
        // Read cmsg_len (first 8 bytes on x86_64)
        let cmsg_len_bytes: [u8; 8] = control_buf[offset..offset + 8]
            .try_into()
            .unwrap_or([0; 8]);
        let cmsg_len = usize::from_ne_bytes(cmsg_len_bytes);

        if cmsg_len < CMSGHDR_SIZE || offset + cmsg_len > control_buf.len() {
            break; // Invalid or truncated
        }

        // Read cmsg_level (next 4 bytes)
        let level_bytes: [u8; 4] = control_buf[offset + 8..offset + 12]
            .try_into()
            .unwrap_or([0; 4]);
        let cmsg_level = i32::from_ne_bytes(level_bytes);

        // Read cmsg_type (next 4 bytes)
        let type_bytes: [u8; 4] = control_buf[offset + 12..offset + 16]
            .try_into()
            .unwrap_or([0; 4]);
        let cmsg_type = i32::from_ne_bytes(type_bytes);

        let data_start = offset + CMSGHDR_SIZE;
        let data_len = cmsg_len - CMSGHDR_SIZE;
        let data_end = data_start + data_len;

        if cmsg_level == SOL_SOCKET {
            match cmsg_type {
                SCM_RIGHTS => {
                    // — ColdCipher: Each fd is an i32 (4 bytes)
                    let num_fds = data_len / 4;
                    let mut fds = Vec::with_capacity(num_fds);
                    for i in 0..num_fds {
                        let fd_offset = data_start + i * 4;
                        if fd_offset + 4 <= control_buf.len() {
                            let fd_bytes: [u8; 4] = control_buf[fd_offset..fd_offset + 4]
                                .try_into()
                                .unwrap_or([0; 4]);
                            fds.push(i32::from_ne_bytes(fd_bytes));
                        }
                    }
                    if !fds.is_empty() {
                        result.push(CmsgData::Rights(fds));
                    }
                }
                SCM_CREDENTIALS => {
                    // struct ucred { pid_t pid; uid_t uid; gid_t gid; } = 12 bytes
                    if data_len >= 12 && data_end <= control_buf.len() {
                        let pid = u32::from_ne_bytes(
                            control_buf[data_start..data_start + 4]
                                .try_into()
                                .unwrap_or([0; 4]),
                        );
                        let uid = u32::from_ne_bytes(
                            control_buf[data_start + 4..data_start + 8]
                                .try_into()
                                .unwrap_or([0; 4]),
                        );
                        let gid = u32::from_ne_bytes(
                            control_buf[data_start + 8..data_start + 12]
                                .try_into()
                                .unwrap_or([0; 4]),
                        );
                        result.push(CmsgData::Credentials { pid, uid, gid });
                    }
                }
                _ => {} // Unknown cmsg_type — skip
            }
        }

        // Move to next cmsg (aligned)
        offset += cmsg_align(cmsg_len);
    }

    result
}

/// Serialize ancillary data into a userspace msg_control buffer.
/// — ColdCipher: Produces Linux-compatible cmsghdr chain.
/// Returns the number of bytes written.
pub fn serialize_cmsg(cmsg_list: &[CmsgData], out_buf: &mut [u8]) -> usize {
    let mut offset = 0;

    for cmsg in cmsg_list {
        match cmsg {
            CmsgData::Rights(fds) => {
                let data_len = fds.len() * 4;
                let total_len = cmsg_len(data_len);
                let aligned_len = cmsg_space(data_len);

                if offset + aligned_len > out_buf.len() {
                    break; // Buffer too small — truncate
                }

                // Write cmsghdr
                out_buf[offset..offset + 8].copy_from_slice(&total_len.to_ne_bytes());
                out_buf[offset + 8..offset + 12]
                    .copy_from_slice(&SOL_SOCKET.to_ne_bytes());
                out_buf[offset + 12..offset + 16]
                    .copy_from_slice(&SCM_RIGHTS.to_ne_bytes());

                // Write fd array
                for (i, fd) in fds.iter().enumerate() {
                    let fd_offset = offset + CMSGHDR_SIZE + i * 4;
                    out_buf[fd_offset..fd_offset + 4].copy_from_slice(&fd.to_ne_bytes());
                }

                // Zero padding to alignment
                let data_end = offset + total_len;
                for byte in out_buf[data_end..offset + aligned_len].iter_mut() {
                    *byte = 0;
                }

                offset += aligned_len;
            }
            CmsgData::Credentials { pid, uid, gid } => {
                let data_len = 12; // sizeof(struct ucred)
                let total_len = cmsg_len(data_len);
                let aligned_len = cmsg_space(data_len);

                if offset + aligned_len > out_buf.len() {
                    break;
                }

                // Write cmsghdr
                out_buf[offset..offset + 8].copy_from_slice(&total_len.to_ne_bytes());
                out_buf[offset + 8..offset + 12]
                    .copy_from_slice(&SOL_SOCKET.to_ne_bytes());
                out_buf[offset + 12..offset + 16]
                    .copy_from_slice(&SCM_CREDENTIALS.to_ne_bytes());

                // Write ucred
                let data_start = offset + CMSGHDR_SIZE;
                out_buf[data_start..data_start + 4].copy_from_slice(&pid.to_ne_bytes());
                out_buf[data_start + 4..data_start + 8]
                    .copy_from_slice(&uid.to_ne_bytes());
                out_buf[data_start + 8..data_start + 12]
                    .copy_from_slice(&gid.to_ne_bytes());

                // Zero padding
                let data_end = offset + total_len;
                for byte in out_buf[data_end..offset + aligned_len].iter_mut() {
                    *byte = 0;
                }

                offset += aligned_len;
            }
        }
    }

    offset
}

/// Calculate the total buffer space needed to serialize a list of cmsgs.
pub fn cmsg_total_space(cmsg_list: &[CmsgData]) -> usize {
    let mut total = 0;
    for cmsg in cmsg_list {
        total += match cmsg {
            CmsgData::Rights(fds) => cmsg_space(fds.len() * 4),
            CmsgData::Credentials { .. } => cmsg_space(12),
        };
    }
    total
}
