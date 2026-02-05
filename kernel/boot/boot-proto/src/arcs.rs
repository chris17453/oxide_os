//! ARCS Boot Protocol Support
//!
//! ARCS (Advanced RISC Computing Specification) is the boot firmware
//! used by Silicon Graphics (SGI) MIPS workstations.
//!
//! ## Platforms
//!
//! - IP22: Indy, Indigo2
//! - IP27: Origin 200/2000, Onyx2
//! - IP30: Octane, Octane2
//! - IP32: O2, O2+
//!
//! ## Key Differences from UEFI
//!
//! - **Big-endian** data structures
//! - Pointer-heavy (32-bit pointers even on 64-bit systems)
//! - Firmware callbacks remain accessible (unlike UEFI ExitBootServices)
//! - Memory descriptors use page-based addressing
//!
//! — NeonRoot

use crate::{MemoryRegion, MemoryType};

/// ARCS memory descriptor type
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcsMemoryType {
    /// ROM exception vectors
    ExceptionBlock = 0,
    /// System Parameter Block
    SpbPage = 1,
    /// Free memory available for use
    FreeMemory = 2,
    /// PROM temporary storage
    FirmwareTemporary = 3,
    /// PROM permanent data
    FirmwarePermanent = 4,
    /// Contiguous free memory
    FreeContiguous = 5,
    /// Bad/defective memory
    BadMemory = 6,
    /// Loaded program/kernel
    LoadedProgram = 7,
    /// PROM code
    FirmwareCode = 8,
}

impl ArcsMemoryType {
    /// Convert ARCS memory type to generic MemoryType
    pub fn to_generic(self) -> MemoryType {
        match self {
            Self::FreeMemory | Self::FreeContiguous => MemoryType::Usable,
            Self::LoadedProgram => MemoryType::Kernel,
            Self::FirmwareCode | Self::FirmwarePermanent => MemoryType::Reserved,
            Self::FirmwareTemporary => MemoryType::BootServices,
            _ => MemoryType::Reserved,
        }
    }
}

/// ARCS memory descriptor
///
/// Note: This is a big-endian structure!
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArcsMemoryDescriptor {
    /// Memory type
    pub memory_type: u32,
    /// Base page number (page size = 4KB typically)
    pub base_page: u32,
    /// Number of pages
    pub page_count: u32,
}

impl ArcsMemoryDescriptor {
    /// Convert to generic MemoryRegion
    ///
    /// # Arguments
    /// * `page_size` - Page size in bytes (typically 4096)
    pub fn to_generic(&self, page_size: u64) -> MemoryRegion {
        // Note: Fields are big-endian, need to swap on little-endian host
        let memory_type = u32::from_be(self.memory_type);
        let base_page = u32::from_be(self.base_page);
        let page_count = u32::from_be(self.page_count);

        let arcs_type = unsafe { core::mem::transmute::<u32, ArcsMemoryType>(memory_type) };

        MemoryRegion {
            start: (base_page as u64) * page_size,
            len: (page_count as u64) * page_size,
            ty: arcs_type.to_generic(),
            _padding: 0,
        }
    }
}

/// ARCS System Parameter Block (SPB)
///
/// Contains pointers to firmware services and configuration
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArcsSpb {
    /// Signature (should be 0x53435241 "ARCS" in big-endian)
    pub signature: u32,
    /// SPB length
    pub length: u32,
    /// SPB version
    pub version: u16,
    /// SPB revision
    pub revision: u16,
    /// Pointer to restart block
    pub restart_block: u32,
    /// Pointer to debug block
    pub debug_block: u32,
    /// Pointer to general exception vector
    pub gev: u32,
    /// Pointer to TLB miss exception vector
    pub utlb_miss_vector: u32,
    /// Firmware vector table length
    pub firmware_vector_length: u32,
    /// Pointer to firmware vector table
    pub firmware_vector: u32,
    /// Pointer to private vector
    pub private_vector: u32,
    /// Adapter count
    pub adapter_count: u32,
    /// Adapter vector table
    pub adapter_vector: u32,
}

/// ARCS Firmware Vector Table
///
/// Function pointers to firmware services
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArcsFirmwareVector {
    /// Load a file
    pub load: u32,
    /// Invoke a program
    pub invoke: u32,
    /// Execute a program
    pub execute: u32,
    /// Halt the system
    pub halt: u32,
    /// Power down
    pub power_down: u32,
    /// Restart the system
    pub restart: u32,
    /// Reboot
    pub reboot: u32,
    /// Enter interactive mode
    pub enter_interactive_mode: u32,
    /// Get peer component
    pub get_peer: u32,
    /// Get child component
    pub get_child: u32,
    /// Get parent component
    pub get_parent: u32,
    /// Get configuration data
    pub get_config_data: u32,
    /// Get component
    pub get_component: u32,
    /// Get system ID
    pub get_system_id: u32,
    /// Get memory descriptor
    pub get_memory_descriptor: u32,
    /// Get time
    pub get_time: u32,
    /// Get relative time
    pub get_relative_time: u32,
    /// Get directory entry
    pub get_directory_entry: u32,
    /// Open file
    pub open: u32,
    /// Close file
    pub close: u32,
    /// Read from file
    pub read: u32,
    /// Get read status
    pub get_read_status: u32,
    /// Write to file
    pub write: u32,
    /// Seek in file
    pub seek: u32,
    /// Get environment variable
    pub get_environment_variable: u32,
    /// Set environment variable
    pub set_environment_variable: u32,
    /// Get file information
    pub get_file_information: u32,
    /// Set file information
    pub set_file_information: u32,
    /// Flush all caches
    pub flush_all_caches: u32,
}

/// ARCS environment variable structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArcsEnvironmentVariable {
    /// Variable name pointer (C string)
    pub name: u32,
    /// Variable value pointer (C string)
    pub value: u32,
}

/// Parse ARCS memory map
///
/// # Safety
///
/// The caller must ensure the ARCS firmware vector and SPB are valid
pub unsafe fn parse_arcs_memory_map(
    _firmware_vector: *const ArcsFirmwareVector,
    _max_regions: usize,
) -> (usize, [MemoryRegion; crate::MAX_MEMORY_REGIONS]) {
    let regions = [MemoryRegion::empty(); crate::MAX_MEMORY_REGIONS];
    let count = 0;

    // -- SableWire: ARCS is MIPS/SGI-specific (IP22 Indy, IP27 Origin, IP30 Octane, IP32 O2)
    // OXIDE targets x86_64 exclusively. This stub is retained for potential future MIPS port.
    // Blocked: Requires MIPS cross-compilation toolchain + SGI firmware test environment.
    // Implementation would call GetMemoryDescriptor via ArcsFirmwareVector callback table,
    // iterating descriptors until null, converting ArcsMemoryType to our MemoryRegion format.
    (count, regions)
}

/// ARCS magic signature
pub const ARCS_SIGNATURE: u32 = 0x53435241; // "ARCS" in big-endian

/// Validate ARCS SPB signature
pub fn validate_arcs_spb(spb: &ArcsSpb) -> bool {
    u32::from_be(spb.signature) == ARCS_SIGNATURE
}
