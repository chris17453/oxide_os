//! MIPS64 serial output stubs
//! — NeonRoot: placeholder until SGI Zilog 8530 driver is implemented.
//! All arch crates export the same serial API so arch.rs doesn't need cfg gates.

/// Serial writer that implements core::fmt::Write
pub struct SerialWriter;

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, _s: &str) -> core::fmt::Result {
        // TODO: Zilog 8530 UART output
        Ok(())
    }
}

/// Initialize serial hardware
pub fn init() {
    // TODO: Zilog 8530 init
}

/// Write a single byte
pub fn write_byte(_byte: u8) {
    // TODO: Zilog 8530 write
}
