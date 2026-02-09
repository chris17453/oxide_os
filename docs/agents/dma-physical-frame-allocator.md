# Agent Rule: DMA Buffers Must Use Physical Frame Allocator

## Rule
**ALL memory shared with hardware (DMA buffers, VirtIO rings, MMIO-backed structs)
MUST be allocated from the physical frame allocator (`mm().alloc_contiguous()`),
NOT from the kernel heap (`alloc_zeroed()` / `Box::new()` / `Vec::new()`).**

## Why

The kernel heap lives at `KERNEL_VIRT_BASE` (`0xFFFF_FFFF_8000_0000`). The naive
`virt_to_phys()` function (`addr - PHYS_MAP_BASE`) returns a bogus ~128TB "physical"
address for heap pointers — this is NOT the real physical address the hardware DMAs to.

```
Heap address:    0xFFFF_FFFF_8010_0000
- PHYS_MAP_BASE: 0xFFFF_8000_0000_0000
= WRONG:         0x0000_7FFF_8010_0000  (~512GB — not real RAM!)
```

The physical frame allocator returns **real physical addresses**. Access them through
the direct physical map (`PHYS_MAP_BASE + phys_addr`), and pass `phys_addr` directly
to hardware.

## Correct Pattern (from virtio-blk `new_legacy()`)

```rust
use mm_manager::mm;
use mm_traits::FrameAllocator;

// Allocate physical frames — returns REAL physical address
let phys_addr = mm().alloc_contiguous(num_pages).map_err(|_| "alloc")?;
let phys_base = phys_addr.as_u64();

// Access via direct physical map
let virt_base = (PHYS_MAP_BASE + phys_base) as *mut u8;
unsafe { core::ptr::write_bytes(virt_base, 0, num_pages * 4096); }

// Pass PHYSICAL address to hardware
transport.set_queue_desc(phys_base);

// Read/write via virtual pointer (through direct map)
let desc = virt_base as *mut VirtqDesc;
```

## Wrong Pattern (was in virtio-input, would break any DMA driver)

```rust
// WRONG: heap allocation returns KERNEL_VIRT_BASE address
let ptr = unsafe { alloc_zeroed(layout) };

// WRONG: this gives ~128TB bogus physical address
let phys = ptr as u64 - PHYS_MAP_BASE;
transport.set_queue_desc(phys);  // Hardware DMAs to garbage!
```

## Which Drivers Are Affected

Any driver that shares memory with hardware:
- VirtIO input, block, net, GPU (virtqueue rings + data buffers)
- Intel HDA (command/response rings)
- Any future DMA-capable driver

## Verification

If a VirtIO device "initializes OK" but never delivers events, the first thing to
check is whether DMA addresses are from the frame allocator or the kernel heap.

— SableWire: Found this the hard way. The VirtIO input device happily accepted
  128TB physical addresses, wrote events into the void, and nobody heard the
  keystrokes scream.
