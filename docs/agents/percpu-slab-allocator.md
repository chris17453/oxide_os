# Agent Rule: Per-CPU Slab Allocator

## Rule
Small allocations (≤2048 bytes) MUST go through the per-CPU slab allocator
when SLAB_READY is true. The global heap lock is the fallback, not the default.

## Why
The global LinkedListAllocator uses a KernelMutex — every alloc/dealloc across
4 CPUs contends on one spinlock. Page fault handlers re-entering the allocator
cause deadlock. The slab's per-CPU free lists have ZERO lock contention.

## Architecture
```
Hot path (no lock):     Per-CPU free lists [16,32,64,128,256,512,1024,2048]
                              |
Refill (rare):          Buddy allocator (alloc one 4K page, carve into objects)
                              |
Large alloc fallback:   LinkedListAllocator (KernelMutex)
```

## Invariants
- Preemption MUST be disabled when accessing CPU_CACHES[this_cpu()]
- SLAB_READY is false during early boot (before SMP is up)
- Pointers from the static heap range go to LinkedListAllocator on dealloc
- Pointers from buddy pages go to slab caches on dealloc

## How to Apply
- Never allocate large arrays in hot paths — keep them ≤2048 bytes
- ISR code benefits most from slab (no lock contention with preempted tasks)
- If adding a new allocator feature, integrate with SlabAllocator not KernelHeap
