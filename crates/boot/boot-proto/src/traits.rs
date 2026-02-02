//! Boot Protocol Traits
//!
//! Defines trait-based abstractions for different boot protocols.
//! Supports UEFI (x86/ARM), ARCS (SGI MIPS), and Device Tree.
//!
//! — NeonRoot

/// Boot protocol trait
///
/// Implemented by different boot protocols (UEFI, ARCS, Device Tree)
pub trait BootProtocol {
    /// Boot protocol name
    fn protocol_name(&self) -> &'static str;

    /// Get memory map from bootloader
    fn memory_map(&self) -> &[crate::MemoryRegion];

    /// Get framebuffer information if available
    fn framebuffer(&self) -> Option<crate::FramebufferInfo>;

    /// Get kernel physical base address
    fn kernel_phys_base(&self) -> u64;

    /// Get kernel virtual base address
    fn kernel_virt_base(&self) -> u64;

    /// Get kernel size
    fn kernel_size(&self) -> u64;

    /// Get page table root physical address
    ///
    /// - x86_64: PML4 physical address
    /// - ARM64: TTBR1_EL1 value
    /// - MIPS64: CP0 Context PTEBase
    fn page_table_root(&self) -> u64;

    /// Get physical memory direct map base
    ///
    /// Virtual address where physical memory is linearly mapped
    fn phys_map_base(&self) -> u64;

    /// Get initramfs if loaded
    fn initramfs(&self) -> Option<InitramfsInfo>;

    /// Get command line if provided
    fn command_line(&self) -> Option<&str> {
        None
    }

    /// Get architecture-specific data
    fn arch_data(&self) -> Option<&dyn core::any::Any> {
        None
    }
}

/// Initramfs information
#[derive(Debug, Clone, Copy)]
pub struct InitramfsInfo {
    /// Physical address
    pub phys_addr: u64,
    /// Size in bytes
    pub size: u64,
}

impl InitramfsInfo {
    /// Create new initramfs info
    pub const fn new(phys_addr: u64, size: u64) -> Self {
        Self { phys_addr, size }
    }

    /// Get as virtual address slice through physical map
    pub unsafe fn as_slice(&self, phys_map_base: u64) -> &'static [u8] {
        let virt = phys_map_base + self.phys_addr;
        unsafe { core::slice::from_raw_parts(virt as *const u8, self.size as usize) }
    }
}

/// UEFI-specific boot data
#[derive(Debug, Clone, Copy)]
pub struct UefiBootData {
    /// UEFI system table pointer (if still valid)
    pub system_table: Option<u64>,
    /// UEFI runtime services pointer (if still valid)
    pub runtime_services: Option<u64>,
}

/// ARCS-specific boot data (SGI MIPS)
#[derive(Debug, Clone, Copy)]
pub struct ArcsBootData {
    /// ARCS SPB (System Parameter Block) pointer
    pub spb_addr: u64,
    /// ARCS firmware vector table pointer
    pub fw_vector: u64,
    /// Environment variable strings pointer
    pub env_vars: u64,
}

/// Device Tree boot data (ARM, RISC-V)
#[derive(Debug, Clone, Copy)]
pub struct DeviceTreeBootData {
    /// Device tree blob physical address
    pub dtb_addr: u64,
    /// Device tree size
    pub dtb_size: u64,
}

/// Architecture-agnostic boot information
///
/// This structure can be initialized from any boot protocol
#[derive(Debug, Clone, Copy)]
pub struct GenericBootInfo {
    /// Boot protocol used
    pub protocol: BootProtocolType,
    /// Memory regions
    pub memory_region_count: usize,
    /// Kernel physical base
    pub kernel_phys_base: u64,
    /// Kernel virtual base
    pub kernel_virt_base: u64,
    /// Kernel size
    pub kernel_size: u64,
    /// Page table root
    pub page_table_root: u64,
    /// Physical memory map base
    pub phys_map_base: u64,
    /// Framebuffer if available
    pub framebuffer: Option<crate::FramebufferInfo>,
    /// Initramfs if loaded
    pub initramfs: Option<InitramfsInfo>,
}

/// Boot protocol type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BootProtocolType {
    /// UEFI (x86_64, aarch64)
    Uefi = 0,
    /// ARCS (SGI MIPS64)
    Arcs = 1,
    /// Device Tree (ARM, RISC-V)
    DeviceTree = 2,
    /// Multiboot (x86)
    Multiboot = 3,
    /// Unknown/Custom
    Unknown = 255,
}

impl BootProtocolType {
    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Uefi => "UEFI",
            Self::Arcs => "ARCS",
            Self::DeviceTree => "Device Tree",
            Self::Multiboot => "Multiboot",
            Self::Unknown => "Unknown",
        }
    }
}
