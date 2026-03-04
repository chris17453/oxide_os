//! Kernel Command Line Parser
//!
//! Parses space-separated tokens from the boot manager's command line and applies
//! them to kernel configuration. Called very early in init before most subsystems
//! are up — so this is no_alloc, no_lock, pure register-and-stack territory.
//!
//! Recognized options:
//! - `debug-all`, `debug-sched`, `debug-proc`, etc. → debug feature flags
//! - `root=/dev/vda2` → root device override
//! - `console=serial` → redirect console to serial
//! - `quiet` → suppress non-critical boot messages
//! - `nosmp` → disable SMP (boot on BSP only)
//!
//! — GraveShift: the kernel's first act of self-discovery after birth

use boot_proto::BootInfo;

/// Parsed kernel command line options.
/// — GraveShift: every field defaults to "don't change anything" because
/// the safest thing a config parser can do is nothing
pub struct KernelOptions {
    /// Override root device path (e.g., "/dev/vda2")
    pub root_device: Option<&'static str>,
    /// Redirect console output to serial port
    pub console_serial: bool,
    /// Suppress non-critical boot messages
    pub quiet: bool,
    /// Disable SMP — single CPU mode
    pub nosmp: bool,
    /// Debug flags (parsed from debug-* tokens)
    pub debug_all: bool,
    pub debug_sched: bool,
    pub debug_proc: bool,
    pub debug_fork: bool,
    pub debug_lock: bool,
    pub debug_syscall: bool,
    pub debug_input: bool,
    pub debug_mouse: bool,
    pub debug_tty: bool,
    pub debug_perf: bool,
}

impl KernelOptions {
    pub const fn default() -> Self {
        Self {
            root_device: None,
            console_serial: false,
            quiet: false,
            nosmp: false,
            debug_all: false,
            debug_sched: false,
            debug_proc: false,
            debug_fork: false,
            debug_lock: false,
            debug_syscall: false,
            debug_input: false,
            debug_mouse: false,
            debug_tty: false,
            debug_perf: false,
        }
    }
}

/// Global kernel options — set once during early init, read forever after
/// — GraveShift: static mut because we parse before anything concurrent exists
static mut KERNEL_OPTIONS: KernelOptions = KernelOptions::default();

/// Parse the boot info command line and store results globally.
/// Must be called once during early kernel init (before SMP).
///
/// — GraveShift: reading the boot manager's last will and testament
pub fn parse_cmdline(boot_info: &'static BootInfo) {
    let cmdline = match boot_info.cmdline() {
        Some(s) if !s.is_empty() => s,
        _ => return, // — GraveShift: nothing to parse, carry on
    };

    // SAFETY: Called once during single-threaded early init, before SMP or interrupts.
    let opts = unsafe { &mut *core::ptr::addr_of_mut!(KERNEL_OPTIONS) };

    for token in CmdlineTokenizer::new(cmdline) {
        match token {
            "debug-all" => opts.debug_all = true,
            "debug-sched" => opts.debug_sched = true,
            "debug-proc" => opts.debug_proc = true,
            "debug-fork" => opts.debug_fork = true,
            "debug-lock" => opts.debug_lock = true,
            "debug-syscall" | "debug-syscall-perf" => opts.debug_syscall = true,
            "debug-input" => opts.debug_input = true,
            "debug-mouse" => opts.debug_mouse = true,
            "debug-tty" | "debug-tty-read" => opts.debug_tty = true,
            "debug-perf" => opts.debug_perf = true,
            "quiet" => opts.quiet = true,
            "nosmp" => opts.nosmp = true,
            "console=serial" => opts.console_serial = true,
            other => {
                // Check for key=value patterns
                if let Some(val) = strip_prefix(other, "root=") {
                    // Store the root device — this is from boot_info which is 'static
                    opts.root_device = Some(val);
                }
                // Unknown options silently ignored — GraveShift: we're not your mother
            }
        }
    }
}

/// Get the parsed kernel options (read-only after init).
/// — GraveShift: safe to call from anywhere after parse_cmdline() returns
pub fn options() -> &'static KernelOptions {
    // SAFETY: Only written during single-threaded init, read-only after that.
    unsafe { &*core::ptr::addr_of!(KERNEL_OPTIONS) }
}

/// Simple space-separated tokenizer for kernel command line strings.
/// No heap, no allocations, pure iterator-over-&str territory.
/// — GraveShift: strtok for the Rust era
struct CmdlineTokenizer<'a> {
    remaining: &'a str,
}

impl<'a> CmdlineTokenizer<'a> {
    fn new(s: &'a str) -> Self {
        Self { remaining: s.trim() }
    }
}

impl<'a> Iterator for CmdlineTokenizer<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        // Skip leading whitespace
        self.remaining = self.remaining.trim_start();
        if self.remaining.is_empty() {
            return None;
        }

        // Find next space
        match self.remaining.find(' ') {
            Some(pos) => {
                let token = &self.remaining[..pos];
                self.remaining = &self.remaining[pos + 1..];
                Some(token)
            }
            None => {
                let token = self.remaining;
                self.remaining = "";
                Some(token)
            }
        }
    }
}

/// Helper: strip a prefix from a string, returning the remainder
fn strip_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.starts_with(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}
