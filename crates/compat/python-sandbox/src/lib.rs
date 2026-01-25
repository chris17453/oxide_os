//! Python Sandbox for OXIDE OS
//!
//! Provides secure Python script execution with resource limits.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

/// Filesystem policy
#[derive(Debug, Clone)]
pub struct FsPolicy {
    /// Allowed read paths
    pub read_paths: Vec<String>,
    /// Allowed write paths
    pub write_paths: Vec<String>,
    /// Denied path patterns
    pub deny_patterns: Vec<String>,
}

impl FsPolicy {
    /// Create new policy
    pub fn new() -> Self {
        FsPolicy {
            read_paths: Vec::new(),
            write_paths: Vec::new(),
            deny_patterns: Vec::new(),
        }
    }

    /// Allow read access to path
    pub fn allow_read(mut self, path: &str) -> Self {
        self.read_paths.push(String::from(path));
        self
    }

    /// Allow write access to path
    pub fn allow_write(mut self, path: &str) -> Self {
        self.write_paths.push(String::from(path));
        self
    }

    /// Deny access to pattern
    pub fn deny_pattern(mut self, pattern: &str) -> Self {
        self.deny_patterns.push(String::from(pattern));
        self
    }

    /// Check if path is readable
    pub fn can_read(&self, path: &str) -> bool {
        // Check deny patterns first
        for pattern in &self.deny_patterns {
            if path.contains(pattern.as_str()) {
                return false;
            }
        }

        // Check allow list
        for allowed in &self.read_paths {
            if path.starts_with(allowed.as_str()) {
                return true;
            }
        }

        false
    }

    /// Check if path is writable
    pub fn can_write(&self, path: &str) -> bool {
        // Check deny patterns first
        for pattern in &self.deny_patterns {
            if path.contains(pattern.as_str()) {
                return false;
            }
        }

        // Check allow list
        for allowed in &self.write_paths {
            if path.starts_with(allowed.as_str()) {
                return true;
            }
        }

        false
    }
}

impl Default for FsPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Network policy
#[derive(Debug, Clone)]
pub struct NetPolicy {
    /// Allow any network access
    pub allow_network: bool,
    /// Allowed hosts
    pub allowed_hosts: Vec<String>,
    /// Allowed ports
    pub allowed_ports: Vec<u16>,
}

impl NetPolicy {
    /// Create policy that denies all network
    pub fn deny_all() -> Self {
        NetPolicy {
            allow_network: false,
            allowed_hosts: Vec::new(),
            allowed_ports: Vec::new(),
        }
    }

    /// Create policy that allows all network
    pub fn allow_all() -> Self {
        NetPolicy {
            allow_network: true,
            allowed_hosts: Vec::new(),
            allowed_ports: Vec::new(),
        }
    }

    /// Allow specific host
    pub fn allow_host(mut self, host: &str) -> Self {
        self.allowed_hosts.push(String::from(host));
        self
    }

    /// Allow specific port
    pub fn allow_port(mut self, port: u16) -> Self {
        self.allowed_ports.push(port);
        self
    }

    /// Check if network access is allowed
    pub fn can_connect(&self, host: &str, port: u16) -> bool {
        if !self.allow_network {
            return false;
        }

        let host_ok = self.allowed_hosts.is_empty() || self.allowed_hosts.iter().any(|h| h == host);

        let port_ok = self.allowed_ports.is_empty() || self.allowed_ports.contains(&port);

        host_ok && port_ok
    }
}

impl Default for NetPolicy {
    fn default() -> Self {
        Self::deny_all()
    }
}

/// Resource limits
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum CPU time in seconds
    pub cpu_time: u64,
    /// Maximum memory in bytes
    pub memory: usize,
    /// Maximum open files
    pub open_files: usize,
    /// Maximum subprocesses (usually 0)
    pub processes: usize,
    /// Maximum execution steps
    pub max_steps: u64,
    /// Maximum recursion depth
    pub max_recursion: usize,
}

impl ResourceLimits {
    /// Create default limits
    pub fn new() -> Self {
        ResourceLimits {
            cpu_time: 30,
            memory: 100 * 1024 * 1024, // 100MB
            open_files: 10,
            processes: 0,
            max_steps: 10_000_000,
            max_recursion: 100,
        }
    }

    /// Create strict limits
    pub fn strict() -> Self {
        ResourceLimits {
            cpu_time: 5,
            memory: 10 * 1024 * 1024, // 10MB
            open_files: 0,
            processes: 0,
            max_steps: 100_000,
            max_recursion: 20,
        }
    }

    /// Create permissive limits
    pub fn permissive() -> Self {
        ResourceLimits {
            cpu_time: 3600,
            memory: 1024 * 1024 * 1024, // 1GB
            open_files: 100,
            processes: 10,
            max_steps: u64::MAX,
            max_recursion: 1000,
        }
    }

    /// Set CPU time limit
    pub fn max_cpu_time(mut self, seconds: u64) -> Self {
        self.cpu_time = seconds;
        self
    }

    /// Set memory limit
    pub fn max_memory(mut self, bytes: usize) -> Self {
        self.memory = bytes;
        self
    }

    /// Set open files limit
    pub fn max_open_files(mut self, count: usize) -> Self {
        self.open_files = count;
        self
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::new()
    }
}

/// Python sandbox configuration
#[derive(Clone)]
pub struct PythonSandbox {
    /// Allowed modules
    allowed_modules: BTreeSet<String>,
    /// Blocked modules
    blocked_modules: BTreeSet<String>,
    /// Filesystem policy
    fs_policy: FsPolicy,
    /// Network policy
    net_policy: NetPolicy,
    /// Resource limits
    limits: ResourceLimits,
    /// Allow eval/exec
    allow_eval: bool,
    /// Allow import *
    allow_star_import: bool,
    /// Allow __builtins__ access
    allow_builtins_access: bool,
}

impl PythonSandbox {
    /// Create new sandbox
    pub fn new() -> Self {
        let mut allowed = BTreeSet::new();
        // Safe modules by default
        allowed.insert(String::from("math"));
        allowed.insert(String::from("json"));
        allowed.insert(String::from("datetime"));
        allowed.insert(String::from("collections"));
        allowed.insert(String::from("itertools"));
        allowed.insert(String::from("functools"));
        allowed.insert(String::from("operator"));
        allowed.insert(String::from("string"));
        allowed.insert(String::from("re"));
        allowed.insert(String::from("random"));
        allowed.insert(String::from("hashlib"));
        allowed.insert(String::from("base64"));

        let mut blocked = BTreeSet::new();
        // Dangerous modules blocked by default
        blocked.insert(String::from("os"));
        blocked.insert(String::from("sys"));
        blocked.insert(String::from("subprocess"));
        blocked.insert(String::from("shutil"));
        blocked.insert(String::from("ctypes"));
        blocked.insert(String::from("importlib"));
        blocked.insert(String::from("builtins"));
        blocked.insert(String::from("__builtin__"));
        blocked.insert(String::from("gc"));
        blocked.insert(String::from("code"));
        blocked.insert(String::from("codeop"));
        blocked.insert(String::from("compile"));
        blocked.insert(String::from("marshal"));
        blocked.insert(String::from("pickle"));
        blocked.insert(String::from("socket"));
        blocked.insert(String::from("_thread"));
        blocked.insert(String::from("threading"));
        blocked.insert(String::from("multiprocessing"));

        PythonSandbox {
            allowed_modules: allowed,
            blocked_modules: blocked,
            fs_policy: FsPolicy::new(),
            net_policy: NetPolicy::deny_all(),
            limits: ResourceLimits::new(),
            allow_eval: false,
            allow_star_import: false,
            allow_builtins_access: false,
        }
    }

    /// Allow module
    pub fn allow_module(mut self, module: &str) -> Self {
        self.blocked_modules.remove(module);
        self.allowed_modules.insert(String::from(module));
        self
    }

    /// Block module
    pub fn block_module(mut self, module: &str) -> Self {
        self.allowed_modules.remove(module);
        self.blocked_modules.insert(String::from(module));
        self
    }

    /// Allow read access to path
    pub fn allow_read(mut self, path: &str) -> Self {
        self.fs_policy.read_paths.push(String::from(path));
        self
    }

    /// Allow write access to path
    pub fn allow_write(mut self, path: &str) -> Self {
        self.fs_policy.write_paths.push(String::from(path));
        self
    }

    /// Deny network access
    pub fn deny_network(mut self) -> Self {
        self.net_policy = NetPolicy::deny_all();
        self
    }

    /// Allow network access
    pub fn allow_network(mut self) -> Self {
        self.net_policy.allow_network = true;
        self
    }

    /// Set memory limit
    pub fn max_memory(mut self, bytes: usize) -> Self {
        self.limits.memory = bytes;
        self
    }

    /// Set CPU time limit
    pub fn max_cpu_time(mut self, seconds: u64) -> Self {
        self.limits.cpu_time = seconds;
        self
    }

    /// Check if module is allowed
    pub fn is_module_allowed(&self, module: &str) -> bool {
        if self.blocked_modules.contains(module) {
            return false;
        }
        self.allowed_modules.contains(module)
    }

    /// Get filesystem policy
    pub fn fs_policy(&self) -> &FsPolicy {
        &self.fs_policy
    }

    /// Get network policy
    pub fn net_policy(&self) -> &NetPolicy {
        &self.net_policy
    }

    /// Get resource limits
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Execute Python code (stub - actual execution requires interpreter)
    pub fn exec(&self, _code: &str) -> Result<(), SandboxError> {
        // Would execute code in sandboxed Python interpreter
        Ok(())
    }

    /// Execute Python file (stub)
    pub fn exec_file(&self, _path: &str) -> Result<(), SandboxError> {
        // Would load and execute file in sandboxed Python interpreter
        Ok(())
    }
}

impl Default for PythonSandbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Sandbox error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    /// Module not allowed
    ModuleNotAllowed(String),
    /// Filesystem access denied
    FsAccessDenied(String),
    /// Network access denied
    NetworkDenied,
    /// Resource limit exceeded
    ResourceLimitExceeded(String),
    /// Execution timeout
    Timeout,
    /// Syntax error
    SyntaxError(String),
    /// Runtime error
    RuntimeError(String),
    /// Security violation
    SecurityViolation(String),
}

impl core::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ModuleNotAllowed(m) => write!(f, "module '{}' not allowed in sandbox", m),
            Self::FsAccessDenied(p) => write!(f, "filesystem access denied: {}", p),
            Self::NetworkDenied => write!(f, "network access denied"),
            Self::ResourceLimitExceeded(r) => write!(f, "resource limit exceeded: {}", r),
            Self::Timeout => write!(f, "execution timeout"),
            Self::SyntaxError(e) => write!(f, "syntax error: {}", e),
            Self::RuntimeError(e) => write!(f, "runtime error: {}", e),
            Self::SecurityViolation(v) => write!(f, "security violation: {}", v),
        }
    }
}

/// Safe builtins for sandboxed Python
pub struct SafeBuiltins;

impl SafeBuiltins {
    /// Get list of safe builtin functions
    pub fn safe_functions() -> &'static [&'static str] {
        &[
            "abs",
            "all",
            "any",
            "ascii",
            "bin",
            "bool",
            "bytearray",
            "bytes",
            "callable",
            "chr",
            "complex",
            "dict",
            "divmod",
            "enumerate",
            "filter",
            "float",
            "format",
            "frozenset",
            "getattr",
            "hasattr",
            "hash",
            "hex",
            "id",
            "int",
            "isinstance",
            "issubclass",
            "iter",
            "len",
            "list",
            "map",
            "max",
            "min",
            "next",
            "object",
            "oct",
            "ord",
            "pow",
            "print",
            "range",
            "repr",
            "reversed",
            "round",
            "set",
            "slice",
            "sorted",
            "str",
            "sum",
            "tuple",
            "type",
            "zip",
        ]
    }

    /// Get list of unsafe builtin functions to remove
    pub fn unsafe_functions() -> &'static [&'static str] {
        &[
            "compile",
            "eval",
            "exec",
            "globals",
            "locals",
            "open",
            "__import__",
            "input",
            "memoryview",
            "vars",
        ]
    }

    /// Get list of unsafe attributes to block
    pub fn unsafe_attributes() -> &'static [&'static str] {
        &[
            "__class__",
            "__bases__",
            "__subclasses__",
            "__mro__",
            "__code__",
            "__globals__",
            "__builtins__",
            "__dict__",
            "__func__",
            "__self__",
            "__module__",
        ]
    }
}

/// Sandbox execution context
pub struct SandboxContext {
    /// Sandbox configuration
    pub sandbox: PythonSandbox,
    /// Execution step count
    pub steps: u64,
    /// Start time
    pub start_time: u64,
    /// Current recursion depth
    pub recursion_depth: usize,
    /// Open file handles
    pub open_files: usize,
    /// Memory usage estimate
    pub memory_used: usize,
}

impl SandboxContext {
    /// Create new context
    pub fn new(sandbox: PythonSandbox, start_time: u64) -> Self {
        SandboxContext {
            sandbox,
            steps: 0,
            start_time,
            recursion_depth: 0,
            open_files: 0,
            memory_used: 0,
        }
    }

    /// Check if execution can continue
    pub fn can_continue(&self, current_time: u64) -> Result<(), SandboxError> {
        // Check step limit
        if self.steps >= self.sandbox.limits.max_steps {
            return Err(SandboxError::ResourceLimitExceeded(String::from(
                "max execution steps",
            )));
        }

        // Check time limit
        let elapsed = current_time.saturating_sub(self.start_time);
        if elapsed >= self.sandbox.limits.cpu_time {
            return Err(SandboxError::Timeout);
        }

        // Check memory limit
        if self.memory_used > self.sandbox.limits.memory {
            return Err(SandboxError::ResourceLimitExceeded(String::from("memory")));
        }

        // Check recursion limit
        if self.recursion_depth > self.sandbox.limits.max_recursion {
            return Err(SandboxError::ResourceLimitExceeded(String::from(
                "recursion depth",
            )));
        }

        Ok(())
    }

    /// Increment step counter
    pub fn step(&mut self) {
        self.steps += 1;
    }

    /// Enter function (recursion tracking)
    pub fn enter_function(&mut self) -> Result<(), SandboxError> {
        self.recursion_depth += 1;
        if self.recursion_depth > self.sandbox.limits.max_recursion {
            Err(SandboxError::ResourceLimitExceeded(String::from(
                "recursion depth",
            )))
        } else {
            Ok(())
        }
    }

    /// Exit function
    pub fn exit_function(&mut self) {
        self.recursion_depth = self.recursion_depth.saturating_sub(1);
    }
}
