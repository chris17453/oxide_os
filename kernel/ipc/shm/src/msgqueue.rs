//! System V Message Queues
//!
//! — ThreadRogue: inter-process message passing. Processes send typed messages
//! to a queue, and receivers can filter by message type. Each queue has a
//! fixed capacity and blocks senders when full.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

/// Maximum message size (bytes)
const MSG_MAX_SIZE: usize = 8192;
/// Maximum messages per queue
const MSG_MAX_QUEUE: usize = 256;
/// Maximum total bytes per queue
const MSG_MAX_BYTES: usize = 64 * 1024;

/// IPC constants
pub const IPC_PRIVATE: u32 = 0;
pub const IPC_CREAT: u32 = 0o1000;
pub const IPC_EXCL: u32 = 0o2000;
pub const IPC_RMID: u32 = 0;
pub const IPC_NOWAIT: u32 = 0o4000;

/// A message in the queue
/// — ThreadRogue: type + data. Type 0 is invalid; positive types allow
/// selective receive (msgrcv with type > 0 gets first message of that type).
struct Message {
    mtype: i64,
    data: Vec<u8>,
}

/// A message queue
pub struct MsgQueue {
    pub id: u32,
    pub key: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    messages: VecDeque<Message>,
    total_bytes: usize,
}

impl MsgQueue {
    pub fn new(id: u32, key: u32, mode: u32, uid: u32, gid: u32) -> Self {
        Self {
            id, key, mode, uid, gid,
            messages: VecDeque::new(),
            total_bytes: 0,
        }
    }

    /// Send a message to the queue
    pub fn send(&mut self, mtype: i64, data: &[u8], flags: u32) -> Result<(), i64> {
        if mtype <= 0 { return Err(-22); } // EINVAL
        if data.len() > MSG_MAX_SIZE { return Err(-22); } // EINVAL
        if self.messages.len() >= MSG_MAX_QUEUE || self.total_bytes + data.len() > MSG_MAX_BYTES {
            if flags & IPC_NOWAIT != 0 { return Err(-11); } // EAGAIN
            return Err(-11); // — ThreadRogue: would block. Real impl needs wait queue.
        }
        self.total_bytes += data.len();
        self.messages.push_back(Message { mtype, data: data.to_vec() });
        Ok(())
    }

    /// Receive a message from the queue
    /// — ThreadRogue: msgtype filtering:
    ///   0 = first message of any type
    ///   >0 = first message with this exact type
    ///   <0 = first message with type <= |msgtype|
    pub fn recv(&mut self, msgtype: i64, max_size: usize, flags: u32) -> Result<(i64, Vec<u8>), i64> {
        let idx = if msgtype == 0 {
            // Any type — take first
            if self.messages.is_empty() {
                if flags & IPC_NOWAIT != 0 { return Err(-11); } // EAGAIN
                return Err(-11);
            }
            0
        } else if msgtype > 0 {
            // Exact type match
            match self.messages.iter().position(|m| m.mtype == msgtype) {
                Some(i) => i,
                None => {
                    if flags & IPC_NOWAIT != 0 { return Err(-11); }
                    return Err(-11);
                }
            }
        } else {
            // msgtype < 0: first message with type <= |msgtype|
            let abs_type = -msgtype;
            match self.messages.iter().position(|m| m.mtype <= abs_type) {
                Some(i) => i,
                None => {
                    if flags & IPC_NOWAIT != 0 { return Err(-11); }
                    return Err(-11);
                }
            }
        };

        let msg = self.messages.remove(idx).unwrap();
        self.total_bytes -= msg.data.len();

        if msg.data.len() > max_size {
            return Err(-34); // ERANGE — message too big for buffer
        }

        Ok((msg.mtype, msg.data))
    }
}

/// Global message queue registry
pub struct MsgRegistry {
    queues: BTreeMap<u32, MsgQueue>,
    next_id: u32,
}

impl MsgRegistry {
    pub const fn new() -> Self {
        Self { queues: BTreeMap::new(), next_id: 1 }
    }

    pub fn msgget(&mut self, key: u32, flags: u32, uid: u32, gid: u32) -> Result<u32, i64> {
        if key != IPC_PRIVATE {
            for (id, q) in self.queues.iter() {
                if q.key == key {
                    if flags & IPC_EXCL != 0 && flags & IPC_CREAT != 0 { return Err(-17); }
                    return Ok(*id);
                }
            }
        }
        if key != IPC_PRIVATE && flags & IPC_CREAT == 0 { return Err(-2); }
        let id = self.next_id; self.next_id += 1;
        let mode = flags & 0o777;
        self.queues.insert(id, MsgQueue::new(id, key, mode, uid, gid));
        Ok(id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut MsgQueue> { self.queues.get_mut(&id) }
    pub fn remove(&mut self, id: u32) -> Result<(), i64> {
        self.queues.remove(&id).map(|_| ()).ok_or(-22)
    }
}

static MSG_REGISTRY: Mutex<MsgRegistry> = Mutex::new(MsgRegistry::new());

pub fn sys_msgget(key: u32, flags: u32, uid: u32, gid: u32) -> i64 {
    match MSG_REGISTRY.lock().msgget(key, flags, uid, gid) {
        Ok(id) => id as i64, Err(e) => e,
    }
}

pub fn sys_msgsnd(msqid: u32, msgp: *const u8, msgsz: usize, msgflg: u32) -> i64 {
    if msgp.is_null() || msgsz > MSG_MAX_SIZE { return -22; }
    // — ThreadRogue: msgbuf format: first 8 bytes = mtype (i64), then data
    let mtype = unsafe { *(msgp as *const i64) };
    let data = unsafe { core::slice::from_raw_parts(msgp.add(8), msgsz) };
    match MSG_REGISTRY.lock().get_mut(msqid) {
        Some(q) => match q.send(mtype, data, msgflg) { Ok(()) => 0, Err(e) => e },
        None => -22,
    }
}

pub fn sys_msgrcv(msqid: u32, msgp: *mut u8, msgsz: usize, msgtyp: i64, msgflg: u32) -> i64 {
    if msgp.is_null() { return -22; }
    match MSG_REGISTRY.lock().get_mut(msqid) {
        Some(q) => match q.recv(msgtyp, msgsz, msgflg) {
            Ok((mtype, data)) => {
                unsafe {
                    *(msgp as *mut i64) = mtype;
                    core::ptr::copy_nonoverlapping(data.as_ptr(), msgp.add(8), data.len());
                }
                data.len() as i64
            }
            Err(e) => e,
        },
        None => -22,
    }
}

pub fn sys_msgctl(msqid: u32, cmd: u32) -> i64 {
    match cmd {
        IPC_RMID => match MSG_REGISTRY.lock().remove(msqid) { Ok(()) => 0, Err(e) => e },
        _ => -22,
    }
}
