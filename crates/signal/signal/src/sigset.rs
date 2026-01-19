//! Signal set (mask) implementation
//!
//! A SigSet is a bitmask of signals, used for blocking/pending sets.

use crate::signal::{NSIG, SIGKILL, SIGSTOP};

/// A set of signals (bitmask)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct SigSet {
    /// Bitmask of signals (bit N = signal N+1)
    bits: u64,
}

impl SigSet {
    /// Create an empty signal set
    pub const fn empty() -> Self {
        SigSet { bits: 0 }
    }

    /// Create a full signal set (all signals)
    pub const fn full() -> Self {
        SigSet { bits: !0 }
    }

    /// Create from raw bits
    pub const fn from_bits(bits: u64) -> Self {
        SigSet { bits }
    }

    /// Get raw bits
    pub const fn bits(&self) -> u64 {
        self.bits
    }

    /// Add a signal to the set
    pub fn add(&mut self, sig: i32) {
        if sig >= 1 && sig <= NSIG as i32 {
            self.bits |= 1 << (sig - 1);
        }
    }

    /// Remove a signal from the set
    pub fn remove(&mut self, sig: i32) {
        if sig >= 1 && sig <= NSIG as i32 {
            self.bits &= !(1 << (sig - 1));
        }
    }

    /// Check if a signal is in the set
    pub fn contains(&self, sig: i32) -> bool {
        if sig >= 1 && sig <= NSIG as i32 {
            (self.bits & (1 << (sig - 1))) != 0
        } else {
            false
        }
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Union with another set
    pub fn union(&self, other: &SigSet) -> SigSet {
        SigSet {
            bits: self.bits | other.bits,
        }
    }

    /// Intersection with another set
    pub fn intersection(&self, other: &SigSet) -> SigSet {
        SigSet {
            bits: self.bits & other.bits,
        }
    }

    /// Difference (self - other)
    pub fn difference(&self, other: &SigSet) -> SigSet {
        SigSet {
            bits: self.bits & !other.bits,
        }
    }

    /// Clear all signals
    pub fn clear(&mut self) {
        self.bits = 0;
    }

    /// Fill all signals
    pub fn fill(&mut self) {
        self.bits = !0;
    }

    /// Remove unblockable signals (SIGKILL, SIGSTOP)
    pub fn remove_unblockable(&mut self) {
        self.remove(SIGKILL);
        self.remove(SIGSTOP);
    }

    /// Get the first signal in the set (lowest numbered)
    pub fn first(&self) -> Option<i32> {
        if self.bits == 0 {
            None
        } else {
            Some(self.bits.trailing_zeros() as i32 + 1)
        }
    }

    /// Iterate over signals in the set
    pub fn iter(&self) -> SigSetIter {
        SigSetIter {
            bits: self.bits,
            current: 0,
        }
    }
}

/// Iterator over signals in a SigSet
pub struct SigSetIter {
    bits: u64,
    current: i32,
}

impl Iterator for SigSetIter {
    type Item = i32;

    fn next(&mut self) -> Option<i32> {
        while self.current < NSIG as i32 {
            self.current += 1;
            if (self.bits & (1 << (self.current - 1))) != 0 {
                return Some(self.current);
            }
        }
        None
    }
}

/// How to modify a signal mask (for sigprocmask)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SigHow {
    /// Block signals in set (add to mask)
    Block = 0,
    /// Unblock signals in set (remove from mask)
    Unblock = 1,
    /// Set mask to exactly the given set
    SetMask = 2,
}

impl SigHow {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(SigHow::Block),
            1 => Some(SigHow::Unblock),
            2 => Some(SigHow::SetMask),
            _ => None,
        }
    }
}
