# Bootloader exit_boot_services() Hang

**Status:** CRITICAL BLOCKER - Prevents kernel from loading

## Summary

The UEFI bootloader successfully completes all initialization steps but hangs indefinitely when calling `exit_boot_services()`, preventing the kernel from ever starting.

## Symptoms

- Bootloader displays logo and "UEFI Bootloader Starting..."
- Bootloader finds ACPI RSDP and prints location
- System then hangs with no further output
- QEMU does not exit (so not a triple-fault)
- No kernel messages ever appear

## Debug Investigation

Added extensive debug output to bootloader `main()` function:

```
[ACPI] RSDP v2.0 found at 0x000000000f77e014
[DEBUG] About to create boot info...
[DEBUG] Boot info created, writing to memory...
[DEBUG] Boot info written, calculating addresses...
[DEBUG] kernel_entry_virt = 0xffffffff80000000
[DEBUG] boot_info_virt = 0xffff80000e784000
[DEBUG] Addresses calculated, showing final message...
[DEBUG] Calling log with empty string...

[DEBUG] Called log empty, now calling with message...
Boot complete! Launching OXIDE OS...
[DEBUG] Message printed, calling log empty again...

[DEBUG] All log calls complete!
[DEBUG] About to exit boot services...
[DEBUG] Calling exit_boot_services...
<HANGS HERE>
```

## Root Cause

The hang occurs inside `uefi::table::system_table_boot().exit_boot_services()` from uefi-rs 0.32.0.

According to UEFI spec, `exit_boot_services()` can fail/hang if:
1. **Memory map changed** between retrieval and exit attempt
2. **UEFI protocol still exclusively open** (GOP, FileSystem, etc.)
3. **Event handler not unregistered**
4. **Bug in uefi-rs library**

## Code Location

File: `bootloader/boot-uefi/src/main.rs`
Lines: ~150-156

```rust
// Exit boot services - after this, no more UEFI calls!
let st = uefi::table::system_table_boot().expect("Boot services not available");
unsafe {
    st.exit_boot_services(uefi::table::boot::MemoryType::LOADER_DATA);
}
```

The function signature returns `(SystemTable<Runtime>, MemoryMapOwned)` but the original code didn't capture the return value. Testing with captured return value showed no difference - still hangs.

## Timeline

- Commit 086be5d1 (Feb 6 03:50): "Remove fake EFI progress bars and improve logo design" - massive bootloader changes (411 lines removed)
- Sometime between this commit and current HEAD, the hang was introduced
- User reported system WAS working with service manager disabled

## Attempted Fixes

1. ✅ Captured return value from exit_boot_services() - NO EFFECT
2. ✅ Disabled graphical logo display - NO EFFECT
3. ❌ Getting fresh memory map before exit - NOT YET TESTED
4. ❌ Explicitly closing GOP protocol handles - NOT YET TESTED
5. ❌ Downgrading uefi-rs library version - NOT YET TESTED

## Next Steps

1. Review commit 086be5d1 changes for protocol handle leaks
2. Check if GOP handles are being properly dropped
3. Consider getting memory map immediately before exit_boot_services
4. Test with uefi-rs 0.31.x or earlier
5. Add timeout/watchdog to detect hang earlier
6. Review uefi-rs source for known bugs in exit_boot_services

## Workarounds

None known. This is a hard blocker preventing any kernel execution.

## Related Files

- `bootloader/boot-uefi/src/main.rs` - Main bootloader code
- `bootloader/boot-uefi/Cargo.toml` - Dependencies (uefi = "0.32.0")
- `kernel/boot/boot-proto/src/lib.rs` - Boot protocol definitions

---
— GraveShift: UEFI is the worst thing to happen to operating systems since BIOS.
We're stuck in firmware hell, and exit_boot_services is the gate we can't pass.
