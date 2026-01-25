//! USB Mass Storage Class Driver
//!
//! Implements the USB Mass Storage Bulk-Only Transport (BOT) protocol.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;
use usb::descriptor::TransferType;
use usb::{TransferDirection, UsbClassDriver, UsbDevice, UsbError, UsbResult};

/// Mass storage class code
pub const USB_CLASS_MASS_STORAGE: u8 = 0x08;

/// Mass storage subclass codes
pub mod subclass {
    /// SCSI command set not reported
    pub const SCSI_NOT_REPORTED: u8 = 0x00;
    /// RBC (Reduced Block Commands)
    pub const RBC: u8 = 0x01;
    /// MMC-5 (ATAPI)
    pub const ATAPI: u8 = 0x02;
    /// UFI (USB Floppy Interface)
    pub const UFI: u8 = 0x04;
    /// SCSI transparent command set
    pub const SCSI: u8 = 0x06;
    /// LSD FS
    pub const LSDFS: u8 = 0x07;
    /// IEEE 1667
    pub const IEEE1667: u8 = 0x08;
}

/// Mass storage protocol codes
pub mod protocol {
    /// Control/Bulk/Interrupt with command completion interrupt
    pub const CBI_INTERRUPT: u8 = 0x00;
    /// Control/Bulk/Interrupt without command completion interrupt
    pub const CBI: u8 = 0x01;
    /// Bulk-Only Transport
    pub const BBB: u8 = 0x50;
    /// UAS (USB Attached SCSI)
    pub const UAS: u8 = 0x62;
}

/// Mass storage class requests
pub mod request {
    /// Get max LUN
    pub const GET_MAX_LUN: u8 = 0xFE;
    /// Bulk-only mass storage reset
    pub const BULK_ONLY_RESET: u8 = 0xFF;
}

/// Command Block Wrapper signature
const CBW_SIGNATURE: u32 = 0x43425355;
/// Command Status Wrapper signature
const CSW_SIGNATURE: u32 = 0x53425355;

/// Command Block Wrapper (CBW)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CommandBlockWrapper {
    /// Signature (0x43425355)
    pub signature: u32,
    /// Tag (echoed in CSW)
    pub tag: u32,
    /// Data transfer length
    pub data_transfer_length: u32,
    /// Flags (bit 7: direction, 0=OUT, 1=IN)
    pub flags: u8,
    /// LUN (bits 0-3)
    pub lun: u8,
    /// Command block length (1-16)
    pub cb_length: u8,
    /// Command block (SCSI command)
    pub cb: [u8; 16],
}

impl CommandBlockWrapper {
    /// Create a new CBW
    pub fn new(tag: u32, data_len: u32, direction_in: bool, lun: u8, command: &[u8]) -> Self {
        let mut cb = [0u8; 16];
        let len = command.len().min(16);
        cb[..len].copy_from_slice(&command[..len]);

        CommandBlockWrapper {
            signature: CBW_SIGNATURE,
            tag,
            data_transfer_length: data_len,
            flags: if direction_in { 0x80 } else { 0x00 },
            lun,
            cb_length: len as u8,
            cb,
        }
    }

    /// Convert to bytes
    pub fn to_bytes(&self) -> [u8; 31] {
        let mut bytes = [0u8; 31];
        bytes[0..4].copy_from_slice(&self.signature.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.tag.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.data_transfer_length.to_le_bytes());
        bytes[12] = self.flags;
        bytes[13] = self.lun;
        bytes[14] = self.cb_length;
        bytes[15..31].copy_from_slice(&self.cb);
        bytes
    }
}

/// Command Status Wrapper (CSW)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CommandStatusWrapper {
    /// Signature (0x53425355)
    pub signature: u32,
    /// Tag (from CBW)
    pub tag: u32,
    /// Data residue
    pub data_residue: u32,
    /// Status (0=passed, 1=failed, 2=phase error)
    pub status: u8,
}

impl CommandStatusWrapper {
    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 13 {
            return None;
        }

        let signature = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if signature != CSW_SIGNATURE {
            return None;
        }

        Some(CommandStatusWrapper {
            signature,
            tag: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            data_residue: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            status: bytes[12],
        })
    }

    /// Check if command passed
    pub fn passed(&self) -> bool {
        self.status == 0
    }

    /// Check if command failed
    pub fn failed(&self) -> bool {
        self.status == 1
    }

    /// Check for phase error
    pub fn phase_error(&self) -> bool {
        self.status == 2
    }
}

/// CSW status codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CswStatus {
    /// Command passed
    Passed,
    /// Command failed
    Failed,
    /// Phase error (need reset)
    PhaseError,
    /// Unknown status
    Unknown(u8),
}

impl From<u8> for CswStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => CswStatus::Passed,
            1 => CswStatus::Failed,
            2 => CswStatus::PhaseError,
            n => CswStatus::Unknown(n),
        }
    }
}

/// SCSI commands
pub mod scsi {
    pub const TEST_UNIT_READY: u8 = 0x00;
    pub const REQUEST_SENSE: u8 = 0x03;
    pub const INQUIRY: u8 = 0x12;
    pub const MODE_SENSE_6: u8 = 0x1A;
    pub const START_STOP_UNIT: u8 = 0x1B;
    pub const PREVENT_ALLOW_MEDIUM_REMOVAL: u8 = 0x1E;
    pub const READ_CAPACITY_10: u8 = 0x25;
    pub const READ_10: u8 = 0x28;
    pub const WRITE_10: u8 = 0x2A;
    pub const SYNCHRONIZE_CACHE_10: u8 = 0x35;
    pub const READ_CAPACITY_16: u8 = 0x9E;
    pub const READ_16: u8 = 0x88;
    pub const WRITE_16: u8 = 0x8A;
}

/// SCSI inquiry response
#[derive(Debug, Clone)]
pub struct InquiryResponse {
    /// Peripheral device type
    pub device_type: u8,
    /// Removable media bit
    pub removable: bool,
    /// Vendor identification
    pub vendor: [u8; 8],
    /// Product identification
    pub product: [u8; 16],
    /// Product revision
    pub revision: [u8; 4],
}

impl InquiryResponse {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 36 {
            return None;
        }

        let mut vendor = [0u8; 8];
        let mut product = [0u8; 16];
        let mut revision = [0u8; 4];

        vendor.copy_from_slice(&data[8..16]);
        product.copy_from_slice(&data[16..32]);
        revision.copy_from_slice(&data[32..36]);

        Some(InquiryResponse {
            device_type: data[0] & 0x1F,
            removable: data[1] & 0x80 != 0,
            vendor,
            product,
            revision,
        })
    }
}

/// Read capacity response
#[derive(Debug, Clone, Copy)]
pub struct ReadCapacityResponse {
    /// Last logical block address
    pub last_lba: u64,
    /// Block size in bytes
    pub block_size: u32,
}

/// USB mass storage device
pub struct UsbMassStorage {
    /// USB device
    device: Arc<UsbDevice>,
    /// Bulk IN endpoint
    bulk_in: u8,
    /// Bulk OUT endpoint
    bulk_out: u8,
    /// Max packet size
    max_packet_size: u16,
    /// Max LUN
    max_lun: u8,
    /// Tag counter
    tag: AtomicU32,
    /// Device capacity
    capacity: Mutex<Option<ReadCapacityResponse>>,
}

impl UsbMassStorage {
    /// Create a new mass storage device
    pub fn new(device: Arc<UsbDevice>) -> UsbResult<Self> {
        // Find bulk endpoints
        let config = device.configuration().ok_or(UsbError::NotConfigured)?;

        let mut bulk_in = None;
        let mut bulk_out = None;
        let mut max_packet_size = 64u16;

        for interface in &config.interfaces {
            if interface.interface_class == USB_CLASS_MASS_STORAGE {
                for endpoint in &interface.endpoints {
                    if endpoint.transfer_type() == TransferType::Bulk {
                        if endpoint.is_in() {
                            bulk_in = Some(endpoint.endpoint_address);
                            max_packet_size = endpoint.max_packet_size;
                        } else {
                            bulk_out = Some(endpoint.endpoint_address);
                        }
                    }
                }
            }
        }

        let bulk_in = bulk_in.ok_or(UsbError::InvalidEndpoint)?;
        let bulk_out = bulk_out.ok_or(UsbError::InvalidEndpoint)?;

        // Get max LUN
        let max_lun = Self::get_max_lun_static(&device).unwrap_or(0);

        Ok(UsbMassStorage {
            device,
            bulk_in,
            bulk_out,
            max_packet_size,
            max_lun,
            tag: AtomicU32::new(1),
            capacity: Mutex::new(None),
        })
    }

    /// Get max LUN (static helper)
    fn get_max_lun_static(device: &Arc<UsbDevice>) -> UsbResult<u8> {
        let mut buf = [0u8; 1];
        device.control_transfer(
            0xA1, // Class, interface, IN
            request::GET_MAX_LUN,
            0,
            0,
            Some(&mut buf),
        )?;
        Ok(buf[0])
    }

    /// Get max LUN
    pub fn max_lun(&self) -> u8 {
        self.max_lun
    }

    /// Perform bulk-only mass storage reset
    pub fn reset(&self) -> UsbResult<()> {
        self.device.control_transfer(
            0x21, // Class, interface, OUT
            request::BULK_ONLY_RESET,
            0,
            0,
            None,
        )?;
        Ok(())
    }

    /// Get next tag
    fn next_tag(&self) -> u32 {
        self.tag.fetch_add(1, Ordering::SeqCst)
    }

    /// Execute SCSI command
    pub fn execute_command(
        &self,
        lun: u8,
        command: &[u8],
        data: Option<&mut [u8]>,
        direction_in: bool,
    ) -> UsbResult<u32> {
        let data_len = data.as_ref().map(|d| d.len()).unwrap_or(0) as u32;
        let tag = self.next_tag();

        // Send CBW
        let cbw = CommandBlockWrapper::new(tag, data_len, direction_in, lun, command);
        let cbw_bytes = cbw.to_bytes();
        let mut cbw_buf = cbw_bytes;
        self.device
            .bulk_transfer(self.bulk_out, &mut cbw_buf, TransferDirection::Out)?;

        // Data phase (if any)
        if let Some(buf) = data {
            if direction_in {
                self.device
                    .bulk_transfer(self.bulk_in, buf, TransferDirection::In)?;
            } else {
                self.device
                    .bulk_transfer(self.bulk_out, buf, TransferDirection::Out)?;
            }
        }

        // Receive CSW
        let mut csw_bytes = [0u8; 13];
        self.device
            .bulk_transfer(self.bulk_in, &mut csw_bytes, TransferDirection::In)?;

        let csw = CommandStatusWrapper::from_bytes(&csw_bytes).ok_or(UsbError::ProtocolError)?;

        if csw.tag != tag {
            return Err(UsbError::ProtocolError);
        }

        if csw.phase_error() {
            self.reset()?;
            return Err(UsbError::ProtocolError);
        }

        if csw.failed() {
            return Err(UsbError::DeviceError);
        }

        Ok(csw.data_residue)
    }

    /// Test unit ready
    pub fn test_unit_ready(&self, lun: u8) -> UsbResult<bool> {
        let cmd = [scsi::TEST_UNIT_READY, 0, 0, 0, 0, 0];
        match self.execute_command(lun, &cmd, None, false) {
            Ok(_) => Ok(true),
            Err(UsbError::DeviceError) | Err(UsbError::TransferError) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// SCSI Inquiry
    pub fn inquiry(&self, lun: u8) -> UsbResult<InquiryResponse> {
        let cmd = [scsi::INQUIRY, 0, 0, 0, 36, 0];
        let mut data = [0u8; 36];
        self.execute_command(lun, &cmd, Some(&mut data), true)?;
        InquiryResponse::from_bytes(&data).ok_or(UsbError::InvalidDescriptor)
    }

    /// Read capacity (10-byte)
    pub fn read_capacity_10(&self, lun: u8) -> UsbResult<ReadCapacityResponse> {
        let cmd = [scsi::READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut data = [0u8; 8];
        self.execute_command(lun, &cmd, Some(&mut data), true)?;

        let last_lba = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as u64;
        let block_size = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

        let response = ReadCapacityResponse {
            last_lba,
            block_size,
        };
        *self.capacity.lock() = Some(response);

        Ok(response)
    }

    /// Read capacity (16-byte for large disks)
    pub fn read_capacity_16(&self, lun: u8) -> UsbResult<ReadCapacityResponse> {
        let cmd = [
            scsi::READ_CAPACITY_16,
            0x10, // Service action
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            32, // Allocation length
            0,
            0,
        ];
        let mut data = [0u8; 32];
        self.execute_command(lun, &cmd, Some(&mut data), true)?;

        let last_lba = u64::from_be_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);
        let block_size = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);

        let response = ReadCapacityResponse {
            last_lba,
            block_size,
        };
        *self.capacity.lock() = Some(response);

        Ok(response)
    }

    /// Read blocks (10-byte command)
    pub fn read_10(&self, lun: u8, lba: u32, blocks: u16, buffer: &mut [u8]) -> UsbResult<()> {
        let cmd = [
            scsi::READ_10,
            0,
            (lba >> 24) as u8,
            (lba >> 16) as u8,
            (lba >> 8) as u8,
            lba as u8,
            0,
            (blocks >> 8) as u8,
            blocks as u8,
            0,
        ];
        self.execute_command(lun, &cmd, Some(buffer), true)?;
        Ok(())
    }

    /// Write blocks (10-byte command)
    pub fn write_10(&self, lun: u8, lba: u32, blocks: u16, buffer: &mut [u8]) -> UsbResult<()> {
        let cmd = [
            scsi::WRITE_10,
            0,
            (lba >> 24) as u8,
            (lba >> 16) as u8,
            (lba >> 8) as u8,
            lba as u8,
            0,
            (blocks >> 8) as u8,
            blocks as u8,
            0,
        ];
        self.execute_command(lun, &cmd, Some(buffer), false)?;
        Ok(())
    }

    /// Read blocks (16-byte command for large disks)
    pub fn read_16(&self, lun: u8, lba: u64, blocks: u32, buffer: &mut [u8]) -> UsbResult<()> {
        let cmd = [
            scsi::READ_16,
            0,
            (lba >> 56) as u8,
            (lba >> 48) as u8,
            (lba >> 40) as u8,
            (lba >> 32) as u8,
            (lba >> 24) as u8,
            (lba >> 16) as u8,
            (lba >> 8) as u8,
            lba as u8,
            (blocks >> 24) as u8,
            (blocks >> 16) as u8,
            (blocks >> 8) as u8,
            blocks as u8,
            0,
            0,
        ];
        self.execute_command(lun, &cmd, Some(buffer), true)?;
        Ok(())
    }

    /// Write blocks (16-byte command for large disks)
    pub fn write_16(&self, lun: u8, lba: u64, blocks: u32, buffer: &mut [u8]) -> UsbResult<()> {
        let cmd = [
            scsi::WRITE_16,
            0,
            (lba >> 56) as u8,
            (lba >> 48) as u8,
            (lba >> 40) as u8,
            (lba >> 32) as u8,
            (lba >> 24) as u8,
            (lba >> 16) as u8,
            (lba >> 8) as u8,
            lba as u8,
            (blocks >> 24) as u8,
            (blocks >> 16) as u8,
            (blocks >> 8) as u8,
            blocks as u8,
            0,
            0,
        ];
        self.execute_command(lun, &cmd, Some(buffer), false)?;
        Ok(())
    }

    /// Synchronize cache
    pub fn sync_cache(&self, lun: u8) -> UsbResult<()> {
        let cmd = [scsi::SYNCHRONIZE_CACHE_10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        self.execute_command(lun, &cmd, None, false)?;
        Ok(())
    }

    /// Request sense data
    pub fn request_sense(&self, lun: u8) -> UsbResult<Vec<u8>> {
        let cmd = [scsi::REQUEST_SENSE, 0, 0, 0, 18, 0];
        let mut data = vec![0u8; 18];
        self.execute_command(lun, &cmd, Some(&mut data), true)?;
        Ok(data)
    }

    /// Start/stop unit
    pub fn start_stop(&self, lun: u8, start: bool, load_eject: bool) -> UsbResult<()> {
        let flags = if start { 0x01 } else { 0x00 } | if load_eject { 0x02 } else { 0x00 };
        let cmd = [scsi::START_STOP_UNIT, 0, 0, 0, flags, 0];
        self.execute_command(lun, &cmd, None, false)?;
        Ok(())
    }

    /// Get cached capacity
    pub fn capacity(&self) -> Option<ReadCapacityResponse> {
        *self.capacity.lock()
    }

    /// Get block size
    pub fn block_size(&self) -> Option<u32> {
        self.capacity.lock().map(|c| c.block_size)
    }

    /// Get total blocks
    pub fn total_blocks(&self) -> Option<u64> {
        self.capacity.lock().map(|c| c.last_lba + 1)
    }
}

/// Mass storage class driver
pub struct MassStorageDriver;

impl UsbClassDriver for MassStorageDriver {
    fn name(&self) -> &str {
        "usb-mass-storage"
    }

    fn probe(&self, device: &UsbDevice) -> bool {
        if let Some(config) = device.configuration() {
            for interface in &config.interfaces {
                if interface.interface_class == USB_CLASS_MASS_STORAGE
                    && interface.interface_protocol == protocol::BBB
                {
                    return true;
                }
            }
        }
        false
    }

    fn attach(&self, device: &Arc<UsbDevice>) -> UsbResult<()> {
        let _storage = UsbMassStorage::new(device.clone())?;
        // In a full implementation, register with block device subsystem
        Ok(())
    }

    fn detach(&self, _device: &Arc<UsbDevice>) -> UsbResult<()> {
        // Unregister from block device subsystem
        Ok(())
    }
}
