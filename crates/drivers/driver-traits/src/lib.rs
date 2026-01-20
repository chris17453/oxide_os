//! OXIDE Driver Traits
//!
//! Defines interfaces for device drivers.

#![no_std]

/// Serial port driver trait
pub trait SerialDriver: Send {
    /// Initialize the driver
    fn init(&mut self);

    /// Write a byte
    fn write_byte(&mut self, byte: u8);

    /// Read a byte (non-blocking)
    fn read_byte(&mut self) -> Option<u8>;

    /// Check if transmit buffer is empty
    fn tx_empty(&self) -> bool;
}
