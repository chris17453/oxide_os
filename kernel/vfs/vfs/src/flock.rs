//! Advisory file locking (BSD flock semantics)
//!
//! — ColdCipher: Every database, every pid file, every log rotator depends on this.
//! Advisory locks are per open file description (Arc<File>), not per fd.
//! dup()/fork() share the same lock. Last close releases automatically.
//! No pressure.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::error::VfsError;

/// Unique inode identity across filesystems
/// — ColdCipher: (dev, ino) is the canonical way to identify a file.
/// Same trick Unix has used since the 70s. If it ain't broke...
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InodeId {
    pub dev: u64,
    pub ino: u64,
}

/// What kind of lock a file description holds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockType {
    /// Multiple readers allowed, blocks exclusive
    Shared,
    /// Single writer, blocks everything
    Exclusive,
}

/// A single lock held by one open file description
struct LockEntry {
    /// Unique ID for this open file description (Arc<File> identity)
    owner_id: u64,
    lock_type: LockType,
}

/// Per-inode lock state
///
/// — EmberLock: One Vec per inode. Shared locks pile up,
/// exclusive lock means the Vec has exactly one entry from one owner.
/// waiters holds PIDs sleeping on this inode's lock — they get a direct
/// kick in the pants when the lock releases instead of spinning on timer ticks.
struct InodeLockState {
    locks: Vec<LockEntry>,
    /// PIDs blocked waiting for this inode's lock to be released.
    /// — EmberLock: u32 because Pid = u32. No sched dep needed here.
    /// The syscall layer translates these into actual wake_up() calls.
    waiters: Vec<u32>,
}

impl InodeLockState {
    fn new() -> Self {
        InodeLockState {
            locks: Vec::new(),
            waiters: Vec::new(),
        }
    }

    /// Find the index of a lock owned by this owner
    fn find_owner(&self, owner_id: u64) -> Option<usize> {
        self.locks.iter().position(|e| e.owner_id == owner_id)
    }

    /// Check if any exclusive lock is held by someone other than owner_id
    fn has_foreign_exclusive(&self, owner_id: u64) -> bool {
        self.locks.iter().any(|e| {
            e.lock_type == LockType::Exclusive && e.owner_id != owner_id
        })
    }

    /// Check if any lock is held by someone other than owner_id
    fn has_foreign_lock(&self, owner_id: u64) -> bool {
        self.locks.iter().any(|e| e.owner_id != owner_id)
    }

    /// Is this inode state empty (no locks and no waiters)?
    fn is_empty(&self) -> bool {
        self.locks.is_empty() && self.waiters.is_empty()
    }

    /// Register a PID as blocked on this inode's lock.
    /// — EmberLock: Dedup so a re-polling task doesn't bloat the list.
    fn add_waiter(&mut self, pid: u32) {
        if !self.waiters.contains(&pid) {
            self.waiters.push(pid);
        }
    }

    /// Remove a PID from the waiter list (called on cancellation / EINTR).
    /// — EmberLock: Clean up your mess. Don't leave ghost PIDs in the queue.
    fn remove_waiter(&mut self, pid: u32) {
        self.waiters.retain(|&w| w != pid);
    }

    /// Drain all waiters and return them.
    /// — EmberLock: Every PID in this list deserves a wake_up() call.
    /// Spurious wakeups are fine — they'll retry the lock and go back to sleep.
    fn drain_waiters(&mut self) -> Vec<u32> {
        core::mem::take(&mut self.waiters)
    }
}

/// Global flock registry
/// — ColdCipher: The Rosetta Stone of "who locked what." One spin::Mutex
/// guarding a BTreeMap of per-inode lock state. Not glamorous, but it works.
pub struct FlockRegistry {
    inner: Mutex<BTreeMap<InodeId, InodeLockState>>,
}

/// — ColdCipher: Monotonic counter for generating unique owner IDs.
/// Each Arc<File> gets its own identity at birth. Like a social security
/// number, but for file descriptions. And equally impossible to change.
static NEXT_OWNER_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a unique owner ID for a new open file description
pub fn next_owner_id() -> u64 {
    NEXT_OWNER_ID.fetch_add(1, Ordering::Relaxed)
}

/// — ColdCipher: The one true registry. All lock operations go through here.
pub static FLOCK_REGISTRY: FlockRegistry = FlockRegistry::new();

impl FlockRegistry {
    pub const fn new() -> Self {
        FlockRegistry {
            inner: Mutex::new(BTreeMap::new()),
        }
    }

    /// Try to acquire a shared (read) lock.
    ///
    /// — ColdCipher: Shared locks are friendly — multiple readers welcome.
    /// Only fails if someone ELSE holds an exclusive lock.
    /// If we already hold a lock, upgrade/downgrade in place.
    pub fn try_lock_shared(
        &self,
        inode_id: InodeId,
        owner_id: u64,
    ) -> Result<(), VfsError> {
        let mut map = self.inner.lock();
        let state = map.entry(inode_id).or_insert_with(InodeLockState::new);

        // — ColdCipher: If someone else holds exclusive, we can't share.
        if state.has_foreign_exclusive(owner_id) {
            return Err(VfsError::WouldBlock);
        }

        // Upgrade/downgrade if we already hold a lock
        if let Some(idx) = state.find_owner(owner_id) {
            state.locks[idx].lock_type = LockType::Shared;
        } else {
            state.locks.push(LockEntry {
                owner_id,
                lock_type: LockType::Shared,
            });
        }

        Ok(())
    }

    /// Try to acquire an exclusive (write) lock.
    ///
    /// — ColdCipher: Exclusive means exclusive. If ANYONE else holds
    /// any kind of lock, you wait. Or fail. Depends on NB.
    pub fn try_lock_exclusive(
        &self,
        inode_id: InodeId,
        owner_id: u64,
    ) -> Result<(), VfsError> {
        let mut map = self.inner.lock();
        let state = map.entry(inode_id).or_insert_with(InodeLockState::new);

        // — ColdCipher: If anyone else holds any lock, exclusive fails.
        if state.has_foreign_lock(owner_id) {
            return Err(VfsError::WouldBlock);
        }

        // Upgrade/downgrade if we already hold a lock
        if let Some(idx) = state.find_owner(owner_id) {
            state.locks[idx].lock_type = LockType::Exclusive;
        } else {
            state.locks.push(LockEntry {
                owner_id,
                lock_type: LockType::Exclusive,
            });
        }

        Ok(())
    }

    /// Release any lock held by this owner on this inode.
    ///
    /// Returns the list of waiter PIDs that were blocked on this inode.
    /// The caller is responsible for calling wake_up() on each returned PID.
    ///
    /// — EmberLock: unlock() now hands back the waiter list so the syscall
    /// layer can directly kick sleeping tasks awake. No more 10ms timer-tick
    /// lottery for processes waiting on a file lock. We still keep the registry
    /// locked while draining to avoid races — wake_up() happens after we drop it.
    pub fn unlock(&self, inode_id: InodeId, owner_id: u64) -> Vec<u32> {
        let mut map = self.inner.lock();
        if let Some(state) = map.get_mut(&inode_id) {
            state.locks.retain(|e| e.owner_id != owner_id);
            // — EmberLock: Drain waiters under the lock so no new waiter
            // can sneak in and miss the wake signal.
            let waiters = state.drain_waiters();
            if state.is_empty() {
                map.remove(&inode_id);
            }
            waiters
        } else {
            Vec::new()
        }
    }

    /// Register a PID as waiting for a lock on this inode.
    ///
    /// — EmberLock: Called before HLT so the task is visible to unlock().
    /// Must be called with no locks held (takes registry lock internally).
    pub fn register_waiter(&self, inode_id: InodeId, pid: u32) {
        let mut map = self.inner.lock();
        let state = map.entry(inode_id).or_insert_with(InodeLockState::new);
        state.add_waiter(pid);
    }

    /// Remove a PID from the waiter list for an inode.
    ///
    /// — EmberLock: Called on EINTR or after lock acquisition so we don't
    /// leave dead PIDs in the queue haunting future unlock() calls.
    pub fn unregister_waiter(&self, inode_id: InodeId, pid: u32) {
        let mut map = self.inner.lock();
        if let Some(state) = map.get_mut(&inode_id) {
            state.remove_waiter(pid);
            if state.is_empty() {
                map.remove(&inode_id);
            }
        }
    }
}
