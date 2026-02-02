//! File-backed memory mappings

use alloc::vec::Vec;
use core::ptr;

/// Page state for file mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageState {
    /// Not yet faulted in
    NotPresent,
    /// Clean (matches file contents)
    Clean(usize), // physical address
    /// Dirty (modified, needs writeback)
    Dirty(usize), // physical address
}

/// File-backed mapping handler
pub struct FileMapping {
    /// Start address
    start: usize,
    /// Size
    size: usize,
    /// File descriptor
    fd: i32,
    /// File offset
    offset: u64,
    /// Is this a shared mapping
    shared: bool,
    /// Page states
    pages: Vec<PageState>,
}

impl FileMapping {
    /// Create new file mapping
    pub fn new(start: usize, size: usize, fd: i32, offset: u64, shared: bool) -> Self {
        let num_pages = (size + 0xFFF) / 0x1000;
        let mut pages = Vec::with_capacity(num_pages);
        for _ in 0..num_pages {
            pages.push(PageState::NotPresent);
        }

        FileMapping {
            start,
            size,
            fd,
            offset,
            shared,
            pages,
        }
    }

    /// Handle page fault for this mapping
    ///
    /// The read_file callback reads data from the file into the buffer
    pub fn handle_fault(
        &mut self,
        addr: usize,
        allocate_page: impl FnOnce() -> Option<usize>,
        read_file: impl FnOnce(i32, u64, &mut [u8]) -> Result<usize, ()>,
    ) -> Option<usize> {
        if addr < self.start || addr >= self.start + self.size {
            return None;
        }

        let page_idx = (addr - self.start) / 0x1000;

        match self.pages[page_idx] {
            PageState::Clean(phys) | PageState::Dirty(phys) => {
                // Already mapped
                return Some(phys);
            }
            PageState::NotPresent => {}
        }

        // Allocate new page
        let phys = allocate_page()?;

        // Read from file
        let file_offset = self.offset + (page_idx as u64 * 0x1000);
        let buf = unsafe { core::slice::from_raw_parts_mut(phys as *mut u8, 0x1000) };

        // Zero buffer first (in case file is shorter than expected)
        buf.fill(0);

        // Read file contents
        let _ = read_file(self.fd, file_offset, buf);

        self.pages[page_idx] = PageState::Clean(phys);
        Some(phys)
    }

    /// Handle write fault (for private mappings that need COW)
    pub fn handle_write_fault(
        &mut self,
        addr: usize,
        allocate_page: impl FnOnce() -> Option<usize>,
    ) -> Option<usize> {
        if addr < self.start || addr >= self.start + self.size {
            return None;
        }

        let page_idx = (addr - self.start) / 0x1000;

        match self.pages[page_idx] {
            PageState::NotPresent => {
                // This shouldn't happen - should fault in first
                return None;
            }
            PageState::Dirty(phys) => {
                // Already writable
                return Some(phys);
            }
            PageState::Clean(old_phys) => {
                if self.shared {
                    // Shared mapping - just mark as dirty
                    self.pages[page_idx] = PageState::Dirty(old_phys);
                    return Some(old_phys);
                }

                // Private mapping - need to copy
                let new_phys = allocate_page()?;

                unsafe {
                    ptr::copy_nonoverlapping(old_phys as *const u8, new_phys as *mut u8, 0x1000);
                }

                self.pages[page_idx] = PageState::Dirty(new_phys);
                Some(new_phys)
            }
        }
    }

    /// Sync dirty pages back to file
    pub fn msync(
        &mut self,
        write_file: impl Fn(i32, u64, &[u8]) -> Result<usize, ()>,
    ) -> Result<(), ()> {
        for (page_idx, state) in self.pages.iter_mut().enumerate() {
            if let PageState::Dirty(phys) = *state {
                if self.shared {
                    // Write back to file
                    let file_offset = self.offset + (page_idx as u64 * 0x1000);
                    let buf = unsafe { core::slice::from_raw_parts(phys as *const u8, 0x1000) };
                    write_file(self.fd, file_offset, buf)?;
                    *state = PageState::Clean(phys);
                }
            }
        }
        Ok(())
    }

    /// Fork mapping
    pub fn fork(&self) -> Self {
        FileMapping {
            start: self.start,
            size: self.size,
            fd: self.fd,
            offset: self.offset,
            shared: self.shared,
            pages: self.pages.clone(),
        }
    }

    /// Free all allocated pages
    pub fn free(&mut self, free_page: impl Fn(usize)) {
        for state in self.pages.iter_mut() {
            match *state {
                PageState::Clean(phys) | PageState::Dirty(phys) => {
                    free_page(phys);
                }
                PageState::NotPresent => {}
            }
            *state = PageState::NotPresent;
        }
    }

    /// Get file descriptor
    pub fn fd(&self) -> i32 {
        self.fd
    }

    /// Get file offset
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Is this a shared mapping
    pub fn is_shared(&self) -> bool {
        self.shared
    }

    /// Get number of dirty pages
    pub fn dirty_pages(&self) -> usize {
        self.pages
            .iter()
            .filter(|p| matches!(p, PageState::Dirty(_)))
            .count()
    }

    /// Get number of allocated pages
    pub fn allocated_pages(&self) -> usize {
        self.pages
            .iter()
            .filter(|p| !matches!(p, PageState::NotPresent))
            .count()
    }
}
