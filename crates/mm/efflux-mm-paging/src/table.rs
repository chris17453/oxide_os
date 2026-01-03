//! Page table structure

use crate::entry::PageTableEntry;
use crate::ENTRIES_PER_TABLE;

/// A page table (512 entries, 4KB total)
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; ENTRIES_PER_TABLE],
}

impl PageTable {
    /// Create an empty page table (all entries not present)
    pub const fn empty() -> Self {
        Self {
            entries: [PageTableEntry::empty(); ENTRIES_PER_TABLE],
        }
    }

    /// Get an entry by index
    #[inline]
    pub fn entry(&self, index: usize) -> &PageTableEntry {
        &self.entries[index]
    }

    /// Get a mutable entry by index
    #[inline]
    pub fn entry_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }

    /// Iterate over all entries
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &PageTableEntry> {
        self.entries.iter()
    }

    /// Iterate mutably over all entries
    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PageTableEntry> {
        self.entries.iter_mut()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        for entry in &mut self.entries {
            entry.clear();
        }
    }

    /// Check if the table is empty (no present entries)
    pub fn is_empty(&self) -> bool {
        self.entries.iter().all(|e| !e.is_present())
    }
}

impl core::ops::Index<usize> for PageTable {
    type Output = PageTableEntry;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl core::ops::IndexMut<usize> for PageTable {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}
