//! ARM64 serial output stubs
//! — NeonRoot: placeholder until UART/PL011 driver is implemented.
//! All arch crates export the same serial API so arch.rs doesn't need cfg gates.

/// Serial writer that implements core::fmt::Write
pub struct SerialWriter;

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, _s: &str) -> core::fmt::Result {
        // TODO: PL011 UART output
        Ok(())
    }
}

/// Initialize serial hardware
pub fn init() {
    // TODO: PL011 UART init
}

/// Write a single byte
pub fn write_byte(_byte: u8) {
    // TODO: PL011 UART write
}
