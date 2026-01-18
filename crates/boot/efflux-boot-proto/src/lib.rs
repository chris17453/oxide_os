//! EFFLUX Boot Protocol
//!
//! Defines the interface between bootloader and kernel.
//! This crate is shared by both the UEFI bootloader and the kernel.

#![no_std]

/// Magic number to verify boot info validity
pub const BOOT_INFO_MAGIC: u64 = 0xEFF1_0000_B007_1AF0;

/// Maximum number of memory regions we support
pub const MAX_MEMORY_REGIONS: usize = 128;

/// Boot information passed from bootloader to kernel
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BootInfo {
    /// Magic number for validation
    pub magic: u64,
    /// Physical address where kernel was loaded
    pub kernel_phys_base: u64,
    /// Virtual address where kernel is mapped
    pub kernel_virt_base: u64,
    /// Size of kernel in memory
    pub kernel_size: u64,
    /// Physical address of PML4 table
    pub pml4_phys: u64,
    /// Base of direct physical memory map
    pub phys_map_base: u64,
    /// Number of memory regions
    pub memory_region_count: u64,
    /// Memory regions from UEFI
    pub memory_regions: [MemoryRegion; MAX_MEMORY_REGIONS],
    /// Framebuffer info (if available)
    pub framebuffer: Option<FramebufferInfo>,
}

impl BootInfo {
    /// Create an empty boot info structure
    pub const fn empty() -> Self {
        Self {
            magic: 0,
            kernel_phys_base: 0,
            kernel_virt_base: 0,
            kernel_size: 0,
            pml4_phys: 0,
            phys_map_base: 0,
            memory_region_count: 0,
            memory_regions: [MemoryRegion::empty(); MAX_MEMORY_REGIONS],
            framebuffer: None,
        }
    }

    /// Validate the boot info magic number
    pub fn is_valid(&self) -> bool {
        self.magic == BOOT_INFO_MAGIC
    }

    /// Get memory regions as a slice
    pub fn memory_regions(&self) -> &[MemoryRegion] {
        &self.memory_regions[..self.memory_region_count as usize]
    }
}

/// Type of memory region
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    /// Unusable memory
    Reserved = 0,
    /// Free memory available for use
    Usable = 1,
    /// ACPI reclaimable memory
    AcpiReclaimable = 2,
    /// ACPI NVS memory
    AcpiNvs = 3,
    /// Memory containing boot services code/data (can be reclaimed)
    BootServices = 4,
    /// Kernel code/data
    Kernel = 5,
    /// Bootloader data (page tables, etc.)
    Bootloader = 6,
    /// Framebuffer memory
    Framebuffer = 7,
}

/// A memory region descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    /// Physical start address
    pub start: u64,
    /// Length in bytes
    pub len: u64,
    /// Type of memory
    pub ty: MemoryType,
    /// Padding for alignment
    pub _padding: u32,
}

impl MemoryRegion {
    /// Create an empty memory region
    pub const fn empty() -> Self {
        Self {
            start: 0,
            len: 0,
            ty: MemoryType::Reserved,
            _padding: 0,
        }
    }

    /// Create a new memory region
    pub const fn new(start: u64, len: u64, ty: MemoryType) -> Self {
        Self {
            start,
            len,
            ty,
            _padding: 0,
        }
    }

    /// Get end address (exclusive)
    pub const fn end(&self) -> u64 {
        self.start + self.len
    }

    /// Check if this is usable memory
    pub const fn is_usable(&self) -> bool {
        matches!(self.ty, MemoryType::Usable)
    }
}

/// Framebuffer information
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    /// Physical address of framebuffer
    pub base: u64,
    /// Size in bytes
    pub size: u64,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bytes per scanline
    pub stride: u32,
    /// Bits per pixel
    pub bpp: u32,
    /// Pixel format
    pub format: PixelFormat,
}

/// Pixel format for framebuffer
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// RGB with 8 bits per channel
    Rgb = 0,
    /// BGR with 8 bits per channel
    Bgr = 1,
    /// Unknown format
    Unknown = 255,
}

/// Kernel entry point signature
///
/// The bootloader jumps to this function after setting up the environment.
/// The kernel must be compiled with this signature for the entry point.
pub type KernelEntry = extern "C" fn(boot_info: &'static BootInfo) -> !;

/// Expected virtual address for kernel entry
pub const KERNEL_VIRT_BASE: u64 = 0xFFFF_FFFF_8000_0000;

/// Physical memory direct map base
pub const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;
