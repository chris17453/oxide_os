# OH NO IT BROKE - APIC Timer Calibration Debug State

**Date:** 2026-02-05
**Issue:** APIC timer calibration returns 4 ticks/ms instead of ~62,000

## Symptom

```
[APIC] Timer calibrated: 4 ticks/ms
[APIC] Starting timer at 100Hz (count: 40)
```

With count=40, the timer fires almost instantly → interrupt storm → crash.

## Root Cause Analysis (In Progress)

The PIT-based calibration is failing. Suspected issues:

1. **PIT OUT signal already HIGH** — The wait loop exits immediately if OUT was HIGH from BIOS
2. **Build not being picked up** — Debug output added to `calibrate_timer()` is NOT appearing, suggesting the new kernel isn't being loaded

## What We Tried

### Fix 1: Linux-style PIT State Reset
Added to `kernel/arch/arch-x86_64/src/apic.rs`:
- Force gate LOW before programming
- Program PIT mode 0 while gate is LOW
- Set gate HIGH to start countdown
- Verify OUT went LOW (sanity check)
- Wait for OUT to go HIGH
- Sanity check: if result < 1000 or > 10M, use fallback

### Fix 2: Extensive Debug Output
Added `[APIC-CAL]` prefixed debug prints at every step:
- port61 initial value
- port61 after gate LOW
- port61 after gate HIGH
- OUT sanity check iteration count
- PIT wait iteration count
- APIC timer start/end values
- Final result

**BUT: Debug output not appearing!** Build compiles but new code isn't executing.

## Build Investigation

```
Kernel binary: /home/nd/repos/oxide_os/target/x86_64-unknown-none/debug/kernel
Last modified: Feb 5 22:33 (recent)
```

`make run` does:
1. `kill-qemu`
2. `clean-rootfs`
3. `create-rootfs` (rebuilds disk image)
4. `run-fedora` or `run-rhel`

The kernel binary IS being rebuilt, but something in the rootfs creation or QEMU loading might be using a cached version.

## Files Modified

- `kernel/arch/arch-x86_64/src/apic.rs` — `calibrate_timer()` with debug output and fixes
- `docs/agents/apic-timer-calibration.md` — Documentation of the calibration process
- `CLAUDE.md` — Added pointer to new doc

## Next Steps

1. **Force full rebuild**: `cargo clean && make build`
2. **Check if initramfs contains kernel**: The kernel might be embedded in initramfs
3. **Check QEMU disk image**: Maybe it's loading an old EFI bootloader
4. **Try direct kernel boot**: Use `-kernel` flag instead of EFI boot

## Current calibrate_timer() Code

```rust
pub fn calibrate_timer() -> u32 {
    let cached = CACHED_TICKS_PER_MS.load(Ordering::Acquire);
    if cached != 0 {
        return cached;
    }

    crate::serial_println!("[APIC-CAL] Starting calibration...");
    // ... extensive debug output at every step ...

    // Sanity check
    if ticks_per_ms < 1000 || ticks_per_ms > 10_000_000 {
        let fallback = 62500;
        // ... use fallback ...
    }
}
```

## How Linux Does It

Linux uses multiple calibration sources in order:
1. HPET (High Precision Event Timer)
2. PM-timer (ACPI Power Management Timer)
3. PIT (8254 Programmable Interval Timer) — last resort

Linux also:
- Takes multiple samples and averages
- Has extensive sanity checks
- Falls back to safe defaults

— TorqueJax: "We're in the 'staring at the abyss' phase of debugging. The code compiles, the binary updates, but the universe refuses to cooperate."
