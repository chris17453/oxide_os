# UART Bounded Spin Safety

**Rule:** UART TX wait loops MUST have a bounded iteration limit. Never spin indefinitely waiting for UART THRE (Transmit Holding Register Empty).

## Problem

The generic UART8250 driver had an unbounded while loop waiting for the TX buffer to empty:

```rust
// DANGEROUS - can hang system indefinitely
fn write_byte(&mut self, byte: u8) {
    while !self.tx_empty() {
        core::hint::spin_loop();
    }
    self.write_reg(regs::DATA, byte);
}
```

When serial output saturates (e.g., heavy debug logging), the FIFO backs up and this loop blocks the entire system. No other code can run - including interrupt handlers, the scheduler, or critical kernel functions.

## Solution

Add a spin limit constant and bail after exceeding it:

```rust
const UART_TX_SPIN_LIMIT: u32 = 2048;

fn write_byte(&mut self, byte: u8) {
    let mut spins: u32 = 0;
    while !self.tx_empty() {
        spins += 1;
        if spins >= UART_TX_SPIN_LIMIT {
            return; // Drop byte rather than hang system
        }
        core::hint::spin_loop();
    }
    self.write_reg(regs::DATA, byte);
}
```

## Rationale

At 115200 baud, one byte takes ~87 microseconds to transmit. With 2048 spins at ~50ns each (rough estimate for spin_loop hint), we wait ~100 microseconds - enough time for one byte to drain from the FIFO. If it's still full after that, the output queue is backed up and dropping bytes is the correct behavior.

**Debug output is best-effort; system liveness is sacred.**

## Files

- `kernel/drivers/serial/driver-uart-8250/src/lib.rs` - Generic UART8250 driver (FIXED)
- `kernel/arch/arch-x86_64/src/serial.rs` - Architecture-specific serial (already correct)

## See Also

- `docs/agents/serial-saturation-safety.md` - Related serial write safety rules

— BlackLatch: "An infinite loop in a debug output path is the universe's way of punishing hubris. We write bounded loops because we've been burned before."
