//! Anonymous memory mappings

use alloc::vec::Vec;
use core::ptr;

/// Anonymous mapping handler
pub struct AnonymousMapping {
    /// Start address
    start: usize,
    /// Size
    size: usize,
    /// Pages that have been faulted in (page number -> physical frame)
    pages: Vec<Option<usize>>,
}

impl AnonymousMapping {
    /// Create new anonymous mapping
    pub fn new(start: usize, size: usize) -> Self {
        let num_pages = (size + 0xFFF) / 0x1000;
        let mut pages = Vec::with_capacity(num_pages);
        for _ in 0..num_pages {
            pages.push(None);
        }

        AnonymousMapping { start, size, pages }
    }

    /// Handle page fault for this mapping
    ///
    /// Returns physical address of the page to map
    pub fn handle_fault(
        &mut self,
        addr: usize,
        allocate_page: impl FnOnce() -> Option<usize>,
    ) -> Option<usize> {
        if addr < self.start || addr >= self.start + self.size {
            return None;
        }

        let page_idx = (addr - self.start) / 0x1000;

        if let Some(phys) = self.pages[page_idx] {
            // Already mapped
            return Some(phys);
        }

        // Allocate new page
        let phys = allocate_page()?;

        // Zero the page
        unsafe {
            ptr::write_bytes(phys as *mut u8, 0, 0x1000);
        }

        self.pages[page_idx] = Some(phys);
        Some(phys)
    }

    /// Handle copy-on-write fault
    pub fn handle_cow(
        &mut self,
        addr: usize,
        old_phys: usize,
        allocate_page: impl FnOnce() -> Option<usize>,
    ) -> Option<usize> {
        if addr < self.start || addr >= self.start + self.size {
            return None;
        }

        let page_idx = (addr - self.start) / 0x1000;

        // Allocate new page
        let new_phys = allocate_page()?;

        // Copy contents
        unsafe {
            ptr::copy_nonoverlapping(old_phys as *const u8, new_phys as *mut u8, 0x1000);
        }

        self.pages[page_idx] = Some(new_phys);
        Some(new_phys)
    }

    /// Fork mapping (mark all pages as copy-on-write)
    pub fn fork(&self) -> Self {
        // For fork, we share the same physical pages initially
        // and mark them as copy-on-write
        AnonymousMapping {
            start: self.start,
            size: self.size,
            pages: self.pages.clone(),
        }
    }

    /// Free all allocated pages
    pub fn free(&mut self, free_page: impl Fn(usize)) {
        for page in self.pages.iter_mut() {
            if let Some(phys) = page.take() {
                free_page(phys);
            }
        }
    }

    /// Get number of allocated pages
    pub fn allocated_pages(&self) -> usize {
        self.pages.iter().filter(|p| p.is_some()).count()
    }

    /// Get total number of pages
    pub fn total_pages(&self) -> usize {
        self.pages.len()
    }
}
