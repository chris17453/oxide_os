//! Syscall Test Suite for OXIDE OS
//!
//! Tests kernel syscalls and reports pass/fail status.

#![no_std]
#![no_main]
#![allow(unused)]
#![allow(unsafe_op_in_unsafe_fn)]

extern crate alloc;

/// Test result
#[derive(Clone, Copy, PartialEq)]
enum TestResult {
    Pass,
    Fail,
    Skip,
}

/// Statistics tracker
struct Stats {
    passed: usize,
    failed: usize,
    skipped: usize,
}

impl Stats {
    fn new() -> Self {
        Stats {
            passed: 0,
            failed: 0,
            skipped: 0,
        }
    }

    fn record(&mut self, result: TestResult) {
        match result {
            TestResult::Pass => self.passed += 1,
            TestResult::Fail => self.failed += 1,
            TestResult::Skip => self.skipped += 1,
        }
    }
}

/// Print test result
fn report(name: &str, result: TestResult, details: &str) {
    let status = match result {
        TestResult::Pass => "[PASS]",
        TestResult::Fail => "[FAIL]",
        TestResult::Skip => "[SKIP]",
    };
    if details.is_empty() {
        libc::println!("{} {}", status, name);
    } else {
        libc::println!("{} {} - {}", status, name, details);
    }
}

/// Assert helper
fn assert_eq<T: PartialEq + core::fmt::Debug>(a: T, b: T, msg: &str) -> TestResult {
    if a == b {
        TestResult::Pass
    } else {
        libc::println!("  Expected: {:?}, got: {:?}", b, a);
        libc::println!("  {}", msg);
        TestResult::Fail
    }
}

fn assert_true(cond: bool, msg: &str) -> TestResult {
    if cond {
        TestResult::Pass
    } else {
        libc::println!("  {}", msg);
        TestResult::Fail
    }
}

fn assert_ge(a: i64, b: i64, msg: &str) -> TestResult {
    if a >= b {
        TestResult::Pass
    } else {
        libc::println!("  Expected >= {}, got {}", b, a);
        libc::println!("  {}", msg);
        TestResult::Fail
    }
}

// ============================================================================
// Process syscalls tests
// ============================================================================

fn test_getpid() -> TestResult {
    let pid = libc::getpid();
    assert_true(pid > 0, "getpid should return positive value")
}

fn test_getppid() -> TestResult {
    let ppid = libc::getppid();
    assert_ge(ppid as i64, 0, "getppid should return non-negative value")
}

fn test_getuid() -> TestResult {
    let uid = libc::getuid();
    // UID can be 0 (root) or positive
    TestResult::Pass // Just check it doesn't crash
}

fn test_getgid() -> TestResult {
    let gid = libc::getgid();
    TestResult::Pass // Just check it doesn't crash
}

fn test_geteuid() -> TestResult {
    let euid = libc::geteuid();
    TestResult::Pass
}

fn test_getegid() -> TestResult {
    let egid = libc::getegid();
    TestResult::Pass
}

fn test_gettid() -> TestResult {
    let tid = libc::gettid();
    assert_true(tid > 0, "gettid should return positive value")
}

// ============================================================================
// File descriptor syscalls tests
// ============================================================================

fn test_open_close() -> TestResult {
    // Try to open /etc/passwd which should exist
    let fd = libc::open("/etc/passwd", libc::O_RDONLY, 0);
    if fd < 0 {
        libc::println!("  open(/etc/passwd) returned {}", fd);
        return TestResult::Fail;
    }

    let result = libc::close(fd);
    if result != 0 {
        libc::println!("  close() returned {}", result);
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_read_file() -> TestResult {
    let fd = libc::open("/etc/passwd", libc::O_RDONLY, 0);
    if fd < 0 {
        libc::println!("  open failed: {}", fd);
        return TestResult::Fail;
    }

    let mut buf = [0u8; 64];
    let n = libc::read(fd, &mut buf);
    libc::close(fd);

    if n <= 0 {
        libc::println!("  read returned {}", n);
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_write_file() -> TestResult {
    // Create a test file
    let fd = libc::open(
        "/tmp/test_write",
        libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
        0o644,
    );
    if fd < 0 {
        libc::println!("  open for write failed: {}", fd);
        return TestResult::Fail;
    }

    let data = b"Hello, OXIDE!";
    let n = libc::write(fd, data);
    libc::close(fd);

    if n != data.len() as isize {
        libc::println!("  write returned {}, expected {}", n, data.len());
        return TestResult::Fail;
    }

    // Read it back
    let fd = libc::open("/tmp/test_write", libc::O_RDONLY, 0);
    if fd < 0 {
        libc::println!("  open for read failed: {}", fd);
        return TestResult::Fail;
    }

    let mut buf = [0u8; 64];
    let n = libc::read(fd, &mut buf);
    libc::close(fd);

    if n != data.len() as isize {
        libc::println!("  read back returned {}, expected {}", n, data.len());
        return TestResult::Fail;
    }

    if &buf[..data.len()] != data {
        libc::println!("  data mismatch");
        return TestResult::Fail;
    }

    // Cleanup
    libc::unlink("/tmp/test_write");

    TestResult::Pass
}

fn test_dup() -> TestResult {
    let fd = libc::open("/etc/passwd", libc::O_RDONLY, 0);
    if fd < 0 {
        return TestResult::Fail;
    }

    let fd2 = libc::dup(fd);
    if fd2 < 0 {
        libc::close(fd);
        libc::println!("  dup failed: {}", fd2);
        return TestResult::Fail;
    }

    // Both fds should be able to read
    let mut buf1 = [0u8; 10];
    let mut buf2 = [0u8; 10];

    let n1 = libc::read(fd, &mut buf1);
    let n2 = libc::read(fd2, &mut buf2);

    libc::close(fd);
    libc::close(fd2);

    // Since they share the same file position, fd2 should continue where fd left off
    if n1 <= 0 || n2 <= 0 {
        libc::println!("  reads failed: n1={}, n2={}", n1, n2);
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_dup2() -> TestResult {
    let fd = libc::open("/etc/passwd", libc::O_RDONLY, 0);
    if fd < 0 {
        return TestResult::Fail;
    }

    // Dup to a specific fd number
    let target_fd = 10;
    let result = libc::dup2(fd, target_fd);

    if result != target_fd {
        libc::close(fd);
        libc::println!("  dup2 returned {}, expected {}", result, target_fd);
        return TestResult::Fail;
    }

    // Read from the new fd
    let mut buf = [0u8; 10];
    let n = libc::read(target_fd, &mut buf);

    libc::close(fd);
    libc::close(target_fd);

    if n <= 0 {
        libc::println!("  read from dup2'd fd failed: {}", n);
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_lseek() -> TestResult {
    let fd = libc::open("/etc/passwd", libc::O_RDONLY, 0);
    if fd < 0 {
        return TestResult::Fail;
    }

    // Read 5 bytes
    let mut buf = [0u8; 5];
    libc::read(fd, &mut buf);

    // Seek back to start
    let pos = libc::lseek(fd, 0, libc::SEEK_SET);
    if pos != 0 {
        libc::close(fd);
        libc::println!("  lseek SEEK_SET returned {}", pos);
        return TestResult::Fail;
    }

    // Seek to end
    let end_pos = libc::lseek(fd, 0, libc::SEEK_END);
    if end_pos < 0 {
        libc::close(fd);
        libc::println!("  lseek SEEK_END returned {}", end_pos);
        return TestResult::Fail;
    }

    // Seek relative
    libc::lseek(fd, 0, libc::SEEK_SET);
    let cur_pos = libc::lseek(fd, 5, libc::SEEK_CUR);
    if cur_pos != 5 {
        libc::close(fd);
        libc::println!("  lseek SEEK_CUR returned {}", cur_pos);
        return TestResult::Fail;
    }

    libc::close(fd);
    TestResult::Pass
}

fn test_stat() -> TestResult {
    let mut st = libc::Stat::zeroed();
    let result = libc::stat("/etc/passwd", &mut st);

    if result < 0 {
        libc::println!("  stat failed: {}", result);
        return TestResult::Fail;
    }

    // Check it's a regular file
    if (st.mode & 0o170000) != 0o100000 {
        libc::println!("  /etc/passwd is not a regular file, mode={:o}", st.mode);
        return TestResult::Fail;
    }

    // Size should be > 0
    if st.size == 0 {
        libc::println!("  stat reported size 0");
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_fstat() -> TestResult {
    let fd = libc::open("/etc/passwd", libc::O_RDONLY, 0);
    if fd < 0 {
        return TestResult::Fail;
    }

    let mut st = libc::Stat::zeroed();
    let result = libc::fstat(fd, &mut st);
    libc::close(fd);

    if result < 0 {
        libc::println!("  fstat failed: {}", result);
        return TestResult::Fail;
    }

    if st.size == 0 {
        libc::println!("  fstat reported size 0");
        return TestResult::Fail;
    }

    TestResult::Pass
}

// ============================================================================
// Directory syscalls tests
// ============================================================================

fn test_mkdir_rmdir() -> TestResult {
    let dir = "/tmp/test_mkdir";

    // Create directory
    let result = libc::mkdir(dir, 0o755);
    if result < 0 {
        libc::println!("  mkdir failed: {}", result);
        return TestResult::Fail;
    }

    // Verify it exists with stat
    let mut st = libc::Stat::zeroed();
    if libc::stat(dir, &mut st) < 0 {
        libc::println!("  stat on new dir failed");
        libc::rmdir(dir);
        return TestResult::Fail;
    }

    // Check it's a directory
    if (st.mode & 0o170000) != 0o040000 {
        libc::println!("  created entry is not a directory");
        libc::rmdir(dir);
        return TestResult::Fail;
    }

    // Remove directory
    let result = libc::rmdir(dir);
    if result < 0 {
        libc::println!("  rmdir failed: {}", result);
        return TestResult::Fail;
    }

    // Verify it's gone
    if libc::stat(dir, &mut st) >= 0 {
        libc::println!("  directory still exists after rmdir");
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_chdir_getcwd() -> TestResult {
    let mut buf = [0u8; 256];
    let mut original_buf = [0u8; 256];

    // Get current directory
    let len = libc::getcwd(&mut original_buf);
    if len <= 0 {
        libc::println!("  getcwd failed: {}", len);
        return TestResult::Fail;
    }

    let original_cwd = core::str::from_utf8(&original_buf[..len as usize]).unwrap_or("");

    // Change to /tmp
    let result = libc::chdir("/tmp");
    if result < 0 {
        libc::println!("  chdir failed: {}", result);
        return TestResult::Fail;
    }

    // Verify we're in /tmp
    let len = libc::getcwd(&mut buf);
    if len <= 0 {
        libc::println!("  getcwd after chdir failed");
        return TestResult::Fail;
    }

    let new_cwd = core::str::from_utf8(&buf[..len as usize]).unwrap_or("");
    if new_cwd != "/tmp" {
        libc::println!("  after chdir(/tmp), getcwd returned '{}'", new_cwd);
        libc::chdir(original_cwd);
        return TestResult::Fail;
    }

    // Change back
    libc::chdir(original_cwd);

    TestResult::Pass
}

fn test_getdents() -> TestResult {
    let fd = libc::open("/bin", libc::O_RDONLY | libc::O_DIRECTORY, 0);
    if fd < 0 {
        libc::println!("  open(/bin) failed: {}", fd);
        return TestResult::Fail;
    }

    let mut buf = [0u8; 512];
    let n = libc::getdents(fd, &mut buf);
    libc::close(fd);

    if n <= 0 {
        libc::println!("  getdents returned {}", n);
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_unlink() -> TestResult {
    // Create a file
    let path = "/tmp/test_unlink";
    let fd = libc::open(path, libc::O_WRONLY | libc::O_CREAT, 0o644);
    if fd < 0 {
        return TestResult::Fail;
    }
    libc::close(fd);

    // Verify it exists
    let mut st = libc::Stat::zeroed();
    if libc::stat(path, &mut st) < 0 {
        libc::println!("  file doesn't exist after creation");
        return TestResult::Fail;
    }

    // Unlink it
    let result = libc::unlink(path);
    if result < 0 {
        libc::println!("  unlink failed: {}", result);
        return TestResult::Fail;
    }

    // Verify it's gone
    if libc::stat(path, &mut st) >= 0 {
        libc::println!("  file still exists after unlink");
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_rename() -> TestResult {
    let old_path = "/tmp/test_rename_old";
    let new_path = "/tmp/test_rename_new";

    // Create old file
    let fd = libc::open(old_path, libc::O_WRONLY | libc::O_CREAT, 0o644);
    if fd < 0 {
        return TestResult::Fail;
    }
    libc::write(fd, b"test data");
    libc::close(fd);

    // Rename it
    let result = libc::rename(old_path, new_path);
    if result < 0 {
        libc::unlink(old_path);
        libc::println!("  rename failed: {}", result);
        return TestResult::Fail;
    }

    // Verify old is gone
    let mut st = libc::Stat::zeroed();
    if libc::stat(old_path, &mut st) >= 0 {
        libc::println!("  old file still exists");
        libc::unlink(new_path);
        return TestResult::Fail;
    }

    // Verify new exists
    if libc::stat(new_path, &mut st) < 0 {
        libc::println!("  new file doesn't exist");
        return TestResult::Fail;
    }

    // Cleanup
    libc::unlink(new_path);

    TestResult::Pass
}

// ============================================================================
// Pipe syscalls tests
// ============================================================================

fn test_pipe() -> TestResult {
    let mut pipefd = [0i32; 2];
    let result = libc::pipe(&mut pipefd);

    if result < 0 {
        libc::println!("  pipe failed: {}", result);
        return TestResult::Fail;
    }

    let read_fd = pipefd[0];
    let write_fd = pipefd[1];

    // Write to pipe
    let msg = b"pipe test";
    let n = libc::write(write_fd, msg);
    if n != msg.len() as isize {
        libc::println!("  write to pipe returned {}", n);
        libc::close(read_fd);
        libc::close(write_fd);
        return TestResult::Fail;
    }

    // Read from pipe
    let mut buf = [0u8; 32];
    let n = libc::read(read_fd, &mut buf);
    if n != msg.len() as isize {
        libc::println!("  read from pipe returned {}", n);
        libc::close(read_fd);
        libc::close(write_fd);
        return TestResult::Fail;
    }

    if &buf[..msg.len()] != msg {
        libc::println!("  pipe data mismatch");
        libc::close(read_fd);
        libc::close(write_fd);
        return TestResult::Fail;
    }

    libc::close(read_fd);
    libc::close(write_fd);

    TestResult::Pass
}

// ============================================================================
// Process control syscalls tests
// ============================================================================

fn test_fork_wait() -> TestResult {
    let pid = libc::fork();

    if pid < 0 {
        libc::println!("  fork failed: {}", pid);
        return TestResult::Fail;
    }

    if pid == 0 {
        // Child process - exit with status 42
        libc::_exit(42);
    }

    // Parent process - wait for child
    let mut status = 0i32;
    let child = libc::waitpid(pid, &mut status, 0);

    if child != pid {
        libc::println!("  waitpid returned {}, expected {}", child, pid);
        return TestResult::Fail;
    }

    // Check exit status (WEXITSTATUS)
    let exit_code = (status >> 8) & 0xff;
    if exit_code != 42 {
        libc::println!("  child exit code was {}, expected 42", exit_code);
        return TestResult::Fail;
    }

    TestResult::Pass
}

// ============================================================================
// Signal syscalls tests
// ============================================================================

fn test_kill_self() -> TestResult {
    // Sending signal 0 tests if process exists without sending a real signal
    let pid = libc::getpid();
    let result = libc::kill(pid, 0);

    if result < 0 {
        libc::println!("  kill(self, 0) failed: {}", result);
        return TestResult::Fail;
    }

    TestResult::Pass
}

// ============================================================================
// Memory mapping syscalls tests
// ============================================================================

fn test_mmap_anonymous() -> TestResult {
    use libc::syscall::{MAP_FAILED, map_flags, prot, sys_mmap, sys_munmap};

    // Map 4KB of anonymous memory
    let addr = sys_mmap(
        core::ptr::null_mut(),
        4096,
        prot::PROT_READ | prot::PROT_WRITE,
        map_flags::MAP_PRIVATE | map_flags::MAP_ANONYMOUS,
        -1,
        0,
    );

    if addr == MAP_FAILED {
        libc::println!("  mmap returned MAP_FAILED");
        return TestResult::Fail;
    }

    // Write to the mapped memory
    unsafe {
        let ptr = addr as *mut u8;
        ptr.write(0x42);
        ptr.add(4095).write(0x99);

        // Read back
        if ptr.read() != 0x42 {
            libc::println!("  first byte mismatch");
            sys_munmap(addr, 4096);
            return TestResult::Fail;
        }
        if ptr.add(4095).read() != 0x99 {
            libc::println!("  last byte mismatch");
            sys_munmap(addr, 4096);
            return TestResult::Fail;
        }
    }

    // Unmap
    let result = sys_munmap(addr, 4096);
    if result != 0 {
        libc::println!("  munmap returned {}", result);
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_mmap_large() -> TestResult {
    use libc::syscall::{MAP_FAILED, map_flags, prot, sys_mmap, sys_munmap};

    // Map 64KB of memory
    let size = 64 * 1024;
    let addr = sys_mmap(
        core::ptr::null_mut(),
        size,
        prot::PROT_READ | prot::PROT_WRITE,
        map_flags::MAP_PRIVATE | map_flags::MAP_ANONYMOUS,
        -1,
        0,
    );

    if addr == MAP_FAILED {
        libc::println!("  mmap 64KB failed");
        return TestResult::Fail;
    }

    // Write pattern to verify all pages
    unsafe {
        for i in 0..16 {
            let offset = i * 4096;
            let ptr = (addr as *mut u8).add(offset);
            ptr.write(i as u8);
        }

        // Verify
        for i in 0..16 {
            let offset = i * 4096;
            let ptr = (addr as *mut u8).add(offset);
            if ptr.read() != i as u8 {
                libc::println!("  page {} data mismatch", i);
                sys_munmap(addr, size);
                return TestResult::Fail;
            }
        }
    }

    sys_munmap(addr, size);
    TestResult::Pass
}

fn test_mmap_hint_addr() -> TestResult {
    use libc::syscall::{MAP_FAILED, map_flags, prot, sys_mmap, sys_munmap};

    // Request mapping at a specific hint address
    let hint = 0x1000_0000 as *mut u8;
    let addr = sys_mmap(
        hint,
        4096,
        prot::PROT_READ | prot::PROT_WRITE,
        map_flags::MAP_PRIVATE | map_flags::MAP_ANONYMOUS,
        -1,
        0,
    );

    if addr == MAP_FAILED {
        libc::println!("  mmap with hint failed");
        return TestResult::Fail;
    }

    // Note: The system may or may not honor the hint
    // Success is getting a valid address back

    sys_munmap(addr, 4096);
    TestResult::Pass
}

fn test_mmap_fixed() -> TestResult {
    use libc::syscall::{MAP_FAILED, map_flags, prot, sys_mmap, sys_munmap};

    // First allocate some memory to get a valid address
    let temp = sys_mmap(
        core::ptr::null_mut(),
        4096,
        prot::PROT_READ | prot::PROT_WRITE,
        map_flags::MAP_PRIVATE | map_flags::MAP_ANONYMOUS,
        -1,
        0,
    );

    if temp == MAP_FAILED {
        return TestResult::Skip;
    }

    // Unmap it
    sys_munmap(temp, 4096);

    // Now try to map at that exact address with MAP_FIXED
    let addr = sys_mmap(
        temp,
        4096,
        prot::PROT_READ | prot::PROT_WRITE,
        map_flags::MAP_PRIVATE | map_flags::MAP_ANONYMOUS | map_flags::MAP_FIXED,
        -1,
        0,
    );

    if addr == MAP_FAILED {
        libc::println!("  MAP_FIXED at {} failed", temp as usize);
        return TestResult::Fail;
    }

    if addr != temp {
        libc::println!("  MAP_FIXED returned different address");
        sys_munmap(addr, 4096);
        return TestResult::Fail;
    }

    sys_munmap(addr, 4096);
    TestResult::Pass
}

fn test_mprotect() -> TestResult {
    use libc::syscall::{MAP_FAILED, map_flags, prot, sys_mmap, sys_mprotect, sys_munmap};

    // Allocate read-only memory
    let addr = sys_mmap(
        core::ptr::null_mut(),
        4096,
        prot::PROT_READ,
        map_flags::MAP_PRIVATE | map_flags::MAP_ANONYMOUS,
        -1,
        0,
    );

    if addr == MAP_FAILED {
        return TestResult::Fail;
    }

    // Change to read-write
    let result = sys_mprotect(addr, 4096, prot::PROT_READ | prot::PROT_WRITE);
    if result != 0 {
        libc::println!("  mprotect failed: {}", result);
        sys_munmap(addr, 4096);
        return TestResult::Fail;
    }

    // Now we should be able to write
    unsafe {
        (addr as *mut u8).write(0x42);
    }

    sys_munmap(addr, 4096);
    TestResult::Pass
}

// ============================================================================
// Main entry point
// ============================================================================

#[unsafe(no_mangle)]
pub extern "Rust" fn main() -> i32 {
    libc::println!("=== OXIDE Syscall Test Suite ===");
    libc::println!("");

    let mut stats = Stats::new();

    // Process syscalls
    libc::println!("-- Process syscalls --");
    let tests = [
        ("getpid", test_getpid as fn() -> TestResult),
        ("getppid", test_getppid),
        ("getuid", test_getuid),
        ("getgid", test_getgid),
        ("geteuid", test_geteuid),
        ("getegid", test_getegid),
        ("gettid", test_gettid),
    ];
    for (name, test) in tests {
        let result = test();
        report(name, result, "");
        stats.record(result);
    }

    // File descriptor syscalls
    libc::println!("");
    libc::println!("-- File descriptor syscalls --");
    let tests = [
        ("open/close", test_open_close as fn() -> TestResult),
        ("read", test_read_file),
        ("write", test_write_file),
        ("dup", test_dup),
        ("dup2", test_dup2),
        ("lseek", test_lseek),
        ("stat", test_stat),
        ("fstat", test_fstat),
    ];
    for (name, test) in tests {
        let result = test();
        report(name, result, "");
        stats.record(result);
    }

    // Directory syscalls
    libc::println!("");
    libc::println!("-- Directory syscalls --");
    let tests = [
        ("mkdir/rmdir", test_mkdir_rmdir as fn() -> TestResult),
        ("chdir/getcwd", test_chdir_getcwd),
        ("getdents", test_getdents),
        ("unlink", test_unlink),
        ("rename", test_rename),
    ];
    for (name, test) in tests {
        let result = test();
        report(name, result, "");
        stats.record(result);
    }

    // Pipe syscalls
    libc::println!("");
    libc::println!("-- Pipe syscalls --");
    let result = test_pipe();
    report("pipe", result, "");
    stats.record(result);

    // Process control
    libc::println!("");
    libc::println!("-- Process control syscalls --");
    let result = test_fork_wait();
    report("fork/wait", result, "");
    stats.record(result);

    // Signal syscalls
    libc::println!("");
    libc::println!("-- Signal syscalls --");
    let result = test_kill_self();
    report("kill (signal 0)", result, "");
    stats.record(result);

    // Memory mapping syscalls
    libc::println!("");
    libc::println!("-- Memory mapping syscalls --");
    let tests = [
        ("mmap anonymous", test_mmap_anonymous as fn() -> TestResult),
        ("mmap large", test_mmap_large),
        ("mmap hint addr", test_mmap_hint_addr),
        ("mmap fixed", test_mmap_fixed),
        ("mprotect", test_mprotect),
    ];
    for (name, test) in tests {
        let result = test();
        report(name, result, "");
        stats.record(result);
    }

    // Week 2: Modern filesystem syscalls
    libc::println!("");
    libc::println!("-- Week 2: Modern filesystem syscalls --");
    let tests = [
        ("statx", test_statx as fn() -> TestResult),
        ("faccessat2", test_faccessat2),
    ];
    for (name, test) in tests {
        let result = test();
        report(name, result, "");
        stats.record(result);
    }

    // Week 6: Security syscalls
    libc::println!("");
    libc::println!("-- Week 6: Security syscalls --");
    let tests = [
        ("prctl", test_prctl as fn() -> TestResult),
        ("capget", test_capget),
    ];
    for (name, test) in tests {
        let result = test();
        report(name, result, "");
        stats.record(result);
    }

    // New syscalls availability
    libc::println!("");
    libc::println!("-- New syscalls availability --");
    let result = test_new_syscalls_available();
    report("syscall registration", result, "");
    stats.record(result);

    // Summary
    libc::println!("");
    libc::println!("=== Summary ===");
    libc::println!("Passed:  {}", stats.passed);
    libc::println!("Failed:  {}", stats.failed);
    libc::println!("Skipped: {}", stats.skipped);
    libc::println!("Total:   {}", stats.passed + stats.failed + stats.skipped);

    if stats.failed > 0 { 1 } else { 0 }
}

// Global allocator is provided by libc

// ============================================================================
// Week 2: Modern filesystem syscalls tests
// ============================================================================

fn test_statx() -> TestResult {
    use libc::syscall::sys_statx;

    #[repr(C)]
    struct Statx {
        stx_mask: u32,
        stx_blksize: u32,
        stx_attributes: u64,
        stx_nlink: u32,
        stx_uid: u32,
        stx_gid: u32,
        stx_mode: u16,
        _spare0: [u16; 1],
        stx_ino: u64,
        stx_size: u64,
        stx_blocks: u64,
        stx_attributes_mask: u64,
        stx_atime_sec: i64,
        stx_atime_nsec: u32,
        stx_btime_sec: i64,
        stx_btime_nsec: u32,
        stx_ctime_sec: i64,
        stx_ctime_nsec: u32,
        stx_mtime_sec: i64,
        stx_mtime_nsec: u32,
        stx_rdev_major: u32,
        stx_rdev_minor: u32,
        stx_dev_major: u32,
        stx_dev_minor: u32,
        _spare2: [u64; 14],
    }

    let mut statx: Statx = unsafe { core::mem::zeroed() };
    let result = sys_statx(
        -100, // AT_FDCWD
        "/etc/passwd".as_ptr() as u64,
        "/etc/passwd".len(),
        0,
        0x7FF, // All fields
        &mut statx as *mut _ as u64,
    );

    if result < 0 {
        libc::println!("  statx returned {}", result);
        return TestResult::Fail;
    }

    if statx.stx_size == 0 {
        libc::println!("  statx reported size 0");
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_faccessat2() -> TestResult {
    use libc::syscall::sys_faccessat2;

    let result = sys_faccessat2(
        -100, // AT_FDCWD
        "/etc/passwd".as_ptr() as u64,
        "/etc/passwd".len(),
        0, // F_OK - test existence
        0, // flags
    );

    if result < 0 {
        libc::println!("  faccessat2(/etc/passwd) returned {}", result);
        return TestResult::Fail;
    }

    // Test non-existent file
    let result = sys_faccessat2(
        -100,
        "/nonexistent".as_ptr() as u64,
        "/nonexistent".len(),
        0,
        0,
    );

    if result >= 0 {
        libc::println!("  faccessat2 returned success for non-existent file");
        return TestResult::Fail;
    }

    TestResult::Pass
}

// ============================================================================
// Week 6: Security syscalls tests
// ============================================================================

fn test_prctl() -> TestResult {
    use libc::syscall::sys_prctl;

    // PR_GET_DUMPABLE = 3
    let result = sys_prctl(3, 0, 0, 0, 0);
    if result < 0 {
        libc::println!("  prctl(PR_GET_DUMPABLE) returned {}", result);
        return TestResult::Fail;
    }

    // PR_SET_DUMPABLE = 4
    let result = sys_prctl(4, 1, 0, 0, 0);
    if result < 0 {
        libc::println!("  prctl(PR_SET_DUMPABLE) returned {}", result);
        return TestResult::Fail;
    }

    TestResult::Pass
}

fn test_capget() -> TestResult {
    use libc::syscall::sys_capget;

    #[repr(C)]
    struct CapUserHeader {
        version: u32,
        pid: i32,
    }

    #[repr(C)]
    struct CapUserData {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }

    let hdr = CapUserHeader {
        version: 0x20080522, // LINUX_CAPABILITY_VERSION_3
        pid: 0,              // current process
    };

    let mut data: CapUserData = unsafe { core::mem::zeroed() };

    let result = sys_capget(&hdr as *const _ as u64, &mut data as *mut _ as u64);

    if result < 0 {
        libc::println!("  capget returned {}", result);
        return TestResult::Fail;
    }

    // Should have some capabilities set
    if data.effective == 0 && data.permitted == 0 {
        libc::println!("  capget returned no capabilities");
        return TestResult::Fail;
    }

    TestResult::Pass
}

// ============================================================================
// New syscalls availability tests
// ============================================================================

fn test_new_syscalls_available() -> TestResult {
    // Test that all new syscalls are registered and return expected errors
    use libc::syscall::{
        sys_epoll_pwait2, sys_pidfd_open, sys_recvmmsg, sys_sendmmsg, sys_signalfd,
        sys_timerfd_create, sys_unshare,
    };

    // These should return ENOSYS (-38) not EINVAL or other errors
    let result = sys_timerfd_create(1, 0);
    if result != -38 {
        libc::println!("  timerfd_create returned {} instead of ENOSYS", result);
        return TestResult::Fail;
    }

    let result = sys_signalfd(-1, 0, 0);
    if result != -38 {
        libc::println!("  signalfd returned {} instead of ENOSYS", result);
        return TestResult::Fail;
    }

    let result = sys_unshare(0);
    if result != -38 {
        libc::println!("  unshare returned {} instead of ENOSYS", result);
        return TestResult::Fail;
    }

    let result = sys_pidfd_open(1, 0);
    if result != -38 {
        libc::println!("  pidfd_open returned {} instead of ENOSYS", result);
        return TestResult::Fail;
    }

    TestResult::Pass
}
