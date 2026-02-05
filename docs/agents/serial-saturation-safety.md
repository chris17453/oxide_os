# Serial Output Saturation Safety

**Author:** SableWire
**Last audit:** 2026-02-05

## The Rule

**Every serial write function MUST have a bounded spin limit.** If the UART
transmit holding register is not empty after `SPIN_LIMIT` iterations, **drop
the byte** and return. Debug output is best-effort; a stalled CPU is not
acceptable.

The 8250 UART FIFO is 16 bytes deep. At 115200 baud (~11.5 KB/s), each byte
takes ~87 microseconds to transmit. Under heavy debug load (rapid keyboard
input, context switches, syscall tracing), the FIFO fills faster than it
drains, causing any unbounded spin to block the caller indefinitely.

## Why This Is Critical in ISR Context

If a keyboard IRQ handler writes debug output (`[KB_IRQ] CB `) and the UART
FIFO is full:
1. The ISR spins waiting for THRE (transmit holding register empty)
2. The CPU is stuck in the interrupt handler — can't run any other code
3. The code that would drain the FIFO (timer tick, other interrupts) can't run
4. **Permanent system hang**

## Canonical Write Functions

All serial output should go through one of these paths:

| Function | Location | Context | Bounded? |
|----------|----------|---------|----------|
| `serial::write_byte_unsafe()` | `arch-x86_64/serial.rs` | ISR | YES (2048 spins) |
| `serial::write_str_unsafe()` | `arch-x86_64/serial.rs` | ISR | YES (calls write_byte_unsafe) |
| `serial::write_byte()` | `arch-x86_64/serial.rs` | Syscall | YES (2048 spins) |
| `os_log::write_byte_raw()` | `os_log/lib.rs` | Any | YES (calls registered writer) |
| `os_log::write_str_raw()` | `os_log/lib.rs` | Any | YES (calls registered writer) |

## Banned Patterns

**NEVER** write inline serial I/O with unbounded spin:

```rust
// BAD — unbounded spin, will hang under serial saturation
loop {
    core::arch::asm!("in al, dx", out("al") status, in("dx") 0x3FDu16, ...);
    if status & 0x20 != 0 { break; }
}
core::arch::asm!("out dx, al", in("al") b, in("dx") 0x3F8u16, ...);
```

**Instead**, use `os_log::write_str_raw()` or `os_log::write_byte_raw()`:

```rust
// GOOD — delegates to bounded-spin registered writer
unsafe { os_log::write_str_raw("[DEBUG] message\n"); }
```

If the crate cannot depend on `os_log` (e.g., `vfs` is too foundational), use
a bounded inline spin:

```rust
fn write_byte(b: u8) {
    const SPIN_LIMIT: u32 = 2048;
    unsafe {
        let mut status: u8;
        let mut spins: u32 = 0;
        loop {
            core::arch::asm!("in al, dx", out("al") status, in("dx") 0x3FDu16, ...);
            if status & 0x20 != 0 { break; }
            spins += 1;
            if spins >= SPIN_LIMIT { return; }
        }
        core::arch::asm!("out dx, al", in("al") b, in("dx") 0x3F8u16, ...);
    }
}
```

## Debug Output Volume

With `debug-all` enabled, each keystroke generates ~15 serial writes:
- `[KB_IRQ] CB ` (ISR, ~12 bytes)
- `[SWITCH] N(name)->N(name) cs=kern` (ISR, ~40 bytes × N switches)
- `[CON] read() enter`, `read() -> backend` (syscall, ~40 bytes)
- `[VT] read() enter`, `Drained N bytes` (syscall, ~60 bytes)
- `[TTY-READ] queue_len=N ...` (syscall, ~50 bytes)
- `[CON] write() enter/done` (syscall, ~50 bytes)
- `[VT] write() enter/done` (syscall, ~50 bytes)
- `[SYSCALL] read/write took N cycles` (syscall, ~60 bytes)

Total: **~300+ bytes per keystroke**. At 115200 baud, this limits
interactive typing to ~3-4 keys/second before saturation. Rapid typing
WILL saturate the serial port and drop debug bytes. This is by design.

## Files Audited

| File | Status | Notes |
|------|--------|-------|
| `kernel/arch/arch-x86_64/src/serial.rs` | **SAFE** | Bounded spin (2048) |
| `kernel/tty/vt/src/lib.rs` | **SAFE** | Delegates to `os_log::write_str_raw()` |
| `kernel/vfs/devfs/src/devices.rs` | **SAFE** | Delegates to `os_log::write_str_raw()` |
| `kernel/vfs/vfs/src/file.rs` | **SAFE** | Inline bounded spin (2048) |
| `kernel/syscall/syscall/src/vfs.rs` | **SAFE** | Inline bounded spin (2048, 4 copies) |
| `kernel/core/os_log/src/lib.rs` | **SAFE** | Calls registered writer (bounded) |
| `kernel/src/scheduler.rs` | **SAFE** | Uses `write_str_unsafe` / `write_byte_unsafe` (bounded) |
