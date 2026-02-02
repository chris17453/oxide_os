//! Guest Memory Management

use alloc::vec::Vec;

use crate::{VmmError, VmmResult};

/// Guest physical address
pub type Gpa = u64;
/// Host virtual address
pub type Hva = u64;
/// Host physical address
pub type Hpa = u64;

/// Guest physical address range
#[derive(Debug, Clone)]
pub struct GpaRange {
    pub start: Gpa,
    pub end: Gpa,
}

impl GpaRange {
    pub fn new(start: Gpa, size: u64) -> Self {
        GpaRange {
            start,
            end: start + size,
        }
    }

    pub fn size(&self) -> u64 {
        self.end - self.start
    }

    pub fn contains(&self, gpa: Gpa) -> bool {
        gpa >= self.start && gpa < self.end
    }

    pub fn overlaps(&self, other: &GpaRange) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Memory region flags
#[derive(Debug, Clone, Copy)]
pub struct MemoryFlags {
    /// Readable
    pub read: bool,
    /// Writable
    pub write: bool,
    /// Executable
    pub execute: bool,
    /// Device memory (no caching)
    pub device: bool,
}

impl Default for MemoryFlags {
    fn default() -> Self {
        MemoryFlags {
            read: true,
            write: true,
            execute: false,
            device: false,
        }
    }
}

impl MemoryFlags {
    pub fn read_only() -> Self {
        MemoryFlags {
            read: true,
            write: false,
            execute: false,
            device: false,
        }
    }

    pub fn read_write() -> Self {
        Self::default()
    }

    pub fn read_execute() -> Self {
        MemoryFlags {
            read: true,
            write: false,
            execute: true,
            device: false,
        }
    }

    pub fn all() -> Self {
        MemoryFlags {
            read: true,
            write: true,
            execute: true,
            device: false,
        }
    }
}

/// Guest memory region
#[derive(Clone)]
pub struct GuestMemoryRegion {
    /// Guest physical address range
    pub gpa_range: GpaRange,
    /// Host virtual address of backing memory
    pub hva: Hva,
    /// Memory flags
    pub flags: MemoryFlags,
    /// Region name for debugging
    pub name: &'static str,
}

impl GuestMemoryRegion {
    /// Create new region
    pub fn new(gpa: Gpa, size: u64, hva: Hva, flags: MemoryFlags, name: &'static str) -> Self {
        GuestMemoryRegion {
            gpa_range: GpaRange::new(gpa, size),
            hva,
            flags,
            name,
        }
    }

    /// Translate GPA to HVA
    pub fn translate(&self, gpa: Gpa) -> Option<Hva> {
        if self.gpa_range.contains(gpa) {
            let offset = gpa - self.gpa_range.start;
            Some(self.hva + offset)
        } else {
            None
        }
    }
}

/// Guest memory manager
pub struct GuestMemory {
    /// Memory regions
    regions: Vec<GuestMemoryRegion>,
    /// Total memory size
    total_size: u64,
}

impl GuestMemory {
    /// Create new guest memory manager
    pub fn new() -> Self {
        GuestMemory {
            regions: Vec::new(),
            total_size: 0,
        }
    }

    /// Add memory region
    pub fn add_region(&mut self, region: GuestMemoryRegion) -> VmmResult<()> {
        // Check for overlaps
        for existing in &self.regions {
            if existing.gpa_range.overlaps(&region.gpa_range) {
                return Err(VmmError::InvalidMemory);
            }
        }

        self.total_size += region.gpa_range.size();
        self.regions.push(region);
        Ok(())
    }

    /// Get total memory size
    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    /// Get region count
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Get all regions
    pub fn regions(&self) -> &[GuestMemoryRegion] {
        &self.regions
    }

    /// Translate GPA to HVA
    pub fn translate(&self, gpa: Gpa) -> Option<Hva> {
        for region in &self.regions {
            if let Some(hva) = region.translate(gpa) {
                return Some(hva);
            }
        }
        None
    }

    /// Find region containing GPA
    pub fn find_region(&self, gpa: Gpa) -> Option<&GuestMemoryRegion> {
        self.regions.iter().find(|r| r.gpa_range.contains(gpa))
    }

    /// Read from guest memory
    pub fn read(&self, gpa: Gpa, buf: &mut [u8]) -> VmmResult<()> {
        let region = self.find_region(gpa).ok_or(VmmError::InvalidMemory)?;

        if !region.flags.read {
            return Err(VmmError::InvalidMemory);
        }

        let hva = region.translate(gpa).ok_or(VmmError::InvalidMemory)?;

        // Safety: HVA should be valid host memory
        unsafe {
            core::ptr::copy_nonoverlapping(hva as *const u8, buf.as_mut_ptr(), buf.len());
        }
        Ok(())
    }

    /// Write to guest memory
    pub fn write(&self, gpa: Gpa, buf: &[u8]) -> VmmResult<()> {
        let region = self.find_region(gpa).ok_or(VmmError::InvalidMemory)?;

        if !region.flags.write {
            return Err(VmmError::InvalidMemory);
        }

        let hva = region.translate(gpa).ok_or(VmmError::InvalidMemory)?;

        // Safety: HVA should be valid host memory
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), hva as *mut u8, buf.len());
        }
        Ok(())
    }

    /// Read u8 from guest
    pub fn read_u8(&self, gpa: Gpa) -> VmmResult<u8> {
        let mut buf = [0u8; 1];
        self.read(gpa, &mut buf)?;
        Ok(buf[0])
    }

    /// Read u16 from guest
    pub fn read_u16(&self, gpa: Gpa) -> VmmResult<u16> {
        let mut buf = [0u8; 2];
        self.read(gpa, &mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    /// Read u32 from guest
    pub fn read_u32(&self, gpa: Gpa) -> VmmResult<u32> {
        let mut buf = [0u8; 4];
        self.read(gpa, &mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    /// Read u64 from guest
    pub fn read_u64(&self, gpa: Gpa) -> VmmResult<u64> {
        let mut buf = [0u8; 8];
        self.read(gpa, &mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Write u8 to guest
    pub fn write_u8(&self, gpa: Gpa, value: u8) -> VmmResult<()> {
        self.write(gpa, &[value])
    }

    /// Write u16 to guest
    pub fn write_u16(&self, gpa: Gpa, value: u16) -> VmmResult<()> {
        self.write(gpa, &value.to_le_bytes())
    }

    /// Write u32 to guest
    pub fn write_u32(&self, gpa: Gpa, value: u32) -> VmmResult<()> {
        self.write(gpa, &value.to_le_bytes())
    }

    /// Write u64 to guest
    pub fn write_u64(&self, gpa: Gpa, value: u64) -> VmmResult<()> {
        self.write(gpa, &value.to_le_bytes())
    }
}

impl Default for GuestMemory {
    fn default() -> Self {
        Self::new()
    }
}
