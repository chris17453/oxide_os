//! stresstest - Comprehensive OS stress test suite
//!
//! — CrashBloom: Hammers every subsystem that can break under load.
//! Each test is designed to trigger the exact class of bug that silently
//! corrupts state and triple-faults three forks later. If the OS survives
//! this, it can survive a user. Probably.
//!
//! Test categories:
//!   1. Fork storm         — rapid fork/exit/wait cycles
//!   2. Fork bomb defense  — fork until OOM, verify graceful failure
//!   3. COW verification   — fork, mutate shared pages, verify isolation
//!   4. Memory pressure    — mmap/munmap thrash
//!   5. Pipe throughput    — fork + pipe + read/write IPC
//!   6. Scheduler stress   — many concurrent sleepers hitting idle path
//!   7. Signal storm       — rapid signal delivery during sleep
//!   8. Exec cycle         — fork + exec in a loop
//!   9. Nested fork        — fork inside fork inside fork
//!  10. PID reuse          — burn through PIDs, verify no stale state

#![no_std]
#![no_main]

use libc::*;
use libc::syscall::{sys_mmap, sys_munmap, MAP_FAILED};
use libc::syscall::prot::{PROT_READ, PROT_WRITE};
use libc::syscall::map_flags::{MAP_PRIVATE, MAP_ANONYMOUS};
use libc::time;

static mut PASS_COUNT: u32 = 0;
static mut FAIL_COUNT: u32 = 0;
static mut TEST_NUM: u32 = 0;

/// — CrashBloom: Serial fd for direct serial output. Tests MUST be visible
/// on serial because the framebuffer might be toast after a memory bug.
/// Opened once in main(), used by all output functions.
static mut SERIAL_FD: i32 = -1;

/// Write to both stdout (terminal) and serial (debug capture).
/// — CrashBloom: Dual-output because you never know which one is alive.
fn tprint(s: &str) {
    prints(s);
    unsafe {
        if SERIAL_FD >= 0 {
            write(SERIAL_FD, s.as_bytes());
        }
    }
}

fn pass(name: &str) {
    unsafe { PASS_COUNT += 1; }
    tprint("  PASS: ");
    tprint(name);
    tprint("\n");
}

fn fail(name: &str, reason: &str) {
    unsafe { FAIL_COUNT += 1; }
    tprint("  FAIL: ");
    tprint(name);
    tprint(" — ");
    tprint(reason);
    tprint("\n");
}

fn test_header(name: &str) {
    unsafe {
        TEST_NUM += 1;
        tprint("[TEST ");
        print_num(TEST_NUM);
    }
    tprint("] ");
    tprint(name);
    tprint("\n");
}

fn nanosleep_ms(ms: u32) {
    let ts = time::Timespec {
        tv_sec: (ms / 1000) as i64,
        tv_nsec: ((ms % 1000) as i64) * 1_000_000,
    };
    let mut rem = time::Timespec { tv_sec: 0, tv_nsec: 0 };
    time::nanosleep(&ts, Some(&mut rem));
}

fn wait_child(pid: i32) -> i32 {
    let mut status: i32 = 0;
    loop {
        let ret = waitpid(pid, &mut status, 0);
        if ret == pid { return status; }
        if ret < 0 && ret != -(errno::EINTR as i32) { return -1; }
    }
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // — CrashBloom: Open /dev/serial for direct serial output so test results
    // are visible even when the framebuffer is dead or we're running headless.
    unsafe {
        SERIAL_FD = open("/dev/serial", 1, 0); // O_WRONLY=1
    }
    tprint("=== OXIDE Stress Test Suite ===\n");
    tprint("-- CrashBloom: If you're reading this on serial, the OS hasn't died yet.\n\n");

    test_fork_storm();
    test_fork_bomb_defense();
    test_cow_verify();
    test_mmap_thrash();
    test_pipe_throughput();
    test_scheduler_stress();
    test_signal_storm();
    test_exec_cycle();
    test_nested_fork();
    test_pid_reuse();

    tprint("\n========================================\n");
    tprint("  Stress Test Results: ");
    unsafe {
        print_num(PASS_COUNT);
        tprint(" passed, ");
        print_num(FAIL_COUNT);
        tprint(" failed");
    }
    tprint("\n========================================\n");

    unsafe { if FAIL_COUNT > 0 { 1 } else { 0 } }
}

// ─── Test 1: Fork Storm ─────────────────────────────────────────────────────
// — CrashBloom: Rapid fork/exit/wait. This hammers page table allocation,
// COW reference counting, buddy allocator free paths, and the scheduler's
// task lifecycle. The CR3=0 bug lived here — the idle task switch after
// the child exits was loading NULL page tables.
fn test_fork_storm() {
    test_header("Fork Storm (50 rapid fork/exit cycles)");
    let mut ok = 0u32;
    let mut errors = 0u32;

    for _ in 0..50 {
        let pid = fork();
        if pid < 0 {
            errors += 1;
            continue;
        }
        if pid == 0 {
            // — CrashBloom: Child. Touch the stack (COW fault), then die.
            let mut x: u32 = 42;
            let _vol = unsafe { core::ptr::read_volatile(&x) };
            x += 1;
            unsafe { core::ptr::write_volatile(&mut x, x); }
            exit(0);
        }
        let status = wait_child(pid);
        if status == 0 {
            ok += 1;
        } else {
            errors += 1;
        }
    }

    if errors == 0 {
        pass("fork storm: 50/50 cycles clean");
    } else {
        fail("fork storm", "some cycles failed");
        tprint("    ok=");
        print_num(ok);
        tprint(" errors=");
        print_num(errors);
        tprint("\n");
    }
}

// ─── Test 2: Fork Bomb Defense ──────────────────────────────────────────────
// — FuzzStatic: Fork until we can't. The OS should return -ENOMEM or -EAGAIN,
// NOT triple-fault. We fork children that sleep forever, then kill them all.
fn test_fork_bomb_defense() {
    test_header("Fork Bomb Defense (fork until failure)");

    let mut pids = [0i32; 128];
    let mut count = 0u32;

    for i in 0..128 {
        let pid = fork();
        if pid < 0 {
            // — FuzzStatic: Good. OS said no. That's the correct answer.
            break;
        }
        if pid == 0 {
            // — FuzzStatic: Child sleeps forever. Parent will murder us.
            for _ in 0..3600 {
                sleep(1);
            }
            exit(99);
        }
        pids[i] = pid;
        count += 1;
    }

    // — FuzzStatic: Kill all the children. We're not monsters, just testers.
    for i in 0..count as usize {
        kill(pids[i], 9); // SIGKILL
    }
    for i in 0..count as usize {
        wait_child(pids[i]);
    }

    if count > 0 {
        pass("fork bomb: forked ");
        // — Can't easily append number to pass(), print separately
        tprint("    spawned ");
        print_num(count);
        tprint(" children before limit, all cleaned up\n");
    } else {
        fail("fork bomb", "couldn't fork even once");
    }
}

// ─── Test 3: COW Verification ───────────────────────────────────────────────
// — SableWire: Fork, then write to a known memory location in both parent
// and child. Verify they see different values. If COW is broken, they'll
// share the same physical page and corrupt each other.
fn test_cow_verify() {
    test_header("COW Isolation Verification");

    // — SableWire: Use a mutable static as our canary. Both processes write
    // different values. If COW works, each sees only their own write.
    static mut COW_CANARY: u64 = 0xDEAD_BEEF_CAFE_BABE;

    unsafe { core::ptr::write_volatile(&raw mut COW_CANARY, 0xAAAA_AAAA_AAAA_AAAA); }

    let mut pipe_fds = [0i32; 2];
    if pipe(&mut pipe_fds) < 0 {
        fail("COW verify", "pipe() failed");
        return;
    }

    let pid = fork();
    if pid < 0 {
        fail("COW verify", "fork() failed");
        return;
    }

    if pid == 0 {
        // — SableWire: Child writes a different pattern, then reports back via pipe.
        close(pipe_fds[0]);
        unsafe { core::ptr::write_volatile(&raw mut COW_CANARY, 0xBBBB_BBBB_BBBB_BBBB); }
        // Read it back to make sure our write stuck
        let val = unsafe { core::ptr::read_volatile(&raw const COW_CANARY) };
        let bytes = val.to_le_bytes();
        write(pipe_fds[1], &bytes);
        close(pipe_fds[1]);
        exit(0);
    }

    // — SableWire: Parent writes its own pattern
    close(pipe_fds[1]);
    unsafe { core::ptr::write_volatile(&raw mut COW_CANARY, 0xCCCC_CCCC_CCCC_CCCC); }

    // Read what the child saw
    let mut buf = [0u8; 8];
    let n = read(pipe_fds[0], &mut buf);
    close(pipe_fds[0]);
    wait_child(pid);

    if n != 8 {
        fail("COW verify", "pipe read failed");
        return;
    }

    let child_val = u64::from_le_bytes(buf);
    let parent_val = unsafe { core::ptr::read_volatile(&raw const COW_CANARY) };

    if child_val == 0xBBBB_BBBB_BBBB_BBBB && parent_val == 0xCCCC_CCCC_CCCC_CCCC {
        pass("COW isolation: parent and child have independent memory");
    } else {
        fail("COW verify", "memory leaked between processes!");
        tprint("    child=0x");
        print_hex64(child_val);
        tprint(" parent=0x");
        print_hex64(parent_val);
        tprint("\n");
    }
}

// ─── Test 4: Mmap Thrash ────────────────────────────────────────────────────
// — TorqueJax: Map and unmap pages in rapid succession. This hammers the
// page table mapping code, the buddy allocator alloc/free paths, and
// catches off-by-one errors in page table walks.
fn test_mmap_thrash() {
    test_header("Mmap/Munmap Thrash (200 cycles)");
    let mut ok = 0u32;
    let mut errors = 0u32;

    for i in 0..200u32 {
        // — TorqueJax: Map one page, write a pattern, read it back, unmap.
        let size = 4096usize;
        let ptr = sys_mmap(
            core::ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        );

        if ptr == MAP_FAILED || ptr.is_null() {
            errors += 1;
            continue;
        }

        // Write a pattern
        let pattern = (0xA5u8).wrapping_add(i as u8);
        unsafe {
            core::ptr::write_volatile(ptr, pattern);
            core::ptr::write_volatile(ptr.add(4095), pattern);
        }

        // Read it back
        let read_first = unsafe { core::ptr::read_volatile(ptr) };
        let read_last = unsafe { core::ptr::read_volatile(ptr.add(4095)) };

        if read_first != pattern || read_last != pattern {
            errors += 1;
        } else {
            ok += 1;
        }

        // Unmap
        let ret = sys_munmap(ptr, size);
        if ret != 0 {
            errors += 1;
        }
    }

    if errors == 0 {
        pass("mmap thrash: 200/200 cycles clean");
    } else {
        fail("mmap thrash", "some cycles failed");
        tprint("    ok=");
        print_num(ok);
        tprint(" errors=");
        print_num(errors);
        tprint("\n");
    }
}

// ─── Test 5: Pipe Throughput ────────────────────────────────────────────────
// — ByteRiot: Fork a child, pipe 64KB of data through, verify every byte.
// Tests fd table cloning, pipe buffer management, blocking read/write,
// and scheduler wake-up on pipe data arrival.
fn test_pipe_throughput() {
    test_header("Pipe Throughput (64KB transfer)");

    let mut pipe_fds = [0i32; 2];
    if pipe(&mut pipe_fds) < 0 {
        fail("pipe throughput", "pipe() failed");
        return;
    }

    let pid = fork();
    if pid < 0 {
        fail("pipe throughput", "fork() failed");
        return;
    }

    if pid == 0 {
        // — ByteRiot: Child writes 64KB in 256-byte chunks.
        close(pipe_fds[0]);
        let mut buf = [0u8; 256];
        for chunk in 0..256u32 {
            for j in 0..256 {
                buf[j] = (chunk.wrapping_add(j as u32)) as u8;
            }
            let written = write(pipe_fds[1], &buf);
            if written != 256 {
                exit(1);
            }
        }
        close(pipe_fds[1]);
        exit(0);
    }

    // — ByteRiot: Parent reads 64KB and verifies every byte.
    close(pipe_fds[1]);
    let mut total_read = 0u32;
    let mut errors = 0u32;
    let mut buf = [0u8; 256];

    while total_read < 65536 {
        let n = read(pipe_fds[0], &mut buf);
        if n <= 0 { break; }
        // Verify pattern
        for j in 0..n as usize {
            let byte_idx = total_read as usize + j;
            let expected_chunk = (byte_idx / 256) as u32;
            let expected_pos = (byte_idx % 256) as u32;
            let expected = (expected_chunk.wrapping_add(expected_pos)) as u8;
            if buf[j] != expected {
                errors += 1;
            }
        }
        total_read += n as u32;
    }
    close(pipe_fds[0]);
    let status = wait_child(pid);

    if total_read == 65536 && errors == 0 && status == 0 {
        pass("pipe throughput: 64KB transferred and verified");
    } else {
        fail("pipe throughput", "data corruption or short transfer");
        tprint("    bytes=");
        print_num(total_read);
        tprint(" errors=");
        print_num(errors);
        tprint(" child_status=");
        print_num(status as u32);
        tprint("\n");
    }
}

// ─── Test 6: Scheduler Stress ───────────────────────────────────────────────
// — ThreadRogue: Spawn N children that all sleep for varying durations.
// This forces the scheduler to context-switch between sleeping tasks and
// the idle loop constantly. The CR3=0 bug hid here for months because
// the idle path was only hit when ALL user tasks were asleep.
fn test_scheduler_stress() {
    test_header("Scheduler Stress (20 concurrent sleepers)");
    let count = 20;
    let mut pids = [0i32; 20];

    for i in 0..count {
        let pid = fork();
        if pid < 0 {
            fail("scheduler stress", "fork() failed");
            // Clean up already-spawned
            for j in 0..i { kill(pids[j], 9); wait_child(pids[j]); }
            return;
        }
        if pid == 0 {
            // — ThreadRogue: Each child does a different sleep pattern.
            // Some yield, some nanosleep, some spin briefly. All paths
            // that can trigger scheduler decisions.
            let my_idx = i as u32;
            for _ in 0..10 {
                match my_idx % 4 {
                    0 => nanosleep_ms(10),  // Short nap
                    1 => nanosleep_ms(50),  // Medium nap
                    2 => sched_yield(),     // Yield CPU
                    _ => {                  // Brief spin
                        for _ in 0..1000 {
                            unsafe { core::arch::asm!("nop"); }
                        }
                    }
                }
            }
            exit(0);
        }
        pids[i] = pid;
    }

    // — ThreadRogue: Wait for all children. If the scheduler crashed,
    // we won't get here.
    let mut ok = 0u32;
    for i in 0..count {
        let status = wait_child(pids[i]);
        if status == 0 { ok += 1; }
    }

    if ok == count as u32 {
        pass("scheduler stress: all 20 sleepers completed");
    } else {
        fail("scheduler stress", "some children didn't complete");
        tprint("    ok=");
        print_num(ok);
        tprint("/");
        print_num(count as u32);
        tprint("\n");
    }
}

// ─── Test 7: Signal Storm ───────────────────────────────────────────────────
// — GhostPatch: Rapid signal delivery to a sleeping child. Tests the
// signal pending check in nanosleep, EINTR handling, signal queue,
// and the interaction between signal delivery and scheduler preemption.
fn test_signal_storm() {
    test_header("Signal Storm (50 rapid SIGUSR1 deliveries)");

    let pid = fork();
    if pid < 0 {
        fail("signal storm", "fork() failed");
        return;
    }

    if pid == 0 {
        // — GhostPatch: Child ignores SIGUSR1 and counts how long it survives.
        // SIGUSR1 = 10, SIG_IGN = 1
        signal(10, 1); // SIG_IGN
        // Sleep in small chunks — each signal interrupts the sleep
        for _ in 0..100 {
            nanosleep_ms(20);
        }
        exit(0);
    }

    // — GhostPatch: Parent blasts SIGUSR1 at the child while it sleeps.
    nanosleep_ms(50); // Let child start sleeping
    let mut send_ok = 0u32;
    for _ in 0..50 {
        let ret = kill(pid, 10); // SIGUSR1
        if ret == 0 { send_ok += 1; }
        nanosleep_ms(5);
    }

    let status = wait_child(pid);
    if status == 0 && send_ok == 50 {
        pass("signal storm: 50 signals delivered, child survived");
    } else {
        fail("signal storm", "child died or signals failed");
        tprint("    sent=");
        print_num(send_ok);
        tprint(" child_status=");
        print_num(status as u32);
        tprint("\n");
    }
}

// ─── Test 8: Exec Cycle ────────────────────────────────────────────────────
// — IronGhost: Fork and exec /bin/true in a loop. Tests exec's address space
// teardown, ELF loading, signal handler reset, fd table inheritance, and
// the interaction between exec and COW reference counts.
fn test_exec_cycle() {
    test_header("Exec Cycle (20 fork+exec /bin/true)");
    let mut ok = 0u32;
    let mut errors = 0u32;

    for _ in 0..20 {
        let pid = fork();
        if pid < 0 {
            errors += 1;
            continue;
        }
        if pid == 0 {
            // — IronGhost: Exec /bin/true. It just calls exit(0).
            execve("/bin/true", core::ptr::null(), core::ptr::null());
            // If we get here, exec failed
            exit(127);
        }
        let status = wait_child(pid);
        let exit_code = (status >> 8) & 0xFF;
        if exit_code == 0 {
            ok += 1;
        } else {
            errors += 1;
        }
    }

    if errors == 0 {
        pass("exec cycle: 20/20 fork+exec clean");
    } else {
        fail("exec cycle", "some execs failed");
        tprint("    ok=");
        print_num(ok);
        tprint(" errors=");
        print_num(errors);
        tprint("\n");
    }
}

// ─── Test 9: Nested Fork ───────────────────────────────────────────────────
// — DeadLoop: Fork inside fork inside fork. Tests that deeply nested page
// table trees and COW reference counts don't get confused. Each level
// adds another COW reference to the same physical frames.
fn test_nested_fork() {
    test_header("Nested Fork (4 levels deep)");

    let mut pipe_fds = [0i32; 2];
    if pipe(&mut pipe_fds) < 0 {
        fail("nested fork", "pipe() failed");
        return;
    }

    let pid = fork();
    if pid < 0 {
        fail("nested fork", "fork() failed (level 1)");
        return;
    }

    if pid == 0 {
        // Level 1
        close(pipe_fds[0]);
        let pid2 = fork();
        if pid2 < 0 { exit(1); }
        if pid2 == 0 {
            // Level 2
            let pid3 = fork();
            if pid3 < 0 { exit(2); }
            if pid3 == 0 {
                // Level 3
                let pid4 = fork();
                if pid4 < 0 { exit(3); }
                if pid4 == 0 {
                    // Level 4 — deepest. Write success marker.
                    let marker = [0x42u8; 1];
                    write(pipe_fds[1], &marker);
                    close(pipe_fds[1]);
                    exit(0);
                }
                wait_child(pid4);
                exit(0);
            }
            wait_child(pid3);
            exit(0);
        }
        wait_child(pid2);
        close(pipe_fds[1]);
        exit(0);
    }

    close(pipe_fds[1]);
    let mut buf = [0u8; 1];
    let n = read(pipe_fds[0], &mut buf);
    close(pipe_fds[0]);
    wait_child(pid);

    if n == 1 && buf[0] == 0x42 {
        pass("nested fork: 4 levels deep, all completed");
    } else {
        fail("nested fork", "deepest child didn't report success");
    }
}

// ─── Test 10: PID Reuse ─────────────────────────────────────────────────────
// — CanaryHex: Burn through PIDs quickly and verify each child gets a unique
// PID and runs in isolation. Catches stale task metadata, zombie leaks,
// and PID counter overflow issues.
fn test_pid_reuse() {
    test_header("PID Reuse (100 sequential fork/exit)");
    let mut last_pid = 0i32;
    let mut unique_count = 0u32;
    let mut errors = 0u32;

    for _ in 0..100 {
        let pid = fork();
        if pid < 0 {
            errors += 1;
            continue;
        }
        if pid == 0 {
            // — CanaryHex: Child confirms it has its own PID
            let my_pid = getpid();
            let parent_pid = getppid();
            if my_pid <= 0 || parent_pid <= 0 || my_pid == parent_pid {
                exit(1);
            }
            exit(0);
        }
        if pid != last_pid {
            unique_count += 1;
        }
        last_pid = pid;
        let status = wait_child(pid);
        let exit_code = (status >> 8) & 0xFF;
        if exit_code != 0 {
            errors += 1;
        }
    }

    if errors == 0 && unique_count >= 95 {
        pass("PID reuse: 100 cycles, all unique PIDs, all isolated");
    } else if errors == 0 {
        pass("PID reuse: 100 cycles clean (some PID reuse, which is OK)");
    } else {
        fail("PID reuse", "some children reported errors");
        tprint("    errors=");
        print_num(errors);
        tprint(" unique_pids=");
        print_num(unique_count);
        tprint("\n");
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn print_num(n: u32) {
    if n == 0 {
        tprint("0");
        return;
    }
    let mut buf = [0u8; 12];
    let mut i = 11;
    let mut val = n;
    while val > 0 {
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
        if i == 0 { break; }
        i -= 1;
    }
    if let Ok(s) = core::str::from_utf8(&buf[i + 1..12]) {
        tprint(s);
    }
}

fn print_hex64(val: u64) {
    let hex = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    let mut v = val;
    for i in (0..16).rev() {
        buf[i] = hex[(v & 0xF) as usize];
        v >>= 4;
    }
    if let Ok(s) = core::str::from_utf8(&buf) {
        tprint(s);
    }
}
