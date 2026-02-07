# Performance Monitoring Infrastructure

**Author:** PatchBay
**Date:** 2026-02-06
**Status:** Implemented

## Overview

OXIDE OS now includes a comprehensive performance monitoring subsystem inspired by Linux's `perf_events`. This provides cycle-accurate profiling of interrupt handlers, scheduler operations, and system-wide performance metrics.

## Architecture

### Core Components

1. **`kernel/perf` crate** - Performance counter infrastructure
   - Atomic counters for all major subsystems
   - RDTSC-based cycle measurement
   - RAII scope guards for automatic timing

2. **ISR Instrumentation** - All interrupt handlers instrumented
   - Timer IRQ (min/max/avg cycles)
   - Keyboard IRQ (count + average cycles)
   - Mouse IRQ (count + average cycles)

3. **Scheduler Metrics** - Context switch tracking
   - Total context switches
   - Preemption count
   - need_resched events

4. **Serial Port Health** - UART saturation detection
   - Bytes written vs. dropped
   - Drop rate percentage
   - Spin limit hit count

5. **Terminal Rendering** - Frame rendering performance
   - Render count
   - Average render cycles
   - Tick frequency

## Usage

### Reading Statistics

Performance statistics are automatically printed to **stderr** every 5 seconds (500 ticks @ 100Hz). This output is captured by journald and can be viewed in system logs:

```bash
# In QEMU, stderr goes to the console
# With -serial mon:stdio, you'll see perf stats on terminal

# journald captures all stderr output
journalctl -f | grep "OXIDE OS Performance"
```

Example output:

```
╔══════════════════════════════════════════════════════════════════╗
║  OXIDE OS Performance Statistics — PatchBay's Scoreboard        ║
╠══════════════════════════════════════════════════════════════════╣
║  Uptime: 127s (12700 ticks @ 100Hz)                             ║
╠══════════════════════════════════════════════════════════════════╣
║  INTERRUPT STATISTICS                                            ║
╠══════════════════════════════════════════════════════════════════╣
║  Timer IRQ:     12700 calls  │  avg: 42384 cyc                  ║
║                               │  min: 8432 cyc  max: 1024893 cyc║
║  Keyboard IRQ:  234 calls    │  avg: 12456 cyc                  ║
╠══════════════════════════════════════════════════════════════════╣
║  SCHEDULER STATISTICS                                            ║
╠══════════════════════════════════════════════════════════════════╣
║  Context switches: 5623                                          ║
║  Preemptions:      892                                           ║
╠══════════════════════════════════════════════════════════════════╣
║  SERIAL PORT HEALTH                                              ║
╠══════════════════════════════════════════════════════════════════╣
║  Bytes written:    123456                                        ║
║  Bytes dropped:    8234  (6%)                                    ║
║  Spin limit hits:  127                                           ║
╚══════════════════════════════════════════════════════════════════╝
```

### Programmatic Access

```rust
use perf;

// Get global counters
let counters = perf::counters();

// Read specific metrics
let timer_avg = counters.timer_irq_avg_cycles();
let ctx_switches = counters.context_switches.load(Ordering::Relaxed);

// Measure code section
let _scope = perf::PerfScope::timer_irq();
// ... code to measure ...
// Cycles automatically recorded on drop
```

### Adding New Counters

1. Add counter to `PerfCounters` struct in `kernel/perf/src/lib.rs`
2. Add recording method (e.g., `record_my_event()`)
3. Call recording method from instrumented code
4. Add display to `kernel/perf/src/stats.rs`

## Performance Impact

The monitoring infrastructure is designed for **zero overhead** when not actively measuring:

- Counters use relaxed atomic operations (no memory barriers)
- RDTSC is ~20 cycles on modern CPUs
- Recording a timer IRQ: ~100 cycles total overhead (<0.1% @ 100K cycle ISR)
- Statistics printing: Only every 5 seconds, ISR-safe bounded writes

## Interpreting Results

### Timer IRQ Cycles

**Normal range:** 10K - 100K cycles (3.3us - 33us @ 3GHz)

- < 10K cycles: Very light ISR, scheduler not doing much
- 10K - 50K: Normal, scheduler ticking + occasional context switch
- 50K - 200K: Heavy load, frequent context switches
- > 200K: **Problem!** ISR taking too long, investigate:
  - Terminal rendering bottleneck?
  - Mouse event flood?
  - Serial output saturation?

**max > 1M cycles** triggers automatic warning in serial output.

### Serial Drop Rate

**Normal range:** 0% - 5%

- 0%: Serial not saturated, all debug output delivered
- 1-5%: Light saturation, acceptable for heavy debug output
- 5-20%: **Moderate saturation**, debug output throttling ISRs
- > 20%: **Heavy saturation**, serial writes wasting significant CPU time

**Solution:** Disable `debug-timer`, `debug-lock`, or reduce debug output

### Context Switch Rate

**Normal range:** Depends on workload

- < 10/sec: Idle system, few runnable tasks
- 10-100/sec: Light interactive workload
- 100-1000/sec: Heavy interactive or I/O-bound workload
- > 1000/sec: **Problem!** Scheduler thrashing, investigate:
  - Tasks blocking/waking too frequently?
  - Busy-wait loops instead of proper blocking?
  - Timer tick rate too high?

## Troubleshooting

### ISR Taking Too Long

If timer ISR avg > 50K cycles:

1. Check terminal rendering: Does it improve with `-nographic`?
2. Check mouse events: Disable mouse processing temporarily
3. Check serial saturation: Disable debug features
4. Add more granular timing to narrow down bottleneck

### Serial Saturation

If drop rate > 5%:

1. Disable `debug-timer` (400 messages/sec on 4-CPU SMP)
2. Disable `debug-lock` (contention warnings spam output)
3. Reduce terminal tick rate (increase `TERMINAL_TICK_INTERVAL`)
4. Use `-serial null` if serial output not needed

### Scheduler Thrashing

If context switches > 1000/sec:

1. Profile what tasks are context-switching frequently
2. Check for busy-wait loops (should use `nanosleep` or `poll`)
3. Verify `kernel_preempt_ok` flag usage in blocking syscalls
4. Check if timer tick rate is too high (should be 100 Hz)

## Related Agent Rules

- `docs/agents/isr-lock-safety.md` - ISR context lock safety
- `docs/agents/serial-saturation-safety.md` - Serial bounded spins
- `docs/agents/smp-timer-safety.md` - SMP timer interrupt rules

## Future Enhancements

- Per-CPU performance counters (separate stats for each core)
- Histogram-style latency tracking (percentiles, not just avg)
- `/proc/perfstat` interface for userspace access
- Hardware Performance Monitoring Unit (PMU) integration
- Flamegraph-style call stack sampling

— PatchBay: "If you can't measure it, you can't optimize it. Now measure everything."
