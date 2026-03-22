//! System V Shared Memory Implementation
//!
//! — ThreadRogue: the shared memory subsystem. Processes create named segments
//! via shmget(), attach them to their address space via shmat(), and share
//! physical pages across process boundaries. The kernel maintains a global
//! registry of segments, each backed by a pool of physical frames that are
//! mapped into every attaching process's page tables.
//!
//! Key design:
//! - Segments are identified by integer key (IPC_PRIVATE = 0 for anonymous)
//! - Physical frames are allocated lazily on first access (demand-paged)
//! - PageDB refcounting tracks shared ownership — frames freed when last process detaches
//! - Segment metadata persists until explicit IPC_RMID or system shutdown

#![no_std]

extern crate alloc;

pub mod msgqueue;
pub mod semaphore;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use os_core::PhysAddr;
use spin::Mutex;

/// IPC key for private (unkeyed) segments
pub const IPC_PRIVATE: u32 = 0;

/// IPC flags
pub const IPC_CREAT: u32 = 0o1000;
pub const IPC_EXCL: u32 = 0o2000;
pub const IPC_RMID: u32 = 0;
pub const IPC_SET: u32 = 1;
pub const IPC_STAT: u32 = 2;

/// Permission bits mask
pub const SHM_PERM_MASK: u32 = 0o777;

/// — ThreadRogue: a shared memory segment. Tracks the physical frames backing
/// the segment and which processes have attached it. Frames are allocated
/// on first attach (or lazily on fault) and freed when nattch drops to 0
/// AND the segment is marked for removal.
pub struct ShmSegment {
    /// Unique segment ID (returned by shmget)
    pub id: u32,
    /// IPC key (0 = IPC_PRIVATE)
    pub key: u32,
    /// Segment size in bytes (page-aligned internally)
    pub size: usize,
    /// Creator's UID
    pub uid: u32,
    /// Creator's GID
    pub gid: u32,
    /// Permission mode bits (rwxrwxrwx)
    pub mode: u32,
    /// Number of current attachments
    pub nattch: AtomicU32,
    /// Physical frames backing this segment (indexed by page number within segment)
    /// — ThreadRogue: None = not yet allocated (demand-page on fault)
    pub frames: Vec<Option<PhysAddr>>,
    /// Marked for removal (IPC_RMID called but nattch > 0)
    pub marked_for_removal: bool,
}

impl ShmSegment {
    /// Create a new shared memory segment
    pub fn new(id: u32, key: u32, size: usize, uid: u32, gid: u32, mode: u32) -> Self {
        let num_pages = (size + 4095) / 4096;
        let mut frames = Vec::with_capacity(num_pages);
        for _ in 0..num_pages {
            frames.push(None);
        }
        Self {
            id,
            key,
            size,
            uid,
            gid,
            mode,
            nattch: AtomicU32::new(0),
            frames,
            marked_for_removal: false,
        }
    }

    /// Number of pages in this segment
    pub fn num_pages(&self) -> usize {
        self.frames.len()
    }

    /// Get or allocate a physical frame for a page in the segment.
    /// Returns the physical address of the frame.
    /// — ThreadRogue: thread-safe — called from page fault handler
    pub fn get_or_alloc_frame(&mut self, page_idx: usize, allocator: &dyn mm_traits::FrameAllocator) -> Option<PhysAddr> {
        if page_idx >= self.frames.len() {
            return None;
        }
        if let Some(frame) = self.frames[page_idx] {
            return Some(frame);
        }
        // Allocate a new frame
        let frame = allocator.alloc_frame()?;
        // Zero the frame
        let virt = mm_paging::phys_to_virt(frame);
        unsafe {
            core::ptr::write_bytes(virt.as_mut_ptr::<u8>(), 0, 4096);
        }
        self.frames[page_idx] = Some(frame);
        Some(frame)
    }
}

/// — ThreadRogue: global shared memory registry. All segments live here,
/// keyed by segment ID. Processes reference segments by ID.
pub struct ShmRegistry {
    segments: BTreeMap<u32, ShmSegment>,
    next_id: u32,
}

impl ShmRegistry {
    pub const fn new() -> Self {
        Self {
            segments: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// shmget — create or find a shared memory segment.
    /// Returns segment ID or error.
    /// — ThreadRogue: if key == IPC_PRIVATE, always creates new segment.
    /// If key != 0 and IPC_CREAT set, creates if doesn't exist.
    /// If key != 0 and IPC_EXCL set, fails if already exists.
    pub fn shmget(&mut self, key: u32, size: usize, flags: u32, uid: u32, gid: u32) -> Result<u32, i64> {
        if size == 0 || size > 256 * 1024 * 1024 {
            // — ThreadRogue: max 256MB per segment
            return Err(-22); // EINVAL
        }

        // Check for existing segment with this key
        if key != IPC_PRIVATE {
            for (id, seg) in self.segments.iter() {
                if seg.key == key {
                    if flags & IPC_EXCL != 0 && flags & IPC_CREAT != 0 {
                        return Err(-17); // EEXIST
                    }
                    return Ok(*id);
                }
            }
        }

        // Create new segment
        if key != IPC_PRIVATE && flags & IPC_CREAT == 0 {
            return Err(-2); // ENOENT
        }

        let id = self.next_id;
        self.next_id += 1;
        let mode = flags & SHM_PERM_MASK;
        let segment = ShmSegment::new(id, key, size, uid, gid, mode);
        self.segments.insert(id, segment);
        Ok(id)
    }

    /// Get a mutable reference to a segment by ID
    pub fn get_mut(&mut self, id: u32) -> Option<&mut ShmSegment> {
        self.segments.get_mut(&id)
    }

    /// Get an immutable reference to a segment by ID
    pub fn get(&self, id: u32) -> Option<&ShmSegment> {
        self.segments.get(&id)
    }

    /// Remove a segment (IPC_RMID).
    /// If nattch > 0, mark for deferred removal.
    /// If nattch == 0, remove immediately and free frames.
    pub fn remove(&mut self, id: u32, allocator: &dyn mm_traits::FrameAllocator) -> Result<(), i64> {
        let seg = self.segments.get_mut(&id).ok_or(-22i64)?; // EINVAL
        if seg.nattch.load(Ordering::Relaxed) > 0 {
            seg.marked_for_removal = true;
            Ok(())
        } else {
            // Free all allocated frames
            let seg = self.segments.remove(&id).unwrap();
            for frame_opt in &seg.frames {
                if let Some(frame) = frame_opt {
                    allocator.free_frame(*frame);
                }
            }
            Ok(())
        }
    }

    /// Called when a process detaches — if marked for removal and nattch == 0, clean up.
    pub fn try_cleanup(&mut self, id: u32, allocator: &dyn mm_traits::FrameAllocator) {
        if let Some(seg) = self.segments.get(&id) {
            if seg.marked_for_removal && seg.nattch.load(Ordering::Relaxed) == 0 {
                let seg = self.segments.remove(&id).unwrap();
                for frame_opt in &seg.frames {
                    if let Some(frame) = frame_opt {
                        allocator.free_frame(*frame);
                    }
                }
            }
        }
    }
}

/// Global shared memory registry
static SHM_REGISTRY: Mutex<ShmRegistry> = Mutex::new(ShmRegistry::new());

/// Get the global SHM registry
pub fn registry() -> &'static Mutex<ShmRegistry> {
    &SHM_REGISTRY
}

/// — ThreadRogue: shmget syscall implementation
pub fn sys_shmget(key: u32, size: usize, flags: u32, uid: u32, gid: u32) -> i64 {
    match SHM_REGISTRY.lock().shmget(key, size, flags, uid, gid) {
        Ok(id) => id as i64,
        Err(e) => e,
    }
}

/// — ThreadRogue: shmctl syscall implementation (IPC_RMID only for now)
pub fn sys_shmctl(shmid: u32, cmd: u32, allocator: &dyn mm_traits::FrameAllocator) -> i64 {
    match cmd {
        IPC_RMID => {
            match SHM_REGISTRY.lock().remove(shmid, allocator) {
                Ok(()) => 0,
                Err(e) => e,
            }
        }
        IPC_STAT => {
            // — ThreadRogue: return segment info. Would need a shmid_ds struct.
            // For now, return 0 (success) if segment exists.
            if SHM_REGISTRY.lock().get(shmid).is_some() { 0 } else { -22 }
        }
        _ => -22, // EINVAL
    }
}
