//! 8250/16550 UART Driver
//!
//! Generic driver for 8250-compatible UARTs.
//! Used for COM ports on x86.

#![no_std]

use efflux_driver_traits::SerialDriver;

#[cfg(target_arch = "x86_64")]
use efflux_arch_x86_64::{inb, outb};

/// UART register offsets
mod regs {
    pub const DATA: u16 = 0;
    pub const IER: u16 = 1;
    pub const FCR: u16 = 2;
    pub const LCR: u16 = 3;
    pub const MCR: u16 = 4;
    pub const LSR: u16 = 5;
    pub const DLL: u16 = 0;
    pub const DLH: u16 = 1;
}

/// Line status register bits
mod lsr {
    pub const DR: u8 = 1 << 0;
    pub const THRE: u8 = 1 << 5;
}

/// Line control register bits
mod lcr {
    pub const DLAB: u8 = 1 << 7;
    pub const MODE_8N1: u8 = 0x03;
}

/// 8250 UART instance
pub struct Uart8250 {
    base: u16,
}

impl Uart8250 {
    /// Create a new UART instance at the given I/O port base
    pub const fn new(base: u16) -> Self {
        Self { base }
    }

    /// COM1 port
    pub const fn com1() -> Self {
        Self::new(0x3F8)
    }

    /// COM2 port
    pub const fn com2() -> Self {
        Self::new(0x2F8)
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    fn read_reg(&self, reg: u16) -> u8 {
        unsafe { inb(self.base + reg) }
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    fn write_reg(&self, reg: u16, value: u8) {
        unsafe { outb(self.base + reg, value) }
    }
}

impl SerialDriver for Uart8250 {
    fn init(&mut self) {
        // Disable interrupts
        self.write_reg(regs::IER, 0x00);

        // Set baud rate to 115200 (divisor = 1)
        self.write_reg(regs::LCR, lcr::DLAB);
        self.write_reg(regs::DLL, 1);
        self.write_reg(regs::DLH, 0);

        // 8N1 mode
        self.write_reg(regs::LCR, lcr::MODE_8N1);

        // Enable FIFO
        self.write_reg(regs::FCR, 0xC7);

        // Enable IRQs, RTS/DSR
        self.write_reg(regs::MCR, 0x0B);
    }

    fn write_byte(&mut self, byte: u8) {
        // Wait for transmit buffer empty
        while !self.tx_empty() {
            core::hint::spin_loop();
        }
        self.write_reg(regs::DATA, byte);
    }

    fn read_byte(&mut self) -> Option<u8> {
        if (self.read_reg(regs::LSR) & lsr::DR) != 0 {
            Some(self.read_reg(regs::DATA))
        } else {
            None
        }
    }

    fn tx_empty(&self) -> bool {
        (self.read_reg(regs::LSR) & lsr::THRE) != 0
    }
}
