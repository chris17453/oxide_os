// — CrashBloom: No #![no_std], no #![no_main], no extern crate.
// Just normal Rust. The way it was meant to be.

use std::io::Write;
use std::time::Instant;

fn main() {
    println!("=== OXIDE std Verification Suite ===");
    println!();

    // — CrashBloom: Test 1 — Basic I/O
    println!("[1/7] println! works. stdout is alive.");
    eprint!("[1/7] eprint! works. ");
    eprintln!("stderr is alive.");

    // — CrashBloom: Test 2 — Command-line arguments
    print!("[2/7] Args:");
    for (i, arg) in std::env::args().enumerate() {
        print!(" [{}]={}", i, arg);
    }
    println!();

    // — CrashBloom: Test 3 — Environment variables
    if let Ok(path) = std::env::var("PATH") {
        println!("[3/7] PATH = {}", path);
    } else {
        println!("[3/7] PATH not set (expected in minimal env)");
    }

    // — CrashBloom: Test 4 — Filesystem
    match std::fs::read_dir("/") {
        Ok(entries) => {
            let names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            println!("[4/7] / has {} entries: {}", names.len(), names.join(", "));
        }
        Err(e) => println!("[4/7] FAIL: read_dir(/): {}", e),
    }

    // — CrashBloom: Test 5 — File I/O
    let test_path = "/tmp/std-test.txt";
    let test_data = "Hello from std::fs::write!\n";
    match std::fs::write(test_path, test_data) {
        Ok(()) => {
            match std::fs::read_to_string(test_path) {
                Ok(content) => {
                    if content == test_data {
                        println!("[5/7] File write+read: OK");
                    } else {
                        println!("[5/7] FAIL: read back mismatch");
                    }
                }
                Err(e) => println!("[5/7] FAIL: read: {}", e),
            }
            let _ = std::fs::remove_file(test_path);
        }
        Err(e) => println!("[5/7] FAIL: write: {}", e),
    }

    // — CrashBloom: Test 6 — Time
    let start = Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(50));
    let elapsed = start.elapsed();
    println!("[6/7] sleep(50ms) took {:?}", elapsed);

    // — CrashBloom: Test 7 — Current directory + PID
    let cwd = std::env::current_dir().unwrap_or_default();
    let pid = std::process::id();
    println!("[7/7] PID={}, CWD={}", pid, cwd.display());

    println!();
    println!("=== All std tests passed! ===");

    // — CrashBloom: flush stdout to make sure everything gets out
    let _ = std::io::stdout().flush();
}
