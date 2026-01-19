//! PID Namespace

use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};
use crate::{alloc_ns_id, NsResult, NsError};

/// Process ID type
pub type Pid = u32;

/// PID namespace
pub struct PidNamespace {
    /// Unique namespace ID
    id: u64,
    /// Parent namespace (None for root)
    parent: Option<Arc<PidNamespace>>,
    /// Namespace level (root = 0)
    level: u32,
    /// Next PID to allocate
    next_pid: AtomicU32,
    /// PID 1 in this namespace
    init_pid: Mutex<Option<Pid>>,
    /// Mapping: local PID -> global PID
    local_to_global: RwLock<BTreeMap<Pid, Pid>>,
    /// Mapping: global PID -> local PID
    global_to_local: RwLock<BTreeMap<Pid, Pid>>,
    /// Processes in this namespace
    processes: RwLock<Vec<Pid>>,
}

impl PidNamespace {
    /// Create root PID namespace
    pub fn root() -> Self {
        PidNamespace {
            id: alloc_ns_id(),
            parent: None,
            level: 0,
            next_pid: AtomicU32::new(1),
            init_pid: Mutex::new(None),
            local_to_global: RwLock::new(BTreeMap::new()),
            global_to_local: RwLock::new(BTreeMap::new()),
            processes: RwLock::new(Vec::new()),
        }
    }

    /// Create child PID namespace
    pub fn new(parent: Option<Arc<PidNamespace>>) -> Self {
        let level = parent.as_ref().map(|p| p.level + 1).unwrap_or(0);
        PidNamespace {
            id: alloc_ns_id(),
            parent,
            level,
            next_pid: AtomicU32::new(1),
            init_pid: Mutex::new(None),
            local_to_global: RwLock::new(BTreeMap::new()),
            global_to_local: RwLock::new(BTreeMap::new()),
            processes: RwLock::new(Vec::new()),
        }
    }

    /// Get namespace ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get namespace level
    pub fn level(&self) -> u32 {
        self.level
    }

    /// Get parent namespace
    pub fn parent(&self) -> Option<&Arc<PidNamespace>> {
        self.parent.as_ref()
    }

    /// Allocate a new local PID
    pub fn alloc_pid(&self, global_pid: Pid) -> Pid {
        let local_pid = self.next_pid.fetch_add(1, Ordering::SeqCst);

        // Set as init if PID 1
        if local_pid == 1 {
            *self.init_pid.lock() = Some(global_pid);
        }

        // Add mappings
        self.local_to_global.write().insert(local_pid, global_pid);
        self.global_to_local.write().insert(global_pid, local_pid);
        self.processes.write().push(local_pid);

        local_pid
    }

    /// Free a PID
    pub fn free_pid(&self, local_pid: Pid) {
        if let Some(global_pid) = self.local_to_global.write().remove(&local_pid) {
            self.global_to_local.write().remove(&global_pid);
        }
        self.processes.write().retain(|&p| p != local_pid);
    }

    /// Translate local PID to global PID
    pub fn local_to_global(&self, local_pid: Pid) -> Option<Pid> {
        self.local_to_global.read().get(&local_pid).copied()
    }

    /// Translate global PID to local PID
    pub fn global_to_local(&self, global_pid: Pid) -> Option<Pid> {
        self.global_to_local.read().get(&global_pid).copied()
    }

    /// Get init process PID (global)
    pub fn init_pid(&self) -> Option<Pid> {
        *self.init_pid.lock()
    }

    /// Check if namespace is active (has processes)
    pub fn is_active(&self) -> bool {
        !self.processes.read().is_empty()
    }

    /// Get all PIDs in this namespace
    pub fn pids(&self) -> Vec<Pid> {
        self.processes.read().clone()
    }

    /// Check if global PID is visible in this namespace
    pub fn is_visible(&self, global_pid: Pid) -> bool {
        self.global_to_local.read().contains_key(&global_pid)
    }

    /// Get PID as seen from ancestor namespace at given level
    pub fn get_pid_at_level(&self, local_pid: Pid, target_level: u32) -> Option<Pid> {
        if target_level > self.level {
            return None;
        }

        if target_level == self.level {
            return Some(local_pid);
        }

        // Need to walk up the namespace hierarchy
        let global_pid = self.local_to_global(local_pid)?;

        if let Some(ref parent) = self.parent {
            parent.get_pid_at_level_from_global(global_pid, target_level)
        } else {
            // We're at root
            if target_level == 0 {
                Some(global_pid)
            } else {
                None
            }
        }
    }

    fn get_pid_at_level_from_global(&self, global_pid: Pid, target_level: u32) -> Option<Pid> {
        if target_level == self.level {
            return self.global_to_local(global_pid);
        }

        if let Some(ref parent) = self.parent {
            parent.get_pid_at_level_from_global(global_pid, target_level)
        } else {
            if target_level == 0 {
                Some(global_pid)
            } else {
                None
            }
        }
    }
}
