/* OXIDE Kernel Linker Script */

ENTRY(kernel_main)

/* Physical load address (where bootloader places the kernel in physical memory) */
KERNEL_PHYS_BASE = 0x200000;

/* Virtual address where kernel runs (higher half) */
KERNEL_VIRT_BASE = 0xFFFFFFFF80000000;

SECTIONS
{
    /* Kernel runs at high virtual address */
    . = KERNEL_VIRT_BASE;

    .text : ALIGN(4K)
    {
        *(.text .text.*)
    }

    .rodata : ALIGN(4K)
    {
        *(.rodata .rodata.*)

        /* Driver registration sections for compile-time driver discovery */
        /* — BlackLatch: the linker collects drivers from the void */
        . = ALIGN(8);
        __start_pci_drivers = .;
        *(.pci_drivers)
        __stop_pci_drivers = .;

        . = ALIGN(8);
        __start_isa_drivers = .;
        *(.isa_drivers)
        __stop_isa_drivers = .;
    }

    .data : ALIGN(4K)
    {
        *(.data .data.*)
    }

    .bss : ALIGN(4K)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /DISCARD/ :
    {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.note*)
        *(.comment)
    }
}
