# APIC Timer Calibration via PIT

**Rule:** PIT-based APIC timer calibration MUST reset PIT state before measuring. The OUT signal may be HIGH from BIOS/previous operations.

## Problem

The original calibration code assumed PIT channel 2's OUT signal was LOW:

```rust
// DANGEROUS - OUT may already be HIGH from BIOS
while crate::inb(0x61) & 0x20 == 0 {}  // Exits immediately if OUT=HIGH!
```

When OUT was already HIGH (common after BIOS POST), the wait loop exited immediately, producing garbage calibration results like "4 ticks/ms" instead of ~62,000.

## Solution (Linux-style)

1. **Force gate LOW** — Reset PIT channel 2 state before programming
2. **Program mode while gate LOW** — Load count value before starting
3. **Set gate HIGH** — Triggers countdown, OUT goes LOW immediately
4. **Verify OUT went LOW** — Sanity check that PIT is responding
5. **Wait for OUT HIGH** — Actual calibration measurement
6. **Validate result** — Reject values outside reasonable range (1k-10M ticks/ms)

```rust
// Step 1: Force gate LOW
crate::outb(0x61, (port61 & 0xFC) | 0x00);

// Step 2: Program PIT mode 0
crate::outb(0x43, 0xB0);
crate::outb(0x42, count_lo);
crate::outb(0x42, count_hi);

// Step 3: Start APIC timer
write(TIMER_INIT, 0xFFFF_FFFF);

// Step 4: Set gate HIGH (starts PIT countdown, OUT goes LOW)
crate::outb(0x61, (port61 & 0xFC) | 0x01);

// Step 5: Wait for OUT to go HIGH (count reached 0)
while (crate::inb(0x61) & 0x20) == 0 {}
```

## PIT Mode 0 Timing

- **Gate LOW→HIGH**: Latches count, starts decrementing, OUT goes LOW
- **Count = 0**: OUT goes HIGH
- At 1.193182 MHz, 11,931 ticks ≈ 10ms

## Expected APIC Values

| Bus Clock | Divider | Ticks/ms |
|-----------|---------|----------|
| 100 MHz   | /16     | 6,250    |
| 133 MHz   | /16     | 8,312    |
| 200 MHz   | /16     | 12,500   |
| 400 MHz   | /16     | 25,000   |

Values below 1,000 or above 10,000,000 indicate calibration failure.

## How Linux Does It

Linux (`arch/x86/kernel/tsc.c`, `arch/x86/kernel/apic/apic.c`):
- Tries multiple calibration sources: HPET, PM-timer, PIT
- Takes multiple samples and averages
- Has extensive sanity checks
- Falls back to safe defaults if calibration fails

## Files

- `kernel/arch/arch-x86_64/src/apic.rs` — `calibrate_timer()` function

— TorqueJax: "The PIT is older than most kernel developers. It's been running since BIOS POST. Never assume it's in a known state."
