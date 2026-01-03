# efflux.fs — Filesystem Specification

**Version:** 1.0  
**Status:** Draft  
**License:** MIT  

---

## 0) Overview

efflux.fs is the native filesystem for EFFLUX OS. It is designed with:

- Modern on-disk structures (B-tree, extents, COW)
- AI-native extended metadata (embeddings, tags, semantic data)
- Built-in encryption and signing support
- Crash safety via journaling and COW
- Scalability for large files and many files

---

## 1) Design Principles

1. **Reliability first.** Multiple superblock copies, checksums everywhere, crash-safe updates.
2. **AI-native metadata.** Extended attributes for embeddings, tags, descriptions are first-class.
3. **Security built-in.** Per-file encryption, signing, integrity verification.
4. **Scalability.** B-trees for directories and extents. Handles billions of files.
5. **Copy-on-write optional.** COW mode for snapshots and crash safety. Traditional mode for performance.

---

## 2) On-Disk Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ Block 0: Boot sector (reserved, unused by efflux.fs)            │
├─────────────────────────────────────────────────────────────────┤
│ Block 1: Primary Superblock                                     │
├─────────────────────────────────────────────────────────────────┤
│ Block 2-N: Block Group Descriptors                              │
├─────────────────────────────────────────────────────────────────┤
│ Block Group 0                                                   │
│   ├── Block Bitmap                                              │
│   ├── Inode Bitmap                                              │
│   ├── Inode Table                                               │
│   └── Data Blocks                                               │
├─────────────────────────────────────────────────────────────────┤
│ Block Group 1                                                   │
│   └── ...                                                       │
├─────────────────────────────────────────────────────────────────┤
│ ...                                                             │
├─────────────────────────────────────────────────────────────────┤
│ Backup Superblock (at block group N)                            │
├─────────────────────────────────────────────────────────────────┤
│ Journal (dedicated region or file)                              │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3) Block Sizes

| Block size | Use case |
|------------|----------|
| 4 KiB | Default, general purpose |
| 8 KiB | Large files, fewer metadata blocks |
| 16 KiB | Very large files |
| 64 KiB | Specialized workloads |

Block size is fixed at filesystem creation.

---

## 4) Superblock

Located at block 1. Backup copies at start of select block groups (e.g., groups 0, 1, 3, 5, 7, 9, 25, 49, ...).

```rust
#[repr(C)]
pub struct Superblock {
    // Identity
    pub magic: u32,                     // 0x45464653 "EFFS"
    pub version_major: u16,
    pub version_minor: u16,
    pub uuid: [u8; 16],
    pub label: [u8; 64],                // UTF-8 volume label
    
    // Geometry
    pub block_size_log2: u8,            // 12 = 4K, 13 = 8K, etc.
    pub blocks_total: u64,
    pub blocks_free: u64,
    pub blocks_reserved: u64,           // Reserved for root
    
    // Inodes
    pub inodes_total: u64,
    pub inodes_free: u64,
    pub inode_size: u16,                // Bytes per inode (256, 512, etc.)
    pub first_inode: u32,               // First non-reserved inode
    
    // Block groups
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub block_group_count: u32,
    
    // Features
    pub features_compat: u64,
    pub features_incompat: u64,
    pub features_ro_compat: u64,
    
    // Timestamps
    pub created_at: u64,                // Unix timestamp
    pub mounted_at: u64,
    pub written_at: u64,
    pub mount_count: u32,
    pub max_mount_count: u32,           // fsck after this many mounts
    
    // State
    pub state: u16,                     // Clean, dirty, error
    pub errors_behavior: u16,           // Continue, remount-ro, panic
    
    // Journal
    pub journal_inode: u64,
    pub journal_uuid: [u8; 16],
    
    // Encryption
    pub encryption_algo: u8,            // 0=none, 1=AES-256-GCM, 2=ChaCha20
    pub kdf_algo: u8,                   // 0=none, 1=Argon2id
    pub master_key_hash: [u8; 32],      // For verification
    
    // Root CA fingerprint (for trust)
    pub root_ca_fingerprint: [u8; 32],
    
    // Checksums
    pub checksum_algo: u8,              // 0=CRC32, 1=BLAKE3
    pub superblock_checksum: [u8; 32],
    
    // Padding to 1024 bytes
    pub reserved: [u8; ...],
}
```

### 4.1 Feature Flags

**Compatible features** (can mount without support):
```rust
pub const COMPAT_DIR_INDEX: u64      = 1 << 0;  // B-tree directories
pub const COMPAT_EXTENDED_META: u64  = 1 << 1;  // AI metadata
pub const COMPAT_SPARSE_SUPER: u64   = 1 << 2;  // Sparse superblock copies
```

**Incompatible features** (must support to mount):
```rust
pub const INCOMPAT_64BIT: u64        = 1 << 0;  // 64-bit block numbers
pub const INCOMPAT_EXTENTS: u64      = 1 << 1;  // Extent trees
pub const INCOMPAT_COW: u64          = 1 << 2;  // Copy-on-write mode
pub const INCOMPAT_ENCRYPTION: u64   = 1 << 3;  // Per-file encryption
pub const INCOMPAT_COMPRESSION: u64  = 1 << 4;  // Transparent compression
```

**Read-only compatible features** (can mount read-only without support):
```rust
pub const RO_COMPAT_BTREE_DIR: u64   = 1 << 0;  // B-tree directories
pub const RO_COMPAT_LARGE_FILE: u64  = 1 << 1;  // Files > 2GB
pub const RO_COMPAT_HUGE_FILE: u64   = 1 << 2;  // Files > 2TB
```

---

## 5) Block Group Descriptor

```rust
#[repr(C)]
pub struct BlockGroupDescriptor {
    pub block_bitmap_block: u64,
    pub inode_bitmap_block: u64,
    pub inode_table_block: u64,
    pub free_blocks_count: u32,
    pub free_inodes_count: u32,
    pub used_dirs_count: u32,
    pub flags: u16,
    pub checksum: u32,
    pub reserved: [u8; 12],
}
```

---

## 6) Inode

```rust
#[repr(C)]
pub struct Inode {
    // Standard fields
    pub mode: u16,                      // File type and permissions
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub atime: u64,                     // Access time (nanoseconds)
    pub ctime: u64,                     // Change time
    pub mtime: u64,                     // Modification time
    pub crtime: u64,                    // Creation time
    pub links_count: u32,
    pub blocks_count: u64,              // 512-byte blocks
    pub flags: u32,
    
    // Data location
    pub extent_tree_root: ExtentHeader, // Inline extent tree (60 bytes)
    
    // Extended attributes
    pub xattr_block: u64,               // Block with extended attributes
    pub xattr_inline_size: u16,         // Size of inline xattrs
    
    // Security
    pub content_hash: [u8; 32],         // BLAKE3 hash of content
    pub signature_block: u64,           // Block with signature data
    pub encryption_key_id: u32,         // Key slot for encryption
    pub integrity_flags: u16,
    
    // AI metadata (inline for small, pointer for large)
    pub ai_meta_flags: u16,
    pub embedding_tier: u8,
    pub embedding_block: u64,           // Block with embedding vector
    
    // Generation and version
    pub generation: u32,
    pub version: u64,
    
    // Checksum
    pub checksum: u32,
    
    // Inline data or extent tree continues here
    pub inline_data: [u8; ...],         // Remaining space
}
```

### 6.1 Inode Flags

```rust
pub const INODE_SYNC: u32           = 1 << 0;   // Synchronous updates
pub const INODE_IMMUTABLE: u32      = 1 << 1;   // Cannot modify
pub const INODE_APPEND: u32         = 1 << 2;   // Append only
pub const INODE_NODUMP: u32         = 1 << 3;   // Don't dump
pub const INODE_NOATIME: u32        = 1 << 4;   // Don't update atime
pub const INODE_ENCRYPTED: u32      = 1 << 5;   // File is encrypted
pub const INODE_COMPRESSED: u32     = 1 << 6;   // File is compressed
pub const INODE_SIGNED: u32         = 1 << 7;   // File has signature
pub const INODE_VERIFIED: u32       = 1 << 8;   // Signature verified
pub const INODE_COW: u32            = 1 << 9;   // Copy-on-write
pub const INODE_INLINE_DATA: u32    = 1 << 10;  // Data in inode
pub const INODE_HAS_EMBEDDING: u32  = 1 << 11;  // Has AI embedding
```

### 6.2 Integrity Flags

```rust
pub const INTEGRITY_VERIFY_READ: u16  = 1 << 0; // Verify hash on read
pub const INTEGRITY_VERIFY_EXEC: u16  = 1 << 1; // Verify before execute
pub const INTEGRITY_SEALED: u16       = 1 << 2; // Immutable + signed
```

---

## 7) Extent Tree

Efficient storage for file block mappings.

```rust
#[repr(C)]
pub struct ExtentHeader {
    pub magic: u16,                     // 0xF30A
    pub entries: u16,                   // Number of entries
    pub max_entries: u16,               // Max entries in this node
    pub depth: u16,                     // 0 = leaf, >0 = index
    pub generation: u32,
}

#[repr(C)]
pub struct ExtentIndex {
    pub logical_block: u64,             // First logical block this covers
    pub leaf_block: u64,                // Block containing child node
}

#[repr(C)]
pub struct Extent {
    pub logical_block: u64,             // First logical block
    pub physical_block: u64,            // First physical block
    pub length: u32,                    // Number of blocks (max 2^32)
    pub flags: u16,
    pub checksum: u16,
}
```

### 7.1 Extent Flags

```rust
pub const EXTENT_UNWRITTEN: u16   = 1 << 0;  // Allocated but not written
pub const EXTENT_ENCRYPTED: u16   = 1 << 1;  // Extent is encrypted
pub const EXTENT_COMPRESSED: u16  = 1 << 2;  // Extent is compressed
```

---

## 8) Directory Entries

### 8.1 Linear Directory (small directories)

```rust
#[repr(C)]
pub struct DirEntry {
    pub inode: u64,
    pub rec_len: u16,                   // Total entry length
    pub name_len: u8,
    pub file_type: u8,
    pub name: [u8; ...],                // Variable length, max 255
}
```

### 8.2 B-Tree Directory (large directories)

For directories with many entries, a B-tree is used.

```rust
#[repr(C)]
pub struct DirBtreeHeader {
    pub magic: u32,                     // 0x44425452 "DBTR"
    pub root_block: u64,
    pub height: u16,
    pub entry_count: u64,
}

#[repr(C)]
pub struct DirBtreeNode {
    pub magic: u16,
    pub level: u16,                     // 0 = leaf
    pub count: u16,
    pub entries: [DirBtreeEntry; ...],
}

#[repr(C)]
pub struct DirBtreeEntry {
    pub hash: u64,                      // Filename hash (for lookup)
    pub inode: u64,
    pub name_len: u8,
    pub file_type: u8,
    pub name: [u8; ...],
}
```

Hash function: BLAKE3 truncated to 64 bits.

---

## 9) Extended Attributes

### 9.1 Inline Extended Attributes

Small xattrs stored directly after inode.

```rust
#[repr(C)]
pub struct XattrInlineHeader {
    pub magic: u32,                     // 0x58415452 "XATR"
    pub count: u16,
    pub used_size: u16,
}

#[repr(C)]
pub struct XattrEntry {
    pub name_len: u8,
    pub value_len: u16,
    pub name_index: u8,                 // Namespace (user, system, security, trusted)
    pub name: [u8; ...],
    pub value: [u8; ...],
}
```

### 9.2 External Extended Attributes

Large xattrs stored in separate block.

```rust
#[repr(C)]
pub struct XattrBlockHeader {
    pub magic: u32,                     // 0x58414232 "XAB2"
    pub refcount: u32,                  // Shared across inodes
    pub blocks: u32,
    pub hash: u32,
    pub checksum: u32,
}
```

### 9.3 Standard Extended Attribute Names

| Namespace | Name | Description |
|-----------|------|-------------|
| user | efflux.type | MIME type |
| user | efflux.description | Human description |
| user | efflux.tags | Comma-separated tags |
| user | efflux.source | Origin URL/path |
| user | efflux.created_by | Creator identity |
| user | efflux.ai_summary | AI-generated summary |
| user | efflux.confidence | AI confidence score |
| security | efflux.signature | Detached signature |
| security | efflux.signer_cert | Signer certificate |
| security | efflux.trust_level | Trust level (system/vendor/user/untrusted) |
| security | efflux.encryption_iv | Encryption IV |
| system | efflux.embedding | Vector embedding |
| system | efflux.relationships | Related files (JSON) |

---

## 10) AI Metadata Block

For embeddings and large AI metadata.

```rust
#[repr(C)]
pub struct AiMetaBlock {
    pub magic: u32,                     // 0x41494D42 "AIMB"
    pub version: u16,
    pub embedding_tier: u8,             // 0=micro, 1=small, 2=medium, 3=large
    pub embedding_dims: u16,            // 128, 384, 768, 1536
    pub embedding_offset: u32,          // Offset to embedding data
    pub embedding_size: u32,            // Size in bytes
    pub model_id: [u8; 32],             // Model identifier hash
    pub generated_at: u64,
    pub checksum: u32,
    // Embedding data follows
}
```

### 10.1 Embedding Tiers

| Tier | Dimensions | Size | Use case |
|------|------------|------|----------|
| micro | 128 | 512 bytes | Filenames, short text |
| small | 384 | 1.5 KB | General files |
| medium | 768 | 3 KB | Documents, code |
| large | 1536 | 6 KB | High-fidelity search |

---

## 11) Signature Block

```rust
#[repr(C)]
pub struct SignatureBlock {
    pub magic: u32,                     // 0x53494742 "SIGB"
    pub version: u16,
    pub algo: u16,                      // 0=Ed25519, 1=RSA-4096
    pub signed_at: u64,
    pub signer_fingerprint: [u8; 32],
    pub signature_len: u16,
    pub cert_len: u16,
    pub signature: [u8; ...],           // Variable length
    pub certificate: [u8; ...],         // X.509 DER
}
```

---

## 12) Encryption

### 12.1 Per-File Encryption

Each encrypted file has:
- Unique file encryption key (FEK)
- FEK encrypted with master encryption key (MEK)
- IV stored in extended attributes

```rust
#[repr(C)]
pub struct EncryptionContext {
    pub algo: u8,                       // 0=AES-256-GCM, 1=ChaCha20-Poly1305
    pub key_slot: u32,                  // Reference to encrypted FEK
    pub iv: [u8; 16],
    pub auth_tag: [u8; 16],             // For authenticated encryption
}
```

### 12.2 Key Slots

```rust
#[repr(C)]
pub struct KeySlot {
    pub flags: u32,
    pub key_algo: u8,
    pub kdf_algo: u8,
    pub kdf_salt: [u8; 32],
    pub kdf_iterations: u32,
    pub kdf_memory: u32,                // For Argon2
    pub encrypted_key: [u8; 64],        // FEK encrypted with derived key
}
```

---

## 13) Journal

Write-ahead logging for crash safety.

### 13.1 Journal Superblock

```rust
#[repr(C)]
pub struct JournalSuperblock {
    pub magic: u32,                     // 0x4A524E4C "JRNL"
    pub version: u32,
    pub block_size: u32,
    pub block_count: u64,
    pub first_block: u64,
    pub sequence: u64,                  // Current sequence number
    pub head: u64,                      // First valid block
    pub tail: u64,                      // Last valid block
    pub checksum: u32,
}
```

### 13.2 Journal Block Types

```rust
pub const JOURNAL_DESCRIPTOR: u32 = 1;  // Describes following blocks
pub const JOURNAL_COMMIT: u32 = 2;      // Transaction commit
pub const JOURNAL_REVOKE: u32 = 3;      // Block revocation
```

### 13.3 Transaction Flow

1. Begin transaction
2. Write descriptor block (lists blocks being modified)
3. Write data blocks
4. Write commit block
5. Checkpoint: write dirty blocks to final locations
6. Update journal head

---

## 14) Copy-on-Write Mode

Optional COW mode for snapshots and additional crash safety.

### 14.1 COW B-Tree

When COW is enabled, all trees (extent, directory) use COW semantics:
- Never modify in place
- Clone path from leaf to root
- Old roots retained for snapshots

### 14.2 Snapshot

```rust
#[repr(C)]
pub struct Snapshot {
    pub id: u64,
    pub created_at: u64,
    pub root_inode: u64,                // Root directory at snapshot time
    pub name: [u8; 64],
    pub flags: u32,
}
```

Snapshots stored in reserved inode (inode 8).

---

## 15) Reserved Inodes

| Inode | Purpose |
|-------|---------|
| 1 | Bad blocks |
| 2 | Root directory |
| 3 | ACL index (reserved) |
| 4 | ACL data (reserved) |
| 5 | Bootloader |
| 6 | Undelete directory (reserved) |
| 7 | Journal |
| 8 | Snapshots |
| 9 | Key slots (encryption) |
| 10 | Trust store (certificates) |
| 11 | First non-reserved inode (configurable) |

---

## 16) Special Files

### 16.1 Trust Store (Inode 10)

Stores trusted certificates for this filesystem.

```rust
#[repr(C)]
pub struct TrustStoreHeader {
    pub magic: u32,                     // 0x54525354 "TRST"
    pub version: u16,
    pub cert_count: u32,
    pub revoked_count: u32,
}

#[repr(C)]
pub struct TrustStoreEntry {
    pub fingerprint: [u8; 32],
    pub trust_level: u8,
    pub flags: u8,
    pub added_at: u64,
    pub expires_at: u64,
    pub cert_offset: u32,
    pub cert_len: u16,
}
```

---

## 17) Filesystem Operations

### 17.1 Mount

1. Read primary superblock
2. Verify magic and checksum
3. Check feature flags
4. Replay journal if dirty
5. Load block group descriptors
6. Ready

### 17.2 Create File

1. Allocate inode from bitmap
2. Initialize inode fields
3. Add directory entry
4. Write to journal
5. Commit

### 17.3 Write File

1. Allocate blocks if needed
2. If COW: clone extent tree path
3. Write data blocks
4. Update inode size/mtime
5. If signed: invalidate signature
6. Write to journal
7. Commit

### 17.4 Read File

1. Lookup inode
2. If encrypted: decrypt
3. If INTEGRITY_VERIFY_READ: verify hash
4. Walk extent tree
5. Read data blocks
6. Return data

### 17.5 Sign File

1. Compute BLAKE3 hash of content
2. Sign hash with user's private key
3. Store signature in signature block
4. Store certificate reference
5. Set INODE_SIGNED flag
6. Update content_hash in inode

### 17.6 Verify File

1. Read signature block
2. Verify certificate chain to trusted root
3. Check certificate not revoked
4. Compute content hash
5. Verify signature against hash
6. Set INODE_VERIFIED flag if valid

---

## 18) Utilities

### 18.1 mkfs.efflux

```bash
mkfs.efflux [options] <device>

Options:
  -L <label>          Volume label
  -b <size>           Block size (4096, 8192, 16384, 65536)
  -i <ratio>          Bytes per inode ratio
  -N <count>          Number of inodes
  -O <features>       Enable features (extents,encryption,cow,compression)
  -E <options>        Extended options
  -U <uuid>           Set UUID
  --encrypt           Enable encryption (prompts for passphrase)
  --cow               Enable copy-on-write mode
```

### 18.2 fsck.efflux

```bash
fsck.efflux [options] <device>

Options:
  -a                  Auto-repair
  -n                  No changes (check only)
  -y                  Assume yes to all questions
  -f                  Force check even if clean
  -v                  Verbose
  --verify-hashes     Verify all content hashes
  --verify-signatures Verify all signatures
```

### 18.3 efflux-tune

```bash
efflux-tune [options] <device>

Options:
  -L <label>          Set volume label
  -O <features>       Enable/disable features
  -r <blocks>         Set reserved blocks
  --add-key           Add encryption key slot
  --remove-key        Remove encryption key slot
  --change-key        Change encryption passphrase
```

---

## 19) Compatibility

### 19.1 Forward Compatibility

Unknown compatible features are ignored. Unknown incompatible features prevent mount.

### 19.2 Version Policy

- Major version change: incompatible on-disk format change
- Minor version change: compatible additions

### 19.3 Migration

Tools provided to:
- Convert ext4 to efflux.fs (offline)
- Add encryption to existing filesystem
- Enable COW mode (requires reformat)

---

## 20) Performance Considerations

### 20.1 Allocation Strategies

- Block group locality: keep file data near inode
- Preallocation: allocate contiguous extents for large files
- Delayed allocation: batch allocations for better contiguity

### 20.2 Caching

- Inode cache
- Directory entry cache
- Extent tree cache
- Embedding cache (for search)

### 20.3 Concurrency

- Per-inode locks
- Per-block-group allocation locks
- Journal lock
- Read-write locks for trees

---

## 21) Limits

| Limit | Value |
|-------|-------|
| Max file size | 16 EiB (2^64 bytes) |
| Max filesystem size | 16 EiB |
| Max filename length | 255 bytes |
| Max path length | 4096 bytes |
| Max hard links | 2^32 |
| Max extended attribute size | 64 KiB |
| Max embedding size | 6 KiB (1536 dimensions) |

---

## 22) Implementation Phases

### Phase 1: Basic Filesystem
- [ ] Superblock read/write
- [ ] Block group management
- [ ] Inode operations
- [ ] Linear directories
- [ ] Extent tree (read-only)
- [ ] Basic read/write

### Phase 2: Full Features
- [ ] Extent tree (read-write)
- [ ] B-tree directories
- [ ] Extended attributes
- [ ] Journal
- [ ] fsck

### Phase 3: Security
- [ ] Content hashing
- [ ] File signing
- [ ] Signature verification
- [ ] Per-file encryption
- [ ] Key management

### Phase 4: AI Metadata
- [ ] Embedding storage
- [ ] AI metadata blocks
- [ ] Indexer integration

### Phase 5: Advanced
- [ ] COW mode
- [ ] Snapshots
- [ ] Compression
- [ ] Online resize

---

## 23) Testing

### 23.1 Unit Tests
- Superblock serialization
- Extent tree operations
- B-tree operations
- Hash/signature verification

### 23.2 Integration Tests
- Mount/unmount cycle
- File operations
- Directory operations
- Concurrent access

### 23.3 Stress Tests
- Large file creation
- Many small files
- Deep directory trees
- Power failure simulation

### 23.4 Fuzz Tests
- Corrupted superblock handling
- Invalid extent trees
- Malformed directories

---

*End of efflux.fs Specification*
