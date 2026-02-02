//! AHCI HBA Memory Structures

/// HBA Memory Registers (Generic Host Control)
#[repr(C)]
pub struct HbaMemory {
    // Generic Host Control (0x00 - 0x2B)
    /// Host Capabilities
    pub cap: u32, // 0x00
    /// Global Host Control
    pub ghc: u32, // 0x04
    /// Interrupt Status
    pub is: u32, // 0x08
    /// Ports Implemented
    pub pi: u32, // 0x0C
    /// Version
    pub vs: u32, // 0x10
    /// Command Completion Coalescing Control
    pub ccc_ctl: u32, // 0x14
    /// Command Completion Coalescing Ports
    pub ccc_ports: u32, // 0x18
    /// Enclosure Management Location
    pub em_loc: u32, // 0x1C
    /// Enclosure Management Control
    pub em_ctl: u32, // 0x20
    /// Host Capabilities Extended
    pub cap2: u32, // 0x24
    /// BIOS/OS Handoff Control and Status
    pub bohc: u32, // 0x28

    // Reserved (0x2C - 0x9F)
    _reserved: [u8; 0x74],

    // Vendor Specific (0xA0 - 0xFF)
    _vendor: [u8; 0x60],

    // Port Control Registers (0x100 - 0x10FF)
    // Each port has 0x80 bytes, up to 32 ports
    pub ports: [HbaPort; 32],
}

/// HBA Port Registers
#[repr(C)]
pub struct HbaPort {
    /// Command List Base Address (low)
    pub clb: u32, // 0x00
    /// Command List Base Address (high)
    pub clbu: u32, // 0x04
    /// FIS Base Address (low)
    pub fb: u32, // 0x08
    /// FIS Base Address (high)
    pub fbu: u32, // 0x0C
    /// Interrupt Status
    pub is: u32, // 0x10
    /// Interrupt Enable
    pub ie: u32, // 0x14
    /// Command and Status
    pub cmd: u32, // 0x18
    /// Reserved
    _reserved0: u32, // 0x1C
    /// Task File Data
    pub tfd: u32, // 0x20
    /// Signature
    pub sig: u32, // 0x24
    /// Serial ATA Status
    pub ssts: u32, // 0x28
    /// Serial ATA Control
    pub sctl: u32, // 0x2C
    /// Serial ATA Error
    pub serr: u32, // 0x30
    /// Serial ATA Active
    pub sact: u32, // 0x34
    /// Command Issue
    pub ci: u32, // 0x38
    /// Serial ATA Notification
    pub sntf: u32, // 0x3C
    /// FIS-based Switching Control
    pub fbs: u32, // 0x40
    /// Device Sleep
    pub devslp: u32, // 0x44
    /// Reserved
    _reserved1: [u32; 10], // 0x48 - 0x6F
    /// Vendor Specific
    _vendor: [u32; 4], // 0x70 - 0x7F
}

/// Received FIS structure
#[repr(C)]
pub struct ReceivedFis {
    /// DMA Setup FIS
    pub dsfis: [u8; 0x1C],
    _reserved0: [u8; 4],
    /// PIO Setup FIS
    pub psfis: [u8; 0x14],
    _reserved1: [u8; 12],
    /// D2H Register FIS
    pub rfis: [u8; 0x14],
    _reserved2: [u8; 4],
    /// Set Device Bits FIS
    pub sdbfis: [u8; 8],
    /// Unknown FIS
    pub ufis: [u8; 64],
    _reserved3: [u8; 96],
}

/// Command table
#[repr(C)]
pub struct CommandTable {
    /// Command FIS (up to 64 bytes)
    pub cfis: [u8; 64],
    /// ATAPI Command (12-16 bytes)
    pub acmd: [u8; 16],
    /// Reserved
    _reserved: [u8; 48],
    // PRDT entries follow (variable size)
}

impl HbaPort {
    /// Check if device is present
    pub fn device_present(&self) -> bool {
        // Check SSTS.DET field
        (self.ssts & 0xF) == 3
    }

    /// Check if port is idle
    pub fn is_idle(&self) -> bool {
        // Check CMD.ST and CMD.CR and CMD.FRE and CMD.FR
        (self.cmd & 0xC001) == 0
    }

    /// Get device signature
    pub fn signature(&self) -> u32 {
        self.sig
    }

    /// Check if SATA drive
    pub fn is_sata(&self) -> bool {
        self.sig == 0x00000101
    }

    /// Check if ATAPI drive
    pub fn is_atapi(&self) -> bool {
        self.sig == 0xEB140101
    }
}
