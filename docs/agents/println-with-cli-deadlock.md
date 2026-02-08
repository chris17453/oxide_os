# CRITICAL: Never call println!() with Interrupts Disabled

## The Deadlock Bug

**NEVER** call `println!()`, `os_log::println!()`, or any logging macro while interrupts are disabled (`cli()`). This causes a **guaranteed deadlock**.

## Why It Deadlocks

```rust
unsafe { cli(); }  // Disable interrupts

println!("message");  // DEADLOCK!
//  ↓
// os_log::println!() → LogWriter::write_str()
//  ↓
// WRITER.lock() → tries to acquire os_log WRITER mutex
//  ↓
// OsLogConsoleWriter::write_str()
//  ↓
// terminal::write() → TERMINAL.lock()
//  ↓
// Mutex::lock() spins waiting for lock
//  ↓
// But lock holder needs interrupts to release!
//  ↓
// DEADLOCK - spin forever

unsafe { sti(); }  // Never reached
```

## The Bug in PS2 Driver

**Before (DEADLOCK):**
```rust
pub fn init() -> bool {
    unsafe { cli(); }  // Disable interrupts

    println!("[PS2] init: starting");  // DEADLOCK!
    // ... 15 more println! calls ...

    unsafe { sti(); }  // Never reached
    true
}
```

**After (FIXED):**
```rust
pub fn init() -> bool {
    unsafe { cli(); }  // Disable interrupts

    // Do hardware init (no println!)
    let kbd_ok = keyboard.init();
    let mouse_ok = mouse.init();

    unsafe { sti(); }  // Re-enable interrupts

    // NOW safe to println!
    println!("[PS2] init: keyboard {}, mouse {}",
        if kbd_ok { "OK" } else { "FAILED" },
        if mouse_ok { "OK" } else { "FAILED" });

    true
}
```

## Why Interrupts Matter for Locks

Mutexes (`spin::Mutex`) work by:
1. Spin-waiting if lock is held
2. Expecting lock holder to eventually release it

With **interrupts enabled:**
- Timer ISR can preempt lock holder
- Scheduler can switch tasks
- Lock gets released eventually

With **interrupts disabled:**
- No preemption possible
- If lock is held, **infinite spin**
- System hangs

## Critical Sections and Logging

If you need to log from a critical section:

### Option 1: Move logging outside cli/sti
```rust
unsafe { cli(); }
let result = do_hardware_thing();
unsafe { sti(); }

println!("Result: {}", result);  // Safe now
```

### Option 2: Use status codes
```rust
unsafe { cli(); }
let status = do_hardware_thing();
unsafe { sti(); }

match status {
    Ok(_) => println!("Success"),
    Err(e) => println!("Failed: {:?}", e),
}
```

### Option 3: ISR-safe logging (NOT RECOMMENDED)
```rust
unsafe {
    cli();
    os_log::write_str_raw("[DEBUG] message\n");  // Lock-free, but still bad
    sti();
}
```

**Why NOT RECOMMENDED:** Even `write_str_raw()` eventually writes to terminal, which may have lock contention issues.

## Rules

1. **NEVER** call these with interrupts disabled:
   - `println!()`, `print!()`
   - `os_log::println!()`, `os_log::info!()`, etc.
   - `terminal::write()`
   - Any function that takes a `Mutex` lock

2. **cli/sti blocks must be:**
   - As short as possible
   - Hardware operations only
   - No memory allocations
   - No lock acquisitions
   - No logging

3. **If you need debug output from critical sections:**
   - Store result/status in variable
   - Re-enable interrupts (`sti`)
   - THEN log the status

## Detection

If your system hangs with:
- Last message before hang contains "IRQs disabled" or similar
- Hang happens during hardware init (PS2, ACPI, PCI, etc.)
- QEMU shows CPU spinning at 100%

**Likely cause:** println!() with cli() active.

**Debugging:**
1. Check call stack for `cli()` → `println!()` pattern
2. Look for hardware init functions calling logging macros
3. Grep for `unsafe.*cli.*println` patterns

## Related Issues

- SMP deadlock: CPU0 holds TERMINAL lock, CPU1 does cli() + println!() → hang
- ISR deadlock: ISR tries to log while terminal locked → hang
- Nested locks: cli() → lock A → println!() → lock TERMINAL → deadlock if TERMINAL already held

## The Fix Checklist

When adding hardware init code:
- [ ] Identify `cli()`/`sti()` boundaries
- [ ] Ensure NO println!/logging between cli/sti
- [ ] Move all logging to after `sti()`
- [ ] Test boot sequence multiple times

— SableWire: Interrupts disabled = locks poisoned. Keep cli/sti blocks CLEAN.
