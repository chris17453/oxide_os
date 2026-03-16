# clock_gettime MUST use TSC for sub-tick precision — 100Hz timer gives 10ms granularity which fails sequential-call tests

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `93b11f121e23` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-05T01:03:03.962330+00:00 |
| Keywords | clock_gettime, TSC, monotonic, precision, timer, rdtsc |
| Session | [a7ce0b3b29c8](../sessions/fix-test-clock-gettime-by-adding-tsc-based-sub-tick-precision-to-get-monotonic-t.md) |

## Details

get_monotonic_time() and get_realtime() must use arch::read_tsc() / arch::tsc_frequency() for nanosecond precision. The old approach (timer_ticks * NS_PER_TICK) gave 10ms granularity — two back-to-back clock_gettime syscalls returned identical values. TSC is calibrated at boot via PIT (~4.2GHz on QEMU). Split computation into secs = tsc/freq, nsec = (tsc%freq)*1e9/freq to avoid u64 overflow.
