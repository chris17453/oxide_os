//! IPC Namespace (System V IPC)

use crate::{NsError, NsResult, alloc_ns_id};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

/// IPC key type
pub type IpcKey = i32;
/// IPC ID type
pub type IpcId = i32;

/// Shared memory segment
#[derive(Clone)]
pub struct ShmSegment {
    /// Segment ID
    pub id: IpcId,
    /// Key
    pub key: IpcKey,
    /// Size in bytes
    pub size: usize,
    /// Owner UID
    pub uid: u32,
    /// Owner GID
    pub gid: u32,
    /// Creator UID
    pub cuid: u32,
    /// Creator GID
    pub cgid: u32,
    /// Permissions
    pub mode: u16,
    /// Number of attaches
    pub nattch: u32,
}

/// Semaphore set
#[derive(Clone)]
pub struct SemaphoreSet {
    /// Set ID
    pub id: IpcId,
    /// Key
    pub key: IpcKey,
    /// Number of semaphores
    pub nsems: u32,
    /// Owner UID
    pub uid: u32,
    /// Owner GID
    pub gid: u32,
    /// Permissions
    pub mode: u16,
    /// Semaphore values
    pub values: Vec<i16>,
}

/// Message queue
#[derive(Clone)]
pub struct MessageQueue {
    /// Queue ID
    pub id: IpcId,
    /// Key
    pub key: IpcKey,
    /// Owner UID
    pub uid: u32,
    /// Owner GID
    pub gid: u32,
    /// Permissions
    pub mode: u16,
    /// Number of messages
    pub msg_qnum: u32,
    /// Max bytes in queue
    pub msg_qbytes: u32,
}

/// IPC namespace
pub struct IpcNamespace {
    /// Unique namespace ID
    id: u64,
    /// Parent namespace
    parent: Option<Arc<IpcNamespace>>,
    /// Shared memory segments
    shm_segments: RwLock<BTreeMap<IpcId, ShmSegment>>,
    /// Semaphore sets
    sem_sets: RwLock<BTreeMap<IpcId, SemaphoreSet>>,
    /// Message queues
    msg_queues: RwLock<BTreeMap<IpcId, MessageQueue>>,
    /// Next SHM ID
    next_shm_id: AtomicU32,
    /// Next SEM ID
    next_sem_id: AtomicU32,
    /// Next MSG ID
    next_msg_id: AtomicU32,
}

impl IpcNamespace {
    /// Create root IPC namespace
    pub fn root() -> Self {
        IpcNamespace {
            id: alloc_ns_id(),
            parent: None,
            shm_segments: RwLock::new(BTreeMap::new()),
            sem_sets: RwLock::new(BTreeMap::new()),
            msg_queues: RwLock::new(BTreeMap::new()),
            next_shm_id: AtomicU32::new(0),
            next_sem_id: AtomicU32::new(0),
            next_msg_id: AtomicU32::new(0),
        }
    }

    /// Create child IPC namespace (starts empty)
    pub fn new(parent: Option<Arc<IpcNamespace>>) -> Self {
        IpcNamespace {
            id: alloc_ns_id(),
            parent,
            shm_segments: RwLock::new(BTreeMap::new()),
            sem_sets: RwLock::new(BTreeMap::new()),
            msg_queues: RwLock::new(BTreeMap::new()),
            next_shm_id: AtomicU32::new(0),
            next_sem_id: AtomicU32::new(0),
            next_msg_id: AtomicU32::new(0),
        }
    }

    /// Get namespace ID
    pub fn id(&self) -> u64 {
        self.id
    }

    // Shared Memory operations

    /// Create shared memory segment
    pub fn shmget(
        &self,
        key: IpcKey,
        size: usize,
        flags: u32,
        uid: u32,
        gid: u32,
    ) -> NsResult<IpcId> {
        // Check if key already exists (unless IPC_PRIVATE)
        if key != 0 {
            let segments = self.shm_segments.read();
            if let Some(seg) = segments.values().find(|s| s.key == key) {
                if flags & 0x200 != 0 {
                    // IPC_EXCL
                    return Err(NsError::InvalidOperation);
                }
                return Ok(seg.id);
            }
        }

        let id = self.next_shm_id.fetch_add(1, Ordering::SeqCst) as IpcId;
        let segment = ShmSegment {
            id,
            key,
            size,
            uid,
            gid,
            cuid: uid,
            cgid: gid,
            mode: (flags & 0x1FF) as u16,
            nattch: 0,
        };

        self.shm_segments.write().insert(id, segment);
        Ok(id)
    }

    /// Get shared memory segment
    pub fn shm_get(&self, id: IpcId) -> Option<ShmSegment> {
        self.shm_segments.read().get(&id).cloned()
    }

    /// Remove shared memory segment
    pub fn shmctl_rmid(&self, id: IpcId) -> NsResult<()> {
        if self.shm_segments.write().remove(&id).is_some() {
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }

    // Semaphore operations

    /// Create semaphore set
    pub fn semget(
        &self,
        key: IpcKey,
        nsems: u32,
        flags: u32,
        uid: u32,
        gid: u32,
    ) -> NsResult<IpcId> {
        if key != 0 {
            let sets = self.sem_sets.read();
            if let Some(set) = sets.values().find(|s| s.key == key) {
                if flags & 0x200 != 0 {
                    return Err(NsError::InvalidOperation);
                }
                return Ok(set.id);
            }
        }

        let id = self.next_sem_id.fetch_add(1, Ordering::SeqCst) as IpcId;
        let set = SemaphoreSet {
            id,
            key,
            nsems,
            uid,
            gid,
            mode: (flags & 0x1FF) as u16,
            values: alloc::vec![0; nsems as usize],
        };

        self.sem_sets.write().insert(id, set);
        Ok(id)
    }

    /// Get semaphore set
    pub fn sem_get(&self, id: IpcId) -> Option<SemaphoreSet> {
        self.sem_sets.read().get(&id).cloned()
    }

    /// Remove semaphore set
    pub fn semctl_rmid(&self, id: IpcId) -> NsResult<()> {
        if self.sem_sets.write().remove(&id).is_some() {
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }

    // Message queue operations

    /// Create message queue
    pub fn msgget(&self, key: IpcKey, flags: u32, uid: u32, gid: u32) -> NsResult<IpcId> {
        if key != 0 {
            let queues = self.msg_queues.read();
            if let Some(queue) = queues.values().find(|q| q.key == key) {
                if flags & 0x200 != 0 {
                    return Err(NsError::InvalidOperation);
                }
                return Ok(queue.id);
            }
        }

        let id = self.next_msg_id.fetch_add(1, Ordering::SeqCst) as IpcId;
        let queue = MessageQueue {
            id,
            key,
            uid,
            gid,
            mode: (flags & 0x1FF) as u16,
            msg_qnum: 0,
            msg_qbytes: 16384,
        };

        self.msg_queues.write().insert(id, queue);
        Ok(id)
    }

    /// Get message queue
    pub fn msg_get(&self, id: IpcId) -> Option<MessageQueue> {
        self.msg_queues.read().get(&id).cloned()
    }

    /// Remove message queue
    pub fn msgctl_rmid(&self, id: IpcId) -> NsResult<()> {
        if self.msg_queues.write().remove(&id).is_some() {
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }
}
