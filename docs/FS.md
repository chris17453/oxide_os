# ext4 Filesystem Implementation Progress

## Overview

Implement complete ext4 filesystem support for OXIDE OS including:
- Full read-write ext4 driver
- VirtIO-blk disk I/O (currently stubbed)
- GPT partition integration
- Boot-time mounting of ext4 root partition
- `mkfs.ext4` userspace utility

---

## Progress Tracking

### Phase 1: VirtIO-blk I/O (~400 LOC)
- [x] Initialize virtqueue in `probe()` (descriptor table, available/used rings)
- [x] Implement `read()` (build request, submit to virtqueue, poll completion)
- [x] Implement `write()` (same flow with write request type)
- [x] Implement `flush()` (flush request type)
- [x] Add completion handling (polling-based)

**Status:** Complete
**File:** `crates/drivers/block/virtio-blk/src/lib.rs`

---

### Phase 2: ext4 Core Read Support (~1200 LOC)

#### superblock.rs (~200 LOC)
- [x] Parse superblock from block device (offset 1024)
- [x] Validate magic (0xEF53)
- [x] Extract parameters (block size, inodes per group, etc.)
- [x] Check feature flags

#### group_desc.rs (~150 LOC)
- [x] Read block group descriptor table
- [x] Handle 32/64-byte variants
- [x] Cache in memory

#### inode.rs (~300 LOC)
- [x] Calculate inode location from number
- [x] Read inode from disk
- [x] Parse mode, size, timestamps
- [x] Detect extent vs indirect mode

#### extent.rs (~250 LOC)
- [x] Parse extent header (magic 0xF30A)
- [x] Traverse tree (recursive for depth > 0)
- [x] Map logical block to physical

#### dir.rs (~200 LOC)
- [x] Iterate directory entries
- [x] Lookup by name
- [x] Handle file_type field

#### file.rs (~100 LOC)
- [x] Read file data via extents

**Status:** Complete
**Files:** `crates/fs/ext4/src/*.rs`

---

### Phase 3: ext4 Write Support (~800 LOC)

#### bitmap.rs (~200 LOC)
- [ ] Read block bitmap
- [ ] Find free block
- [ ] Allocate block (set bit)
- [ ] Free block (clear bit)
- [ ] Same operations for inode bitmap

#### inode.rs additions (~150 LOC)
- [ ] Write inode back to disk
- [ ] Update timestamps
- [ ] Update size, blocks count

#### extent.rs additions (~200 LOC)
- [ ] Insert new extent
- [ ] Split extents if needed
- [ ] Handle tree growth

#### dir.rs additions (~150 LOC)
- [ ] Add directory entry
- [ ] Remove directory entry
- [ ] Grow directory

#### file.rs additions (~100 LOC)
- [ ] Write file data
- [ ] Extend file (allocate blocks)
- [ ] Truncate file (free blocks)

**Status:** Not Started

---

### Phase 4: VnodeOps Integration (~400 LOC)

- [x] `vtype()` - from inode mode
- [x] `lookup()` - directory search
- [x] `read()` - file data via extents
- [ ] `write()` - file data with allocation (requires write support)
- [x] `readdir()` - directory iteration
- [x] `stat()` - inode metadata
- [ ] `create()` - new file (requires write support)
- [ ] `mkdir()` - new directory (requires write support)
- [ ] `unlink()` - remove file (requires write support)
- [ ] `rmdir()` - remove directory (requires write support)
- [ ] `truncate()` - resize file (requires write support)
- [ ] `rename()` - move/rename (requires write support)
- [x] `readlink()` - read symlink target

**Status:** Read-only operations complete
**File:** `crates/fs/ext4/src/vnode.rs`

---

### Phase 5: Journal Support (~400 LOC)

#### Recovery on mount
- [ ] Read journal superblock
- [ ] Replay committed transactions
- [ ] Mark journal clean

#### Transaction support
- [ ] Begin transaction
- [ ] Log metadata changes
- [ ] Commit transaction

**Status:** Not Started
**File:** `crates/fs/ext4/src/journal.rs`

---

### Phase 6: Boot Integration (~150 LOC)

- [ ] Initialize VirtIO-blk devices
- [ ] Register block devices
- [ ] Parse GPT partitions
- [ ] Find ext4 root partition
- [ ] Mount ext4 at `/`

**Status:** Not Started
**File:** `kernel/src/init.rs`

---

### Phase 7: mkfs.ext4 Utility (~600 LOC)

- [ ] Parse command line (device, block size, label)
- [ ] Initialize superblock with defaults
- [ ] Calculate block groups
- [ ] Write block group descriptors
- [ ] Initialize bitmaps (all free except metadata)
- [ ] Create root inode (inode 2)
- [ ] Create lost+found directory
- [ ] Write superblock

**Status:** Not Started
**Files:** `userspace/mkfs-ext4/src/main.rs`

---

## Key Data Structures

### Superblock (1024 bytes at offset 1024)
```rust
#[repr(C)]
pub struct Ext4Superblock {
    pub s_inodes_count: u32,
    pub s_blocks_count_lo: u32,
    pub s_free_blocks_count_lo: u32,
    pub s_free_inodes_count: u32,
    pub s_first_data_block: u32,
    pub s_log_block_size: u32,        // block_size = 1024 << this
    pub s_blocks_per_group: u32,
    pub s_inodes_per_group: u32,
    pub s_magic: u16,                 // 0xEF53
    pub s_inode_size: u16,            // 128, 256, or 512
    pub s_feature_compat: u32,
    pub s_feature_incompat: u32,
    pub s_feature_ro_compat: u32,
    // ... 64-bit fields at offset 0x150+
}
```

### Block Group Descriptor (32 or 64 bytes)
```rust
#[repr(C)]
pub struct Ext4GroupDesc {
    pub bg_block_bitmap: u64,
    pub bg_inode_bitmap: u64,
    pub bg_inode_table: u64,
    pub bg_free_blocks_count: u32,
    pub bg_free_inodes_count: u32,
    pub bg_used_dirs_count: u32,
}
```

### Inode (128-512 bytes)
```rust
#[repr(C)]
pub struct Ext4Inode {
    pub i_mode: u16,
    pub i_uid: u16,
    pub i_size_lo: u32,
    pub i_atime: u32,
    pub i_ctime: u32,
    pub i_mtime: u32,
    pub i_gid: u16,
    pub i_links_count: u16,
    pub i_blocks_lo: u32,
    pub i_flags: u32,
    pub i_block: [u32; 15],  // Extent tree or block pointers
}
```

### Extent Tree
```rust
#[repr(C)]
pub struct Ext4ExtentHeader {
    pub eh_magic: u16,      // 0xF30A
    pub eh_entries: u16,
    pub eh_max: u16,
    pub eh_depth: u16,
}

#[repr(C)]
pub struct Ext4Extent {
    pub ee_block: u32,      // Logical block
    pub ee_len: u16,        // Length
    pub ee_start_hi: u16,   // Physical block high
    pub ee_start_lo: u32,   // Physical block low
}
```

---

## Files Summary

### Files to Modify
| File | Changes |
|------|---------|
| `crates/drivers/block/virtio-blk/src/lib.rs` | Complete I/O implementation |
| `kernel/src/init.rs` | Block device init, ext4 mount |
| `kernel/Cargo.toml` | Add ext4 dependency |
| `Cargo.toml` (workspace) | Add ext4 crate |
| `Makefile` | Add mkfs-ext4 to build |

### Files to Create
| File | Purpose |
|------|---------|
| `crates/fs/ext4/Cargo.toml` | Crate manifest |
| `crates/fs/ext4/src/lib.rs` | Main module |
| `crates/fs/ext4/src/superblock.rs` | Superblock handling |
| `crates/fs/ext4/src/group_desc.rs` | Block group descriptors |
| `crates/fs/ext4/src/inode.rs` | Inode operations |
| `crates/fs/ext4/src/extent.rs` | Extent tree |
| `crates/fs/ext4/src/dir.rs` | Directory operations |
| `crates/fs/ext4/src/bitmap.rs` | Allocation bitmaps |
| `crates/fs/ext4/src/file.rs` | File operations |
| `crates/fs/ext4/src/vnode.rs` | VFS integration |
| `crates/fs/ext4/src/journal.rs` | Journal support |
| `crates/fs/ext4/src/error.rs` | Error types |
| `userspace/mkfs-ext4/Cargo.toml` | mkfs manifest |
| `userspace/mkfs-ext4/src/main.rs` | mkfs implementation |

---

## Estimated Total LOC

| Component | Lines |
|-----------|-------|
| VirtIO-blk I/O | 400 |
| ext4 read support | 1,200 |
| ext4 write support | 800 |
| VnodeOps | 400 |
| Journal | 400 |
| Boot integration | 150 |
| mkfs.ext4 | 600 |
| **Total** | **~3,950** |

---

## Testing Strategy

1. **Create test ext4 image on host:**
   ```bash
   dd if=/dev/zero of=test.ext4 bs=1M count=64
   mkfs.ext4 test.ext4
   mount test.ext4 /mnt && echo "test" > /mnt/file.txt && umount /mnt
   ```

2. **Boot QEMU with test disk:**
   ```bash
   qemu-system-x86_64 ... -drive file=test.ext4,format=raw,if=virtio
   ```

3. **Test operations:**
   - Mount ext4 partition
   - Read existing files
   - Create new files
   - Create directories
   - Delete files
   - Verify persistence across reboots

---

## Implementation Notes

*(Add notes here as implementation progresses)*
