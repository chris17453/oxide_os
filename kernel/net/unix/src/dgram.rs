//! Unix Datagram Socket — SOCK_DGRAM for AF_UNIX
//!
//! — ShadePacket: Unlike stream sockets, datagrams preserve message boundaries.
//! Each sendto() produces exactly one recvfrom(). No connection needed — just
//! bind and fire. DBus uses SOCK_STREAM, but some legacy IPC uses SOCK_DGRAM.
//!
//! Since everything is local (no network), datagrams are "reliable" — no drops,
//! no reordering. The only failure mode is a full receive queue (EAGAIN).

use alloc::vec::Vec;

use crate::ancillary::CmsgData;
use crate::{UnixAddr, UnixError, UnixSocket, UnixSocketType, UNIX_DGRAM_MAX_MSG};

impl UnixSocket {
    /// Send a datagram to a destination address.
    /// — ShadePacket: Looks up the target in the registry, queues the message
    /// in their dgram_queue, wakes their readers. Fire and forget.
    pub fn dgram_sendto(
        &self,
        buf: &[u8],
        dest: &UnixAddr,
        cmsg: Vec<CmsgData>,
    ) -> Result<usize, UnixError> {
        if self.sock_type != UnixSocketType::Dgram {
            return Err(UnixError::WrongType);
        }

        if buf.len() > UNIX_DGRAM_MAX_MSG {
            return Err(UnixError::MsgTooLong);
        }

        // Find the target socket
        let target = crate::lookup_socket(dest).ok_or(UnixError::AddrNotFound)?;

        if target.sock_type != UnixSocketType::Dgram {
            return Err(UnixError::WrongType);
        }

        // Queue the message with our return address
        let sender_addr = self.local_addr.lock().clone();
        let mut queue = target.dgram_queue.lock();

        // — ShadePacket: Arbitrary limit — 256 pending messages. Linux uses
        // sk_max_ack_backlog for this but we keep it simple.
        if queue.len() >= 256 {
            return Err(UnixError::WouldBlock);
        }

        let data = buf.to_vec();
        let len = data.len();
        queue.push((data, sender_addr, cmsg));
        drop(queue);

        // Wake anyone blocking on recvfrom
        target.dgram_recv_wq.wake_all();

        Ok(len)
    }

    /// Receive a datagram from the socket's queue.
    /// — ShadePacket: Returns (data, sender_addr, ancillary_data) or WouldBlock
    /// if the queue is empty. Caller handles blocking.
    pub fn dgram_recvfrom(
        &self,
        buf: &mut [u8],
    ) -> Result<(usize, UnixAddr, Vec<CmsgData>), UnixError> {
        if self.sock_type != UnixSocketType::Dgram {
            return Err(UnixError::WrongType);
        }

        let mut queue = self.dgram_queue.lock();
        if queue.is_empty() {
            return Err(UnixError::WouldBlock);
        }

        let (data, sender, cmsg) = queue.remove(0);
        let copy_len = buf.len().min(data.len());
        buf[..copy_len].copy_from_slice(&data[..copy_len]);

        Ok((copy_len, sender, cmsg))
    }
}
