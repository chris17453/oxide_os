//! Comprehensive kernel memory subsystem validation
//!
//! — CrashBloom: Runs at boot to verify the buddy allocator, heap, and DMA
//! paths actually work before we need them. Every test that fails here would
//! have been a mysterious crash later. Trust nothing, verify everything.

extern crate alloc;

use alloc::vec::Vec;
use alloc::vec;
use core::fmt::Write;
use mm_manager::mm;
use os_core::PhysAddr;
use mm_paging::phys_to_virt;

use crate::globals::HEAP_ALLOCATOR;

struct SerialWriter;
impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        unsafe { os_log::write_str_raw(s); }
        Ok(())
    }
}

macro_rules! test_print {
    ($($arg:tt)*) => {
        {
            let mut w = SerialWriter;
            let _ = write!(w, $($arg)*);
        }
    };
}

/// Run all memory tests. Returns (passed, failed) counts.
pub fn run_all() -> (u32, u32) {
    let mut passed = 0u32;
    let mut failed = 0u32;

    test_print!("\n");
    test_print!("╔══════════════════════════════════════════════════════╗\n");
    test_print!("║  OXIDE KERNEL MEMORY SUBSYSTEM VALIDATION           ║\n");
    test_print!("╚══════════════════════════════════════════════════════╝\n\n");

    // — CrashBloom: dump the state of the world before we touch anything
    dump_memory_state();

    // Test 1: Buddy allocator — single frame alloc/free
    run_test("buddy: single frame alloc/free", test_buddy_single_frame, &mut passed, &mut failed);

    // Test 2: Buddy allocator — all orders 0-10
    run_test("buddy: alloc orders 0-10", test_buddy_all_orders, &mut passed, &mut failed);

    // Test 3: Buddy allocator — contiguous alloc (GPU-sized)
    run_test("buddy: contiguous 1024 pages (4MB)", test_buddy_contiguous_4mb, &mut passed, &mut failed);

    // Test 4: Buddy allocator — DMA zone alloc
    run_test("buddy: DMA zone alloc", test_buddy_dma, &mut passed, &mut failed);

    // Test 5: Buddy allocator — alloc/free cycle stress
    run_test("buddy: 100x alloc/free cycle", test_buddy_stress, &mut passed, &mut failed);

    // Test 6: Buddy — free memory accounting
    run_test("buddy: free memory accounting", test_buddy_accounting, &mut passed, &mut failed);

    // Test 7: Heap — small allocation
    run_test("heap: small alloc (64 bytes)", test_heap_small, &mut passed, &mut failed);

    // Test 8: Heap — medium allocation
    run_test("heap: medium alloc (4KB)", test_heap_medium, &mut passed, &mut failed);

    // Test 9: Heap — large allocation
    run_test("heap: large alloc (1MB)", test_heap_large, &mut passed, &mut failed);

    // Test 10: Heap — Vec growth
    run_test("heap: Vec push 10000 elements", test_heap_vec_growth, &mut passed, &mut failed);

    // Test 11: Heap — alloc/free stress
    run_test("heap: 500x alloc/free stress", test_heap_stress, &mut passed, &mut failed);

    // Test 12: Physical page read/write through direct map
    run_test("phys: read/write through direct map", test_phys_readwrite, &mut passed, &mut failed);

    // Test 13: Buddy — verify no order-10 fragmentation
    run_test("buddy: order-10 availability check", test_buddy_order10, &mut passed, &mut failed);

    test_print!("\n══════════════════════════════════════════════════════\n");
    test_print!("  RESULTS: {} passed, {} failed\n", passed, failed);
    if failed == 0 {
        test_print!("  ALL TESTS PASSED\n");
    } else {
        test_print!("  !!! FAILURES DETECTED !!!\n");
    }
    test_print!("══════════════════════════════════════════════════════\n\n");

    (passed, failed)
}

fn run_test(name: &str, f: fn() -> Result<(), &'static str>, passed: &mut u32, failed: &mut u32) {
    test_print!("  [TEST] {}...", name);
    match f() {
        Ok(()) => {
            test_print!(" PASS\n");
            *passed += 1;
        }
        Err(msg) => {
            test_print!(" FAIL: {}\n", msg);
            *failed += 1;
        }
    }
}

fn dump_memory_state() {
    let total = mm().total_bytes();
    let free = mm().free_bytes();
    let used = mm().used_bytes();

    test_print!("  Memory: total={}MB free={}MB used={}MB\n",
        total / 1024 / 1024, free / 1024 / 1024, used / 1024 / 1024);

    test_print!("  Heap: used={}KB free={}KB\n",
        HEAP_ALLOCATOR.used() / 1024, HEAP_ALLOCATOR.free() / 1024);

    // — CrashBloom: THE critical dump — free block counts per order.
    // If order 10 shows 0, GPU DMA will fail. Period.
    test_print!("  Buddy free blocks by order:\n");
    test_print!("    ");
    for order in 0..=10 {
        let count = mm().free_at_order(order);
        let size_kb = (4u64 << order) as u64; // 4KB * 2^order
        test_print!("O{}:{}({}KB) ", order, count, size_kb);
    }
    test_print!("\n\n");
}

// ─── Individual Tests ──────────────────────────────────────────────

fn test_buddy_single_frame() -> Result<(), &'static str> {
    let free_before = mm().free_bytes();
    let addr = mm().alloc_frame().map_err(|_| "alloc_frame failed")?;
    if addr.as_u64() == 0 {
        return Err("alloc returned address 0");
    }
    let free_during = mm().free_bytes();
    if free_during >= free_before {
        return Err("free bytes didn't decrease after alloc");
    }
    mm().free_frame(addr).map_err(|_| "free_frame failed")?;
    let free_after = mm().free_bytes();
    if free_after != free_before {
        return Err("free bytes didn't restore after free");
    }
    Ok(())
}

fn test_buddy_all_orders() -> Result<(), &'static str> {
    // — CrashBloom: try every order 0-10. This is the test that would
    // have caught the GPU DMA failure before it happened.
    for order in 0..=10 {
        let count = 1usize << order;
        match mm().alloc_contiguous(count) {
            Ok(addr) => {
                // Verify the address is reasonable (below 4GB for 512MB system)
                if addr.as_u64() > 0x2000_0000 {
                    // 512MB = 0x20000000, shouldn't exceed this
                    test_print!("\n    [WARN] order {} addr=0x{:x} seems high", order, addr.as_u64());
                }
                mm().free_contiguous(addr, count).map_err(|_| "free_contiguous failed")?;
            }
            Err(_) => {
                test_print!("\n    [FAIL] order {} ({} pages, {}KB) failed",
                    order, count, count * 4);
                // Dump what IS available
                for o in 0..=10 {
                    test_print!("\n      O{}: {} blocks", o, mm().free_at_order(o));
                }
                return Err("alloc_contiguous failed at order");
            }
        }
    }
    Ok(())
}

fn test_buddy_contiguous_4mb() -> Result<(), &'static str> {
    // — CrashBloom: This is the exact allocation the GPU needs.
    // 1024 pages = 4MB = order 10.
    let addr = mm().alloc_contiguous(1024).map_err(|_| {
        // Dump state on failure
        test_print!("\n    free={}MB", mm().free_bytes() / 1024 / 1024);
        for o in 0..=10 {
            test_print!(" O{}:{}", o, mm().free_at_order(o));
        }
        "4MB contiguous alloc failed"
    })?;

    // Verify we can actually write to all 4MB through the direct map
    let virt = phys_to_virt(addr).as_u64() as *mut u8;
    for offset in (0..4 * 1024 * 1024).step_by(4096) {
        unsafe {
            core::ptr::write_volatile(virt.add(offset), 0xAA);
            let val = core::ptr::read_volatile(virt.add(offset));
            if val != 0xAA {
                return Err("direct map write/read mismatch");
            }
        }
    }

    mm().free_contiguous(addr, 1024).map_err(|_| "free 4MB failed")?;
    Ok(())
}

fn test_buddy_dma() -> Result<(), &'static str> {
    // DMA zone = below 16MB. Order 0 = single page.
    let addr = mm().alloc_dma(0).map_err(|_| "DMA alloc failed")?;
    if addr.as_u64() >= 0x0100_0000 {
        mm().free_frames(addr, 0).map_err(|_| "free failed")?;
        return Err("DMA addr not in low 16MB");
    }
    mm().free_frames(addr, 0).map_err(|_| "DMA free failed")?;
    Ok(())
}

fn test_buddy_stress() -> Result<(), &'static str> {
    let mut addrs = Vec::new();
    for _ in 0..100 {
        match mm().alloc_frame() {
            Ok(addr) => addrs.push(addr),
            Err(_) => {
                // Free what we got
                for a in &addrs {
                    let _ = mm().free_frame(*a);
                }
                return Err("alloc_frame failed during stress");
            }
        }
    }
    let free_mid = mm().free_bytes();
    for a in &addrs {
        mm().free_frame(*a).map_err(|_| "free_frame failed during stress")?;
    }
    let free_after = mm().free_bytes();
    // Should have freed exactly 100 pages = 400KB
    let freed = free_after - free_mid;
    if freed != 100 * 4096 {
        return Err("accounting mismatch after stress free");
    }
    Ok(())
}

fn test_buddy_accounting() -> Result<(), &'static str> {
    let total = mm().total_bytes();
    let free = mm().free_bytes();
    let used = mm().used_bytes();

    if total == 0 {
        return Err("total_bytes is 0");
    }
    if free == 0 {
        return Err("free_bytes is 0");
    }
    if free > total {
        return Err("free > total");
    }
    // used + free should roughly equal total (within margin for overhead)
    let sum = used + free;
    if sum > total + 1024 * 1024 || total > sum + 1024 * 1024 {
        return Err("used+free doesn't match total (>1MB off)");
    }
    Ok(())
}

fn test_heap_small() -> Result<(), &'static str> {
    let v: Vec<u8> = vec![0xBB; 64];
    if v.len() != 64 || v[0] != 0xBB || v[63] != 0xBB {
        return Err("small heap alloc data mismatch");
    }
    Ok(())
}

fn test_heap_medium() -> Result<(), &'static str> {
    let v: Vec<u8> = vec![0xCC; 4096];
    if v.len() != 4096 || v[0] != 0xCC || v[4095] != 0xCC {
        return Err("medium heap alloc data mismatch");
    }
    Ok(())
}

fn test_heap_large() -> Result<(), &'static str> {
    let v: Vec<u8> = vec![0xDD; 1024 * 1024];
    if v.len() != 1024 * 1024 || v[0] != 0xDD || v[1024 * 1024 - 1] != 0xDD {
        return Err("large heap alloc data mismatch");
    }
    Ok(())
}

fn test_heap_vec_growth() -> Result<(), &'static str> {
    let mut v = Vec::new();
    for i in 0..10000u32 {
        v.push(i);
    }
    if v.len() != 10000 || v[0] != 0 || v[9999] != 9999 {
        return Err("Vec growth data mismatch");
    }
    Ok(())
}

fn test_heap_stress() -> Result<(), &'static str> {
    let used_before = HEAP_ALLOCATOR.used();
    {
        let mut vecs: Vec<Vec<u8>> = Vec::new();
        for i in 0..500 {
            let size = 64 + (i % 16) * 256; // 64 to 3904 bytes
            vecs.push(vec![0xFF; size]);
        }
        // Verify all allocations are intact
        for (i, v) in vecs.iter().enumerate() {
            let expected_size = 64 + (i % 16) * 256;
            if v.len() != expected_size || v[0] != 0xFF {
                return Err("stress alloc data corruption");
            }
        }
        // All vecs drop here
    }
    let used_after = HEAP_ALLOCATOR.used();
    // Heap usage should return to approximately the same level
    // (not exact due to fragmentation, but within 64KB)
    let diff = if used_after > used_before {
        used_after - used_before
    } else {
        used_before - used_after
    };
    if diff > 64 * 1024 {
        return Err("heap didn't reclaim memory after stress (>64KB leak)");
    }
    Ok(())
}

fn test_phys_readwrite() -> Result<(), &'static str> {
    let addr = mm().alloc_frame().map_err(|_| "alloc_frame failed")?;
    let virt = phys_to_virt(addr).as_u64() as *mut u64;

    // Write a pattern to every 8 bytes in the page
    for i in 0..512 {
        unsafe {
            core::ptr::write_volatile(virt.add(i), 0xDEAD_BEEF_0000_0000 | i as u64);
        }
    }
    // Read back and verify
    for i in 0..512 {
        unsafe {
            let val = core::ptr::read_volatile(virt.add(i));
            let expected = 0xDEAD_BEEF_0000_0000 | i as u64;
            if val != expected {
                mm().free_frame(addr).ok();
                return Err("phys page read/write mismatch");
            }
        }
    }
    mm().free_frame(addr).map_err(|_| "free_frame failed")?;
    Ok(())
}

fn test_buddy_order10() -> Result<(), &'static str> {
    let count = mm().free_at_order(10);
    if count == 0 {
        // Check what the highest available order is
        let mut highest = 0;
        for o in (0..=10).rev() {
            if mm().free_at_order(o) > 0 {
                highest = o;
                break;
            }
        }
        test_print!("\n    [INFO] No order-10 blocks. Highest available: order {} ({} blocks, {}KB each)",
            highest, mm().free_at_order(highest), 4 << highest);
        // Try to allocate and see if splitting works
        match mm().alloc_contiguous(1024) {
            Ok(addr) => {
                test_print!("\n    [INFO] But alloc_contiguous(1024) succeeded via splitting!");
                mm().free_contiguous(addr, 1024).ok();
                return Ok(());
            }
            Err(_) => {
                return Err("no order-10 blocks AND splitting failed");
            }
        }
    }
    test_print!(" ({} blocks available)", count);
    Ok(())
}
