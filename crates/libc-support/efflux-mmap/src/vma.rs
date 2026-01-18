//! Virtual Memory Area

use crate::prot;
use crate::flags;

/// VMA flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmaFlags {
    /// Readable
    pub read: bool,
    /// Writable
    pub write: bool,
    /// Executable
    pub exec: bool,
    /// Shared (vs private/copy-on-write)
    pub shared: bool,
    /// Grows downward (stack)
    pub growsdown: bool,
}

impl VmaFlags {
    /// Create flags from protection and map flags
    pub fn from_prot_and_flags(protection: i32, map_flags: i32) -> Self {
        VmaFlags {
            read: protection & prot::PROT_READ != 0,
            write: protection & prot::PROT_WRITE != 0,
            exec: protection & prot::PROT_EXEC != 0,
            shared: map_flags & flags::MAP_SHARED != 0,
            growsdown: map_flags & flags::MAP_GROWSDOWN != 0,
        }
    }

    /// Create flags from protection only
    pub fn from_prot(protection: i32) -> Self {
        VmaFlags {
            read: protection & prot::PROT_READ != 0,
            write: protection & prot::PROT_WRITE != 0,
            exec: protection & prot::PROT_EXEC != 0,
            shared: false,
            growsdown: false,
        }
    }

    /// Convert to protection flags
    pub fn to_prot(&self) -> i32 {
        let mut p = 0;
        if self.read { p |= prot::PROT_READ; }
        if self.write { p |= prot::PROT_WRITE; }
        if self.exec { p |= prot::PROT_EXEC; }
        p
    }
}

impl Default for VmaFlags {
    fn default() -> Self {
        Self {
            read: false,
            write: false,
            exec: false,
            shared: false,
            growsdown: false,
        }
    }
}

/// VMA backing type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmaType {
    /// Anonymous memory (zero-filled on demand)
    Anonymous,
    /// File-backed mapping
    FileBacked {
        /// File descriptor
        fd: i32,
        /// Offset in file
        offset: u64,
    },
    /// Stack mapping (special case of anonymous)
    Stack,
    /// Heap mapping (special case of anonymous)
    Heap,
}

/// Virtual Memory Area
#[derive(Debug, Clone)]
pub struct VirtualMemoryArea {
    /// Start address (inclusive)
    pub start: usize,
    /// End address (exclusive)
    pub end: usize,
    /// Flags
    pub flags: VmaFlags,
    /// Backing type
    pub vma_type: VmaType,
}

impl VirtualMemoryArea {
    /// Create new VMA
    pub fn new(start: usize, end: usize, flags: VmaFlags, vma_type: VmaType) -> Self {
        VirtualMemoryArea {
            start,
            end,
            flags,
            vma_type,
        }
    }

    /// Get the size of this VMA
    pub fn size(&self) -> usize {
        self.end - self.start
    }

    /// Check if address is contained in this VMA
    pub fn contains(&self, addr: usize) -> bool {
        addr >= self.start && addr < self.end
    }

    /// Check if this VMA overlaps with a range
    pub fn overlaps(&self, start: usize, end: usize) -> bool {
        self.start < end && self.end > start
    }

    /// Check if readable
    pub fn is_readable(&self) -> bool {
        self.flags.read
    }

    /// Check if writable
    pub fn is_writable(&self) -> bool {
        self.flags.write
    }

    /// Check if executable
    pub fn is_executable(&self) -> bool {
        self.flags.exec
    }

    /// Check if shared
    pub fn is_shared(&self) -> bool {
        self.flags.shared
    }

    /// Check if anonymous
    pub fn is_anonymous(&self) -> bool {
        matches!(self.vma_type, VmaType::Anonymous | VmaType::Stack | VmaType::Heap)
    }

    /// Check if file-backed
    pub fn is_file_backed(&self) -> bool {
        matches!(self.vma_type, VmaType::FileBacked { .. })
    }

    /// Split VMA at address, returning the upper part
    pub fn split(&mut self, addr: usize) -> Option<Self> {
        if addr <= self.start || addr >= self.end {
            return None;
        }

        let mut upper = self.clone();
        upper.start = addr;
        self.end = addr;

        Some(upper)
    }

    /// Merge with adjacent VMA if compatible
    pub fn can_merge(&self, other: &Self) -> bool {
        // Must be adjacent
        if self.end != other.start {
            return false;
        }

        // Must have same flags
        if self.flags != other.flags {
            return false;
        }

        // Must have compatible types
        match (&self.vma_type, &other.vma_type) {
            (VmaType::Anonymous, VmaType::Anonymous) => true,
            (VmaType::Stack, VmaType::Stack) => true,
            (VmaType::Heap, VmaType::Heap) => true,
            (VmaType::FileBacked { fd: fd1, offset: off1 }, VmaType::FileBacked { fd: fd2, offset: off2 }) => {
                fd1 == fd2 && *off1 + self.size() as u64 == *off2
            }
            _ => false,
        }
    }

    /// Merge with adjacent VMA
    pub fn merge(&mut self, other: Self) {
        self.end = other.end;
    }
}
