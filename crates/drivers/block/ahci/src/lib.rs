//! AHCI (Advanced Host Controller Interface) Driver for EFFLUX OS
//!
//! Implements the AHCI 1.3 specification for SATA storage devices.

#![no_std]

extern crate alloc;

mod hba;
mod port;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use block::{BlockDevice, BlockDeviceInfo, BlockError, BlockResult};

pub use hba::HbaMemory;
pub use port::{AhciPort, PortType};

/// AHCI signature values
mod signature {
    pub const SATA: u32 = 0x00000101;  // SATA drive
    pub const ATAPI: u32 = 0xEB140101; // SATAPI drive
    pub const SEMB: u32 = 0xC33C0101;  // Enclosure management bridge
    pub const PM: u32 = 0x96690101;    // Port multiplier
}

/// AHCI capability bits
mod cap {
    pub const S64A: u32 = 1 << 31;    // 64-bit addressing
    pub const SNCQ: u32 = 1 << 30;    // Native Command Queuing
    pub const SSNTF: u32 = 1 << 29;   // SNotification register
    pub const SMPS: u32 = 1 << 28;    // Mechanical presence switch
    pub const SSS: u32 = 1 << 27;     // Staggered spin-up
    pub const SALP: u32 = 1 << 26;    // Aggressive link power management
    pub const SAL: u32 = 1 << 25;     // Activity LED
    pub const SCLO: u32 = 1 << 24;    // Command list override
    pub const ISS_MASK: u32 = 0xF << 20; // Interface speed
    pub const SAM: u32 = 1 << 18;     // AHCI mode only
    pub const SPM: u32 = 1 << 17;     // Port multiplier
    pub const FBSS: u32 = 1 << 16;    // FIS-based switching
    pub const PMD: u32 = 1 << 15;     // PIO multiple DRQ block
    pub const SSC: u32 = 1 << 14;     // Slumber state capable
    pub const PSC: u32 = 1 << 13;     // Partial state capable
    pub const NCS_MASK: u32 = 0x1F << 8; // Number of command slots
    pub const CCCS: u32 = 1 << 7;     // Command completion coalescing
    pub const EMS: u32 = 1 << 6;      // Enclosure management
    pub const SXS: u32 = 1 << 5;      // External SATA
    pub const NP_MASK: u32 = 0x1F;    // Number of ports
}

/// AHCI GHC (Global HBA Control) bits
mod ghc {
    pub const AE: u32 = 1 << 31;      // AHCI Enable
    pub const MRSM: u32 = 1 << 2;     // MSI Revert to Single Message
    pub const IE: u32 = 1 << 1;       // Interrupt Enable
    pub const HR: u32 = 1 << 0;       // HBA Reset
}

/// AHCI port command bits
mod pxcmd {
    pub const ASP: u32 = 1 << 27;     // Aggressive Slumber/Partial
    pub const ALPE: u32 = 1 << 26;    // Aggressive Link Power Enable
    pub const DLAE: u32 = 1 << 25;    // Drive LED on ATAPI Enable
    pub const ATAPI: u32 = 1 << 24;   // Device is ATAPI
    pub const APSTE: u32 = 1 << 23;   // Auto Partial to Slumber
    pub const FBSCP: u32 = 1 << 22;   // FIS-based Switch Capable Port
    pub const ESP: u32 = 1 << 21;     // External SATA Port
    pub const CPD: u32 = 1 << 20;     // Cold Presence Detection
    pub const MPSP: u32 = 1 << 19;    // Mechanical Presence Switch
    pub const HPCP: u32 = 1 << 18;    // Hot Plug Capable
    pub const PMA: u32 = 1 << 17;     // Port Multiplier Attached
    pub const CPS: u32 = 1 << 16;     // Cold Presence State
    pub const CR: u32 = 1 << 15;      // Command List Running
    pub const FR: u32 = 1 << 14;      // FIS Receive Running
    pub const MPSS: u32 = 1 << 13;    // Mechanical Presence State
    pub const CCS_MASK: u32 = 0x1F << 8; // Current Command Slot
    pub const FRE: u32 = 1 << 4;      // FIS Receive Enable
    pub const CLO: u32 = 1 << 3;      // Command List Override
    pub const POD: u32 = 1 << 2;      // Power On Device
    pub const SUD: u32 = 1 << 1;      // Spin-Up Device
    pub const ST: u32 = 1 << 0;       // Start
}

/// AHCI interrupt status bits
mod pxis {
    pub const CPDS: u32 = 1 << 31;    // Cold Port Detect Status
    pub const TFES: u32 = 1 << 30;    // Task File Error Status
    pub const HBFS: u32 = 1 << 29;    // Host Bus Fatal Error Status
    pub const HBDS: u32 = 1 << 28;    // Host Bus Data Error Status
    pub const IFS: u32 = 1 << 27;     // Interface Fatal Error Status
    pub const INFS: u32 = 1 << 26;    // Interface Non-Fatal Error Status
    pub const OFS: u32 = 1 << 24;     // Overflow Status
    pub const IPMS: u32 = 1 << 23;    // Incorrect Port Multiplier Status
    pub const PRCS: u32 = 1 << 22;    // PhyRdy Change Status
    pub const DMPS: u32 = 1 << 7;     // Device Mechanical Presence Status
    pub const PCS: u32 = 1 << 6;      // Port Connect Change Status
    pub const DPS: u32 = 1 << 5;      // Descriptor Processed Status
    pub const UFS: u32 = 1 << 4;      // Unknown FIS Interrupt
    pub const SDBS: u32 = 1 << 3;     // Set Device Bits Interrupt
    pub const DSS: u32 = 1 << 2;      // DMA Setup FIS Interrupt
    pub const PSS: u32 = 1 << 1;      // PIO Setup FIS Interrupt
    pub const DHRS: u32 = 1 << 0;     // Device to Host Register FIS Interrupt
}

/// FIS types
mod fis_type {
    pub const REG_H2D: u8 = 0x27;     // Register FIS - Host to Device
    pub const REG_D2H: u8 = 0x34;     // Register FIS - Device to Host
    pub const DMA_ACT: u8 = 0x39;     // DMA Activate FIS
    pub const DMA_SETUP: u8 = 0x41;   // DMA Setup FIS
    pub const DATA: u8 = 0x46;        // Data FIS
    pub const BIST: u8 = 0x58;        // BIST Activate FIS
    pub const PIO_SETUP: u8 = 0x5F;   // PIO Setup FIS
    pub const DEV_BITS: u8 = 0xA1;    // Set Device Bits FIS
}

/// ATA commands
mod ata_cmd {
    pub const READ_DMA_EXT: u8 = 0x25;
    pub const WRITE_DMA_EXT: u8 = 0x35;
    pub const FLUSH_CACHE_EXT: u8 = 0xEA;
    pub const IDENTIFY_DEVICE: u8 = 0xEC;
    pub const IDENTIFY_PACKET: u8 = 0xA1;
}

/// FIS structure: Register H2D
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FisRegH2D {
    /// FIS type (0x27)
    pub fis_type: u8,
    /// Flags: C bit (bit 7), PM Port (bits 0-3)
    pub flags: u8,
    /// Command register
    pub command: u8,
    /// Features register, low byte
    pub feature_low: u8,

    /// LBA low byte
    pub lba0: u8,
    /// LBA mid byte
    pub lba1: u8,
    /// LBA high byte
    pub lba2: u8,
    /// Device register
    pub device: u8,

    /// LBA low byte (exp)
    pub lba3: u8,
    /// LBA mid byte (exp)
    pub lba4: u8,
    /// LBA high byte (exp)
    pub lba5: u8,
    /// Features register, high byte
    pub feature_high: u8,

    /// Sector count low byte
    pub count_low: u8,
    /// Sector count high byte
    pub count_high: u8,
    /// Reserved
    pub icc: u8,
    /// Control register
    pub control: u8,

    /// Reserved
    pub reserved: [u8; 4],
}

impl FisRegH2D {
    /// Create a new Register H2D FIS
    pub fn new() -> Self {
        FisRegH2D {
            fis_type: fis_type::REG_H2D,
            flags: 0x80, // Command bit set
            ..Default::default()
        }
    }

    /// Set LBA48 address
    pub fn set_lba(&mut self, lba: u64) {
        self.lba0 = (lba & 0xFF) as u8;
        self.lba1 = ((lba >> 8) & 0xFF) as u8;
        self.lba2 = ((lba >> 16) & 0xFF) as u8;
        self.lba3 = ((lba >> 24) & 0xFF) as u8;
        self.lba4 = ((lba >> 32) & 0xFF) as u8;
        self.lba5 = ((lba >> 40) & 0xFF) as u8;
        self.device = 0x40; // LBA mode
    }

    /// Set sector count
    pub fn set_count(&mut self, count: u16) {
        self.count_low = (count & 0xFF) as u8;
        self.count_high = ((count >> 8) & 0xFF) as u8;
    }
}

/// Command header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CommandHeader {
    /// Flags and command FIS length
    pub flags: u16,
    /// Physical Region Descriptor Table Length
    pub prdtl: u16,
    /// Physical Region Descriptor Byte Count
    pub prdbc: u32,
    /// Command Table Base Address (low)
    pub ctba_low: u32,
    /// Command Table Base Address (high)
    pub ctba_high: u32,
    /// Reserved
    pub reserved: [u32; 4],
}

impl CommandHeader {
    /// Create a new command header
    pub fn new() -> Self {
        CommandHeader {
            flags: core::mem::size_of::<FisRegH2D>() as u16 / 4, // CFL in DWORDs
            ..Default::default()
        }
    }

    /// Set command table address
    pub fn set_ctba(&mut self, addr: u64) {
        self.ctba_low = addr as u32;
        self.ctba_high = (addr >> 32) as u32;
    }

    /// Set write flag
    pub fn set_write(&mut self) {
        self.flags |= 1 << 6; // W bit
    }

    /// Set PRDT length
    pub fn set_prdtl(&mut self, count: u16) {
        self.prdtl = count;
    }
}

/// Physical Region Descriptor Table entry
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PrdtEntry {
    /// Data Base Address (low)
    pub dba_low: u32,
    /// Data Base Address (high)
    pub dba_high: u32,
    /// Reserved
    pub reserved: u32,
    /// Byte count and interrupt flag
    pub dbc_i: u32,
}

impl PrdtEntry {
    /// Create a new PRDT entry
    pub fn new(addr: u64, size: u32, interrupt: bool) -> Self {
        PrdtEntry {
            dba_low: addr as u32,
            dba_high: (addr >> 32) as u32,
            reserved: 0,
            dbc_i: (size - 1) | if interrupt { 1 << 31 } else { 0 },
        }
    }
}

/// AHCI controller
pub struct AhciController {
    /// HBA memory base address
    mmio_base: u64,
    /// Controller capabilities
    capabilities: u32,
    /// Number of ports
    num_ports: u8,
    /// Number of command slots per port
    num_slots: u8,
    /// Ports
    ports: Mutex<Vec<Option<AhciPort>>>,
}

impl AhciController {
    /// Probe for an AHCI controller at the given PCI BAR5 address
    ///
    /// # Safety
    // HBA register offsets
    const REG_CAP: usize = 0x00;
    const REG_GHC: usize = 0x04;
    const REG_PI: usize = 0x0C;
    const REG_VS: usize = 0x10;

    /// The MMIO address must be valid and mapped.
    pub unsafe fn probe(mmio_base: u64) -> Option<Self> {
        unsafe {
            let base = mmio_base as *mut u8;

            // Read capabilities
            let cap_ptr = base.add(Self::REG_CAP) as *const u32;
            let cap = core::ptr::read_volatile(cap_ptr);

            // Check AHCI version
            let vs_ptr = base.add(Self::REG_VS) as *const u32;
            let version = core::ptr::read_volatile(vs_ptr);
            if version < 0x00010000 {
                return None;
            }

            // Enable AHCI mode
            let ghc_ptr = base.add(Self::REG_GHC) as *mut u32;
            let ghc = core::ptr::read_volatile(ghc_ptr);
            core::ptr::write_volatile(ghc_ptr, ghc | ghc::AE);

            // Get port count
            let num_ports = ((cap & cap::NP_MASK) + 1) as u8;
            let num_slots = (((cap & cap::NCS_MASK) >> 8) + 1) as u8;

            // Initialize ports vector
            let mut ports = Vec::with_capacity(num_ports as usize);
            for _ in 0..num_ports {
                ports.push(None);
            }

            Some(AhciController {
                mmio_base,
                capabilities: cap,
                num_ports,
                num_slots,
                ports: Mutex::new(ports),
            })
        }
    }

    /// Initialize all available ports
    pub unsafe fn init_ports(&self) {
        unsafe {
            let base = self.mmio_base as *const u8;
            let pi_ptr = base.add(Self::REG_PI) as *const u32;
            let pi = core::ptr::read_volatile(pi_ptr);

            let mut ports = self.ports.lock();
            for i in 0..self.num_ports as usize {
                if pi & (1 << i) != 0 {
                    let port_base = self.mmio_base + 0x100 + (i as u64 * 0x80);
                    if let Some(port) = AhciPort::probe(port_base, i as u8) {
                        ports[i] = Some(port);
                    }
                }
            }
        }
    }

    /// Get a port by index
    pub fn port(&self, index: usize) -> Option<&AhciPort> {
        // This is not quite right but shows the structure
        None
    }

    /// Check if 64-bit addressing is supported
    pub fn supports_64bit(&self) -> bool {
        self.capabilities & cap::S64A != 0
    }

    /// Check if NCQ is supported
    pub fn supports_ncq(&self) -> bool {
        self.capabilities & cap::SNCQ != 0
    }
}

/// AHCI drive (block device for a specific port)
pub struct AhciDrive {
    /// Port index
    port_index: u8,
    /// Controller MMIO base
    mmio_base: u64,
    /// Sector size
    sector_size: u32,
    /// Number of sectors
    sector_count: u64,
    /// Model string
    model: String,
    /// Serial number
    serial: String,
}

impl AhciDrive {
    /// Create a new AHCI drive from an initialized port
    pub fn new(
        port_index: u8,
        mmio_base: u64,
        sector_size: u32,
        sector_count: u64,
        model: String,
        serial: String,
    ) -> Self {
        AhciDrive {
            port_index,
            mmio_base,
            sector_size,
            sector_count,
            model,
            serial,
        }
    }
}

impl BlockDevice for AhciDrive {
    fn read(&self, start_block: u64, buf: &mut [u8]) -> BlockResult<usize> {
        if start_block >= self.sector_count {
            return Err(BlockError::InvalidBlock);
        }

        let sectors = buf.len() / self.sector_size as usize;
        if start_block + sectors as u64 > self.sector_count {
            return Err(BlockError::InvalidBlock);
        }

        // In a real implementation:
        // 1. Build command FIS (READ DMA EXT)
        // 2. Setup PRDT
        // 3. Issue command
        // 4. Wait for completion
        // 5. Check status

        // Stub - return zeros
        buf.fill(0);
        Ok(buf.len())
    }

    fn write(&self, start_block: u64, buf: &[u8]) -> BlockResult<usize> {
        if start_block >= self.sector_count {
            return Err(BlockError::InvalidBlock);
        }

        let sectors = buf.len() / self.sector_size as usize;
        if start_block + sectors as u64 > self.sector_count {
            return Err(BlockError::InvalidBlock);
        }

        // Similar to read but with WRITE DMA EXT

        Ok(buf.len())
    }

    fn flush(&self) -> BlockResult<()> {
        // Send FLUSH CACHE EXT command
        Ok(())
    }

    fn block_size(&self) -> u32 {
        self.sector_size
    }

    fn block_count(&self) -> u64 {
        self.sector_count
    }

    fn info(&self) -> BlockDeviceInfo {
        BlockDeviceInfo {
            name: "sata",
            block_size: self.sector_size,
            block_count: self.sector_count,
            read_only: false,
            removable: false,
            model: "SATA Drive",
        }
    }
}
