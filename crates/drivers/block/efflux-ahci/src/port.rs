//! AHCI Port Management

use alloc::vec::Vec;
use spin::Mutex;

use crate::hba::HbaPort;
use crate::{ata_cmd, fis_type, pxcmd, pxis, signature, CommandHeader, FisRegH2D, PrdtEntry};

/// Port type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortType {
    /// Not present
    None,
    /// SATA drive
    Sata,
    /// SATAPI drive
    Satapi,
    /// Enclosure management bridge
    Semb,
    /// Port multiplier
    Pm,
    /// Unknown
    Unknown,
}

/// HBA port register offsets
mod port_reg {
    pub const CLB: usize = 0x00;
    pub const CLBU: usize = 0x04;
    pub const FB: usize = 0x08;
    pub const FBU: usize = 0x0C;
    pub const IS: usize = 0x10;
    pub const IE: usize = 0x14;
    pub const CMD: usize = 0x18;
    pub const TFD: usize = 0x20;
    pub const SIG: usize = 0x24;
    pub const SSTS: usize = 0x28;
    pub const SCTL: usize = 0x2C;
    pub const SERR: usize = 0x30;
    pub const SACT: usize = 0x34;
    pub const CI: usize = 0x38;
}

/// AHCI port
pub struct AhciPort {
    /// Port base address
    base: u64,
    /// Port index
    index: u8,
    /// Port type
    port_type: PortType,
    /// Command list (32 entries)
    command_list: Mutex<Vec<CommandHeader>>,
    /// Received FIS area
    received_fis: u64,
    /// Sector size
    sector_size: u32,
    /// Sector count
    sector_count: u64,
}

impl AhciPort {
    /// Probe a port for devices
    ///
    /// # Safety
    /// The port address must be valid.
    pub unsafe fn probe(port_base: u64, index: u8) -> Option<Self> {
        unsafe {
            let base = port_base as *const u8;

            // Check if device is present
            let ssts_ptr = base.add(port_reg::SSTS) as *const u32;
            let ssts = core::ptr::read_volatile(ssts_ptr);
            let det = ssts & 0xF;
            let ipm = (ssts >> 8) & 0xF;

            if det != 3 || ipm != 1 {
                return None;
            }

            // Get device type from signature
            let sig_ptr = base.add(port_reg::SIG) as *const u32;
            let sig = core::ptr::read_volatile(sig_ptr);
            let port_type = match sig {
                signature::SATA => PortType::Sata,
                signature::ATAPI => PortType::Satapi,
                signature::SEMB => PortType::Semb,
                signature::PM => PortType::Pm,
                _ => PortType::Unknown,
            };

            if port_type != PortType::Sata && port_type != PortType::Satapi {
                return None;
            }

            // Initialize command list
            let mut command_list = Vec::with_capacity(32);
            for _ in 0..32 {
                command_list.push(CommandHeader::new());
            }

            Some(AhciPort {
                base: port_base,
                index,
                port_type,
                command_list: Mutex::new(command_list),
                received_fis: 0, // Would be allocated
                sector_size: 512,
                sector_count: 0,
            })
        }
    }

    /// Get port index
    pub fn index(&self) -> u8 {
        self.index
    }

    /// Get port type
    pub fn port_type(&self) -> PortType {
        self.port_type
    }

    /// Start the port
    ///
    /// # Safety
    /// The port must be properly initialized.
    pub unsafe fn start(&self) {
        unsafe {
            let base = self.base as *mut u8;
            let cmd_ptr = base.add(port_reg::CMD) as *mut u32;

            // Wait for port to be idle
            for _ in 0..1000 {
                let cmd = core::ptr::read_volatile(cmd_ptr);
                if cmd & pxcmd::CR == 0 {
                    break;
                }
                core::hint::spin_loop();
            }

            // Enable FIS receive
            let cmd = core::ptr::read_volatile(cmd_ptr);
            core::ptr::write_volatile(cmd_ptr, cmd | pxcmd::FRE);

            // Wait for FR
            for _ in 0..1000 {
                let cmd = core::ptr::read_volatile(cmd_ptr);
                if cmd & pxcmd::FR != 0 {
                    break;
                }
                core::hint::spin_loop();
            }

            // Start command processing
            let cmd = core::ptr::read_volatile(cmd_ptr);
            core::ptr::write_volatile(cmd_ptr, cmd | pxcmd::ST);
        }
    }

    /// Stop the port
    ///
    /// # Safety
    /// The port must be running.
    pub unsafe fn stop(&self) {
        unsafe {
            let base = self.base as *mut u8;
            let cmd_ptr = base.add(port_reg::CMD) as *mut u32;

            // Clear ST
            let cmd = core::ptr::read_volatile(cmd_ptr);
            core::ptr::write_volatile(cmd_ptr, cmd & !pxcmd::ST);

            // Wait for CR to clear
            for _ in 0..1000000 {
                let cmd = core::ptr::read_volatile(cmd_ptr);
                if cmd & pxcmd::CR == 0 {
                    break;
                }
                core::hint::spin_loop();
            }

            // Clear FRE
            let cmd = core::ptr::read_volatile(cmd_ptr);
            core::ptr::write_volatile(cmd_ptr, cmd & !pxcmd::FRE);

            // Wait for FR to clear
            for _ in 0..1000000 {
                let cmd = core::ptr::read_volatile(cmd_ptr);
                if cmd & pxcmd::FR == 0 {
                    break;
                }
                core::hint::spin_loop();
            }
        }
    }

    /// Find a free command slot
    fn find_free_slot(&self) -> Option<u8> {
        // In a real implementation, check CI and SACT registers
        // For now, always return slot 0
        Some(0)
    }

    /// Wait for command completion
    unsafe fn wait_completion(&self, slot: u8) -> Result<(), ()> {
        unsafe {
            let base = self.base as *const u8;
            let ci_ptr = base.add(port_reg::CI) as *const u32;
            let is_ptr = base.add(port_reg::IS) as *const u32;
            let tfd_ptr = base.add(port_reg::TFD) as *const u32;

            // Wait for CI bit to clear
            for _ in 0..1000000 {
                let ci = core::ptr::read_volatile(ci_ptr);
                if ci & (1 << slot) == 0 {
                    break;
                }

                // Check for errors
                let is = core::ptr::read_volatile(is_ptr);
                if is & pxis::TFES != 0 {
                    return Err(());
                }

                core::hint::spin_loop();
            }

            // Check for errors
            let tfd = core::ptr::read_volatile(tfd_ptr);
            if tfd & 0x01 != 0 {
                // Error bit set
                return Err(());
            }

            Ok(())
        }
    }

    /// Send identify command
    ///
    /// # Safety
    /// Port must be started.
    pub unsafe fn identify(&mut self) -> Result<IdentifyData, ()> {
        // Would send IDENTIFY DEVICE command and parse results
        Err(())
    }

    /// Read sectors
    ///
    /// # Safety
    /// Port must be started.
    pub unsafe fn read_sectors(&self, lba: u64, count: u16, _buf: &mut [u8]) -> Result<(), ()> {
        unsafe {
            let slot = self.find_free_slot().ok_or(())?;

            // Build FIS
            let mut fis = FisRegH2D::new();
            fis.command = ata_cmd::READ_DMA_EXT;
            fis.set_lba(lba);
            fis.set_count(count);

            // In real implementation:
            // 1. Setup command header
            // 2. Setup command table with FIS
            // 3. Setup PRDT
            // 4. Issue command (write to CI)
            // 5. Wait for completion

            self.wait_completion(slot)
        }
    }

    /// Write sectors
    ///
    /// # Safety
    /// Port must be started.
    pub unsafe fn write_sectors(&self, lba: u64, count: u16, _buf: &[u8]) -> Result<(), ()> {
        unsafe {
            let slot = self.find_free_slot().ok_or(())?;

            // Build FIS
            let mut fis = FisRegH2D::new();
            fis.command = ata_cmd::WRITE_DMA_EXT;
            fis.set_lba(lba);
            fis.set_count(count);

            // Similar to read_sectors

            self.wait_completion(slot)
        }
    }
}

/// Identify device data (partial)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IdentifyData {
    /// General configuration
    pub config: u16,
    _reserved1: [u16; 9],
    /// Serial number
    pub serial: [u8; 20],
    _reserved2: [u16; 3],
    /// Firmware revision
    pub firmware: [u8; 8],
    /// Model number
    pub model: [u8; 40],
    _reserved3: [u16; 13],
    /// Capabilities
    pub capabilities: [u16; 2],
    _reserved4: [u16; 8],
    /// Total sectors (28-bit)
    pub total_sectors_28: u32,
    _reserved5: [u16; 17],
    /// Total sectors (48-bit)
    pub total_sectors_48: u64,
    // ... more fields
}
