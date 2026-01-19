//! Extended Page Tables (EPT)

#![allow(unsafe_op_in_unsafe_fn)]

use alloc::vec::Vec;
use vmm::{VmmResult, VmmError};

/// EPT memory type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EptMemoryType {
    Uncacheable = 0,
    WriteCombining = 1,
    WriteThrough = 4,
    WriteProtected = 5,
    WriteBack = 6,
}

/// EPT entry flags
pub mod flags {
    pub const READ: u64 = 1 << 0;
    pub const WRITE: u64 = 1 << 1;
    pub const EXECUTE: u64 = 1 << 2;
    pub const MEMORY_TYPE_SHIFT: u64 = 3;
    pub const LARGE_PAGE: u64 = 1 << 7;
    pub const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
}

/// EPT entry
#[derive(Debug, Clone, Copy)]
pub struct EptEntry(pub u64);

impl EptEntry {
    /// Create empty entry
    pub fn empty() -> Self {
        EptEntry(0)
    }

    /// Create entry pointing to page table
    pub fn table(phys_addr: u64) -> Self {
        EptEntry((phys_addr & flags::ADDR_MASK) | flags::READ | flags::WRITE | flags::EXECUTE)
    }

    /// Create entry for 4KB page
    pub fn page_4k(phys_addr: u64, read: bool, write: bool, execute: bool, mem_type: EptMemoryType) -> Self {
        let mut entry = phys_addr & flags::ADDR_MASK;
        if read { entry |= flags::READ; }
        if write { entry |= flags::WRITE; }
        if execute { entry |= flags::EXECUTE; }
        entry |= (mem_type as u64) << flags::MEMORY_TYPE_SHIFT;
        EptEntry(entry)
    }

    /// Check if present
    pub fn is_present(&self) -> bool {
        (self.0 & (flags::READ | flags::WRITE | flags::EXECUTE)) != 0
    }

    /// Get physical address
    pub fn phys_addr(&self) -> u64 {
        self.0 & flags::ADDR_MASK
    }
}

/// EPT violation information
#[derive(Debug, Clone)]
pub struct EptViolation {
    /// Guest physical address
    pub gpa: u64,
    /// Was read access
    pub read: bool,
    /// Was write access
    pub write: bool,
    /// Was instruction fetch
    pub execute: bool,
}

/// Single EPT table (512 entries)
struct EptTable {
    /// Physical address
    phys_addr: u64,
    /// Virtual address
    entries: *mut EptEntry,
}

// EPT tables are managed by hypervisor
unsafe impl Send for EptTable {}
unsafe impl Sync for EptTable {}

impl EptTable {
    fn new() -> VmmResult<Self> {
        let layout = alloc::alloc::Layout::from_size_align(4096, 4096)
            .map_err(|_| VmmError::ResourceExhausted)?;
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err(VmmError::ResourceExhausted);
        }

        Ok(EptTable {
            phys_addr: ptr as u64,
            entries: ptr as *mut EptEntry,
        })
    }

    fn entry(&self, index: usize) -> EptEntry {
        unsafe { *self.entries.add(index) }
    }

    fn set_entry(&mut self, index: usize, entry: EptEntry) {
        unsafe { *self.entries.add(index) = entry; }
    }
}

impl Drop for EptTable {
    fn drop(&mut self) {
        let layout = alloc::alloc::Layout::from_size_align(4096, 4096).unwrap();
        unsafe {
            alloc::alloc::dealloc(self.entries as *mut u8, layout);
        }
    }
}

/// EPT structure
pub struct Ept {
    /// PML4 table
    pml4: EptTable,
    /// Allocated page tables
    tables: Vec<EptTable>,
    /// EPT pointer value
    eptp: u64,
}

impl Ept {
    /// Create new EPT
    pub fn new() -> VmmResult<Self> {
        let pml4 = EptTable::new()?;
        let phys_addr = pml4.phys_addr;

        // EPT pointer format
        let eptp = (phys_addr & !0xFFF) | (3 << 3) | 6;

        Ok(Ept {
            pml4,
            tables: Vec::new(),
            eptp,
        })
    }

    /// Get EPT pointer
    pub fn pointer(&self) -> u64 {
        self.eptp
    }

    /// Map guest physical address to host physical address
    pub fn map(&mut self, gpa: u64, hpa: u64, size: u64, read: bool, write: bool, execute: bool) -> VmmResult<()> {
        let mem_type = EptMemoryType::WriteBack;

        let mut offset = 0u64;
        while offset < size {
            let current_gpa = gpa + offset;
            let current_hpa = hpa + offset;
            self.map_4k(current_gpa, current_hpa, read, write, execute, mem_type)?;
            offset += 4096;
        }

        Ok(())
    }

    /// Map a single 4KB page
    fn map_4k(&mut self, gpa: u64, hpa: u64, read: bool, write: bool, execute: bool, mem_type: EptMemoryType) -> VmmResult<()> {
        let pml4_idx = ((gpa >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((gpa >> 30) & 0x1FF) as usize;
        let pd_idx = ((gpa >> 21) & 0x1FF) as usize;
        let pt_idx = ((gpa >> 12) & 0x1FF) as usize;

        // Ensure PDPT exists
        if !self.pml4.entry(pml4_idx).is_present() {
            let table = EptTable::new()?;
            self.pml4.set_entry(pml4_idx, EptEntry::table(table.phys_addr));
            self.tables.push(table);
        }

        let pml4_entry = self.pml4.entry(pml4_idx);
        let pdpt_idx_in_tables = self.find_table_index(pml4_entry.phys_addr())?;

        // Ensure PD exists
        if !self.tables[pdpt_idx_in_tables].entry(pdpt_idx).is_present() {
            let table = EptTable::new()?;
            let phys = table.phys_addr;
            self.tables.push(table);
            self.tables[pdpt_idx_in_tables].set_entry(pdpt_idx, EptEntry::table(phys));
        }

        let pdpt_entry = self.tables[pdpt_idx_in_tables].entry(pdpt_idx);
        let pd_idx_in_tables = self.find_table_index(pdpt_entry.phys_addr())?;

        // Ensure PT exists
        if !self.tables[pd_idx_in_tables].entry(pd_idx).is_present() {
            let table = EptTable::new()?;
            let phys = table.phys_addr;
            self.tables.push(table);
            self.tables[pd_idx_in_tables].set_entry(pd_idx, EptEntry::table(phys));
        }

        let pd_entry = self.tables[pd_idx_in_tables].entry(pd_idx);
        let pt_idx_in_tables = self.find_table_index(pd_entry.phys_addr())?;

        // Set PT entry
        self.tables[pt_idx_in_tables].set_entry(pt_idx, EptEntry::page_4k(hpa, read, write, execute, mem_type));

        Ok(())
    }

    fn find_table_index(&self, phys_addr: u64) -> VmmResult<usize> {
        self.tables.iter()
            .position(|t| t.phys_addr == phys_addr)
            .ok_or(VmmError::InvalidMemory)
    }

    /// Invalidate all EPT mappings
    pub fn invalidate(&self) {
        unsafe {
            let desc: [u64; 2] = [self.eptp, 0];
            core::arch::asm!(
                "invept {}, [{}]",
                in(reg) 2u64,
                in(reg) desc.as_ptr(),
                options(nostack)
            );
        }
    }
}
