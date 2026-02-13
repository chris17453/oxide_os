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
        // Bounded wait for transmit buffer empty — under heavy debug load
        // the UART FIFO can saturate. Rather than stalling the calling task
        // indefinitely, drop the byte after SPIN_LIMIT iterations.
        // — SableWire: debug output is best-effort; system liveness is not.
        let mut spins: u32 = 0;
        while (self.read_reg(regs::LSR) & lsr::THRE) == 0 {
            spins += 1;
            if spins >= UNSAFE_WRITE_SPIN_LIMIT {
                return;
            }
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
/// Maximum spin iterations before dropping a byte in ISR context.
/// At 115200 baud a single byte takes ~87µs; 2048 spins is generous
/// but prevents permanent ISR stall when the FIFO is saturated.
/// — SableWire: ISR must never block. Drop the byte, not the system.
const UNSAFE_WRITE_SPIN_LIMIT: u32 = 2048;

#[inline]
pub unsafe fn write_byte_unsafe(byte: u8) {
    // — PatchBay: SERIAL IS DEPRECATED. This function kept for compatibility
    // but should not be called. All output goes through os_log to console.
    // If you see this being called, FIX THE CALLER.
    use crate::{inb, outb};
    unsafe {
        let mut spins: u32 = 0;
        while (inb(COM1_PORT + regs::LSR) & lsr::THRE) == 0 {
            spins += 1;
            if spins >= UNSAFE_WRITE_SPIN_LIMIT {
                return;
            }
            core::hint::spin_loop();
        }
        outb(COM1_PORT + regs::DATA, byte);
    }
}

/// Write a string to COM1 without taking a lock (for interrupt handlers)
///
/// # Safety
/// See write_byte_unsafe
#[inline]
pub unsafe fn write_str_unsafe(s: &str) {
    for byte in s.bytes() {
        // SAFETY: Caller ensures ISR context with exclusive access; write_byte_unsafe upholds same guarantee
        // — SableWire
        unsafe {
            if byte == b'\n' {
                write_byte_unsafe(b'\r');
            }
            write_byte_unsafe(byte);
        }
    }
}

/// Write a u32 decimal to COM1 without locks (ISR/boot-safe)
///
/// — SableWire: Used by start_timer and other lock-free paths
///
/// # Safety
/// See write_byte_unsafe
#[inline]
pub unsafe fn write_u32_unsafe(n: u32) {
    unsafe {
        if n == 0 {
            write_byte_unsafe(b'0');
            return;
        }
        let mut buf = [0u8; 10];
        let mut v = n;
        let mut pos = 0;
        while v > 0 {
            buf[pos] = b'0' + (v % 10) as u8;
            v /= 10;
            pos += 1;
        }
        for i in (0..pos).rev() {
            write_byte_unsafe(buf[i]);
        }
    }
}

/// Write a u64 as hex (0x prefix) to COM1 without locks (ISR/boot-safe)
///
/// — SableWire: Bounded spin per byte. Use for FATAL/trace addresses in
/// ISR context where you can't afford a mutex. Drops bytes rather than hangs.
///
/// # Safety
/// See write_byte_unsafe
#[inline]
pub unsafe fn write_u64_hex_unsafe(n: u64) {
    unsafe {
        write_byte_unsafe(b'0');
        write_byte_unsafe(b'x');
        // — SableWire: Skip leading zeros, print at least one digit
        let mut started = false;
        for i in (0..16).rev() {
            let nibble = ((n >> (i * 4)) & 0xF) as u8;
            if nibble != 0 {
                started = true;
            }
            if started || i == 0 {
                let ch = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                write_byte_unsafe(ch);
            }
        }
    }
}

/// Read a byte from COM1 (non-blocking)
pub fn read_byte() -> Option<u8> {
    COM1.lock().read_byte()
}

/// Read a byte from COM1 without taking a lock (for interrupt handlers)
///
/// # Safety
/// This function is not thread-safe. Use from interrupt context only,
/// where you know no other interrupt-level code reads serial simultaneously.
#[inline]
pub unsafe fn read_byte_unsafe() -> Option<u8> {
    use crate::inb;
    // SAFETY: Direct port I/O; caller ensures ISR context with no concurrent reads
    // — SableWire
    unsafe {
        if (inb(COM1_PORT + regs::LSR) & lsr::DR) != 0 {
            Some(inb(COM1_PORT + regs::DATA))
        } else {
            None
        }
    }
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
