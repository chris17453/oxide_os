# x86_64 Memory Management

**Architecture:** x86_64
**Parent Spec:** [MEMORY_SPEC.md](../../MEMORY_SPEC.md)

---

## Page Table Structure

- **Levels:** 4 (PML4 → PDPT → PD → PT) or 5 with LA57
- **Entry size:** 8 bytes
- **Entries per table:** 512
- **Page sizes:** 4KB, 2MB (huge), 1GB (giant)

---

## Virtual Address Layout (48-bit)

| Bits | Level | Size |
|------|-------|------|
| 47:39 | PML4 index | 512GB per entry |
| 38:30 | PDPT index | 1GB per entry |
| 29:21 | PD index | 2MB per entry |
| 20:12 | PT index | 4KB per entry |
| 11:0 | Page offset | 4KB |

---

## Page Table Entry Flags

| Bit | Name | Description |
|-----|------|-------------|
| 0 | P | Present |
| 1 | RW | Read/Write |
| 2 | US | User/Supervisor |
| 3 | PWT | Write-through |
| 4 | PCD | Cache disable |
| 5 | A | Accessed |
| 6 | D | Dirty |
| 7 | PS | Page size (2MB/1GB) |
| 8 | G | Global |
| 63 | NX | No execute |

---

## Key Registers

| Register | Purpose |
|----------|---------|
| CR3 | Page table base (PML4 physical address) |
| CR4.PAE | Must be 1 for long mode |
| CR4.LA57 | Enable 5-level paging |
| CR4.PCIDE | Enable PCID |

---

## TLB Management

- `invlpg [addr]` - Invalidate single page
- `mov cr3, rax` - Flush entire TLB (except global)
- `invpcid` - Invalidate by PCID

---

## Memory Layout

```
0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF  User (128TB)
0xFFFF_8000_0000_0000 - 0xFFFF_87FF_FFFF_FFFF  Direct map
0xFFFF_8800_0000_0000+                         Kernel text/data
```

---

## Exit Criteria

- [ ] 4-level paging working
- [ ] 2MB huge pages supported
- [ ] NX bit enforced
- [ ] PCID used if available

---

*End of x86_64 Memory Management*
