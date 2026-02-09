//! Performance statistics reporting
//!
//! Provides formatted output of performance counters, similar to Linux's /proc/stat

use crate::PerfCounters;
use crate::output::*;

/// Print performance statistics to stderr (ISR-safe)
///
/// — PatchBay: Called every ~5 seconds from timer ISR on BSP to show system health.
/// Uses ISR-safe writes via os_log with bounded spins.
pub fn print_perf_stats(counters: &PerfCounters, uptime_ticks: u64) {
    write_str("\n╔══════════════════════════════════════════════════════════════════╗\n");
    write_str("║  OXIDE OS Performance Statistics — PatchBay's Scoreboard        ║\n");
    write_str("╠══════════════════════════════════════════════════════════════════╣\n");

    // Uptime
    let uptime_sec = uptime_ticks / 100; // 100 Hz timer
    write_str("║  Uptime: ");
    print_decimal(uptime_sec);
    write_str("s (");
    print_decimal(uptime_ticks);
    write_str(" ticks @ 100Hz)");
    print_padding(35 - decimal_width(uptime_sec) - decimal_width(uptime_ticks));
    write_str("║\n");

    write_str("╠══════════════════════════════════════════════════════════════════╣\n");
    write_str("║  INTERRUPT STATISTICS                                            ║\n");
    write_str("╠══════════════════════════════════════════════════════════════════╣\n");

    // Timer IRQ
    let timer_count = counters
        .timer_irq_count
        .load(core::sync::atomic::Ordering::Relaxed);
    let timer_avg = counters.timer_irq_avg_cycles();
    let timer_min = counters
        .timer_irq_cycles_min
        .load(core::sync::atomic::Ordering::Relaxed);
    let timer_max = counters
        .timer_irq_cycles_max
        .load(core::sync::atomic::Ordering::Relaxed);

    write_str("║  Timer IRQ:     ");
    print_decimal(timer_count);
    write_str(" calls  │  avg: ");
    print_decimal(timer_avg);
    write_str(" cyc");
    print_padding(27 - decimal_width(timer_count) - decimal_width(timer_avg));
    write_str("║\n");

    write_str("║                             │  min: ");
    print_decimal(timer_min);
    write_str(" cyc  max: ");
    print_decimal(timer_max);
    write_str(" cyc");
    print_padding(16 - decimal_width(timer_min) - decimal_width(timer_max));
    write_str("║\n");

    // Keyboard IRQ
    let kb_count = counters
        .keyboard_irq_count
        .load(core::sync::atomic::Ordering::Relaxed);
    let kb_avg = counters.keyboard_irq_avg_cycles();
    if kb_count > 0 {
        write_str("║  Keyboard IRQ:  ");
        print_decimal(kb_count);
        write_str(" calls  │  avg: ");
        print_decimal(kb_avg);
        write_str(" cyc");
        print_padding(27 - decimal_width(kb_count) - decimal_width(kb_avg));
        write_str("║\n");
    }

    // Mouse IRQ
    let mouse_count = counters
        .mouse_irq_count
        .load(core::sync::atomic::Ordering::Relaxed);
    let mouse_avg = counters.mouse_irq_avg_cycles();
    if mouse_count > 0 {
        write_str("║  Mouse IRQ:     ");
        print_decimal(mouse_count);
        write_str(" calls  │  avg: ");
        print_decimal(mouse_avg);
        write_str(" cyc");
        print_padding(27 - decimal_width(mouse_count) - decimal_width(mouse_avg));
        write_str("║\n");
    }

    write_str("╠══════════════════════════════════════════════════════════════════╣\n");
    write_str("║  SCHEDULER STATISTICS                                            ║\n");
    write_str("╠══════════════════════════════════════════════════════════════════╣\n");

    let ctx_switches = counters
        .context_switches
        .load(core::sync::atomic::Ordering::Relaxed);
    let preemptions = counters
        .preemptions
        .load(core::sync::atomic::Ordering::Relaxed);

    write_str("║  Context switches: ");
    print_decimal(ctx_switches);
    print_padding(46 - decimal_width(ctx_switches));
    write_str("║\n");

    write_str("║  Preemptions:      ");
    print_decimal(preemptions);
    print_padding(46 - decimal_width(preemptions));
    write_str("║\n");

    write_str("╠══════════════════════════════════════════════════════════════════╣\n");
    write_str("║  OUTPUT HEALTH (STDERR)                                          ║\n");
    write_str("╠══════════════════════════════════════════════════════════════════╣\n");

    let serial_written = counters
        .serial_bytes_written
        .load(core::sync::atomic::Ordering::Relaxed);
    let serial_dropped = counters
        .serial_bytes_dropped
        .load(core::sync::atomic::Ordering::Relaxed);
    let serial_spins = counters
        .serial_spin_limit_hits
        .load(core::sync::atomic::Ordering::Relaxed);
    let drop_rate = counters.serial_drop_rate();

    write_str("║  Bytes written:    ");
    print_decimal(serial_written);
    print_padding(46 - decimal_width(serial_written));
    write_str("║\n");

    write_str("║  Bytes dropped:    ");
    print_decimal(serial_dropped);
    write_str("  (");
    print_decimal(drop_rate);
    write_str("%)");
    print_padding(39 - decimal_width(serial_dropped) - decimal_width(drop_rate));
    write_str("║\n");

    write_str("║  Spin limit hits:  ");
    print_decimal(serial_spins);
    print_padding(46 - decimal_width(serial_spins));
    write_str("║\n");

    // Terminal rendering stats
    let term_ticks = counters
        .terminal_ticks
        .load(core::sync::atomic::Ordering::Relaxed);
    let term_renders = counters
        .terminal_renders
        .load(core::sync::atomic::Ordering::Relaxed);
    let term_avg = counters.terminal_render_avg_cycles();

    if term_ticks > 0 {
        write_str("╠══════════════════════════════════════════════════════════════════╣\n");
        write_str("║  TERMINAL RENDERING                                              ║\n");
        write_str("╠══════════════════════════════════════════════════════════════════╣\n");

        write_str("║  Terminal ticks:   ");
        print_decimal(term_ticks);
        print_padding(46 - decimal_width(term_ticks));
        write_str("║\n");

        write_str("║  Renders:          ");
        print_decimal(term_renders);
        write_str("  │  avg: ");
        print_decimal(term_avg);
        write_str(" cyc");
        print_padding(31 - decimal_width(term_renders) - decimal_width(term_avg));
        write_str("║\n");
    }

    write_str("╚══════════════════════════════════════════════════════════════════╝\n\n");
}
