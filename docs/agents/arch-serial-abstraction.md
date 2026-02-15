# Agent Rule: Serial Output MUST Use arch Abstractions

## Rule
**NEVER write inline assembly or direct port I/O for serial output anywhere
outside the arch package.** Use the architecture-abstracted serial API instead.

## Correct Pattern
```rust
use core::fmt::Write;
let _ = write!(arch::serial_writer(), "[DEBUG] message\r\n");
```

Or for single bytes:
```rust
arch::serial_write_byte(b'X');
```

## Wrong Patterns (NEVER do these)
```rust
// WRONG: inline asm for port I/O
core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") byte);

// WRONG: direct port I/O in non-arch code
arch::outb(0x3F8, byte);
while arch::inb(0x3FD) & 0x20 == 0 {}

// WRONG: custom serial_trace functions with raw asm in driver crates
fn serial_trace(msg: &[u8]) {
    for &b in msg {
        unsafe { core::arch::asm!("out dx, al", ...); }
    }
}
```

## Why
- Hardware port I/O is architecture-specific (x86 only)
- The arch package provides properly abstracted serial write with THRE wait
- `arch::serial_writer()` returns `SerialWriter` implementing `core::fmt::Write`
- This allows building for multi-arch targets (x86_64, aarch64, mips64)
- The arch package handles bounded spin waits to prevent system hangs

## Where Serial Code Belongs
- `kernel/arch/arch-x86_64/src/serial.rs` — x86 implementation
- `kernel/src/arch.rs` — architecture-agnostic wrappers
- Driver crates should NOT have any serial output code

— SableWire: Architecture isolation exists for a reason. The day we port to
  ARM64, every raw `out 0x3F8` will be a triple-fault waiting to happen.
