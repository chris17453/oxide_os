//! Audio Ring Buffer

use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Lock-free ring buffer for audio samples
pub struct RingBuffer {
    /// Buffer data
    data: Vec<u8>,
    /// Buffer capacity
    capacity: usize,
    /// Read position
    read_pos: AtomicUsize,
    /// Write position
    write_pos: AtomicUsize,
}

impl RingBuffer {
    /// Create a new ring buffer with given capacity
    pub fn new(capacity: usize) -> Self {
        let mut data = Vec::with_capacity(capacity);
        data.resize(capacity, 0);

        RingBuffer {
            data,
            capacity,
            read_pos: AtomicUsize::new(0),
            write_pos: AtomicUsize::new(0),
        }
    }

    /// Get buffer capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get available space for writing
    pub fn write_available(&self) -> usize {
        let read = self.read_pos.load(Ordering::Acquire);
        let write = self.write_pos.load(Ordering::Acquire);

        if write >= read {
            self.capacity - (write - read) - 1
        } else {
            read - write - 1
        }
    }

    /// Get available data for reading
    pub fn read_available(&self) -> usize {
        let read = self.read_pos.load(Ordering::Acquire);
        let write = self.write_pos.load(Ordering::Acquire);

        if write >= read {
            write - read
        } else {
            self.capacity - read + write
        }
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.read_available() == 0
    }

    /// Check if buffer is full
    pub fn is_full(&self) -> bool {
        self.write_available() == 0
    }

    /// Write data to buffer
    /// Returns number of bytes written
    pub fn write(&self, data: &[u8]) -> usize {
        let available = self.write_available();
        let to_write = data.len().min(available);

        if to_write == 0 {
            return 0;
        }

        let write_pos = self.write_pos.load(Ordering::Acquire);

        // Write in one or two chunks if wrapping
        let first_chunk = (self.capacity - write_pos).min(to_write);
        let second_chunk = to_write - first_chunk;

        // Safety: We're writing to different positions than reader
        unsafe {
            let buf_ptr = self.data.as_ptr() as *mut u8;

            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                buf_ptr.add(write_pos),
                first_chunk,
            );

            if second_chunk > 0 {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr().add(first_chunk),
                    buf_ptr,
                    second_chunk,
                );
            }
        }

        let new_write_pos = (write_pos + to_write) % self.capacity;
        self.write_pos.store(new_write_pos, Ordering::Release);

        to_write
    }

    /// Read data from buffer
    /// Returns number of bytes read
    pub fn read(&self, data: &mut [u8]) -> usize {
        let available = self.read_available();
        let to_read = data.len().min(available);

        if to_read == 0 {
            return 0;
        }

        let read_pos = self.read_pos.load(Ordering::Acquire);

        // Read in one or two chunks if wrapping
        let first_chunk = (self.capacity - read_pos).min(to_read);
        let second_chunk = to_read - first_chunk;

        unsafe {
            core::ptr::copy_nonoverlapping(
                self.data.as_ptr().add(read_pos),
                data.as_mut_ptr(),
                first_chunk,
            );

            if second_chunk > 0 {
                core::ptr::copy_nonoverlapping(
                    self.data.as_ptr(),
                    data.as_mut_ptr().add(first_chunk),
                    second_chunk,
                );
            }
        }

        let new_read_pos = (read_pos + to_read) % self.capacity;
        self.read_pos.store(new_read_pos, Ordering::Release);

        to_read
    }

    /// Peek at data without consuming
    pub fn peek(&self, data: &mut [u8]) -> usize {
        let available = self.read_available();
        let to_read = data.len().min(available);

        if to_read == 0 {
            return 0;
        }

        let read_pos = self.read_pos.load(Ordering::Acquire);

        let first_chunk = (self.capacity - read_pos).min(to_read);
        let second_chunk = to_read - first_chunk;

        unsafe {
            core::ptr::copy_nonoverlapping(
                self.data.as_ptr().add(read_pos),
                data.as_mut_ptr(),
                first_chunk,
            );

            if second_chunk > 0 {
                core::ptr::copy_nonoverlapping(
                    self.data.as_ptr(),
                    data.as_mut_ptr().add(first_chunk),
                    second_chunk,
                );
            }
        }

        to_read
    }

    /// Skip bytes (advance read position)
    pub fn skip(&self, count: usize) -> usize {
        let available = self.read_available();
        let to_skip = count.min(available);

        if to_skip == 0 {
            return 0;
        }

        let read_pos = self.read_pos.load(Ordering::Acquire);
        let new_read_pos = (read_pos + to_skip) % self.capacity;
        self.read_pos.store(new_read_pos, Ordering::Release);

        to_skip
    }

    /// Clear the buffer
    pub fn clear(&self) {
        self.read_pos.store(0, Ordering::Release);
        self.write_pos.store(0, Ordering::Release);
    }

    /// Get fill level as percentage (0-100)
    pub fn fill_level(&self) -> u8 {
        let available = self.read_available();
        ((available * 100) / self.capacity) as u8
    }
}

unsafe impl Send for RingBuffer {}
unsafe impl Sync for RingBuffer {}

/// Double buffer for audio (for smooth playback)
pub struct DoubleBuffer {
    /// Front buffer (currently being read)
    front: Vec<u8>,
    /// Back buffer (currently being written)
    back: Vec<u8>,
    /// Front buffer read position
    front_pos: AtomicUsize,
    /// Back buffer write position
    back_pos: AtomicUsize,
    /// Buffer size
    size: usize,
}

impl DoubleBuffer {
    /// Create a new double buffer
    pub fn new(size: usize) -> Self {
        let mut front = Vec::with_capacity(size);
        front.resize(size, 0);
        let mut back = Vec::with_capacity(size);
        back.resize(size, 0);

        DoubleBuffer {
            front,
            back,
            front_pos: AtomicUsize::new(0),
            back_pos: AtomicUsize::new(0),
            size,
        }
    }

    /// Write to back buffer
    pub fn write(&mut self, data: &[u8]) -> usize {
        let pos = self.back_pos.load(Ordering::Acquire);
        let available = self.size - pos;
        let to_write = data.len().min(available);

        if to_write > 0 {
            self.back[pos..pos + to_write].copy_from_slice(&data[..to_write]);
            self.back_pos.store(pos + to_write, Ordering::Release);
        }

        to_write
    }

    /// Read from front buffer
    pub fn read(&self, data: &mut [u8]) -> usize {
        let pos = self.front_pos.load(Ordering::Acquire);
        let available = self.size - pos;
        let to_read = data.len().min(available);

        if to_read > 0 {
            data[..to_read].copy_from_slice(&self.front[pos..pos + to_read]);
            self.front_pos.store(pos + to_read, Ordering::Release);
        }

        to_read
    }

    /// Swap buffers (call when front is exhausted and back is full)
    pub fn swap(&mut self) {
        core::mem::swap(&mut self.front, &mut self.back);
        self.front_pos.store(0, Ordering::Release);
        self.back_pos.store(0, Ordering::Release);
    }

    /// Check if back buffer is full
    pub fn back_full(&self) -> bool {
        self.back_pos.load(Ordering::Acquire) >= self.size
    }

    /// Check if front buffer is empty
    pub fn front_empty(&self) -> bool {
        self.front_pos.load(Ordering::Acquire) >= self.size
    }
}
