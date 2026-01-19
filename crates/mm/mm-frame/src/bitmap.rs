//! Bitmap-based frame allocator

use crate::{MemoryRegion, PhysFrame, FRAME_SIZE};
use os_core::PhysAddr;
use mm_traits::FrameAllocator;
use spin::Mutex;

/// Maximum physical memory we support (4GB for now)
const MAX_MEMORY: usize = 4 * 1024 * 1024 * 1024;
/// Maximum number of frames
const MAX_FRAMES: usize = MAX_MEMORY / FRAME_SIZE;
/// Size of bitmap in u64 words
const BITMAP_WORDS: usize = MAX_FRAMES / 64;

/// Bitmap frame allocator
///
/// Uses a bitmap to track which frames are free (0) or used (1).
/// Thread-safe via internal locking.
pub struct BitmapFrameAllocator {
    inner: Mutex<BitmapInner>,
}

struct BitmapInner {
    /// Bitmap: bit N represents frame N. 0 = free, 1 = used
    bitmap: [u64; BITMAP_WORDS],
    /// Total number of frames in system
    total_frames: usize,
    /// Number of free frames
    free_frames: usize,
    /// Next frame to check (for faster allocation)
    next_free: usize,
}

impl BitmapFrameAllocator {
    /// Create a new bitmap frame allocator
    ///
    /// Initially all frames are marked as used. Call `add_region` for each
    /// usable memory region.
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(BitmapInner {
                bitmap: [!0u64; BITMAP_WORDS], // All bits set = all used
                total_frames: 0,
                free_frames: 0,
                next_free: 0,
            }),
        }
    }

    /// Initialize from memory regions
    ///
    /// Marks usable regions as free, keeping reserved regions as used.
    pub fn init(&self, regions: &[MemoryRegion]) {
        let mut inner = self.inner.lock();

        // First, determine total memory and mark all as used
        let mut max_addr: u64 = 0;
        for region in regions {
            let end = region.end().as_u64();
            if end > max_addr {
                max_addr = end;
            }
        }

        let total = (max_addr as usize).min(MAX_MEMORY) / FRAME_SIZE;
        inner.total_frames = total;

        // Mark usable regions as free
        for region in regions {
            if region.usable {
                let start_frame = region.start.page_align_up().as_usize() / FRAME_SIZE;
                let end_frame = region.end().page_align_down().as_usize() / FRAME_SIZE;

                for frame in start_frame..end_frame {
                    if frame < MAX_FRAMES {
                        let word = frame / 64;
                        let bit = frame % 64;
                        if inner.bitmap[word] & (1 << bit) != 0 {
                            inner.bitmap[word] &= !(1 << bit);
                            inner.free_frames += 1;
                        }
                    }
                }
            }
        }

        inner.next_free = 0;
    }

    /// Mark a range of frames as used (for kernel, bootloader, etc.)
    pub fn mark_used(&self, start: PhysAddr, len: usize) {
        let mut inner = self.inner.lock();
        let start_frame = start.page_align_down().as_usize() / FRAME_SIZE;
        let end_frame = (start.as_usize() + len + FRAME_SIZE - 1) / FRAME_SIZE;

        for frame in start_frame..end_frame {
            if frame < MAX_FRAMES {
                let word = frame / 64;
                let bit = frame % 64;
                if inner.bitmap[word] & (1 << bit) == 0 {
                    inner.bitmap[word] |= 1 << bit;
                    inner.free_frames = inner.free_frames.saturating_sub(1);
                }
            }
        }
    }

    /// Get total number of frames
    pub fn total_frames(&self) -> usize {
        self.inner.lock().total_frames
    }

    /// Get number of free frames
    pub fn free_frame_count(&self) -> usize {
        self.inner.lock().free_frames
    }

    /// Get number of used frames
    pub fn used_frames(&self) -> usize {
        let inner = self.inner.lock();
        inner.total_frames - inner.free_frames
    }
}

impl FrameAllocator for BitmapFrameAllocator {
    fn alloc_frame(&self) -> Option<PhysAddr> {
        let mut inner = self.inner.lock();

        // Start searching from next_free hint (but never allocate frame 0)
        let start = inner.next_free.max(1) / 64;
        for word_idx in 0..BITMAP_WORDS {
            let idx = (start + word_idx) % BITMAP_WORDS;
            let word = inner.bitmap[idx];

            if word != !0u64 {
                // Find first zero bit
                let bit = (!word).trailing_zeros() as usize;
                let frame = idx * 64 + bit;

                // Never allocate frame 0 (NULL page protection)
                if frame == 0 {
                    // Mark frame 0 as used and continue searching
                    inner.bitmap[0] |= 1;
                    if inner.free_frames > 0 {
                        inner.free_frames -= 1;
                    }
                    continue;
                }

                if frame < inner.total_frames {
                    inner.bitmap[idx] |= 1 << bit;
                    inner.free_frames -= 1;
                    inner.next_free = frame + 1;
                    return Some(PhysFrame::from_number(frame).start_addr());
                }
            }
        }

        None
    }

    fn free_frame(&self, addr: PhysAddr) {
        let mut inner = self.inner.lock();
        let frame = addr.page_align_down().as_usize() / FRAME_SIZE;

        if frame < MAX_FRAMES {
            let word = frame / 64;
            let bit = frame % 64;

            if inner.bitmap[word] & (1 << bit) != 0 {
                inner.bitmap[word] &= !(1 << bit);
                inner.free_frames += 1;

                if frame < inner.next_free {
                    inner.next_free = frame;
                }
            }
        }
    }

    fn alloc_frames(&self, count: usize) -> Option<PhysAddr> {
        if count == 0 {
            return None;
        }
        if count == 1 {
            return self.alloc_frame();
        }

        let mut inner = self.inner.lock();

        // Simple linear scan for contiguous frames
        let mut start_frame = 0;
        let mut consecutive = 0;

        for frame in 0..inner.total_frames {
            let word = frame / 64;
            let bit = frame % 64;

            if inner.bitmap[word] & (1 << bit) == 0 {
                if consecutive == 0 {
                    start_frame = frame;
                }
                consecutive += 1;

                if consecutive == count {
                    // Found enough! Mark them all as used
                    for f in start_frame..start_frame + count {
                        let w = f / 64;
                        let b = f % 64;
                        inner.bitmap[w] |= 1 << b;
                    }
                    inner.free_frames -= count;
                    inner.next_free = start_frame + count;
                    return Some(PhysFrame::from_number(start_frame).start_addr());
                }
            } else {
                consecutive = 0;
            }
        }

        None
    }

    fn free_frames(&self, addr: PhysAddr, count: usize) {
        let mut inner = self.inner.lock();
        let start_frame = addr.page_align_down().as_usize() / FRAME_SIZE;

        for frame in start_frame..start_frame + count {
            if frame < MAX_FRAMES {
                let word = frame / 64;
                let bit = frame % 64;

                if inner.bitmap[word] & (1 << bit) != 0 {
                    inner.bitmap[word] &= !(1 << bit);
                    inner.free_frames += 1;
                }
            }
        }

        if start_frame < inner.next_free {
            inner.next_free = start_frame;
        }
    }
}
