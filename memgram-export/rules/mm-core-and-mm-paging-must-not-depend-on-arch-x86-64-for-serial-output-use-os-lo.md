# mm-core and mm-paging MUST NOT depend on arch_x86_64 for serial output — use os_log

🟡 preference | ❌ dont

| Field | Value |
|-------|-------|
| ID | `f5f7b16c1367` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T16:35:48.181442+00:00 |
| Keywords | arch-coupling, serial, os_log, mm-core, mm-paging, platform-independence |
| Session | [b27c9dc86d34](../sessions/continue-memory-hardening-fix-remaining-audit-issues-cow-toctou-race-exec-as-lea.md) |

## Details

Architecture-agnostic crates (mm-core, mm-paging, proc) must never directly call arch_x86_64::serial::* functions. Use os_log::write_str_raw(), os_log::write_byte_raw(), os_log::write_u32_raw(), os_log::write_u64_hex_raw() instead. These delegate to registered ISR-safe function pointers that are platform-independent. The only acceptable arch_x86_64 import in mm-paging is the TlbControl trait impl, which is properly #[cfg(target_arch = "x86_64")] gated.
