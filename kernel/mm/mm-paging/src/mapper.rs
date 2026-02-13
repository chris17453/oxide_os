//! Page mapper - map/unmap virtual addresses to physical frames

use crate::entry::PageTableFlags;
use crate::table::PageTable;
use crate::{PageLevel, phys_to_virt};
use mm_traits::FrameAllocator;
use os_core::{PhysAddr, VirtAddr};

/// Errors that can occur during page mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    /// The page is already mapped
    AlreadyMapped,
    /// No frame available for allocation
    FrameAllocationFailed,
    /// Parent table entry is a huge page
    ParentIsHugePage,
}

/// Page mapper for managing virtual-to-physical mappings
///
/// Requires the physical memory direct map to be set up at PHYS_MAP_BASE.
pub struct PageMapper {
    /// Physical address of the PML4 table
    pml4_phys: PhysAddr,
}

impl PageMapper {
    /// Create a new page mapper with the given PML4 physical address
    ///
    /// # Safety
    /// The PML4 table must be a valid, properly aligned page table.
    /// The physical memory direct map must be set up.
    pub const unsafe fn new(pml4_phys: PhysAddr) -> Self {
        Self { pml4_phys }
    }

    /// Get the physical address of the PML4 table
    pub const fn pml4_phys(&self) -> PhysAddr {
        self.pml4_phys
    }

    /// Get a reference to the PML4 table
    fn pml4(&self) -> &PageTable {
        let virt = phys_to_virt(self.pml4_phys);
        unsafe { &*virt.as_ptr::<PageTable>() }
    }

    /// Get a mutable reference to the PML4 table
    fn pml4_mut(&mut self) -> &mut PageTable {
        let virt = phys_to_virt(self.pml4_phys);
        unsafe { &mut *virt.as_mut_ptr::<PageTable>() }
    }

    /// Map a virtual address to a physical frame
    ///
    /// Creates any necessary intermediate page tables using the provided allocator.
    pub fn map<A: FrameAllocator>(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageTableFlags,
        allocator: &A,
    ) -> Result<(), MapError> {
        let pml4_idx = PageLevel::Pml4.index(virt);
        let pdpt_idx = PageLevel::Pdpt.index(virt);
        let pd_idx = PageLevel::Pd.index(virt);
        let pt_idx = PageLevel::Pt.index(virt);

        // Walk/create PML4 -> PDPT
        let pdpt = self.get_or_create_table(self.pml4_phys, pml4_idx, allocator)?;

        // Walk/create PDPT -> PD
        let pd = self.get_or_create_table(pdpt, pdpt_idx, allocator)?;

        // Walk/create PD -> PT
        let pt = self.get_or_create_table(pd, pd_idx, allocator)?;

        // Get PT entry
        let pt_virt = phys_to_virt(pt);
        let pt_table = unsafe { &mut *pt_virt.as_mut_ptr::<PageTable>() };
        let entry = &mut pt_table[pt_idx];

        if entry.is_present() {
            return Err(MapError::AlreadyMapped);
        }

        // — ColdCipher: PTE write trace, gated behind debug-paging (fires on EVERY map call)
        #[cfg(feature = "debug-paging")]
        {
            let entry_addr = entry as *const _ as u64;
            let entry_phys = entry_addr - 0xFFFF_8000_0000_0000;
            if entry_phys >= 0x0c400000 && entry_phys <= 0x0c500000 {
                unsafe {
                    arch_x86_64::serial::write_str_unsafe("[PTE-WRITE] entry_phys=");
                    arch_x86_64::serial::write_u64_hex_unsafe(entry_phys);
                    arch_x86_64::serial::write_str_unsafe(" val=");
                    let val = phys.as_u64() | (flags | PageTableFlags::PRESENT).bits();
                    arch_x86_64::serial::write_u64_hex_unsafe(val);
                    arch_x86_64::serial::write_str_unsafe("\n");
                }
            }
        }

        entry.set(phys, flags | PageTableFlags::PRESENT);
        Ok(())
    }

    /// Update flags for an already-mapped page
    ///
    /// Adds the given flags to the existing entry (union operation).
    /// Returns true if the entry was updated, false if not mapped.
    pub fn update_flags(&mut self, virt: VirtAddr, add_flags: PageTableFlags) -> bool {
        let pml4_idx = PageLevel::Pml4.index(virt);
        let pdpt_idx = PageLevel::Pdpt.index(virt);
        let pd_idx = PageLevel::Pd.index(virt);
        let pt_idx = PageLevel::Pt.index(virt);

        // Walk PML4 -> PDPT
        let pdpt = match self.get_table(self.pml4_phys, pml4_idx) {
            Some(p) => p,
            None => return false,
        };

        // Walk PDPT -> PD (skip huge page check for simplicity)
        let pd = match self.get_table(pdpt, pdpt_idx) {
            Some(p) => p,
            None => return false,
        };

        // Walk PD -> PT
        let pt = match self.get_table(pd, pd_idx) {
            Some(p) => p,
            None => return false,
        };

        // Get PT entry
        let pt_virt = phys_to_virt(pt);
        let pt_table = unsafe { &mut *pt_virt.as_mut_ptr::<PageTable>() };
        let entry = &mut pt_table[pt_idx];

        if !entry.is_present() {
            return false;
        }

        // Add the new flags to existing flags
        entry.add_flags(add_flags);
        true
    }

    /// Unmap a virtual address
    ///
    /// Returns the physical address that was mapped, or None if not mapped.
    pub fn unmap(&mut self, virt: VirtAddr) -> Option<PhysAddr> {
        let pml4_idx = PageLevel::Pml4.index(virt);
        let pdpt_idx = PageLevel::Pdpt.index(virt);
        let pd_idx = PageLevel::Pd.index(virt);
        let pt_idx = PageLevel::Pt.index(virt);

        // Walk PML4 -> PDPT
        let pdpt = self.get_table(self.pml4_phys, pml4_idx)?;

        // Walk PDPT -> PD
        let pd = self.get_table(pdpt, pdpt_idx)?;

        // Walk PD -> PT
        let pt = self.get_table(pd, pd_idx)?;

        // Get PT entry
        let pt_virt = phys_to_virt(pt);
        let pt_table = unsafe { &mut *pt_virt.as_mut_ptr::<PageTable>() };
        let entry = &mut pt_table[pt_idx];

        if !entry.is_present() {
            return None;
        }

        let phys = entry.addr();
        entry.clear();
        Some(phys)
    }

    /// Translate a virtual address to physical
    pub fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        let pml4_idx = PageLevel::Pml4.index(virt);
        let pdpt_idx = PageLevel::Pdpt.index(virt);
        let pd_idx = PageLevel::Pd.index(virt);
        let pt_idx = PageLevel::Pt.index(virt);

        // Walk PML4 -> PDPT
        let pdpt = self.get_table(self.pml4_phys, pml4_idx)?;

        // Check for 1GB huge page
        let pdpt_virt = phys_to_virt(pdpt);
        let pdpt_table = unsafe { &*pdpt_virt.as_ptr::<PageTable>() };
        let pdpt_entry = &pdpt_table[pdpt_idx];

        if !pdpt_entry.is_present() {
            return None;
        }
        if pdpt_entry.is_huge() {
            // 1GB page
            let offset = virt.as_u64() & 0x3FFF_FFFF; // 30 bits
            return Some(PhysAddr::new(pdpt_entry.addr().as_u64() + offset));
        }

        // Walk PDPT -> PD
        let pd = pdpt_entry.addr();
        let pd_virt = phys_to_virt(pd);
        let pd_table = unsafe { &*pd_virt.as_ptr::<PageTable>() };
        let pd_entry = &pd_table[pd_idx];

        if !pd_entry.is_present() {
            return None;
        }
        if pd_entry.is_huge() {
            // 2MB page
            let offset = virt.as_u64() & 0x1F_FFFF; // 21 bits
            return Some(PhysAddr::new(pd_entry.addr().as_u64() + offset));
        }

        // Walk PD -> PT
        let pt = pd_entry.addr();
        let pt_virt = phys_to_virt(pt);
        let pt_table = unsafe { &*pt_virt.as_ptr::<PageTable>() };
        let entry = &pt_table[pt_idx];

        if !entry.is_present() {
            return None;
        }

        let offset = virt.as_u64() & 0xFFF; // 12 bits
        Some(PhysAddr::new(entry.addr().as_u64() + offset))
    }

    /// Get or create a child table
    fn get_or_create_table<A: FrameAllocator>(
        &mut self,
        parent_phys: PhysAddr,
        index: usize,
        allocator: &A,
    ) -> Result<PhysAddr, MapError> {
        let parent_virt = phys_to_virt(parent_phys);
        let parent = unsafe { &mut *parent_virt.as_mut_ptr::<PageTable>() };
        let entry = &mut parent[index];

        if entry.is_present() {
            if entry.is_huge() {
                return Err(MapError::ParentIsHugePage);
            }
            Ok(entry.addr())
        } else {
            // Allocate a new table
            let new_table = allocator
                .alloc_frame()
                .ok_or(MapError::FrameAllocationFailed)?;

            // Zero the new table
            let new_virt = phys_to_virt(new_table);
            let table = unsafe { &mut *new_virt.as_mut_ptr::<PageTable>() };

            // — BlackLatch: PT table creation trace, gated behind debug-paging
            #[cfg(feature = "debug-paging")]
            {
                if new_table.as_u64() >= 0x0c400000 && new_table.as_u64() <= 0x0c500000 {
                    unsafe {
                        arch_x86_64::serial::write_str_unsafe("[PT-CLEAR] phys=");
                        arch_x86_64::serial::write_u64_hex_unsafe(new_table.as_u64());
                        arch_x86_64::serial::write_str_unsafe("\n");
                    }
                }
            }

            table.clear();

            // — BlackLatch: Parent entry trace, gated behind debug-paging
            #[cfg(feature = "debug-paging")]
            {
                let parent_entry_addr = entry as *const _ as u64;
                let parent_entry_phys = parent_entry_addr - 0xFFFF_8000_0000_0000;
                if parent_entry_phys >= 0x0c400000 && parent_entry_phys <= 0x0c500000 {
                    unsafe {
                        arch_x86_64::serial::write_str_unsafe("[PT-ENTRY] entry_phys=");
                        arch_x86_64::serial::write_u64_hex_unsafe(parent_entry_phys);
                        arch_x86_64::serial::write_str_unsafe("\n");
                    }
                }
            }

            entry.set(
                new_table,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER,
            );

            Ok(new_table)
        }
    }

    /// Get a child table (read-only)
    fn get_table(&self, parent_phys: PhysAddr, index: usize) -> Option<PhysAddr> {
        let parent_virt = phys_to_virt(parent_phys);
        let parent = unsafe { &*parent_virt.as_ptr::<PageTable>() };
        let entry = &parent[index];

        if entry.is_present() && !entry.is_huge() {
            Some(entry.addr())
        } else {
            None
        }
    }
}

// TLB and CR3 operations - delegates to the arch layer

#[cfg(target_arch = "x86_64")]
use arch_x86_64::X86_64;

#[cfg(target_arch = "x86_64")]
use arch_traits::TlbControl;

/// Flush the TLB for a specific virtual address
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn flush_tlb(addr: VirtAddr) {
    X86_64::flush(addr);
}

/// Flush the entire TLB by reloading CR3
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn flush_tlb_all() {
    X86_64::flush_all();
}

/// Read the current CR3 value (PML4 physical address)
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn read_cr3() -> PhysAddr {
    X86_64::read_root()
}

/// Write a new CR3 value (switches page tables)
///
/// # Safety
/// The new CR3 must point to a valid PML4 table.
#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn write_cr3(pml4: PhysAddr) {
    unsafe { X86_64::write_root(pml4) };
}
