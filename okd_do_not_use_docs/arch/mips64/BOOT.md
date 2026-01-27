# MIPS64 Boot Implementation

**Architecture:** MIPS64 (R4000+)
**Parent Spec:** [BOOT_SPEC.md](../../BOOT_SPEC.md)

---

## Boot Methods

| Method | Use Case |
|--------|----------|
| ARCS | SGI workstations (Indy, Indigo2, O2) |
| U-Boot | Loongson, embedded |
| YAMON | Malta, development boards |
| Bare Metal | Direct ROM boot |

---

## Entry Requirements

- **Mode:** Kernel mode (64-bit)
- **TLB:** Undefined/cleared
- **Caches:** May need init
- **Interrupts:** Disabled (SR.IE = 0)
- **a0:** argc (ARCS) or DTB pointer
- **a1:** argv (ARCS) or zero
- **a2:** envp (ARCS) or zero
- **a3:** Platform info pointer

---

## Boot Sequence

1. **Entry** - Receive ARCS args or DTB
2. **CP0 Init** - Status, Config, PRId check
3. **Cache Init** - Initialize I-cache and D-cache
4. **TLB Clear** - Invalidate all TLB entries
5. **BSS Clear** - Zero BSS
6. **Early Console** - ARCS console or serial (Z85C30 on SGI, 16550 on Malta)
7. **Memory Probe** - ARCS GetMemoryDescriptor() or DTB
8. **TLB Setup** - Wired entries for kernel
9. **Exception Vectors** - Set up at 0x8000_0000/0x8000_0180/0x8000_0200
10. **SMP** - Platform-specific CPU startup

---

## Key CP0 Registers

| Register | Purpose |
|----------|---------|
| Status | Mode, interrupts, coprocessors |
| Cause | Exception cause |
| EPC | Exception program counter |
| Config | Cache/MMU configuration |
| EntryHi/EntryLo0/EntryLo1 | TLB entry |
| Index/Random/Wired | TLB management |
| PageMask | Variable page sizes |
| Context/XContext | TLB refill assist |
| PRId | Processor ID |

---

## Memory Segments

| Segment | Address Range | Cached | Mapped |
|---------|---------------|--------|--------|
| xuseg | 0x0000_0000_0000_0000 | TLB | Yes |
| xsseg | 0x4000_0000_0000_0000 | TLB | Yes |
| xkphys | 0x8000_0000_0000_0000 | varies | No |
| xkseg | 0xC000_0000_0000_0000 | TLB | Yes |
| ckseg0 | 0xFFFF_FFFF_8000_0000 | Yes | No |
| ckseg1 | 0xFFFF_FFFF_A000_0000 | No | No |

---

## SGI-Specific

- **ARCS firmware** provides console, memory map, boot device
- **GBE** graphics (O2) or Newport/Impact
- **CRIME/MACE** chipset on O2
- **HPC3** on Indy/Indigo2
- **IOC3** Ethernet on O2

---

## TLB Refill

- Software-managed TLB (no hardware page walk)
- Exception at 0x8000_0000 (32-bit) or 0x8000_0080 (XTLB)
- Kernel must handle refill in assembly

---

## Exit Criteria

- [ ] ARCS or DTB memory map parsed
- [ ] TLB initialized
- [ ] Exception vectors installed
- [ ] Cache coherent
- [ ] Works on QEMU Malta
- [ ] (Stretch) Works on SGI O2/Indy

---

*End of MIPS64 Boot Implementation*
