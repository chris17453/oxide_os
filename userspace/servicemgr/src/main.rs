//! OXIDE Service Manager
//!
//! A simple service manager that:
//! - Reads service definitions from /etc/services.d/
//! - Starts/stops/monitors services
//! - Provides a CLI for service control
//!
//! Usage:
//!   servicemgr daemon   - Run as daemon (started by init)
//!   servicemgr start <service>
//!   servicemgr stop <service>
//!   servicemgr restart <service>
//!   servicemgr status [service]
//!   servicemgr list

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use libc::*;
use libc::dirent::{opendir, readdir, closedir};
use libc::time::usleep;

/// Maximum number of services
const MAX_SERVICES: usize = 32;

/// Service state
#[derive(Clone, Copy, PartialEq, Eq)]
enum ServiceState {
    Stopped,
    Starting,
    Running,
    Failed,
}

/// Service definition
struct Service {
    /// Service name
    name: [u8; 32],
    /// Service name length
    name_len: usize,
    /// Path to executable
    path: [u8; 128],
    /// Path length
    path_len: usize,
    /// Current state
    state: ServiceState,
    /// Process ID (if running)
    pid: i32,
    /// Auto-restart on failure
    restart: bool,
    /// Restart count
    restart_count: u32,
    /// Maximum restarts before giving up
    max_restarts: u32,
}

impl Service {
    const fn empty() -> Self {
        Service {
            name: [0; 32],
            name_len: 0,
            path: [0; 128],
            path_len: 0,
            state: ServiceState::Stopped,
            pid: 0,
            restart: true,
            restart_count: 0,
            max_restarts: 5,
        }
    }

    fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }

    fn path_str(&self) -> &str {
        core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("")
    }
}

/// Thread-safe cell wrapper
struct SyncUnsafeCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncUnsafeCell<T> {}

impl<T> SyncUnsafeCell<T> {
    const fn new(value: T) -> Self {
        SyncUnsafeCell(UnsafeCell::new(value))
    }

    fn get(&self) -> *mut T {
        self.0.get()
    }
}

/// Service registry
static SERVICES: SyncUnsafeCell<[Service; MAX_SERVICES]> = SyncUnsafeCell::new([const { Service::empty() }; MAX_SERVICES]);
static SERVICE_COUNT: SyncUnsafeCell<usize> = SyncUnsafeCell::new(0);

/// PID file path
const PID_FILE: &str = "/run/servicemgr.pid";

/// Print helper
fn log(msg: &str) {
    prints("[servicemgr] ");
    prints(msg);
    prints("\n");
}

/// Print with service name
fn log_service(service: &str, msg: &str) {
    prints("[servicemgr] ");
    prints(service);
    prints(": ");
    prints(msg);
    prints("\n");
}

/// Helper to convert C string to str
fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

/// Load service definitions from /etc/services.d/
fn load_services() {
    log("Loading service definitions");

    unsafe {
        let services = &mut *SERVICES.get();
        let count = &mut *SERVICE_COUNT.get();
        *count = 0;

        // Open services directory
        let dir = opendir("/etc/services.d");
        if let Some(mut dir) = dir {
            // Read directory entries
            while let Some(entry) = readdir(&mut dir) {
                let name = entry.name();

                // Skip . and ..
                if name == "." || name == ".." {
                    continue;
                }

                // Parse service file
                if *count < MAX_SERVICES {
                    if parse_service_file(name, &mut services[*count]) {
                        *count += 1;
                    }
                }
            }
            closedir(dir);
        } else {
            log("No /etc/services.d directory, using defaults");
            // Add default services
            add_default_services(services, count);
        }

        log("Loaded ");
        print_i64(*count as i64);
        prints(" services\n");
    }
}

/// Add default services if no configuration exists
fn add_default_services(services: &mut [Service; MAX_SERVICES], count: &mut usize) {
    // Add sshd as default service
    if *count < MAX_SERVICES {
        let service = &mut services[*count];
        let name = b"sshd";
        let path = b"/bin/sshd";
        service.name[..name.len()].copy_from_slice(name);
        service.name_len = name.len();
        service.path[..path.len()].copy_from_slice(path);
        service.path_len = path.len();
        service.restart = true;
        *count += 1;
    }
}

/// Parse a service file
fn parse_service_file(name: &str, service: &mut Service) -> bool {
    // Build path
    let mut path_buf = [0u8; 256];
    let prefix = b"/etc/services.d/";
    path_buf[..prefix.len()].copy_from_slice(prefix);

    let name_bytes = name.as_bytes();
    let total_len = prefix.len() + name_bytes.len();
    if total_len >= 256 {
        return false;
    }
    path_buf[prefix.len()..total_len].copy_from_slice(name_bytes);

    // Read file
    let path_str = core::str::from_utf8(&path_buf[..total_len]).unwrap_or("");
    let fd = open2(path_str, O_RDONLY);
    if fd < 0 {
        return false;
    }

    let mut buf = [0u8; 512];
    let n = read(fd, &mut buf);
    close(fd);

    if n <= 0 {
        return false;
    }

    // Parse content (simple format: PATH=<path>\nRESTART=yes/no)
    let content = &buf[..n as usize];

    // Service name from filename
    service.name_len = name_bytes.len().min(31);
    service.name[..service.name_len].copy_from_slice(&name_bytes[..service.name_len]);

    // Parse PATH=
    if let Some(path_start) = find_bytes(content, b"PATH=") {
        let start = path_start + 5;
        let end = find_newline(&content[start..]).unwrap_or(content.len() - start);
        let path_slice = &content[start..start + end];
        service.path_len = path_slice.len().min(127);
        service.path[..service.path_len].copy_from_slice(&path_slice[..service.path_len]);
    } else {
        return false; // PATH is required
    }

    // Parse RESTART=
    if let Some(restart_start) = find_bytes(content, b"RESTART=") {
        let start = restart_start + 8;
        let end = find_newline(&content[start..]).unwrap_or(content.len() - start);
        let value = &content[start..start + end];
        service.restart = value == b"yes" || value == b"true" || value == b"1";
    }

    true
}

/// Find bytes in slice
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// Find newline in slice
fn find_newline(data: &[u8]) -> Option<usize> {
    data.iter().position(|&b| b == b'\n')
}

/// Start a service
fn start_service(name: &str) -> bool {
    unsafe {
        let services = &mut *SERVICES.get();
        let count = *SERVICE_COUNT.get();

        for i in 0..count {
            if services[i].name_str() == name {
                return start_service_by_index(i);
            }
        }
    }

    log_service(name, "Service not found");
    false
}

/// Start service by index
fn start_service_by_index(index: usize) -> bool {
    unsafe {
        let services = &mut *SERVICES.get();
        let service = &mut services[index];

        if service.state == ServiceState::Running {
            log_service(service.name_str(), "Already running");
            return true;
        }

        service.state = ServiceState::Starting;
        log_service(service.name_str(), "Starting");

        // Fork and exec
        let pid = fork();
        if pid < 0 {
            service.state = ServiceState::Failed;
            log_service(service.name_str(), "Fork failed");
            return false;
        }

        if pid == 0 {
            // Child process
            // Redirect stdin/stdout/stderr to /dev/null for daemons
            let null_fd = open2("/dev/null", O_RDWR);
            if null_fd >= 0 {
                dup2(null_fd, 0);
                dup2(null_fd, 1);
                dup2(null_fd, 2);
                if null_fd > 2 {
                    close(null_fd);
                }
            }

            // Exec the service
            exec(service.path_str());
            _exit(1);
        }

        // Parent
        service.pid = pid;
        service.state = ServiceState::Running;
        log_service(service.name_str(), "Started with PID ");
        print_i64(pid as i64);
        prints("\n");

        true
    }
}

/// Stop a service
fn stop_service(name: &str) -> bool {
    unsafe {
        let services = &mut *SERVICES.get();
        let count = *SERVICE_COUNT.get();

        for i in 0..count {
            if services[i].name_str() == name {
                return stop_service_by_index(i);
            }
        }
    }

    log_service(name, "Service not found");
    false
}

/// Stop service by index
fn stop_service_by_index(index: usize) -> bool {
    unsafe {
        let services = &mut *SERVICES.get();
        let service = &mut services[index];

        if service.state != ServiceState::Running {
            log_service(service.name_str(), "Not running");
            return true;
        }

        log_service(service.name_str(), "Stopping");

        // Send SIGTERM
        if service.pid > 0 {
            kill(service.pid, SIGTERM);

            // Wait for process to exit (with timeout)
            let mut status = 0;
            let result = waitpid(service.pid, &mut status, 0);
            if result > 0 {
                service.state = ServiceState::Stopped;
                service.pid = 0;
                log_service(service.name_str(), "Stopped");
                return true;
            }
        }

        service.state = ServiceState::Stopped;
        service.pid = 0;
        true
    }
}

/// Get service status
fn service_status(name: &str) {
    unsafe {
        let services = &*SERVICES.get();
        let count = *SERVICE_COUNT.get();

        for i in 0..count {
            if services[i].name_str() == name {
                print_service_status(&services[i]);
                return;
            }
        }
    }

    prints(name);
    prints(": not found\n");
}

/// Print service status
fn print_service_status(service: &Service) {
    prints(service.name_str());
    prints(": ");

    match service.state {
        ServiceState::Stopped => prints("stopped"),
        ServiceState::Starting => prints("starting"),
        ServiceState::Running => {
            prints("running (pid ");
            print_i64(service.pid as i64);
            prints(")");
        }
        ServiceState::Failed => prints("failed"),
    }

    prints("\n");
}

/// List all services
fn list_services() {
    unsafe {
        let services = &*SERVICES.get();
        let count = *SERVICE_COUNT.get();

        prints("Services:\n");
        for i in 0..count {
            prints("  ");
            print_service_status(&services[i]);
        }

        if count == 0 {
            prints("  (no services configured)\n");
        }
    }
}

/// Check running services and restart failed ones
fn check_services() {
    unsafe {
        let services = &mut *SERVICES.get();
        let count = *SERVICE_COUNT.get();

        for i in 0..count {
            let service = &mut services[i];

            if service.state == ServiceState::Running && service.pid > 0 {
                // Check if process is still alive
                let mut status = 0;
                let result = waitpid(service.pid, &mut status, WNOHANG);

                if result > 0 {
                    // Process exited
                    service.state = ServiceState::Failed;
                    service.pid = 0;
                    log_service(service.name_str(), "Process exited");

                    // Try to restart if configured
                    if service.restart && service.restart_count < service.max_restarts {
                        service.restart_count += 1;
                        log_service(service.name_str(), "Restarting");
                        start_service_by_index(i);
                    }
                }
            }
        }
    }
}

/// Run as daemon
fn run_daemon() {
    log("Starting daemon mode");

    // Load service definitions
    load_services();

    // Create run directory
    let _ = mkdir("/run", 0o755);

    // Write PID file
    let pid = getpid();
    let fd = open(PID_FILE, (O_WRONLY | O_CREAT | O_TRUNC) as u32, 0o644);
    if fd >= 0 {
        let mut buf = [0u8; 16];
        let len = itoa(pid as i64, &mut buf);
        let _ = write(fd, &buf[..len]);
        close(fd);
    }

    // Start all services
    unsafe {
        let count = *SERVICE_COUNT.get();
        for i in 0..count {
            start_service_by_index(i);
        }
    }

    log("All services started, monitoring...");

    // Main loop - monitor services
    loop {
        // Check service health
        check_services();

        // Sleep for a bit before next check
        usleep(1000000); // 1 second
    }
}

/// Convert integer to string
fn itoa(mut n: i64, buf: &mut [u8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }

    let negative = n < 0;
    if negative {
        n = -n;
    }

    let mut i = 0;
    while n > 0 && i < buf.len() {
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
        i += 1;
    }

    if negative && i < buf.len() {
        buf[i] = b'-';
        i += 1;
    }

    // Reverse
    buf[..i].reverse();
    i
}

/// Show usage
fn show_usage() {
    prints("Usage: servicemgr <command> [service]\n");
    prints("\n");
    prints("Commands:\n");
    prints("  daemon           Run as daemon (started by init)\n");
    prints("  start <service>  Start a service\n");
    prints("  stop <service>   Stop a service\n");
    prints("  restart <service> Restart a service\n");
    prints("  status [service] Show service status\n");
    prints("  list             List all services\n");
    prints("  help             Show this help\n");
}

/// Main entry point
#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        show_usage();
        return 1;
    }

    let cmd = cstr_to_str(unsafe { *argv.add(1) });

    let arg = if argc >= 3 {
        Some(cstr_to_str(unsafe { *argv.add(2) }))
    } else {
        None
    };

    match cmd {
        "daemon" => {
            run_daemon();
            0
        }
        "start" => {
            if let Some(service) = arg {
                load_services();
                if start_service(service) { 0 } else { 1 }
            } else {
                prints("Usage: servicemgr start <service>\n");
                1
            }
        }
        "stop" => {
            if let Some(service) = arg {
                load_services();
                if stop_service(service) { 0 } else { 1 }
            } else {
                prints("Usage: servicemgr stop <service>\n");
                1
            }
        }
        "restart" => {
            if let Some(service) = arg {
                load_services();
                stop_service(service);
                if start_service(service) { 0 } else { 1 }
            } else {
                prints("Usage: servicemgr restart <service>\n");
                1
            }
        }
        "status" => {
            load_services();
            if let Some(service) = arg {
                service_status(service);
            } else {
                list_services();
            }
            0
        }
        "list" => {
            load_services();
            list_services();
            0
        }
        "help" | "--help" | "-h" => {
            show_usage();
            0
        }
        _ => {
            prints("Unknown command: ");
            prints(cmd);
            prints("\n");
            show_usage();
            1
        }
    }
}
