# RISC-V 64 Boot Implementation

**Architecture:** RISC-V 64-bit (RV64GC)
**Parent Spec:** [BOOT_SPEC.md](../../BOOT_SPEC.md)

---

## Boot Methods

| Method | Use Case |
|--------|----------|
| OpenSBI + DTB | Primary (provides M-mode services) |
| U-Boot | After OpenSBI or standalone |
| BBL | Legacy (Berkeley Boot Loader) |

---

## Entry Requirements

- **Mode:** S-mode (Supervisor)
- **MMU:** Off (satp = 0)
- **Interrupts:** Disabled
- **a0:** Hart ID
- **a1:** DTB pointer (physical)

---

## Boot Sequence

1. **Entry** - Receive hart ID (a0), DTB (a1)
2. **BSS Clear** - Zero BSS
3. **Early Console** - UART via SBI ecall or direct (16550/ns16550a)
4. **DTB Parse** - Memory, devices, hart topology
5. **Page Tables** - Set up Sv39/Sv48/Sv57
6. **Enable MMU** - Write satp, sfence.vma
7. **Trap Vectors** - Set stvec
8. **PLIC Init** - Platform-Level Interrupt Controller
9. **Timer** - SBI timer or direct CLINT/ACLINT
10. **SMP** - SBI HSM extension to start other harts

---

## Key CSRs

| CSR | Purpose |
|-----|---------|
| sstatus | Supervisor status |
| sie/sip | Interrupt enable/pending |
| stvec | Trap vector base |
| sepc | Exception PC |
| scause | Trap cause |
| stval | Trap value (faulting address) |
| satp | Address translation (page table base + mode) |

---

## Address Translation Modes

| Mode | Levels | Virtual Bits | Physical Bits |
|------|--------|--------------|---------------|
| Sv39 | 3 | 39 | 56 |
| Sv48 | 4 | 48 | 56 |
| Sv57 | 5 | 57 | 56 |

---

## SBI (Supervisor Binary Interface)

OpenSBI provides M-mode services via ecall:
- **Timer:** sbi_set_timer
- **IPI:** sbi_send_ipi
- **Console:** sbi_console_putchar (legacy)
- **HSM:** Hart State Management (start/stop cores)
- **RFENCE:** Remote fence operations

---

## Memory Layout

- **User:** Lower half of Sv39/48/57 space
- **Kernel:** Upper half (high bits set)
- **Direct map:** Physical memory mapped at fixed offset

---

## Exit Criteria

- [ ] DTB parsed
- [ ] Sv39 (or Sv48) MMU enabled
- [ ] Trap handlers installed
- [ ] PLIC working
- [ ] SBI timer functional
- [ ] SMP via HSM working
- [ ] Works on QEMU virt

---

*End of RISC-V 64 Boot Implementation*
