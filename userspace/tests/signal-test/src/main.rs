//! Simple signal test - tests if SIGINT is delivered
#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    println!("=== Signal Delivery Test ===");
    println!("Testing if signals are delivered on syscall return...");

    // Test 1: Send signal to self via kill()
    println!("\nTest 1: Sending SIGTERM (15) to self");
    let pid = getpid();
    println!("  PID: {}", pid);

    let result = kill(pid, 15); // SIGTERM
    if result < 0 {
        println!("  ERROR: kill() failed with {}", result);
        return 1;
    }

    println!("  kill() succeeded, signal sent");
    println!("  If you see this, signal was NOT delivered (BUG!)");
    println!("  (Process should have terminated)");

    // Give time for signal to be delivered
    for _ in 0..10 {
        write(1, b".");
    }
    println!("");

    println!("\nTest FAILED: Process did not terminate on SIGTERM");
    1
}
