//! Stack-based formatting buffer — replaces format!() / String / alloc::fmt.
//! Zero heap allocation. Write into a fixed [u8; N] on the stack, read it back as &str.
//!
//! — SableWire: printf without malloc — the way Kernighan intended

use core::fmt;

/// Fixed-size format buffer that implements core::fmt::Write.
/// Use this instead of format!() to avoid heap allocation.
///
/// ```
/// let mut buf = FmtBuf::<128>::new();
/// write!(buf, "Firmware: {} rev {}", vendor, rev).ok();
/// print_line(buf.as_str());
/// ```
///
/// — SableWire: 128 bytes of stack-allocated formatting nirvana
pub struct FmtBuf<const N: usize> {
    buf: [u8; N],
    pos: usize,
}

impl<const N: usize> FmtBuf<N> {
    /// Create a new empty format buffer
    pub const fn new() -> Self {
        Self {
            buf: [0u8; N],
            pos: 0,
        }
    }

    /// Get the formatted content as a string slice
    /// — SableWire: the payoff — all that writing, distilled into a &str
    pub fn as_str(&self) -> &str {
        // Safety: we only write valid UTF-8 through fmt::Write
        unsafe { core::str::from_utf8_unchecked(&self.buf[..self.pos]) }
    }

    /// Get the formatted content as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.pos]
    }

    /// Get the current length
    pub fn len(&self) -> usize {
        self.pos
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.pos == 0
    }

    /// Reset the buffer for reuse
    pub fn clear(&mut self) {
        self.pos = 0;
    }
}

impl<const N: usize> fmt::Write for FmtBuf<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let available = N - self.pos;
        let to_copy = bytes.len().min(available);
        self.buf[self.pos..self.pos + to_copy].copy_from_slice(&bytes[..to_copy]);
        self.pos += to_copy;
        Ok(())
    }
}
