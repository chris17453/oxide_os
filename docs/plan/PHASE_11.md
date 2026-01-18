# Phase 11: Storage

**Stage:** 3 - Hardware
**Status:** Not Started
**Dependencies:** Phase 10 (Modules)

---

## Goal

Implement block device layer and filesystem drivers for persistent storage.

---

## Deliverables

| Item | Status |
|------|--------|
| Block device interface | [ ] |
| GPT partition parsing | [ ] |
| virtio-blk driver | [ ] |
| NVMe driver | [ ] |
| AHCI/SATA driver | [ ] |
| effluxfs (native filesystem) | [ ] |
| FAT32 driver | [ ] |
| ext2 driver (optional) | [ ] |

---

## Architecture Status

| Arch | Block | virtio | NVMe | AHCI | effluxfs | FAT32 | Done |
|------|-------|--------|------|------|----------|-------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Block Device Interface

```rust
pub trait BlockDevice: Send + Sync {
    /// Read blocks from device
    fn read(&self, block: u64, buf: &mut [u8]) -> Result<()>;

    /// Write blocks to device
    fn write(&self, block: u64, buf: &[u8]) -> Result<()>;

    /// Flush pending writes
    fn flush(&self) -> Result<()>;

    /// Get block size (typically 512 or 4096)
    fn block_size(&self) -> u32;

    /// Get total number of blocks
    fn block_count(&self) -> u64;
}
```

---

## Storage Stack

```
┌─────────────────────────────┐
│        Applications         │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│           VFS               │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│   Filesystem (effluxfs)     │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│    Block Device Layer       │
│  ┌────────────────────────┐ │
│  │   Request Queue        │ │
│  │   I/O Scheduler        │ │
│  └────────────────────────┘ │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│   Block Device Driver       │
│  (virtio-blk, NVMe, AHCI)   │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│      Physical Disk          │
└─────────────────────────────┘
```

---

## GPT Partition Table

```
┌────────────────────────────┐ LBA 0
│  Protective MBR            │
├────────────────────────────┤ LBA 1
│  GPT Header                │
│  - Signature: "EFI PART"   │
│  - First usable LBA        │
│  - Last usable LBA         │
│  - Partition entry LBA     │
├────────────────────────────┤ LBA 2-33
│  Partition Entries         │
│  (128 entries × 128 bytes) │
├────────────────────────────┤
│  ...                       │
│  Partition 1               │
│  ...                       │
├────────────────────────────┤
│  ...                       │
│  Partition N               │
│  ...                       │
├────────────────────────────┤
│  Backup Partition Entries  │
├────────────────────────────┤
│  Backup GPT Header         │
└────────────────────────────┘
```

---

## effluxfs Layout

```
┌────────────────────────────┐ Block 0
│  Superblock                │
│  - Magic: "EFFLUX"         │
│  - Version                 │
│  - Block size              │
│  - Total blocks            │
│  - Free blocks             │
│  - Root inode              │
├────────────────────────────┤ Block 1-N
│  Block Bitmap              │
├────────────────────────────┤
│  Inode Bitmap              │
├────────────────────────────┤
│  Inode Table               │
│  - Mode, size, timestamps  │
│  - Direct blocks (12)      │
│  - Indirect blocks (3)     │
│  - Extended attributes     │
├────────────────────────────┤
│  Data Blocks               │
│  ...                       │
└────────────────────────────┘
```

**effluxfs Features:**
- 64-bit block addresses
- Extended attributes (for AI metadata)
- Copy-on-write snapshots (future)
- Checksums on metadata
- Journal for crash recovery

---

## virtio-blk Driver

```rust
// virtio-blk request structure
#[repr(C)]
struct VirtioBlkReq {
    req_type: u32,    // 0=read, 1=write, 4=flush
    reserved: u32,
    sector: u64,
}

#[repr(C)]
struct VirtioBlkStatus {
    status: u8,       // 0=ok, 1=ioerr, 2=unsupported
}
```

---

## NVMe Driver Basics

```
NVMe Controller
├── Admin Queue (create I/O queues, identify)
├── I/O Submission Queue 1
├── I/O Completion Queue 1
├── I/O Submission Queue N (one per CPU)
└── I/O Completion Queue N

Submission Queue Entry (64 bytes):
- Opcode (read=0x02, write=0x01)
- Namespace ID
- Starting LBA
- Number of blocks
- PRP (Physical Region Page) list
```

---

## Key Files

```
crates/block/efflux-block/src/
├── lib.rs
├── device.rs          # Block device trait
├── request.rs         # I/O request queue
└── scheduler.rs       # I/O scheduler

crates/block/efflux-gpt/src/
├── lib.rs
└── parse.rs           # GPT parsing

crates/drivers/block/efflux-virtio-blk/src/
└── lib.rs

crates/drivers/block/efflux-nvme/src/
├── lib.rs
├── queue.rs           # Submission/completion queues
└── commands.rs        # NVMe commands

crates/drivers/block/efflux-ahci/src/
├── lib.rs
├── hba.rs             # Host bus adapter
└── port.rs            # Port handling

crates/fs/efflux-effluxfs/src/
├── lib.rs
├── superblock.rs
├── inode.rs
├── dir.rs
├── file.rs
└── journal.rs

crates/fs/efflux-fat32/src/
├── lib.rs
├── bpb.rs             # BIOS parameter block
├── fat.rs             # FAT table
└── dir.rs             # Directory entries
```

---

## Exit Criteria

- [ ] Block device abstraction works
- [ ] GPT partitions detected
- [ ] virtio-blk reads/writes
- [ ] NVMe driver functional
- [ ] AHCI driver functional
- [ ] effluxfs mounts and does file I/O
- [ ] FAT32 reads EFI system partition
- [ ] Works on all 8 architectures

---

## Test

```bash
# Create effluxfs filesystem
$ mkfs.effluxfs /dev/nvme0n1p2

# Mount it
$ mount /dev/nvme0n1p2 /mnt

# Test I/O
$ echo "Hello Storage" > /mnt/test.txt
$ cat /mnt/test.txt
Hello Storage

# Check filesystem
$ df /mnt
Filesystem     1K-blocks  Used Available Use% Mounted on
/dev/nvme0n1p2  10485760    16  10485744   1% /mnt
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 11 of EFFLUX Implementation*
