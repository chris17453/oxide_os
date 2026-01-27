# V4 EFI + Memory Management Audit (x86_64)

## Scope
- EFI bootloader memory acquisition, page-table setup, and handoff (`bootloader/boot-uefi/src/{main.rs,paging.rs}`).
- Kernel boot-time region filtering and buddy init (`kernel/src/init.rs`, `globals.rs`, `mm_manager`, `mm-core`).
- Physical frame allocator/buddy, paging/map layer, and stats (`crates/mm/mm-core`, `mm-manager`, `mm-paging`).
- User/kernel heap visibility (global allocator), fragmentation signals, and protected ranges.

## EFI Bootloader Findings
1) **Memory map capture timing:** Bootloader collects memory map *before* building page tables (main.rs `get_memory_map`, step 8) to avoid UEFI allocations corrupting tables—good. ExitBootServices invoked once (LOADER_DATA) after boot info placement (main.rs ~167).
2) **Page tables:** Identity map + phys map first 4 GB using 1 GB huge pages; kernel mapped at 0xFFFF_FFFF_8000_0000 with 4 KB pages (paging.rs). Only 4 GB identity/phys coverage—systems >4 GB rely on buddy to add more later; boot CR3 switch uses UEFI-allocated tables (no guard against >4 GB holes).
3) **Allocations during load:** Kernel/initramfs allocations use UEFI AnyPages without marking reserved in boot_info; kernel later excludes a broad UEFI band (init.rs) to avoid re-use—can over-reserve 16–256 MB+.
4) **Boot info handoff:** BootInfo memory regions truncated to MAX_MEMORY_REGIONS; no coalescing; potential lost ranges >limit.

## Kernel Boot-Time Memory Handling
1) **Protection strategy:** init.rs builds `regions` by stripping: low 1 MB, kernel image, and a conservative UEFI band from ~PML4−16 MB up to max(stack+16 MB, PML4+32 MB, 256 MB). This avoids reclaiming UEFI tables/stack but can waste tens/hundreds of MB and may still miss scattered UEFI allocations outside the band.
2) **Region splitting:** Usable/BootServices regions are page-split to remove protected ranges (page walk). Non-usable regions passed through as non-allocatable.
3) **Buddy init:** MEMORY_MANAGER (mm-manager over mm-core BuddyAllocator) initialized once with derived regions; stats reported; FrameAllocator impl exported globally.
4) **Heap:** Global allocator is a 16 MB static LockedHeap (globals.rs); not backed by pager, no guards/redzones; outside buddy accounting.
5) **AP stacks:** AP stack alloc uses `mm_manager::mm().alloc_contiguous(4)` (16 KB) before SMP bring-up.

## Buddy / Frame Allocator (mm-core)
- Zones: DMA (<16 MB), Normal (<4 GB), High (>=4 GB) with per-zone free lists and stats; MAX_ORDER allows 4 MB blocks. Uses FreeBlock header in freed pages, assumes phys direct-map at 0xFFFF_8000_0000_0000.
- init adds aligned blocks, splitting into largest aligned orders; mark_used removes blocks post-init.
- Alloc prefers requested zone ordering; splits higher orders downward; free coalesces with buddy.
- Stats: total/free bytes tracked; `free_at_order` gives fragmentation view. No per-CPU caches; global mutex per zone.

## Paging Layer (mm-paging)
- PageMapper maps 4 KB pages; creates tables via FrameAllocator; supports update_flags/unmap/translate with huge-page detection for existing entries. No refcounts on tables; MapError::AlreadyMapped only; no lazy free of page tables.

## Observed Risks / Fragmentation Signals
1) **Over-reservation heuristic:** Fixed UEFI band can hide large usable RAM (esp. >4 GB) and still miss stragglers; fragmentation may appear as low free_at_order in Normal/High despite physical availability.
2) **4 GB identity/phys map cap:** Boot tables only map first 4 GB; if firmware placed kernel/initramfs above 4 GB, direct map absent until kernel remaps; potential early panic if phys_to_virt used on >4 GB prior to mapper usage.
3) **Region truncation:** MAX_MEMORY_REGIONS limit in BootInfo can drop ranges silently → buddy sees less RAM.
4) **Allocator contention:** Global zone locks; no per-CPU caches; IRQ contexts share same locks—possible latency; fragmentation increases when frequent split/merge without caches.
5) **Heap isolation:** Kernel heap outside buddy stats; `get_memory_stats` reports only buddy free + heap used/free; no guard pages → corruption risk.
6) **Accounting gaps:** alloc_frames_accounted exists but callers mostly use bare alloc_contiguous/alloc_frame; user mappings and kernel subsystems bypass accounting.

## Quick Health Checkpoints (what to inspect now)
- Log `free_at_order` across orders after boot to detect fragmentation hotspots (mm_manager stats).
- Compare BootInfo region count vs MAX_MEMORY_REGIONS to spot truncation.
- Validate `regions` produced in init.rs against firmware map on real hardware/QEMU to measure over-reservation.
- Confirm phys_map coverage for any >4 GB allocations before kernel remaps.

## Recommendations (priority-ordered)
1) **Tighten UEFI reservation:**
   - Use exact UEFI memory map entries: mark all types != Usable/BootServices as reserved; additionally mark BootServices only after ExitBootServices if not reclaimable.
   - Track UEFI allocations (kernel, initramfs, page tables, boot info) explicitly and mark_used those spans instead of broad band.
2) **Extend phys map coverage:**
   - In bootloader, map full reported RAM (round up to next GB or to highest descriptor) in phys map PDPT to avoid >4 GB gaps pre-kernel.
3) **Preserve all regions:**
   - Coalesce adjacent descriptors in bootloader and expand MAX_MEMORY_REGIONS or chunking to avoid silent drops; warn if truncated.
4) **Buddy visibility & diagnostics:**
   - Export/order-wise stats to /proc (free_at_order per zone) to monitor fragmentation.
   - Add invariant checks: fail init if regions overlap protected spans.
5) **Heap safety and accounting:**
   - Back heap from buddy with guard pages and redzones; include heap consumption in unified stats; consider segregated IRQ-safe pool.
6) **Per-CPU caches:**
   - Add simple per-CPU freelists for order-0/1 to cut lock contention and reduce split/merge churn.
7) **Paging robustness:**
   - Allow optional freeing of empty page tables; add flag replacement (not just add) to mirror mprotect behavior; integrate COW refcounts on unmap.
8) **SMP/interrupt safety:**
   - Ensure alloc/free not used in interrupt paths without bounded latency; if required, add emergency pools.

## References
- Bootloader memory + paging: `bootloader/boot-uefi/src/main.rs` (steps 8–12), `bootloader/boot-uefi/src/paging.rs`.
- Kernel boot memory init and protection band: `kernel/src/init.rs` (heap init, protected regions, buddy init).
- Global alloc/buddy facade: `kernel/src/globals.rs`, `crates/mm/mm-manager/src/lib.rs`.
- Buddy allocator/zones: `crates/mm/mm-core/src/buddy.rs`.
- Page mapper: `crates/mm/mm-paging/src/mapper.rs`.
