# i686 Boot Implementation

**Architecture:** i686 (32-bit x86)
**Parent Spec:** [BOOT_SPEC.md](../../BOOT_SPEC.md)

---

## Boot Methods

| Method | Use Case |
|--------|----------|
| Multiboot2 | Primary (GRUB) |
| UEFI 32-bit | Rare |
| BIOS Direct | Custom bootloader |

---

## Entry Requirements

- **Mode:** Protected Mode (32-bit)
- **Paging:** Disabled
- **Interrupts:** Disabled
- **A20:** Enabled
- **EAX:** 0x36D76289 (Multiboot2 magic)
- **EBX:** Multiboot info pointer

---

## Boot Sequence

1. **Entry** - Multiboot2 stub
2. **Stack Setup** - ESP to kernel stack
3. **BSS Clear**
4. **Early Console** - Serial (COM1) or VGA text
5. **Memory Map** - Multiboot2 mmap tag
6. **GDT/IDT/TSS** - Set up descriptor tables
7. **Paging** - 2-level (PD -> PT) or 3-level with PAE
8. **PIC Init** - Remap IRQs to vectors 32+
9. **ACPI** - Find RSDP
10. **SMP** - (Optional) INIT-SIPI-SIPI

---

## Key Structures

| Structure | Purpose |
|-----------|---------|
| GDT | Null, kernel code/data, user code/data, TSS |
| IDT | 256 entries, 8 bytes each |
| TSS | ESP0 for ring transitions |
| Page Directory | 1024 entries, points to page tables |
| Page Table | 1024 entries, 4KB pages |

---

## Memory Layout

- **User:** 0x00000000 - 0xBFFFFFFF (3GB)
- **Kernel:** 0xC0000000 - 0xFFFFFFFF (1GB)

---

## Paging Options

| Mode | Levels | Physical Limit |
|------|--------|----------------|
| Standard | 2 | 4GB |
| PAE | 3 | 64GB (36-bit physical) |

---

## Syscalls

- Via INT 0x80 (software interrupt)
- EAX = syscall number
- EBX, ECX, EDX, ESI, EDI, EBP = arguments

---

## CPU Features

- FPU (required)
- PSE - 4MB pages
- PAE - Physical Address Extension
- CMOV (required for i686)

---

## Exit Criteria

- [ ] Multiboot2 boot working
- [ ] GDT/IDT/TSS configured
- [ ] 2-level paging enabled
- [ ] 3GB/1GB split
- [ ] PIC initialized
- [ ] Works on QEMU -machine pc

---

*End of i686 Boot Implementation*
