//! Lock-Free Ring Buffer for Keyboard Input
//!
//! ## WHY THIS EXISTS (The Horror Story)
//!
//! Previous implementation used `try_lock()` to push keyboard input from IRQ context.
//! When the lock was held (process reading TTY), keystrokes were **silently dropped**.
//!
//! User: *types password*
//! Kernel: "LOL nope, lock's busy, discarding your 'p' and 's'"
//! User: *gets locked out*
//! Kernel: *chuckles in race condition*
//!
//! ## THE FIX (Chrome Upgrade)
//!
//! This is a Single-Producer Single-Consumer (SPSC) lock-free ring buffer.
//! - **Producer**: IRQ handler (only one, atomic writes)
//! - **Consumer**: VT read syscall (only one per VT)
//! - **No locks**: AtomicUsize for head/tail, never blocks
//! - **No drops**: If full, we know immediately (no silent failures)
//!
//! Welcome to 2077. Your keyboard works now.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Lock-free ring buffer for keyboard input
///
/// Capacity is 256 bytes (one less than BUFFER_SIZE for full/empty distinction).
/// IRQ handler pushes bytes atomically. VT read pops them. Zero locks. Zero tears.
pub struct LockFreeRing {
    /// The actual data buffer (257 bytes, we use 256)
    buffer: [u8; 257],

    /// Write index (modified only by producer/IRQ)
    /// AtomicUsize so reads are atomic even on 32-bit (we're 64-bit but still)
    head: AtomicUsize,

    /// Read index (modified only by consumer/VT)
    tail: AtomicUsize,
}

impl LockFreeRing {
    /// Create a new empty ring buffer
    pub const fn new() -> Self {
        LockFreeRing {
            buffer: [0u8; 257],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Push a byte from IRQ context (lock-free, ISR-safe)
    ///
    /// Returns `true` if pushed, `false` if buffer full.
    ///
    /// **ISR-SAFE**: No locks, no allocations, just atomic CAS magic.
    #[inline]
    pub fn push(&self, byte: u8) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        let next_head = (head + 1) % 257;

        // Buffer full? Sorry choom, you're typing too fast
        if next_head == tail {
            return false;
        }

        // SAFETY: Only we (IRQ handler) write to buffer[head]
        // No other CPU is touching this index right now
        unsafe {
            let ptr = self.buffer.as_ptr() as *mut u8;
            ptr.add(head).write(byte);
        }

        // Publish the write (Release ensures buffer write happens before head update)
        self.head.store(next_head, Ordering::Release);
        true
    }

    /// Pop a byte from process context (lock-free)
    ///
    /// Returns `Some(byte)` if data available, `None` if empty.
    #[inline]
    pub fn pop(&self) -> Option<u8> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        // Empty? Go wait for the user to actually type something
        if head == tail {
            return None;
        }

        // SAFETY: Only we (VT read) read from buffer[tail]
        let byte = unsafe {
            let ptr = self.buffer.as_ptr();
            ptr.add(tail).read()
        };

        let next_tail = (tail + 1) % 257;
        self.tail.store(next_tail, Ordering::Release);

        Some(byte)
    }

    /// Check if buffer is empty (non-blocking, ISR-safe)
    #[inline]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    /// Get current buffer occupancy (for debugging/stats)
    #[inline]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        if head >= tail {
            head - tail
        } else {
            257 - tail + head
        }
    }

    /// Clear the buffer (consumer only, don't call from IRQ!)
    #[allow(dead_code)]
    pub fn clear(&self) {
        let head = self.head.load(Ordering::Acquire);
        self.tail.store(head, Ordering::Release);
    }
}

// SAFETY: LockFreeRing uses atomic operations for thread-safe access
unsafe impl Send for LockFreeRing {}
unsafe impl Sync for LockFreeRing {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let ring = LockFreeRing::new();
        assert!(ring.push(b'H'));
        assert!(ring.push(b'i'));
        assert_eq!(ring.pop(), Some(b'H'));
        assert_eq!(ring.pop(), Some(b'i'));
        assert_eq!(ring.pop(), None);
    }

    #[test]
    fn test_full() {
        let ring = LockFreeRing::new();
        for i in 0..256 {
            assert!(ring.push(i as u8));
        }
        // 257th push should fail (buffer full)
        assert!(!ring.push(0xFF));
    }

    #[test]
    fn test_wrap() {
        let ring = LockFreeRing::new();
        for _ in 0..10 {
            for i in 0..100 {
                assert!(ring.push(i));
            }
            for i in 0..100 {
                assert_eq!(ring.pop(), Some(i));
            }
        }
    }
}
