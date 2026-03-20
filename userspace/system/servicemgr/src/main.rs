//! OXIDE Service Manager v2
//!
//! — NeonRoot: "systemd taught us what a service manager should be.
//!   Then it taught us what one shouldn't be. This is the middle ground."
//!
//! Features:
//!   - Service definitions from /etc/services.d/ (key=value format)
//!   - Dependency ordering via After= and Requires=
//!   - Topological sort for boot startup
//!   - Service types: simple (default), oneshot
//!   - Restart policies: always (default), on-failure, never
//!   - Stop timeout with SIGKILL escalation
//!   - Exponential restart backoff on rapid crashes
//!   - PID file tracking in /run/services/
//!   - CLI: start, stop, restart, status, list, enable, disable
//!
//! Service file format (/etc/services.d/<name>):
//!   PATH=/usr/bin/myservice       (required)
//!   ENABLED=yes                   (default: yes)
//!   RESTART=always|on-failure|never (default: always)
//!   TYPE=simple|oneshot           (default: simple)
//!   AFTER=networkd,resolvd        (comma-separated dependencies)
//!   REQUIRES=networkd             (hard dependencies — fail if missing)
//!   USER=nobody                   (run as user, default: root)
//!   STOP_TIMEOUT=10               (seconds before SIGKILL, default: 10)

#![no_std]
#![no_main]
#![allow(unused)]

extern crate alloc;

use core::cell::UnsafeCell;
use libc::dirent::{closedir, opendir, readdir};
use libc::stat::{S_IFDIR, S_IFMT, Stat, stat as libc_stat};
use libc::time::usleep;
use libc::*;

/// Maximum number of services
const MAX_SERVICES: usize = 32;
/// Maximum dependencies per service
const MAX_DEPS: usize = 8;

// ============================================================================
// Service types and state
// ============================================================================

/// Service execution type
/// — NeonRoot: "simple = stays alive. oneshot = runs and exits. That's it."
#[derive(Clone, Copy, PartialEq, Eq)]
enum ServiceType {
    Simple,  // long-running daemon (default)
    Oneshot, // run once, exit 0 = success
}

/// Restart policy
/// — NeonRoot: "always = cockroach mode. on-failure = trust but verify.
///   never = fire and forget."
#[derive(Clone, Copy, PartialEq, Eq)]
enum RestartPolicy {
    Always,    // restart on any exit (default)
    OnFailure, // restart only on non-zero exit
    Never,     // never restart
}

/// Service state
#[derive(Clone, Copy, PartialEq, Eq)]
enum ServiceState {
    Stopped,
    Starting,
    Running,
    Exited,  // oneshot completed successfully
    Failed,
}

/// Fixed-size name buffer
#[derive(Clone)]
struct NameBuf {
    data: [u8; 32],
    len: usize,
}

impl NameBuf {
    const fn empty() -> Self {
        NameBuf { data: [0; 32], len: 0 }
    }
    fn set(&mut self, s: &[u8]) {
        self.len = s.len().min(31);
        self.data[..self.len].copy_from_slice(&s[..self.len]);
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.len]).unwrap_or("")
    }
    fn matches(&self, s: &str) -> bool {
        self.as_str() == s
    }
}

/// Service definition
struct Service {
    name: NameBuf,
    path: [u8; 128],
    path_len: usize,
    state: ServiceState,
    pid: i32,
    enabled: bool,
    service_type: ServiceType,
    restart_policy: RestartPolicy,
    /// Dependencies: names of services that must start before this one
    after: [NameBuf; MAX_DEPS],
    after_count: usize,
    /// Hard dependencies: services that MUST be running
    requires: [NameBuf; MAX_DEPS],
    requires_count: usize,
    /// Restart tracking
    restart_count: u32,
    max_restarts: u32,
    /// Timestamp of last restart (for backoff calculation)
    last_restart_tick: u64,
    /// Last exit status
    last_exit_status: i32,
    /// Stop timeout in seconds (default 10)
    stop_timeout: u32,
    /// User/group
    user: NameBuf,
    uid: i32,
    gid: i32,
    /// Boot order (set by topological sort)
    boot_order: u32,
}

impl Service {
    const fn empty() -> Self {
        Service {
            name: NameBuf::empty(),
            path: [0; 128],
            path_len: 0,
            state: ServiceState::Stopped,
            pid: 0,
            enabled: true,
            service_type: ServiceType::Simple,
            restart_policy: RestartPolicy::Always,
            after: [const { NameBuf::empty() }; MAX_DEPS],
            after_count: 0,
            requires: [const { NameBuf::empty() }; MAX_DEPS],
            requires_count: 0,
            restart_count: 0,
            max_restarts: 10,
            last_restart_tick: 0,
            last_exit_status: 0,
            stop_timeout: 10,
            user: NameBuf::empty(),
            uid: -1,
            gid: -1,
            boot_order: u32::MAX,
        }
    }

    fn name_str(&self) -> &str { self.name.as_str() }

    fn path_str(&self) -> &str {
        core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("")
    }

    fn state_str(&self) -> &str {
        match self.state {
            ServiceState::Stopped => "stopped",
            ServiceState::Starting => "starting",
            ServiceState::Running => "running",
            ServiceState::Exited => "exited",
            ServiceState::Failed => "failed",
        }
    }

    fn restart_str(&self) -> &str {
        match self.restart_policy {
            RestartPolicy::Always => "always",
            RestartPolicy::OnFailure => "on-failure",
            RestartPolicy::Never => "never",
        }
    }

    fn type_str(&self) -> &str {
        match self.service_type {
            ServiceType::Simple => "simple",
            ServiceType::Oneshot => "oneshot",
        }
    }

    /// Should this service be restarted given its exit status?
    /// — NeonRoot: "The restart decision tree. Three policies, one question."
    fn should_restart(&self, exit_status: i32) -> bool {
        if self.restart_count >= self.max_restarts {
            return false;
        }
        match self.restart_policy {
            RestartPolicy::Always => true,
            RestartPolicy::OnFailure => exit_status != 0,
            RestartPolicy::Never => false,
        }
    }

    /// Calculate restart delay with exponential backoff.
    /// — NeonRoot: "1s, 2s, 4s, 8s, 16s, 30s cap. Crashing 10 times in a row
    ///   means something is fundamentally broken — slow down and think."
    fn restart_delay_secs(&self) -> u64 {
        let base: u64 = 1 << self.restart_count.min(4); // 1, 2, 4, 8, 16
        base.min(30) // cap at 30 seconds
    }
}

/// Thread-safe cell wrapper
struct SyncUnsafeCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncUnsafeCell<T> {}
impl<T> SyncUnsafeCell<T> {
    const fn new(value: T) -> Self { SyncUnsafeCell(UnsafeCell::new(value)) }
    fn get(&self) -> *mut T { self.0.get() }
}

/// Service registry
static SERVICES: SyncUnsafeCell<[Service; MAX_SERVICES]> =
    SyncUnsafeCell::new([const { Service::empty() }; MAX_SERVICES]);
static SERVICE_COUNT: SyncUnsafeCell<usize> = SyncUnsafeCell::new(0);

const PID_FILE: &str = "/run/service.pid";

// ============================================================================
// Logging
// ============================================================================

fn log(msg: &str) {
    prints("[servicemgr] ");
    prints(msg);
    prints("\n");
}

fn log_svc(name: &str, msg: &str) {
    prints("[servicemgr] ");
    prints(name);
    prints(": ");
    prints(msg);
    prints("\n");
}

// ============================================================================
// Helpers
// ============================================================================

fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() { return ""; }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 { len += 1; }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

fn stat_path(path: &str) -> bool {
    let mut st = Stat::zeroed();
    libc_stat(path, &mut st) == 0 && (st.mode & S_IFMT) == S_IFDIR
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn find_newline(data: &[u8]) -> Option<usize> {
    data.iter().position(|&b| b == b'\n')
}

fn parse_i32(s: &str) -> Option<i32> {
    let mut val: i32 = 0;
    let mut negative = false;
    let mut started = false;
    for c in s.bytes() {
        if c == b'-' && !started { negative = true; started = true; }
        else if c.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add((c - b'0') as i32)?;
            started = true;
        } else if started { break; }
    }
    if !started { return None; }
    Some(if negative { -val } else { val })
}

fn parse_u32(s: &str) -> Option<u32> {
    let mut val: u32 = 0;
    for c in s.bytes() {
        if c.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add((c - b'0') as u32)?;
        } else { break; }
    }
    Some(val)
}

fn itoa(mut n: i64, buf: &mut [u8]) -> usize {
    if n == 0 { buf[0] = b'0'; return 1; }
    let negative = n < 0;
    if negative { n = -n; }
    let mut i = 0;
    while n > 0 && i < buf.len() {
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
        i += 1;
    }
    if negative && i < buf.len() { buf[i] = b'-'; i += 1; }
    buf[..i].reverse();
    i
}

fn resolve_user(username: &str) -> Option<(i32, i32)> {
    let fd = open2("/etc/passwd", O_RDONLY);
    if fd < 0 { return None; }
    let mut buf = [0u8; 2048];
    let n = read(fd, &mut buf);
    close(fd);
    if n <= 0 { return None; }
    let content = core::str::from_utf8(&buf[..n as usize]).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let mut fields = line.split(':');
        let name = fields.next()?;
        if name == username {
            let _password = fields.next()?;
            let uid = parse_i32(fields.next()?)?;
            let gid = parse_i32(fields.next()?)?;
            return Some((uid, gid));
        }
    }
    None
}

fn get_value<'a>(content: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let start = find_bytes(content, key)? + key.len();
    let end = find_newline(&content[start..]).unwrap_or(content.len() - start);
    Some(&content[start..start + end])
}

fn get_value_str<'a>(content: &'a [u8], key: &[u8]) -> Option<&'a str> {
    core::str::from_utf8(get_value(content, key)?).ok()
}

fn current_ticks() -> u64 {
    let mut ts = libc::time::Timespec { tv_sec: 0, tv_nsec: 0 };
    libc::time::clock_gettime(1, &mut ts); // CLOCK_MONOTONIC
    ts.tv_sec as u64
}

// ============================================================================
// Service loading and parsing
// ============================================================================

fn load_services() {
    log("Loading service definitions");
    unsafe {
        let services = &mut *SERVICES.get();
        let count = &mut *SERVICE_COUNT.get();
        *count = 0;
        if stat_path("/etc/services.d") {
            load_services_from_dir(services, count);
        } else {
            log("No /etc/services.d, using defaults");
            add_default_services(services, count);
        }
        // Topological sort for boot ordering
        compute_boot_order(services, *count);
        prints("[servicemgr] Loaded ");
        print_i64(*count as i64);
        prints(" services\n");
    }
}

#[inline(never)]
fn load_services_from_dir(services: &mut [Service; MAX_SERVICES], count: &mut usize) {
    let dir = opendir("/etc/services.d");
    if let Some(mut dir) = dir {
        while let Some(entry) = readdir(&mut dir) {
            let name = entry.name();
            if name == "." || name == ".." { continue; }
            if *count < MAX_SERVICES {
                if parse_service_file(name, &mut services[*count]) {
                    *count += 1;
                }
            }
        }
        closedir(dir);
    } else {
        log("opendir failed, using defaults");
        add_default_services(services, count);
    }
}

fn add_default_services(services: &mut [Service; MAX_SERVICES], count: &mut usize) {
    let defaults: &[(&[u8], &[u8])] = &[
        (b"networkd", b"/usr/bin/networkd"),
        (b"resolvd", b"/usr/bin/resolvd"),
        (b"sshd", b"/usr/bin/sshd"),
    ];
    for (name, path) in defaults {
        if *count < MAX_SERVICES {
            let s = &mut services[*count];
            s.name.set(name);
            s.path[..path.len()].copy_from_slice(*path);
            s.path_len = path.len();
            s.restart_policy = RestartPolicy::Always;
            *count += 1;
        }
    }
}

/// Parse a service file from /etc/services.d/<name>
fn parse_service_file(name: &str, service: &mut Service) -> bool {
    let mut path_buf = [0u8; 256];
    let prefix = b"/etc/services.d/";
    let name_bytes = name.as_bytes();
    let total_len = prefix.len() + name_bytes.len();
    if total_len >= 256 { return false; }
    path_buf[..prefix.len()].copy_from_slice(prefix);
    path_buf[prefix.len()..total_len].copy_from_slice(name_bytes);

    let path_str = core::str::from_utf8(&path_buf[..total_len]).unwrap_or("");
    let fd = open2(path_str, O_RDONLY);
    if fd < 0 { return false; }
    let mut buf = [0u8; 512];
    let n = read(fd, &mut buf);
    close(fd);
    if n <= 0 { return false; }

    let content = &buf[..n as usize];
    service.name.set(name_bytes);

    // PATH= (required)
    if let Some(path_val) = get_value(content, b"PATH=") {
        service.path_len = path_val.len().min(127);
        service.path[..service.path_len].copy_from_slice(&path_val[..service.path_len]);
    } else {
        return false;
    }

    // ENABLED=
    if let Some(val) = get_value(content, b"ENABLED=") {
        service.enabled = !(val == b"no" || val == b"false" || val == b"0");
    }

    // RESTART=always|on-failure|never
    if let Some(val) = get_value_str(content, b"RESTART=") {
        service.restart_policy = match val.trim() {
            "on-failure" => RestartPolicy::OnFailure,
            "never" | "no" | "false" => RestartPolicy::Never,
            _ => RestartPolicy::Always, // "always", "yes", "true", default
        };
    }

    // TYPE=simple|oneshot
    if let Some(val) = get_value_str(content, b"TYPE=") {
        service.service_type = match val.trim() {
            "oneshot" => ServiceType::Oneshot,
            _ => ServiceType::Simple,
        };
    }

    // AFTER=svc1,svc2,svc3
    if let Some(val) = get_value_str(content, b"AFTER=") {
        for dep in val.trim().split(',') {
            let dep = dep.trim();
            if !dep.is_empty() && service.after_count < MAX_DEPS {
                service.after[service.after_count].set(dep.as_bytes());
                service.after_count += 1;
            }
        }
    }

    // REQUIRES=svc1,svc2
    if let Some(val) = get_value_str(content, b"REQUIRES=") {
        for dep in val.trim().split(',') {
            let dep = dep.trim();
            if !dep.is_empty() && service.requires_count < MAX_DEPS {
                service.requires[service.requires_count].set(dep.as_bytes());
                service.requires_count += 1;
            }
        }
    }

    // USER=
    if let Some(val) = get_value_str(content, b"USER=") {
        service.user.set(val.trim().as_bytes());
        if let Some((uid, gid)) = resolve_user(val.trim()) {
            service.uid = uid;
            service.gid = gid;
        }
    }

    // STOP_TIMEOUT=
    if let Some(val) = get_value_str(content, b"STOP_TIMEOUT=") {
        if let Some(t) = parse_u32(val.trim()) {
            service.stop_timeout = t.max(1).min(300); // 1-300s
        }
    }

    true
}

// ============================================================================
// Dependency ordering — topological sort
// ============================================================================

/// Compute boot order via topological sort on After= dependencies.
/// — NeonRoot: "Kahn's algorithm. O(V+E). No recursion, no stack overflow.
///   Services without dependencies get order 0, everything else follows."
fn compute_boot_order(services: &mut [Service; MAX_SERVICES], count: usize) {
    // Simple: assign order based on dependency depth
    // Start with services that have no After= deps (order 0)
    // Then services whose deps are all resolved get order = max(dep_orders) + 1
    let mut resolved = [false; MAX_SERVICES];
    let mut order = 0u32;

    // Multiple passes until everything is resolved
    for _pass in 0..count + 1 {
        let mut made_progress = false;
        for i in 0..count {
            if resolved[i] { continue; }

            // Check if all After= dependencies are resolved
            let mut all_deps_resolved = true;
            let mut max_dep_order = 0u32;
            for d in 0..services[i].after_count {
                let dep_name = services[i].after[d].as_str();
                let mut found = false;
                for j in 0..count {
                    if services[j].name.matches(dep_name) {
                        if resolved[j] {
                            max_dep_order = max_dep_order.max(services[j].boot_order);
                            found = true;
                        } else {
                            all_deps_resolved = false;
                        }
                        break;
                    }
                }
                // Dependency not found in service list — treat as resolved
                // (external dep, or typo — don't block boot)
                if !found && all_deps_resolved {
                    // still resolved, just not in our list
                }
            }

            if all_deps_resolved {
                services[i].boot_order = if services[i].after_count > 0 {
                    max_dep_order + 1
                } else {
                    0
                };
                resolved[i] = true;
                made_progress = true;
            }
        }
        if !made_progress { break; }
    }

    // Anything still unresolved (circular dep) gets max order
    for i in 0..count {
        if !resolved[i] {
            services[i].boot_order = 999;
            log_svc(services[i].name_str(), "WARNING: circular dependency, starting last");
        }
    }
}

/// Get sorted boot order indices
fn sorted_boot_indices(services: &[Service; MAX_SERVICES], count: usize, out: &mut [usize; MAX_SERVICES]) {
    // Simple insertion sort by boot_order
    for i in 0..count { out[i] = i; }
    for i in 1..count {
        let key = out[i];
        let key_order = services[key].boot_order;
        let mut j = i;
        while j > 0 && services[out[j - 1]].boot_order > key_order {
            out[j] = out[j - 1];
            j -= 1;
        }
        out[j] = key;
    }
}

// ============================================================================
// PID file management
// ============================================================================

fn pid_path(name: &str, buf: &mut [u8; 64]) -> usize {
    let prefix = b"/run/services/";
    let suffix = b".pid";
    let nb = name.as_bytes();
    let total = prefix.len() + nb.len() + suffix.len();
    if total >= 64 { return 0; }
    buf[..prefix.len()].copy_from_slice(prefix);
    buf[prefix.len()..prefix.len() + nb.len()].copy_from_slice(nb);
    buf[prefix.len() + nb.len()..total].copy_from_slice(suffix);
    total
}

fn write_pid_file(name: &str, pid: i32) {
    let _ = mkdir("/run/services", 0o755);
    let mut path = [0u8; 64];
    let len = pid_path(name, &mut path);
    if len == 0 { return; }
    let ps = core::str::from_utf8(&path[..len]).unwrap_or("");
    let fd = open(ps, (O_WRONLY | O_CREAT | O_TRUNC) as u32, 0o644);
    if fd >= 0 {
        let mut buf = [0u8; 16];
        let n = itoa(pid as i64, &mut buf);
        let _ = write(fd, &buf[..n]);
        close(fd);
    }
}

fn remove_pid_file(name: &str) {
    let mut path = [0u8; 64];
    let len = pid_path(name, &mut path);
    if len == 0 { return; }
    let ps = core::str::from_utf8(&path[..len]).unwrap_or("");
    let _ = unlink(ps);
}

fn read_pid_file(name: &str) -> i32 {
    let mut path = [0u8; 64];
    let len = pid_path(name, &mut path);
    if len == 0 { return 0; }
    let ps = core::str::from_utf8(&path[..len]).unwrap_or("");
    let fd = open2(ps, O_RDONLY);
    if fd < 0 { return 0; }
    let mut buf = [0u8; 16];
    let n = read(fd, &mut buf);
    close(fd);
    if n <= 0 { return 0; }
    let mut pid: i32 = 0;
    for i in 0..n as usize {
        if buf[i] >= b'0' && buf[i] <= b'9' {
            pid = pid * 10 + (buf[i] - b'0') as i32;
        } else { break; }
    }
    pid
}

fn process_exists(pid: i32) -> bool {
    pid > 0 && kill(pid, 0) == 0
}

// ============================================================================
// Service lifecycle: start, stop, restart
// ============================================================================

fn start_service(name: &str) -> bool {
    unsafe {
        let services = &mut *SERVICES.get();
        let count = *SERVICE_COUNT.get();
        for i in 0..count {
            if services[i].name.matches(name) {
                return start_service_idx(i);
            }
        }
    }
    log_svc(name, "not found");
    false
}

fn start_service_idx(idx: usize) -> bool {
    unsafe {
        let services = &mut *SERVICES.get();
        let count = *SERVICE_COUNT.get();

        if services[idx].state == ServiceState::Running {
            return true;
        }

        // Check hard dependencies (Requires=) — read dep names first to avoid borrow conflict
        let req_count = services[idx].requires_count;
        let mut req_names: [[u8; 32]; MAX_DEPS] = [[0; 32]; MAX_DEPS];
        let mut req_lens: [usize; MAX_DEPS] = [0; MAX_DEPS];
        for r in 0..req_count {
            let len = services[idx].requires[r].len;
            req_names[r][..len].copy_from_slice(&services[idx].requires[r].data[..len]);
            req_lens[r] = len;
        }

        for r in 0..req_count {
            let req_name = core::str::from_utf8(&req_names[r][..req_lens[r]]).unwrap_or("");
            let mut req_running = false;
            for j in 0..count {
                if services[j].name.matches(req_name) {
                    req_running = services[j].state == ServiceState::Running
                        || services[j].state == ServiceState::Exited;
                    break;
                }
            }
            if !req_running {
                log_svc(services[idx].name_str(), "required service not running: ");
                prints(req_name);
                prints("\n");
                services[idx].state = ServiceState::Failed;
                return false;
            }
        }

        let service = &mut services[idx];

        service.state = ServiceState::Starting;
        log_svc(service.name_str(), "starting");

        let pid = fork();
        if pid < 0 {
            service.state = ServiceState::Failed;
            log_svc(service.name_str(), "fork failed");
            return false;
        }

        if pid == 0 {
            // Child: redirect I/O
            let null_fd = open2("/dev/null", O_RDONLY);
            if null_fd >= 0 { dup2(null_fd, 0); if null_fd > 0 { close(null_fd); } }

            let kmsg_fd = open2("/dev/kmsg", O_WRONLY);
            if kmsg_fd >= 0 {
                dup2(kmsg_fd, 1);
                dup2(kmsg_fd, 2);
                if kmsg_fd > 2 { close(kmsg_fd); }
            }

            // Drop privileges
            if service.gid >= 0 { if setgid(service.gid as u32) != 0 { _exit(1); } }
            if service.uid >= 0 { if setuid(service.uid as u32) != 0 { _exit(1); } }

            exec(service.path_str());
            _exit(1);
        }

        // Parent
        service.pid = pid;
        service.state = ServiceState::Running;
        write_pid_file(service.name_str(), pid);
        log_svc(service.name_str(), "started pid=");
        print_i64(pid as i64);
        prints("\n");
        true
    }
}

/// Stop a service with timeout and SIGKILL escalation.
/// — NeonRoot: "SIGTERM is a polite request. SIGKILL is an eviction notice.
///   The timeout is how long we wait between asking and demanding."
fn stop_service(name: &str) -> bool {
    unsafe {
        let services = &mut *SERVICES.get();
        let count = *SERVICE_COUNT.get();
        for i in 0..count {
            if services[i].name.matches(name) {
                return stop_service_idx(i);
            }
        }
    }
    log_svc(name, "not found");
    false
}

fn stop_service_idx(idx: usize) -> bool {
    unsafe {
        let services = &mut *SERVICES.get();
        let service = &mut services[idx];

        if service.state != ServiceState::Running || service.pid <= 0 {
            service.state = ServiceState::Stopped;
            return true;
        }

        log_svc(service.name_str(), "stopping");

        // SIGTERM first
        kill(service.pid, SIGTERM);

        // Wait with timeout
        let deadline = current_ticks() + service.stop_timeout as u64;
        loop {
            let mut status = 0;
            let result = waitpid(service.pid, &mut status, WNOHANG);
            if result > 0 {
                // Exited cleanly
                service.state = ServiceState::Stopped;
                service.pid = 0;
                remove_pid_file(service.name_str());
                log_svc(service.name_str(), "stopped");
                return true;
            }
            if current_ticks() >= deadline {
                break;
            }
            usleep(100_000); // 100ms poll
        }

        // Timeout — SIGKILL
        log_svc(service.name_str(), "SIGKILL (stop timeout)");
        kill(service.pid, SIGKILL);
        let mut status = 0;
        waitpid(service.pid, &mut status, 0); // reap

        service.state = ServiceState::Stopped;
        service.pid = 0;
        remove_pid_file(service.name_str());
        true
    }
}

// ============================================================================
// Service monitoring
// ============================================================================

/// Check all running services, handle exits.
/// — NeonRoot: "The monitor loop. Every second, check who's alive.
///   The dead get logged. The worthy get resurrected."
fn check_services() {
    unsafe {
        let services = &mut *SERVICES.get();
        let count = *SERVICE_COUNT.get();
        let now = current_ticks();

        for i in 0..count {
            let service = &mut services[i];

            if service.state != ServiceState::Running || service.pid <= 0 {
                continue;
            }

            let mut status = 0;
            let result = waitpid(service.pid, &mut status, WNOHANG);
            if result <= 0 { continue; } // still running

            // Process exited
            remove_pid_file(service.name_str());
            let exit_code = (status >> 8) & 0xFF;
            service.last_exit_status = exit_code;
            service.pid = 0;

            if service.service_type == ServiceType::Oneshot && exit_code == 0 {
                // Oneshot success — mark as exited, don't restart
                service.state = ServiceState::Exited;
                log_svc(service.name_str(), "completed (oneshot)");
                continue;
            }

            service.state = ServiceState::Failed;
            prints("[servicemgr] ");
            prints(service.name_str());
            prints(": exited with code ");
            print_i64(exit_code as i64);
            prints("\n");

            // Restart logic
            if service.should_restart(exit_code) {
                let delay = service.restart_delay_secs();
                // Don't restart faster than the backoff allows
                if now >= service.last_restart_tick + delay {
                    service.restart_count += 1;
                    service.last_restart_tick = now;
                    prints("[servicemgr] ");
                    prints(service.name_str());
                    prints(": restarting (attempt ");
                    print_i64(service.restart_count as i64);
                    prints("/");
                    print_i64(service.max_restarts as i64);
                    prints(")\n");
                    start_service_idx(i);
                }
            } else if service.restart_count >= service.max_restarts {
                log_svc(service.name_str(), "max restarts reached, giving up");
            }
        }
    }
}

// ============================================================================
// Daemon mode
// ============================================================================

fn run_daemon() {
    // Redirect to kernel log
    let kmsg_fd = open2("/dev/kmsg", O_WRONLY);
    if kmsg_fd >= 0 {
        dup2(kmsg_fd, 1);
        dup2(kmsg_fd, 2);
        if kmsg_fd > 2 { close(kmsg_fd); }
    }

    log("starting daemon (v2)");
    load_services();

    let _ = mkdir("/run", 0o755);
    let pid = getpid();
    let fd = open(PID_FILE, (O_WRONLY | O_CREAT | O_TRUNC) as u32, 0o644);
    if fd >= 0 {
        let mut buf = [0u8; 16];
        let n = itoa(pid as i64, &mut buf);
        let _ = write(fd, &buf[..n]);
        close(fd);
    }

    // Start services in dependency order
    // — NeonRoot: "Boot order matters. journald before everything.
    //   networkd before resolvd. resolvd before sntpd. The dependency
    //   graph is the contract between services."
    unsafe {
        let services = &*SERVICES.get();
        let count = *SERVICE_COUNT.get();
        let mut order = [0usize; MAX_SERVICES];
        sorted_boot_indices(services, count, &mut order);

        for idx in 0..count {
            let i = order[idx];
            if services[i].enabled {
                log_svc(services[i].name_str(), "boot order ");
                print_i64(services[i].boot_order as i64);
                prints("\n");
                start_service_idx(i);
                // Small delay between service starts for stability
                usleep(100_000); // 100ms
            } else {
                log_svc(services[i].name_str(), "disabled, skipping");
            }
        }
    }

    log("all services started, monitoring");

    // Monitor loop
    loop {
        check_services();
        if usleep(1_000_000) < 0 {
            let _ = libc::poll::poll(&mut [], 1000);
        }
    }
}

// ============================================================================
// CLI commands
// ============================================================================

fn print_service_status(service: &Service) {
    prints("  ");
    prints(service.name_str());
    prints(": ");

    // Check live state or PID file
    if service.state == ServiceState::Running && service.pid > 0 {
        prints("running (pid ");
        print_i64(service.pid as i64);
        prints(")");
    } else {
        let pid = read_pid_file(service.name_str());
        if pid > 0 && process_exists(pid) {
            prints("running (pid ");
            print_i64(pid as i64);
            prints(")");
        } else {
            prints(service.state_str());
        }
    }

    prints(" [");
    prints(service.type_str());
    prints(", restart=");
    prints(service.restart_str());
    if service.enabled { prints(", enabled"); } else { prints(", disabled"); }
    if service.after_count > 0 {
        prints(", after=");
        for d in 0..service.after_count {
            if d > 0 { prints(","); }
            prints(service.after[d].as_str());
        }
    }
    prints("]\n");
}

fn list_services() {
    unsafe {
        let services = &*SERVICES.get();
        let count = *SERVICE_COUNT.get();
        prints("Services (");
        print_i64(count as i64);
        prints("):\n");
        let mut order = [0usize; MAX_SERVICES];
        sorted_boot_indices(services, count, &mut order);
        for idx in 0..count {
            print_service_status(&services[order[idx]]);
        }
    }
}

fn enable_service(name: &str) -> bool {
    set_enabled_flag(name, true)
}

fn disable_service(name: &str) -> bool {
    set_enabled_flag(name, false)
}

fn set_enabled_flag(name: &str, enabled: bool) -> bool {
    let mut path_buf = [0u8; 256];
    let prefix = b"/etc/services.d/";
    let nb = name.as_bytes();
    let total = prefix.len() + nb.len();
    if total >= 256 { return false; }
    path_buf[..prefix.len()].copy_from_slice(prefix);
    path_buf[prefix.len()..total].copy_from_slice(nb);
    let ps = core::str::from_utf8(&path_buf[..total]).unwrap_or("");

    let fd = open2(ps, O_RDONLY);
    if fd < 0 { log_svc(name, "not found"); return false; }
    let mut buf = [0u8; 512];
    let n = read(fd, &mut buf);
    close(fd);
    if n <= 0 { return false; }

    let content = &buf[..n as usize];
    let val_str = if enabled { b"ENABLED=yes\n" as &[u8] } else { b"ENABLED=no\n" };
    let mut new = [0u8; 512];
    let mut new_len = 0;

    if let Some(pos) = find_bytes(content, b"ENABLED=") {
        let line_end = find_newline(&content[pos..]).map(|e| pos + e + 1).unwrap_or(content.len());
        new[..pos].copy_from_slice(&content[..pos]);
        new_len = pos;
        new[new_len..new_len + val_str.len()].copy_from_slice(val_str);
        new_len += val_str.len();
        if line_end < content.len() {
            let rem = content.len() - line_end;
            new[new_len..new_len + rem].copy_from_slice(&content[line_end..]);
            new_len += rem;
        }
    } else {
        new[..content.len()].copy_from_slice(content);
        new_len = content.len();
        if new_len > 0 && new[new_len - 1] != b'\n' { new[new_len] = b'\n'; new_len += 1; }
        new[new_len..new_len + val_str.len()].copy_from_slice(val_str);
        new_len += val_str.len();
    }

    let fd = open(ps, (O_WRONLY | O_TRUNC) as u32, 0o644);
    if fd < 0 { return false; }
    let _ = write(fd, &new[..new_len]);
    close(fd);
    log_svc(name, if enabled { "enabled" } else { "disabled" });
    true
}

fn show_usage() {
    prints("Usage: servicemgr <command> [service]\n\n");
    prints("Commands:\n");
    prints("  daemon             Run as daemon (started by init)\n");
    prints("  start <service>    Start a service\n");
    prints("  stop <service>     Stop a service\n");
    prints("  restart <service>  Restart a service\n");
    prints("  enable <service>   Enable auto-start\n");
    prints("  disable <service>  Disable auto-start\n");
    prints("  status [service]   Show service status\n");
    prints("  list               List all services\n");
    prints("  help               Show this help\n");
    prints("\nService file format (/etc/services.d/<name>):\n");
    prints("  PATH=/usr/bin/svc    (required)\n");
    prints("  ENABLED=yes          (default: yes)\n");
    prints("  RESTART=always       (always|on-failure|never)\n");
    prints("  TYPE=simple          (simple|oneshot)\n");
    prints("  AFTER=svc1,svc2      (start after these services)\n");
    prints("  REQUIRES=svc1        (hard dependency)\n");
    prints("  USER=nobody          (run as user)\n");
    prints("  STOP_TIMEOUT=10      (seconds before SIGKILL)\n");
}

// ============================================================================
// Main
// ============================================================================

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 { show_usage(); return 1; }
    let cmd = cstr_to_str(unsafe { *argv.add(1) });
    let arg = if argc >= 3 { Some(cstr_to_str(unsafe { *argv.add(2) })) } else { None };

    match cmd {
        "daemon" => { run_daemon(); 0 }
        "start" => {
            if let Some(s) = arg { load_services(); if start_service(s) { 0 } else { 1 } }
            else { prints("Usage: servicemgr start <service>\n"); 1 }
        }
        "stop" => {
            if let Some(s) = arg { load_services(); if stop_service(s) { 0 } else { 1 } }
            else { prints("Usage: servicemgr stop <service>\n"); 1 }
        }
        "restart" => {
            if let Some(s) = arg { load_services(); stop_service(s); if start_service(s) { 0 } else { 1 } }
            else { prints("Usage: servicemgr restart <service>\n"); 1 }
        }
        "enable" => {
            if let Some(s) = arg { if enable_service(s) { 0 } else { 1 } }
            else { prints("Usage: servicemgr enable <service>\n"); 1 }
        }
        "disable" => {
            if let Some(s) = arg { if disable_service(s) { 0 } else { 1 } }
            else { prints("Usage: servicemgr disable <service>\n"); 1 }
        }
        "status" => {
            load_services();
            if let Some(s) = arg {
                unsafe {
                    let services = &*SERVICES.get();
                    let count = *SERVICE_COUNT.get();
                    let mut found = false;
                    for i in 0..count {
                        if services[i].name.matches(s) {
                            print_service_status(&services[i]);
                            found = true;
                            break;
                        }
                    }
                    if !found { prints(s); prints(": not found\n"); }
                }
            } else { list_services(); }
            0
        }
        "list" => { load_services(); list_services(); 0 }
        "help" | "--help" | "-h" => { show_usage(); 0 }
        _ => { prints("Unknown command: "); prints(cmd); prints("\n"); show_usage(); 1 }
    }
}
