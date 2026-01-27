# i686 Memory Management

**Architecture:** i686
**Parent Spec:** [MEMORY_SPEC.md](../../MEMORY_SPEC.md)

---

## Page Table Structure (Non-PAE)

- **Levels:** 2 (PD → PT)
- **Entry size:** 4 bytes
- **Entries per table:** 1024
- **Page sizes:** 4KB, 4MB (with PSE)

---

## Page Table Structure (PAE)

- **Levels:** 3 (PDPT → PD → PT)
- **Entry size:** 8 bytes
- **PDPT entries:** 4
- **PD/PT entries:** 512
- **Page sizes:** 4KB, 2MB

---

## Virtual Address Layout (Non-PAE)

| Bits | Level | Size |
|------|-------|------|
| 31:22 | PD index | 4MB per entry |
| 21:12 | PT index | 4KB per entry |
| 11:0 | Offset | 4KB |

---

## PTE Flags

| Bit | Name | Description |
|-----|------|-------------|
| 0 | P | Present |
| 1 | RW | Read/Write |
| 2 | US | User/Supervisor |
| 3 | PWT | Write-through |
| 4 | PCD | Cache disable |
| 5 | A | Accessed |
| 6 | D | Dirty |
| 7 | PS | 4MB page (PDE only) |
| 8 | G | Global |

---

## Key Registers

| Register | Purpose |
|----------|---------|
| CR3 | Page directory base |
| CR4.PSE | Enable 4MB pages |
| CR4.PAE | Enable PAE mode |
| CR0.PG | Enable paging |

---

## TLB Management

- `invlpg [addr]` - Invalidate single page
- `mov cr3, eax` - Flush TLB

---

## Memory Layout (3GB/1GB)

```
0x00000000 - 0xBFFFFFFF  User (3GB)
0xC0000000 - 0xFFFFFFFF  Kernel (1GB)
```

---

## Exit Criteria

- [ ] 2-level paging working
- [ ] 4MB pages with PSE
- [ ] PAE mode optional

---

*End of i686 Memory Management*
