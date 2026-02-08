# Serial Output Elimination

## Issue
Direct serial port writes (to COM1/0x3F8) were bypassing the os_log/console infrastructure, causing kernel messages to appear only on the serial port instead of stderr/console.

## Root Causes

### 1. PS2 Driver Raw Serial Access
**Location:** `kernel/drivers/input/ps2/src/lib.rs`

The PS2 driver had a `serial_debug()` function that:
- Wrote directly to UART port 0x3F8
- Used **unbounded spin loop** waiting for THRE (transmit holding register empty)
- Bypassed the entire os_log → console routing system

**Violation:** UART bounded spin rule — serial writes must have iteration limits to prevent system hangs.

### 2. Bootloader Verbose Logging
**Location:** `bootloader/boot-uefi/src/main.rs`

The bootloader logged ACPI RSDP discovery via `log_fmt()` which writes to UEFI stdout. The firmware routes UEFI stdout to serial, causing pre-kernel messages to appear on serial.

## Solution

### PS2 Driver
1. **Removed** `serial_debug()` function entirely
2. **Added** `os_log` dependency to `kernel/drivers/input/ps2/Cargo.toml`
3. **Replaced** all `serial_debug()` calls with `os_log::println!()`
   - Example: `serial_debug(b"[PS2] init\r\n")` → `println!("[PS2] init")`

**Why this works:**
- `os_log::println!()` routes through the registered console writer
- Console writer sends output to terminal/framebuffer (stderr path)
- No direct serial port access

### Bootloader
1. **Removed** verbose `log_fmt()` calls for ACPI discovery
2. Silent ACPI detection — success indicated by kernel boot, failure handled gracefully

## Architecture

```
┌──────────────────────────────────────────────────┐
│ Application / Kernel Code                        │
│   os_log::println!("message")                    │
└────────────────┬─────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────┐
│ os_log (kernel/core/os_log)                      │
│  • Normal path: Mutex-protected writer           │
│  • Unsafe path: Lock-free ISR-safe writer        │
└────────────────┬─────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────┐
│ OsLogConsoleWriter (kernel/src/init.rs)          │
│  • Routes to terminal::write()                   │
│  • Appears on framebuffer/VT                     │
└──────────────────────────────────────────────────┘
```

## Rules for Future Development

1. **NEVER write directly to serial ports** (0x3F8, 0x2F8, etc.)
   - Exception: `arch/arch-x86_64/src/serial.rs` unsafe writer registration only

2. **Use os_log macros:**
   - Normal context: `os_log::println!()`, `os_log::info!()`, `os_log::warn!()`
   - ISR context: `os_log::println_unsafe!()`, `unsafe { os_log::write_str_raw() }`

3. **Driver logging:**
   - Add `os_log = { path = "../../../core/os_log" }` to Cargo.toml
   - Import: `use os_log::println;`
   - Log liberally — it goes to console/stderr, not serial

4. **Bootloader logging:**
   - Keep minimal for production
   - Error messages only (`log_fmt()` for fatal errors)
   - Silent success paths

## Testing
After this change, all PS2 init messages appear on the terminal/console (via os_log → console writer), not on the serial port.

Boot sequence:
1. Bootloader (silent ACPI detection)
2. Kernel init (os_log → console)
3. PS2 init (os_log → console)
4. Userspace (stdout/stderr → terminal)

— PatchBay: Serial port is officially dead. Long live stderr.
