//! x86_64 Serial Port (8250 UART) Driver
//!
//! Provides COM1 serial port access for early boot console.

use crate::{inb, outb};
use spin::Mutex;

/// COM1 base I/O port
const COM1_PORT: u16 = 0x3F8;

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

/// Serial port state
struct SerialPort {
    base: u16,
    initialized: bool,
}

impl SerialPort {
    const fn new(base: u16) -> Self {
        Self {
            base,
            initialized: false,
        }
    }

    #[inline]
    fn read_reg(&self, reg: u16) -> u8 {
        unsafe { inb(self.base + reg) }
    }

    #[inline]
    fn write_reg(&self, reg: u16, value: u8) {
        unsafe { outb(self.base + reg, value) }
    }

    fn init(&mut self) {
        if self.initialized {
            return;
        }

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

        self.initialized = true;
    }

    fn write_byte(&self, byte: u8) {
        // Wait for transmit buffer empty
        while (self.read_reg(regs::LSR) & lsr::THRE) == 0 {
            core::hint::spin_loop();
        }
        self.write_reg(regs::DATA, byte);
    }

    fn read_byte(&self) -> Option<u8> {
        if (self.read_reg(regs::LSR) & lsr::DR) != 0 {
            Some(self.read_reg(regs::DATA))
        } else {
            None
        }
    }
}

/// Global COM1 instance
static COM1: Mutex<SerialPort> = Mutex::new(SerialPort::new(COM1_PORT));

/// Initialize COM1 serial port
pub fn init() {
    COM1.lock().init();
}

/// Write a byte to COM1
pub fn write_byte(byte: u8) {
    COM1.lock().write_byte(byte);
}

/// Write a byte to COM1 without taking a lock (for interrupt handlers)
///
/// # Safety
/// This function is not thread-safe. Only use from interrupt context
/// where you know no other code can be writing to serial at the same time,
/// or when you accept potential garbled output.
#[inline]
pub unsafe fn write_byte_unsafe(byte: u8) {
    use crate::{inb, outb};
    // Wait for transmit buffer empty
    while (inb(COM1_PORT + regs::LSR) & lsr::THRE) == 0 {
        core::hint::spin_loop();
    }
    outb(COM1_PORT + regs::DATA, byte);
}

/// Write a string to COM1 without taking a lock (for interrupt handlers)
///
/// # Safety
/// See write_byte_unsafe
#[inline]
pub unsafe fn write_str_unsafe(s: &str) {
    for byte in s.bytes() {
        if byte == b'\n' {
            write_byte_unsafe(b'\r');
        }
        write_byte_unsafe(byte);
    }
}

/// Read a byte from COM1 (non-blocking)
pub fn read_byte() -> Option<u8> {
    COM1.lock().read_byte()
}

/// Write a string to COM1
pub fn write_str(s: &str) {
    let port = COM1.lock();
    for byte in s.bytes() {
        if byte == b'\n' {
            port.write_byte(b'\r');
        }
        port.write_byte(byte);
    }
}

/// Writer for fmt::Write trait
pub struct SerialWriter;

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        write_str(s);
        Ok(())
    }
}
