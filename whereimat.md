# Where We're At - OXIDE OS

## Current Status: Bootloader Memory Map Fix + Keyboard Debug

### What We Just Fixed

#### 1. Bootloader Memory Map Bug (CRITICAL FIX)
**Problem**: Bootloader allocated kernel stack AFTER getting the UEFI memory map
- Stack allocated at `0x1c023000` (256KB)
- UEFI memory map reported LOADER_DATA starting at `0x1c063000`
- **Gap of 256KB marked as USABLE** → buddy allocator corruption

**Fix**: Moved stack allocation BEFORE `get_memory_map()` call
- Now follows **Linux pattern**: allocate everything first, then get memory map
- Bootloader verification code checks if stack appears in UEFI map
- If missing, manually adds it as `MemoryType::Bootloader`

**Files Changed**:
- `bootloader/boot-uefi/src/main.rs` - moved stack allocation before memory map call
- Removed kernel-side 2MB guard workaround (should trust bootloader now)

#### 2. Interrupt State Diagnostics
**Added**: Interrupt enable checks at critical points
- After `unmask_io_irqs()`
- Before entering userspace

**Purpose**: Debug why keyboard doesn't work
- Should see `[IRQ-CHECK] ... interrupts ENABLED`
- Should see `[KBD-IRQ] sc=0x..` when typing
- If interrupts enabled but no IRQs → IOAPIC/APIC routing issue
- If interrupts disabled → something calling `cli` incorrectly

**Files Changed**:
- `kernel/src/init.rs` - added interrupt state checks
- `kernel/drivers/input/ps2/src/lib.rs` - added IRQ trace logging

### Design Patterns Confirmed

**Linux Bootloader Pattern** ✓
1. Allocate all bootloader memory (stack, page tables, boot info)
2. Call `ExitBootServices()` to get final memory map
3. Memory map accurately reflects what's allocated vs free

**Linux Interrupt Initialization Pattern** ✓
1. Initialize hardware with `cli` (interrupts disabled)
2. Register all IRQ handlers
3. Configure IOAPIC/APIC
4. Enable CPU interrupts with `sti`
5. Unmask specific IRQs in IOAPIC

### Known Issues

#### Keyboard/Mouse Don't Work
**Symptoms**: System boots to login prompt, cursor blinks, but can't type
**Theories**:
1. Interrupts disabled somewhere
2. IRQs masked in IOAPIC (despite unmask call)
3. APIC routing misconfigured
4. PS/2 controller not initialized properly

**Diagnostics Added**:
- `[IRQ-CHECK]` logs show interrupt state
- `[KBD-IRQ]` logs show when keyboard IRQs fire
- `[MOUSE-IRQ]` logs show when mouse IRQs fire

### Next Steps

1. **Test the bootloader fix**: Run `make run` and verify:
   - No buddy allocator corruption
   - System boots reliably
   - More RAM available to kernel

2. **Debug keyboard issue**: Check serial output for:
   - `[IRQ-CHECK] After unmask: interrupts ENABLED`
   - `[IRQ-CHECK] Interrupts ENABLED before userspace`
   - `[KBD-IRQ] sc=0x..` when pressing keys

3. **If keyboard still broken**:
   - Check if `[IRQ-CHECK]` shows DISABLED → find what's calling `cli`
   - Check if IRQs enabled but no `[KBD-IRQ]` → IOAPIC/APIC issue
   - Check if IRQs firing but keys not working → input subsystem issue

### Recent Changes

- **Bootloader**: Stack allocation moved before memory map retrieval
- **Kernel**: Interrupt state diagnostics added
- **PS/2 Driver**: IRQ trace logging for first 5 keyboard/mouse events
- **Init**: Removed manual RSP/PML4 protection (trusting bootloader now)

### Technical Debt

- **TODO**: Remove 2MB kernel-side guard once bootloader fix is verified
- **TODO**: Clean up all the `[INIT-DEBUG]` serial traces after keyboard works
- **TODO**: Remove IRQ trace logging after debugging complete

### Key Files Modified

**Bootloader**:
- `bootloader/boot-uefi/src/main.rs` - stack allocation ordering fix

**Kernel Core**:
- `kernel/src/init.rs` - interrupt diagnostics, removed manual protection
- `kernel/drivers/input/ps2/src/lib.rs` - IRQ trace logging
- `kernel/mm/mm-core/src/buddy.rs` - alignment fix for debug trap

**Architecture**:
- `kernel/arch/arch-x86_64/src/apic.rs` - IOAPIC unmasking
- `kernel/arch/arch-x86_64/src/exceptions.rs` - IRQ handlers
- `kernel/arch/arch-x86_64/src/lib.rs` - interrupt enable/disable

---

**Last Updated**: 2025-02-07
**Status**: Awaiting test results for bootloader fix and keyboard diagnostics
