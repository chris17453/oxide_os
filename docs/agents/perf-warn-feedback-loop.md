# PERF-WARN Feedback Loop Prevention

— PatchBay: The day the warning became the disease.

## The Rule

**NEVER put serial debug output in the timer ISR hot path without a feature gate.** Even a short message (53 bytes) costs ~5M CPU cycles at 115200 baud, which exceeds the 1M-cycle PERF-WARN threshold and triggers the NEXT warning — creating an infinite feedback cascade.

## The Bug

```
[PERF-WARN] Timer ISR took 5784160 cycles (5784K)
[PERF-WARN] Timer ISR took 5612480 cycles (5612K)
[PERF-WARN] Timer ISR took 111346688 cycles (111346K)  ← perf dump piggybacks
```

The PERF-WARN message was 53 bytes of serial output:
- `\n[PERF-WARN] Timer ISR took NNNNNNN cycles (NNNNK)\n`
- At 115200 baud: 53 bytes × ~87µs/byte ≈ 4.6ms ≈ 5M cycles @ 1GHz
- This exceeds the 1M cycle threshold → next ISR also warns → cascade

The perf stats dump (1500 bytes) was even worse: ~130ms of serial inside the ISR.

## The Fix

Both PERF-WARN and perf stats dump are gated behind `debug-perf` feature:

```rust
#[cfg(feature = "debug-perf")]
{
    if elapsed > 1_000_000 {
        // PERF-WARN serial output — only when you WANT the feedback storm
    }
}

#[cfg(feature = "debug-perf")]
{
    if is_bsp && current_tick % 500 == 0 && current_tick > 0 {
        perf::stats::print_perf_stats(perf::counters(), current_tick);
    }
}
```

Counter recording (`perf::counters().record_timer_irq()`) is ALWAYS active — it's just atomic increments, negligible cost. You can read the counters via `/proc/perf` or other interfaces without the ISR spam.

## Serial Cost Table (115200 baud, ~87µs/byte)

| Output | Bytes | Time | CPU Cycles @1GHz |
|--------|-------|------|-------------------|
| PERF-WARN line | 53 | 4.6ms | ~5M |
| Perf stats dump | 1500 | 130ms | ~130M |
| Single `[DEBUG]` line | ~80 | 7ms | ~7M |
| Buddy trace per alloc | ~200 | 17ms | ~17M |

## Prevention

- Timer ISR serial output MUST be feature-gated (never unconditional)
- Perf counters (atomic ops) are fine unconditionally — zero serial I/O
- If you need ISR timing data, read counters from process context
- `debug-perf` is intentionally NOT included in `debug-all`

---

— PatchBay: Heisenberg was right. Observing the ISR changes the ISR.
