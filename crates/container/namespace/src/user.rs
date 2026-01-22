//! User Namespace

use crate::{NsError, NsResult, alloc_ns_id};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

/// UID/GID mapping entry
#[derive(Clone, Copy)]
pub struct IdMapping {
    /// Start of ID range inside namespace
    pub inner_start: u32,
    /// Start of ID range outside namespace
    pub outer_start: u32,
    /// Length of range
    pub count: u32,
}

/// User namespace
pub struct UserNamespace {
    /// Unique namespace ID
    id: u64,
    /// Parent namespace
    parent: Option<Arc<UserNamespace>>,
    /// UID mappings
    uid_map: RwLock<Vec<IdMapping>>,
    /// GID mappings
    gid_map: RwLock<Vec<IdMapping>>,
    /// Owner UID (in parent ns)
    owner_uid: u32,
    /// Owner GID (in parent ns)
    owner_gid: u32,
    /// Namespace level
    level: u32,
}

impl UserNamespace {
    /// Create root user namespace
    pub fn root() -> Self {
        let mut ns = UserNamespace {
            id: alloc_ns_id(),
            parent: None,
            uid_map: RwLock::new(Vec::new()),
            gid_map: RwLock::new(Vec::new()),
            owner_uid: 0,
            owner_gid: 0,
            level: 0,
        };

        // Root namespace has identity mapping
        ns.uid_map.write().push(IdMapping {
            inner_start: 0,
            outer_start: 0,
            count: u32::MAX,
        });
        ns.gid_map.write().push(IdMapping {
            inner_start: 0,
            outer_start: 0,
            count: u32::MAX,
        });

        ns
    }

    /// Create child user namespace
    pub fn new(parent: Option<Arc<UserNamespace>>) -> Self {
        let level = parent.as_ref().map(|p| p.level + 1).unwrap_or(0);
        UserNamespace {
            id: alloc_ns_id(),
            parent,
            uid_map: RwLock::new(Vec::new()),
            gid_map: RwLock::new(Vec::new()),
            owner_uid: 0, // Would be set from creating process
            owner_gid: 0,
            level,
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
    pub fn parent(&self) -> Option<&Arc<UserNamespace>> {
        self.parent.as_ref()
    }

    /// Set UID mappings (can only be done once)
    pub fn set_uid_map(&self, mappings: Vec<IdMapping>) -> NsResult<()> {
        let mut uid_map = self.uid_map.write();
        if !uid_map.is_empty() {
            return Err(NsError::InvalidOperation);
        }

        // Validate mappings don't overlap
        for (i, m1) in mappings.iter().enumerate() {
            for m2 in mappings.iter().skip(i + 1) {
                if ranges_overlap(m1.inner_start, m1.count, m2.inner_start, m2.count) {
                    return Err(NsError::InvalidOperation);
                }
            }
        }

        *uid_map = mappings;
        Ok(())
    }

    /// Set GID mappings (can only be done once)
    pub fn set_gid_map(&self, mappings: Vec<IdMapping>) -> NsResult<()> {
        let mut gid_map = self.gid_map.write();
        if !gid_map.is_empty() {
            return Err(NsError::InvalidOperation);
        }

        // Validate mappings don't overlap
        for (i, m1) in mappings.iter().enumerate() {
            for m2 in mappings.iter().skip(i + 1) {
                if ranges_overlap(m1.inner_start, m1.count, m2.inner_start, m2.count) {
                    return Err(NsError::InvalidOperation);
                }
            }
        }

        *gid_map = mappings;
        Ok(())
    }

    /// Map UID from namespace to parent
    pub fn map_uid_to_parent(&self, uid: u32) -> Option<u32> {
        for mapping in self.uid_map.read().iter() {
            if uid >= mapping.inner_start && uid < mapping.inner_start + mapping.count {
                return Some(mapping.outer_start + (uid - mapping.inner_start));
            }
        }
        None
    }

    /// Map UID from parent to namespace
    pub fn map_uid_from_parent(&self, uid: u32) -> Option<u32> {
        for mapping in self.uid_map.read().iter() {
            if uid >= mapping.outer_start && uid < mapping.outer_start + mapping.count {
                return Some(mapping.inner_start + (uid - mapping.outer_start));
            }
        }
        None
    }

    /// Map GID from namespace to parent
    pub fn map_gid_to_parent(&self, gid: u32) -> Option<u32> {
        for mapping in self.gid_map.read().iter() {
            if gid >= mapping.inner_start && gid < mapping.inner_start + mapping.count {
                return Some(mapping.outer_start + (gid - mapping.inner_start));
            }
        }
        None
    }

    /// Map GID from parent to namespace
    pub fn map_gid_from_parent(&self, gid: u32) -> Option<u32> {
        for mapping in self.gid_map.read().iter() {
            if gid >= mapping.outer_start && gid < mapping.outer_start + mapping.count {
                return Some(mapping.inner_start + (gid - mapping.outer_start));
            }
        }
        None
    }

    /// Check if namespace has UID mapping
    pub fn has_uid_map(&self) -> bool {
        !self.uid_map.read().is_empty()
    }

    /// Check if namespace has GID mapping
    pub fn has_gid_map(&self) -> bool {
        !self.gid_map.read().is_empty()
    }

    /// Get owner UID
    pub fn owner_uid(&self) -> u32 {
        self.owner_uid
    }

    /// Get owner GID
    pub fn owner_gid(&self) -> u32 {
        self.owner_gid
    }

    /// Check if UID is root (0) in this namespace
    pub fn is_root(&self, uid: u32) -> bool {
        uid == 0
    }

    /// Check if process has capability in this namespace
    pub fn has_capability(&self, uid: u32, _cap: u32) -> bool {
        // Simplified: root has all capabilities
        uid == 0
    }
}

fn ranges_overlap(start1: u32, count1: u32, start2: u32, count2: u32) -> bool {
    let end1 = start1.saturating_add(count1);
    let end2 = start2.saturating_add(count2);
    start1 < end2 && start2 < end1
}
