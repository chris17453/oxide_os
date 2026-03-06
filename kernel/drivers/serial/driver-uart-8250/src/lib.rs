//! 8250/16550 UART Driver
//!
//! Generic driver for 8250-compatible UARTs.
//! Used for COM ports on x86.

#![no_std]

use driver_traits::SerialDriver;

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

/// — GraveShift: Max spins waiting for UART TX ready before we drop the byte.
/// Debug output is best-effort; system liveness is sacred. At 115200 baud,
/// one byte takes ~87us. 2048 spins at ~50ns each = ~100us - enough for one
/// byte to drain, but we bail if FIFO is truly backed up.
const UART_TX_SPIN_LIMIT: u32 = 2048;

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

    // — GraveShift: port I/O through os_core hooks now. The cfg-gated arch
    // imports are cremated — os_core owns the instructions, we just read/write.
    #[inline]
    fn read_reg(&self, reg: u16) -> u8 {
        unsafe { os_core::inb(self.base + reg) }
    }

    #[inline]
    fn write_reg(&self, reg: u16, value: u8) {
        unsafe { os_core::outb(self.base + reg, value) }
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
        // — BlackLatch: BOUNDED spin - never hang the system for debug output
        // If UART backs up, drop the byte. System liveness > debug completeness.
        let mut spins: u32 = 0;
        while !self.tx_empty() {
            spins += 1;
            if spins >= UART_TX_SPIN_LIMIT {
                // — GraveShift: FIFO saturated, drop byte and move on
                return;
            }
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
