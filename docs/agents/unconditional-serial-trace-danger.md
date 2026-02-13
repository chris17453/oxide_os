# Unconditional Serial Trace Danger

## Rule
**NEVER add unconditional serial trace output to hot paths.** Any serial write in a
path that fires per-page, per-alloc, per-ISR, or per-syscall will saturate the UART
and effectively freeze the system.

## The Math
- UART baud rate: 115200 → 11,520 bytes/sec effective throughput
- A typical exec allocates ~100 pages
- Each `[FRAME-ALLOC]` + `[ZERO-START]` + `[ZERO-DONE]` trace = ~130 bytes
- 100 pages × 130 bytes = **13,000 bytes** = **1.1 seconds** of pure serial I/O per exec
- Add `[PT-CLEAR]` + `[PT-ENTRY]` + `[PTE-WRITE]` → **30,000+ bytes** per fork+exec = **2.6 seconds**
- The shell literally cannot appear because the exec takes 3 seconds of serial I/O

## Unbounded Spin Is the Worst Form
Raw serial writes using `while inb(0x3FD) & 0x20 == 0 {}` are unbounded spins.
If the UART TX buffer is full, the CPU spins forever. In an ISR, this blocks ALL
other interrupts and effectively hangs the system.

Always use `arch_x86_64::serial::write_str_unsafe()` or `write_byte_unsafe()` which
have a 2048-iteration spin limit per byte (drops rather than hangs).

## Feature-Gate Categories (Excluded from debug-all)
These features are intentionally NOT in `debug-all` because they exceed serial bandwidth:

| Feature | Location | Reason |
|---------|----------|--------|
| `debug-buddy` | mm-core/buddy.rs | Fires on every alloc/free (~45 traces per operation) |
| `debug-paging` | mm-paging/mapper.rs, proc/address_space.rs | Fires on every page map + alloc (~30KB per exec) |
| `debug-perf` | arch-x86_64/exceptions.rs | Per-ISR PERF-WARN (53 bytes × hundreds/sec = feedback loop) |
| `debug-timer` | arch-x86_64/exceptions.rs | Every timer tick on every CPU (400/sec at 4×100Hz) |
| `debug-lock` | terminal, fb | Every terminal tick when contended (60/sec) |

Enable individually: `KERNEL_FEATURES="debug-all,debug-paging"`

## What To Do Instead
1. **Gate behind feature flag**: `#[cfg(feature = "debug-paging")]`
2. **Use bounded serial**: `arch_x86_64::serial::write_str_unsafe()` (never raw `while THRE==0`)
3. **Keep FATAL unconditional**: Corruption detection / OOM should always report
4. **Use helpers**: `write_u64_hex_unsafe()`, `write_u32_unsafe()` for hex/decimal in traces

## Historical Damage
- Timer ISR went from 96M+ cycles to 184K after gating buddy traces
- Shell exec went from 2.6 seconds to milliseconds after gating paging traces
- PERF-WARN created a feedback loop: warning → serial I/O → next ISR exceeds threshold → warning

— GraveShift: If you think "I'll just add a quick serial print for debugging" —
no. You'll forget to remove it, it'll ship in the default build, and some poor
bastard will spend hours wondering why login works but the shell never appears.
