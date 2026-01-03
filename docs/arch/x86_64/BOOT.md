# x86_64 Boot Implementation

**Architecture:** x86_64 (AMD64/Intel 64)
**Parent Spec:** [BOOT_SPEC.md](../../BOOT_SPEC.md)

---

## Boot Methods

| Method | Use Case |
|--------|----------|
| UEFI | Primary (modern systems) |
| Multiboot2 | GRUB, legacy compatibility |
| Linux Boot | bzImage protocol |

---

## Entry Requirements

### UEFI Entry
- **Mode:** Long Mode (64-bit)
- **Paging:** Enabled (identity mapped by firmware)
- **Interrupts:** Disabled
- **RCX:** ImageHandle
- **RDX:** SystemTable pointer

### Multiboot2 Entry (via stub)
- **Mode:** Protected Mode (32-bit) - must transition to long mode
- **Paging:** Disabled
- **EAX:** 0x36D76289 (magic)
- **EBX:** Multiboot info pointer

---

## Boot Sequence

1. **Entry** - UEFI efi_main or Multiboot2 stub
2. **Mode Transition** - (Multiboot2 only) Protected -> Long mode
3. **BSS Clear**
4. **Early Console** - Serial (COM1 0x3F8) or VGA text (0xB8000)
5. **Memory Map** - UEFI GetMemoryMap() or Multiboot2 mmap tag
6. **GDT/IDT/TSS** - Set up descriptor tables
7. **Page Tables** - 4-level (PML4) or 5-level (LA57)
8. **SYSCALL/SYSRET** - Configure MSRs (STAR, LSTAR, SFMASK)
9. **ACPI** - Find RSDP, parse tables
10. **APIC** - Initialize Local APIC, calibrate timer
11. **SMP** - Boot APs via INIT-SIPI-SIPI

---

## Key Structures

| Structure | Purpose |
|-----------|---------|
| GDT | Segment descriptors (null, kernel code/data, user code/data, TSS) |
| IDT | 256 interrupt/exception handlers |
| TSS | Kernel stack pointers (RSP0), IST stacks |
| Page Tables | PML4 -> PDPT -> PD -> PT (4-level) |

---

## Memory Layout

- **User:** 0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF (canonical low)
- **Kernel:** 0xFFFF_8000_0000_0000+ (canonical high)
- **Direct Map:** Physical memory at fixed offset for kernel access

---

## CPU Features to Check

- Long mode (implied by x86_64)
- NX bit (required)
- SSE2 (required, used by Rust)
- PCID, INVPCID (optional, TLB optimization)
- LA57 (optional, 5-level paging)

---

## SMP Boot

1. Copy 16-bit trampoline to low memory (<1MB)
2. Send INIT IPI to target AP
3. Wait 10ms
4. Send SIPI with trampoline vector
5. Send second SIPI
6. AP transitions: Real -> Protected -> Long mode
7. AP signals ready

---

## Exit Criteria

- [ ] UEFI or Multiboot2 boot working
- [ ] Long mode with 4-level paging
- [ ] GDT/IDT/TSS configured
- [ ] SYSCALL path set up
- [ ] APIC initialized
- [ ] SMP APs booted
- [ ] Works on QEMU with OVMF

---

*End of x86_64 Boot Implementation*
