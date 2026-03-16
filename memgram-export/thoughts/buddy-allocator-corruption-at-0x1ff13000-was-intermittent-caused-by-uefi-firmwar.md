# Buddy allocator corruption at 0x1ff13000 was intermittent — caused by UEFI firmware memory map variation between boots, not a buddy allocator bug

| Field | Value |
|-------|-------|
| ID | `04afc563bb92` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T15:44:09.033735+00:00 |
| Accessed | 0 times |
| Keywords | buddy-allocator, corruption, UEFI, BootServices, memory-map, OVMF, intermittent |
| Session | [b65b7f6487d7](../sessions/continue-investigating-buddy-allocator-corruption-corrupted-blocks-at-0x1ff13000.md) |

## Content

Investigation of BUDDY-WARN corrupted blocks at 0x1ff13000 and 0x1feea000 (magic=0x0):

1. Both BUDDY-VERIFY checkpoints (after init, after driver probing) show 0 bad blocks
2. The corruption did NOT reproduce on the next boot
3. Memory map shows 0x1ff13000 is at the exact boundary between BootServices and Reserved regions
4. UEFI firmware memory layout varies between boots — OVMF keeps internal structures at the top of RAM
5. In the previous boot, these addresses were likely in BootServices regions; firmware may have zeroed them during ExitBootServices cleanup

Key fix: alloc_from_zone had a `?` propagation bug where a corrupted head at one order aborted the entire allocation instead of trying the next order. Changed to `match/continue`.
