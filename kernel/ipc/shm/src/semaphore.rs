//! System V Semaphores
//!
//! — ThreadRogue: counting semaphore arrays for inter-process synchronization.
//! Each semaphore set contains an array of semaphores, each with a non-negative
//! integer value. semop() atomically adjusts values — decrement blocks if value
//! would go negative, increment always succeeds.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

/// IPC constants
pub const IPC_PRIVATE: u32 = 0;
pub const IPC_CREAT: u32 = 0o1000;
pub const IPC_EXCL: u32 = 0o2000;
pub const IPC_RMID: u32 = 0;
pub const IPC_NOWAIT: u16 = 0o4000;

/// semop operation flags
pub const SEM_UNDO: u16 = 0x1000;

/// Maximum semaphores per set
const SEM_MAX_PER_SET: usize = 256;

/// A semaphore operation (matches struct sembuf)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SemBuf {
    pub sem_num: u16,
    pub sem_op: i16,
    pub sem_flg: u16,
}

/// A semaphore set
pub struct SemSet {
    pub id: u32,
    pub key: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub values: Vec<i32>,
}

impl SemSet {
    pub fn new(id: u32, key: u32, nsems: usize, mode: u32, uid: u32, gid: u32) -> Self {
        Self {
            id, key, mode, uid, gid,
            values: alloc::vec![0i32; nsems],
        }
    }

    /// Apply a batch of semaphore operations atomically.
    /// — ThreadRogue: all-or-nothing semantics. If any operation would block
    /// (decrement below 0 with no IPC_NOWAIT), the whole batch fails with EAGAIN.
    /// Real Linux blocks the process — we return EAGAIN for now.
    pub fn semop(&mut self, ops: &[SemBuf]) -> Result<(), i64> {
        // — ThreadRogue: check all operations first (dry run)
        for op in ops {
            let idx = op.sem_num as usize;
            if idx >= self.values.len() { return Err(-22); } // EINVAL

            if op.sem_op < 0 {
                let new_val = self.values[idx] + op.sem_op as i32;
                if new_val < 0 {
                    if op.sem_flg & IPC_NOWAIT != 0 { return Err(-11); } // EAGAIN
                    return Err(-11); // — ThreadRogue: would block
                }
            }
            // sem_op == 0 means "wait for zero" — check if already zero
            if op.sem_op == 0 && self.values[idx] != 0 {
                if op.sem_flg & IPC_NOWAIT != 0 { return Err(-11); }
                return Err(-11);
            }
        }

        // — ThreadRogue: all checks passed — apply atomically
        for op in ops {
            let idx = op.sem_num as usize;
            if op.sem_op != 0 {
                self.values[idx] += op.sem_op as i32;
            }
        }
        Ok(())
    }

    /// Set a specific semaphore's value (SETVAL)
    pub fn setval(&mut self, sem_num: usize, val: i32) -> Result<(), i64> {
        if sem_num >= self.values.len() { return Err(-22); }
        self.values[sem_num] = val;
        Ok(())
    }

    /// Get a specific semaphore's value (GETVAL)
    pub fn getval(&self, sem_num: usize) -> Result<i32, i64> {
        if sem_num >= self.values.len() { return Err(-22); }
        Ok(self.values[sem_num])
    }
}

/// Global semaphore registry
pub struct SemRegistry {
    sets: BTreeMap<u32, SemSet>,
    next_id: u32,
}

impl SemRegistry {
    pub const fn new() -> Self {
        Self { sets: BTreeMap::new(), next_id: 1 }
    }

    pub fn semget(&mut self, key: u32, nsems: usize, flags: u32, uid: u32, gid: u32) -> Result<u32, i64> {
        if nsems > SEM_MAX_PER_SET { return Err(-22); }
        if key != IPC_PRIVATE {
            for (id, s) in self.sets.iter() {
                if s.key == key {
                    if flags & IPC_EXCL != 0 && flags & IPC_CREAT != 0 { return Err(-17); }
                    return Ok(*id);
                }
            }
        }
        if key != IPC_PRIVATE && flags & IPC_CREAT == 0 { return Err(-2); }
        if nsems == 0 { return Err(-22); }
        let id = self.next_id; self.next_id += 1;
        let mode = flags & 0o777;
        self.sets.insert(id, SemSet::new(id, key, nsems, mode, uid, gid));
        Ok(id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut SemSet> { self.sets.get_mut(&id) }
    pub fn remove(&mut self, id: u32) -> Result<(), i64> {
        self.sets.remove(&id).map(|_| ()).ok_or(-22)
    }
}

static SEM_REGISTRY: Mutex<SemRegistry> = Mutex::new(SemRegistry::new());

pub fn sys_semget(key: u32, nsems: usize, flags: u32, uid: u32, gid: u32) -> i64 {
    match SEM_REGISTRY.lock().semget(key, nsems, flags, uid, gid) {
        Ok(id) => id as i64, Err(e) => e,
    }
}

pub fn sys_semop(semid: u32, sops: *const SemBuf, nsops: usize) -> i64 {
    if sops.is_null() || nsops == 0 || nsops > 32 { return -22; }
    let ops = unsafe { core::slice::from_raw_parts(sops, nsops) };
    match SEM_REGISTRY.lock().get_mut(semid) {
        Some(set) => match set.semop(ops) { Ok(()) => 0, Err(e) => e },
        None => -22,
    }
}

/// SEMCTL operations
pub const GETVAL: u32 = 12;
pub const SETVAL: u32 = 16;

pub fn sys_semctl(semid: u32, semnum: u32, cmd: u32, arg: u64) -> i64 {
    match cmd {
        0 /* IPC_RMID */ => match SEM_REGISTRY.lock().remove(semid) { Ok(()) => 0, Err(e) => e },
        GETVAL => match SEM_REGISTRY.lock().get_mut(semid) {
            Some(set) => match set.getval(semnum as usize) { Ok(v) => v as i64, Err(e) => e },
            None => -22,
        },
        SETVAL => match SEM_REGISTRY.lock().get_mut(semid) {
            Some(set) => match set.setval(semnum as usize, arg as i32) { Ok(()) => 0, Err(e) => e },
            None => -22,
        },
        _ => -22,
    }
}
