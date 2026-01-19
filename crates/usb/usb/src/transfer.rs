//! USB Transfer Management

pub use crate::descriptor::{EndpointDescriptor, TransferType};

/// Transfer direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    /// Host to device
    Out,
    /// Device to host
    In,
}

/// USB transfer request
pub struct UsbTransfer {
    /// Endpoint address
    pub endpoint: u8,
    /// Transfer type
    pub transfer_type: TransferType,
    /// Direction
    pub direction: TransferDirection,
    /// Data buffer offset
    pub buffer_offset: usize,
    /// Data length
    pub length: usize,
    /// Actual transferred length
    pub actual_length: usize,
    /// Status
    pub status: TransferStatus,
}

/// Transfer status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStatus {
    /// Not yet submitted
    Pending,
    /// In progress
    InProgress,
    /// Completed successfully
    Completed,
    /// Stall/halt condition
    Stall,
    /// Timeout
    Timeout,
    /// Data buffer error
    DataBuffer,
    /// Babble detected
    Babble,
    /// CRC/data error
    CrcError,
    /// Short packet (not an error for some transfers)
    ShortPacket,
    /// Cancelled
    Cancelled,
    /// Unknown error
    Error,
}

impl UsbTransfer {
    /// Create a new transfer
    pub fn new(
        endpoint: u8,
        transfer_type: TransferType,
        direction: TransferDirection,
        length: usize,
    ) -> Self {
        UsbTransfer {
            endpoint,
            transfer_type,
            direction,
            buffer_offset: 0,
            length,
            actual_length: 0,
            status: TransferStatus::Pending,
        }
    }

    /// Create a control transfer
    pub fn control(direction: TransferDirection, length: usize) -> Self {
        UsbTransfer::new(0, TransferType::Control, direction, length)
    }

    /// Create a bulk transfer
    pub fn bulk(endpoint: u8, direction: TransferDirection, length: usize) -> Self {
        UsbTransfer::new(endpoint, TransferType::Bulk, direction, length)
    }

    /// Create an interrupt transfer
    pub fn interrupt(endpoint: u8, direction: TransferDirection, length: usize) -> Self {
        UsbTransfer::new(endpoint, TransferType::Interrupt, direction, length)
    }

    /// Check if transfer completed successfully
    pub fn is_complete(&self) -> bool {
        self.status == TransferStatus::Completed
    }

    /// Check if transfer had an error
    pub fn is_error(&self) -> bool {
        matches!(
            self.status,
            TransferStatus::Stall
                | TransferStatus::Timeout
                | TransferStatus::DataBuffer
                | TransferStatus::Babble
                | TransferStatus::CrcError
                | TransferStatus::Error
        )
    }
}
