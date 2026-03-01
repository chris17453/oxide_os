//! sigtest - Comprehensive signal delivery test
//!
//! — GraveShift: Tests every signal delivery path without needing keyboard input.
//! Each test forks a child, sends SIGINT via different mechanisms, and verifies
//! the child dies properly. Covers:
//!   Test 1: Direct kill(pid, SIGINT) — the baseline
//!   Test 2: PGID-based kill(-pgid, SIGINT) — simulates Ctrl+C path
//!   Test 3: Signal during nanosleep — tests EINTR wakeup + delivery
//!   Test 4: Signal after setpgid + kill(0, SIGINT) — process group self-signal

#![no_std]
#![no_main]

use libc::*;

static mut PASS_COUNT: u32 = 0;
static mut FAIL_COUNT: u32 = 0;

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    prints("=== Signal Delivery Test Suite ===\n\n");

    test_direct_kill();
    test_pgid_kill();
    test_signal_during_nanosleep();
    test_self_group_kill();

    prints("\n=== Results: ");
    unsafe {
        print_num(PASS_COUNT);
        prints(" passed, ");
        print_num(FAIL_COUNT);
        prints(" failed ===\n");
    }

    unsafe { if FAIL_COUNT > 0 { 1 } else { 0 } }
}

/// Test 1: Direct kill(pid, SIGINT)
/// — GraveShift: Baseline test. Parent sends kill() directly to child PID.
/// Signal goes through send_signal_to_pid (blocking lock, blocking wake_up).
fn test_direct_kill() {
    prints("[TEST 1] Direct kill(pid, SIGINT)\n");

    let pid = fork();
    if pid < 0 {
        prints("  FAIL: fork() failed\n");
        unsafe { FAIL_COUNT += 1; }
        return;
    }

    if pid == 0 {
        // — GraveShift: Child — sleep until murdered. SIG_DFL = terminate.
        signal(SIGINT, SIG_DFL);
        for _ in 0..100 {
            sleep(1);
        }
        exit(99);
    }

    // — GraveShift: Parent — let child enter sleep, then kill it.
    nanosleep_ms(200);
    let ret = kill(pid, 2); // SIGINT = 2
    if ret != 0 {
        prints("  FAIL: kill() returned ");
        print_num(ret as u32);
        prints("\n");
    }

    let status = wait_for_child(pid);
    check_signal_death(status, 2, "direct kill");
}

/// Test 2: PGID-based kill(-pgid, SIGINT)
/// — GraveShift: Simulates the exact Ctrl+C signal delivery path.
/// Child calls setpgid(0,0) to become group leader, parent sends to -pgid.
/// Signal goes through send_signal_to_pgrp (iterates PIDs, blocking lock).
fn test_pgid_kill() {
    prints("[TEST 2] PGID kill(-pgid, SIGINT)\n");

    let pid = fork();
    if pid < 0 {
        prints("  FAIL: fork() failed\n");
        unsafe { FAIL_COUNT += 1; }
        return;
    }

    if pid == 0 {
        // — GraveShift: Child — set own PGID like the shell does for fg commands.
        setpgid(0, 0); // PGID = my PID
        signal(SIGINT, SIG_DFL);
        for _ in 0..100 {
            sleep(1);
        }
        exit(99);
    }

    // — GraveShift: Parent also calls setpgid to avoid race (shell pattern).
    setpgid(pid, pid);

    nanosleep_ms(200);

    // Send to process group (negative PID = PGID)
    let ret = kill(-pid, 2); // kill(-pgid, SIGINT)
    if ret != 0 {
        prints("  FAIL: kill(-pgid) returned ");
        print_num(ret as u32);
        prints("\n");
    }

    let status = wait_for_child(pid);
    check_signal_death(status, 2, "PGID kill");
}

/// Test 3: Signal during nanosleep
/// — GraveShift: Tests the nanosleep EINTR path specifically.
/// Child does short sleeps in a loop (like curses-demo's sleep_ms(16)).
/// Parent sends SIGINT while child is mid-nanosleep.
/// nanosleep must detect pending signal → return EINTR → signal delivered on
/// syscall return via check_signals_on_syscall_return().
fn test_signal_during_nanosleep() {
    prints("[TEST 3] Signal during nanosleep (EINTR path)\n");

    let pid = fork();
    if pid < 0 {
        prints("  FAIL: fork() failed\n");
        unsafe { FAIL_COUNT += 1; }
        return;
    }

    if pid == 0 {
        // — GraveShift: Child — rapid short sleeps like curses-demo animation.
        signal(SIGINT, SIG_DFL);
        for _ in 0..6000 {
            nanosleep_ms(16); // 16ms frames, ~100 seconds total
        }
        exit(99);
    }

    // — GraveShift: 200ms wait ensures child is deep in nanosleep loop.
    nanosleep_ms(200);

    let ret = kill(pid, 2);
    if ret != 0 {
        prints("  FAIL: kill() returned ");
        print_num(ret as u32);
        prints("\n");
    }

    let status = wait_for_child(pid);
    check_signal_death(status, 2, "nanosleep EINTR");
}

/// Test 4: Process group self-signal via kill(0, SIGINT)
/// — GraveShift: Child creates its own process group then sends SIGINT to
/// its own group (kill(0, SIGINT)). Tests self-signal delivery.
fn test_self_group_kill() {
    prints("[TEST 4] Self group kill(0, SIGINT)\n");

    let pid = fork();
    if pid < 0 {
        prints("  FAIL: fork() failed\n");
        unsafe { FAIL_COUNT += 1; }
        return;
    }

    if pid == 0 {
        // — GraveShift: Child — set own PGID, then signal own group.
        // SIG_DFL for SIGINT = terminate. Should kill ourselves.
        setpgid(0, 0);
        signal(SIGINT, SIG_DFL);

        // Give a moment then self-signal via process group
        nanosleep_ms(50);
        kill(0, 2); // kill(0, SIGINT) = send to own process group

        // Should be dead by now
        nanosleep_ms(500);
        exit(99);
    }

    let status = wait_for_child(pid);
    check_signal_death(status, 2, "self group kill");
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn nanosleep_ms(ms: u32) {
    let ts = time::Timespec {
        tv_sec: (ms / 1000) as i64,
        tv_nsec: ((ms % 1000) as i64) * 1_000_000,
    };
    let mut rem = time::Timespec { tv_sec: 0, tv_nsec: 0 };
    time::nanosleep(&ts, Some(&mut rem));
}

fn wait_for_child(pid: i32) -> i32 {
    let mut status: i32 = 0;
    loop {
        let ret = waitpid(pid, &mut status, 0);
        if ret == pid {
            return status;
        }
        if ret < 0 && ret != -(libc::errno::EINTR as i32) {
            prints("  FAIL: waitpid error ");
            print_num((-ret) as u32);
            prints("\n");
            return -1;
        }
        // EINTR — retry
    }
}

fn check_signal_death(status: i32, expected_sig: i32, test_name: &str) {
    if status < 0 {
        prints("  FAIL: ");
        prints(test_name);
        prints(" — waitpid error\n");
        unsafe { FAIL_COUNT += 1; }
        return;
    }

    let termsig = status & 0x7F;
    let exit_code = (status >> 8) & 0xFF;

    if termsig != 0 && termsig != 0x7F {
        // — GraveShift: WIFSIGNALED — child killed by signal directly
        if termsig == expected_sig {
            prints("  PASS: ");
            prints(test_name);
            prints(" — killed by signal ");
            print_num(termsig as u32);
            prints("\n");
            unsafe { PASS_COUNT += 1; }
        } else {
            prints("  FAIL: ");
            prints(test_name);
            prints(" — wrong signal ");
            print_num(termsig as u32);
            prints(" (expected ");
            print_num(expected_sig as u32);
            prints(")\n");
            unsafe { FAIL_COUNT += 1; }
        }
    } else if exit_code == (128 + expected_sig) as i32 {
        // — GraveShift: Our set_task_exit_status convention: exit code = 128+signo
        prints("  PASS: ");
        prints(test_name);
        prints(" — exit code ");
        print_num(exit_code as u32);
        prints(" (128+");
        print_num(expected_sig as u32);
        prints(")\n");
        unsafe { PASS_COUNT += 1; }
    } else if exit_code == 99 {
        prints("  FAIL: ");
        prints(test_name);
        prints(" — child survived (exit 99), signal never delivered!\n");
        unsafe { FAIL_COUNT += 1; }
    } else {
        prints("  FAIL: ");
        prints(test_name);
        prints(" — unexpected status=");
        print_num(status as u32);
        prints(" termsig=");
        print_num(termsig as u32);
        prints(" exit=");
        print_num(exit_code as u32);
        prints("\n");
        unsafe { FAIL_COUNT += 1; }
    }
}

fn print_num(n: u32) {
    if n == 0 {
        prints("0");
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
        prints(s);
    }
}
