# Memory Management

## Crates

| Crate | Purpose |
|-------|---------|
| `mm-traits` | Abstract memory management interfaces |
| `mm-core` | Core MM algorithms and data structures |
| `mm-manager` | Top-level memory manager (physical + virtual) |
| `mm-slab` | Slab allocator for kernel objects |
| `mm-paging` | Page table management (4-level x86_64) |
| `mm-heap` | Kernel heap allocator |
| `mm-cow` | Copy-on-write page support for fork() |

## Architecture

The memory subsystem is layered: `mm-traits` defines abstract interfaces,
`mm-core` implements algorithms, and the remaining crates provide specific
allocators and page management. `mm-manager` ties them together as the
kernel's top-level memory interface.

Physical memory is tracked with a bitmap allocator. Virtual memory uses
x86_64 4-level page tables. The heap uses a linked-list allocator bootstrapped
early in kernel init. The slab allocator handles fixed-size kernel object pools.

Copy-on-write (`mm-cow`) enables efficient `fork()` by sharing pages
read-only and copying on write faults.
