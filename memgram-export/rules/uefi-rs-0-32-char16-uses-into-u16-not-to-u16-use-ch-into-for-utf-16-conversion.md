# uefi-rs 0.32 Char16 uses Into<u16> not to_u16() — use (*ch).into() for UTF-16 conversion

🟡 preference | ✅ do

| Field | Value |
|-------|-------|
| ID | `36f74212759b` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T11:05:05.422549+00:00 |
| Keywords | uefi, uefi-rs, Char16, u16, API |
| Session | [505ce53acb4e](../sessions/implement-oxide-boot-manager-custom-pre-boot-environment-with-graphical-boot-men.md) |

## Details

In uefi-rs 0.32, Char16 implements From<Char16> for u16 but does NOT have a .to_u16() method. Use `let c: u16 = (*ch).into();` to convert Char16 to u16. Also, MemoryMapOwned requires `use uefi::mem::memory_map::MemoryMap` trait import for .entries() method.
