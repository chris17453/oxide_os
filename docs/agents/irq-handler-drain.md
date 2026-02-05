# IRQ Handler Data Drain Rule

## Rule
**All hardware IRQ handlers MUST drain the data port BEFORE acquiring any locks, even when no driver is registered.**

## Why
The 8042 PS/2 controller (and similar hardware) uses level-triggered interrupts via the IOAPIC. When data arrives in the output buffer, the interrupt line is asserted and stays asserted until the data is read from port 0x60.

If the IRQ handler sends EOI without reading the data byte:
1. The output buffer remains full
2. The interrupt line stays asserted (level-triggered)
3. After EOI, the APIC immediately re-delivers the interrupt
4. **Result: infinite interrupt storm that starves all normal code**

## Critical: Drain BEFORE Locks

The data port MUST be read **before** any spinlock acquisition. If the handler tries to acquire a lock first and the lock is contended (e.g., init code holds it), the handler spins forever — AND the data port is never drained — creating an unrecoverable deadlock + IRQ storm combo.

Use `try_lock()` instead of `.lock()` in IRQ handlers to avoid spinning. If the lock is contended, the byte is already drained so it's safe to drop.

## Pattern

```rust
// CORRECT: Drain first, then try_lock
pub fn handle_irq() {
    let byte = unsafe { inb(DATA_PORT) }; // ALWAYS drain first
    if let Some(guard) = DEVICE.try_lock() {
        if let Some(dev) = guard.as_ref() {
            dev.handle_byte(byte);
        }
    }
    // If lock contended or device None: byte drained, IRQ cleared, move on
}

// CORRECT: Arch-level handler with callback
extern "C" fn handle_irq() {
    unsafe {
        if let Some(callback) = CALLBACK {
            callback(); // callback drains port 0x60 first
        } else {
            // No driver yet — drain the byte to clear the interrupt
            core::arch::asm!("in al, 0x60", out("al") _, options(nomem, nostack, preserves_flags));
        }
    }
    apic::end_of_interrupt();
}

// WRONG: Lock before drain — deadlocks if lock is contended
pub fn handle_irq() {
    if let Some(dev) = DEVICE.lock().as_ref() { // spins if locked!
        let byte = unsafe { inb(DATA_PORT) };   // never reached
        dev.handle_byte(byte);
    }
    // Data port never read when DEVICE is None → IRQ storm
}
```

## Init Safety: cli/sti

During controller initialization, disable ALL interrupts with `cli` to prevent any IRQ handler from racing with polled `read_data()` calls. Re-enable with `sti` after init is complete.

Without this, the keyboard IRQ handler's drain path steals controller config bytes and mouse responses from the output buffer, causing:
- `read_data()` returns None → config = 0 → wrong config written
- Translation bit (bit 6) cleared → wrong scancodes
- Mouse ACK/self-test missed → protocol desync

## Applies To
- PS/2 keyboard handler (IRQ 1 / vector 33) — arch handler drains in fallback; driver handler drains first then try_lock
- PS/2 mouse handler (IRQ 12 / vector 44) — same pattern
- Any future hardware IRQ that uses level-triggered delivery with shared data ports

## Files
- `kernel/arch/arch-x86_64/src/exceptions.rs` — `handle_mouse()`, `handle_keyboard()`
- `kernel/drivers/input/ps2/src/lib.rs` — `handle_keyboard_irq()`, `handle_mouse_irq()`, `init()` (cli/sti)

## Related
- `docs/agents/syscall-register-clobber.md`
- `docs/agents/syscall-register-restore.md`
