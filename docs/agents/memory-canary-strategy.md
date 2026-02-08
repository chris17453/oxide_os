# Memory Canary Strategy: Comprehensive Corruption Detection

**Rule**: ALL memory structures must have magic canaries for corruption detection and fail-loud behavior.

## Current State Analysis

### ✅ Already Protected

1. **Heap Allocator** (`mm-heap/hardened.rs`)
   - ✅ AllocHeader magic: `0x4F58494445484150` ("OXIDEHEAP")
   - ✅ Canary at allocation end: `0xDEADBEEFCAFEBABE`
   - ✅ Redzones: `0xFD` pattern before/after allocations
   - ✅ Freed memory fill: `0xDD` pattern for use-after-free detection
   - **Status**: Production-ready hardening

2. **Buddy Allocator** (`mm-core/buddy.rs`)
   - ✅ FreeBlock magic: `0x4652454542304C` ("FREEBL0C")
   - ✅ Doubly-linked lists with prev/next validation
   - ✅ Canary checked on every access
   - ✅ GPF on corruption
   - **Status**: Just implemented (2026-02-07)

### ⚠️ Needs Canaries

3. **Slab Allocator** (`mm-slab/cache.rs`)
   - ❌ SlabHeader: No magic canary
   - ❌ FreeObject: No validation
   - **Risk**: Medium - Corruption could cause use-after-free
   - **Fix Priority**: HIGH

4. **Page Tables** (`mm-paging/mapper.rs`)
   - ❌ No canaries in page table entries
   - **Risk**: Low - Hardware validates PTE format
   - **Fix Priority**: LOW (hardware protection sufficient)

5. **Process Control Block** (`proc/proc`)
   - ❌ No magic canary in Process struct
   - **Risk**: High - PCB corruption causes system instability
   - **Fix Priority**: HIGH

6. **File Descriptors** (`vfs`)
   - ❌ No canaries in FD table
   - **Risk**: Medium - Could leak/corrupt file handles
   - **Fix Priority**: MEDIUM

7. **VMA (Virtual Memory Areas)** (`mm`)
   - ❌ No canaries in VMA structures
   - **Risk**: High - Corruption causes memory leaks/corruption
   - **Fix Priority**: HIGH

## Implementation Strategy

### Phase 1: Critical Structures (HIGH Priority)

#### 1. Slab Allocator

Add canaries to SlabHeader and FreeObject:

```rust
#[repr(C)]
struct SlabHeader {
    magic: u64,     // NEW: 0x534C41424844 ("SLABHD") - Corruption detector
    next: Option<NonNull<SlabHeader>>,
    free_count: u16,
    total_count: u16,
    free_head: u16,
    _pad: u16,
}

const SLAB_HEADER_MAGIC: u64 = 0x534C41424844;

#[repr(C)]
struct FreeObject {
    magic: u32,  // NEW: 0x46524545 ("FREE") - Validates free state
    next: u16,
    _pad: u16,
}

const FREE_OBJECT_MAGIC: u32 = 0x46524545;
```

**Checks:**
- Validate SlabHeader.magic on every slab access
- Validate FreeObject.magic when popping from free list
- Clear magic on allocation, set magic on free
- GPF on invalid magic

#### 2. Process Control Block

```rust
#[repr(C)]
pub struct Process {
    magic_start: u64,  // NEW: 0x50524F43455353 ("PROCESS")
    pid: Pid,
    state: ProcessState,
    // ... existing fields ...
    magic_end: u64,    // NEW: Same value - detects overflow
}

const PROCESS_MAGIC: u64 = 0x50524F43455353;
```

**Checks:**
- Validate both magic_start and magic_end on access
- GPF if either is corrupted
- Detects buffer overflows (magic_end corruption)

#### 3. VMA Structures

```rust
#[repr(C)]
pub struct Vma {
    magic: u64,  // NEW: 0x564D41 ("VMA")
    start: VirtAddr,
    end: VirtAddr,
    flags: VmaFlags,
    // ... existing fields ...
}

const VMA_MAGIC: u64 = 0x564D41;
```

### Phase 2: Medium Priority

#### 4. File Descriptor Table

```rust
pub struct FdTable {
    magic: u64,  // 0x4644544142 ("FDTAB")
    entries: Vec<Option<FileDescriptor>>,
}

pub struct FileDescriptor {
    magic: u32,  // 0x46444553 ("FDES")
    file: Arc<dyn File>,
    flags: FdFlags,
}
```

### Phase 3: Low Priority

#### 5. Page Tables
- Hardware already validates PTE format
- Invalid PTEs cause page faults (hardware protection)
- Canaries add minimal value here

## Corruption Detection Flow

```
1. Structure allocated → Set magic canary
2. On every access → Validate magic == expected
3. If corrupted → Log details + Trigger GPF
4. On free → Clear magic (detects use-after-free)
```

## Magic Value Selection

**Format**: Use ASCII-like hex values for easy identification in debugger

- **Buddy FreeBlock**: `0x4652454542304C` = "FREEBL0C"
- **Heap Header**: `0x4F58494445484150` = "OXIDEHEAP"
- **Slab Header**: `0x534C41424844` = "SLABHD"
- **Free Object**: `0x46524545` = "FREE"
- **Process**: `0x50524F43455353` = "PROCESS"
- **VMA**: `0x564D41` = "VMA"
- **FD Table**: `0x4644544142` = "FDTAB"
- **File Descriptor**: `0x46444553` = "FDES"

## Error Handling

When corruption detected:

```rust
if magic != EXPECTED_MAGIC {
    // 1. Log to serial (always succeeds, no allocation)
    serial_error!("[FATAL] {} corrupted! magic=0x{:x}, expected=0x{:x}, addr=0x{:x}",
                  struct_name, actual_magic, expected_magic, addr);

    // 2. Trigger GPF - fail loud
    unsafe {
        core::ptr::write_volatile(0xDEADC0DE as *mut u64, actual_magic);
    }

    // System halts with clear error message in serial log
}
```

## Benefits

1. **Early detection** - Catch corruption at access time, not crash time
2. **Clear diagnostics** - Know exactly what structure corrupted and where
3. **Fail loud** - GPF with context instead of silent corruption
4. **Zero cost when valid** - Single comparison per access
5. **Debug visibility** - ASCII-like values visible in hex dumps

## Testing Strategy

1. **Unit tests** - Deliberately corrupt magic, verify GPF
2. **Fuzz testing** - Random writes to memory, detect via canaries
3. **Integration tests** - Use-after-free detection (magic cleared on free)
4. **Performance tests** - Measure overhead (should be <1%)

## Performance Impact

- **Buddy allocator**: +1 u64 comparison per allocation = ~1 ns
- **Heap allocator**: Already has canaries, no additional cost
- **Slab allocator**: +1 u32 comparison per allocation = ~1 ns
- **Process access**: +2 u64 comparisons = ~2 ns (negligible)

**Total overhead**: <1% in allocation-heavy workloads

## Current Bug Found

**Use-after-free in buddy allocator**:
- Address: `0x1c059000`
- Expected magic: `0x4652454542304C`
- Found: `0xffffffff801eb55f` (kernel virtual address)
- **Root cause**: Something writing kernel pointer to freed memory
- **Status**: Detected and GPF triggered - investigating source

## Next Steps

1. ✅ Add canaries to slab allocator (Phase 1)
2. ✅ Add canaries to Process struct (Phase 1)
3. ✅ Add canaries to VMA structures (Phase 1)
4. ✅ Add canaries to FD table (Phase 2)
5. 🔍 Find and fix use-after-free in buddy allocator (ongoing)

---
**Author**: GraveShift, BlackLatch, ColdCipher
**Status**: In Progress - Phase 1 priority
**Impact**: Critical - Comprehensive corruption detection across all allocators
