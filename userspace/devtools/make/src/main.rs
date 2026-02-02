//! OXIDE Make - Minimal Build Automation
//!
//! A minimal make utility for OXIDE OS.
//!
//! Usage:
//!   make [target]   - Build the specified target (or first target)
//!   make -f file    - Use specified makefile
//!   make -n         - Dry run (print commands without executing)

#![no_std]
#![no_main]

use libc::*;

/// Maximum Makefile size
const MAX_MAKEFILE: usize = 65536;

/// Maximum number of rules
const MAX_RULES: usize = 128;

/// Maximum dependencies per rule
const MAX_DEPS: usize = 32;

/// Maximum variables
const MAX_VARS: usize = 64;

/// Maximum command length
const MAX_CMD: usize = 1024;

/// Maximum targets on command line
const MAX_TARGETS: usize = 16;

/// Variable entry
struct Variable {
    name: [u8; 64],
    value: [u8; 256],
    used: bool,
}

impl Variable {
    const fn new() -> Self {
        Variable {
            name: [0u8; 64],
            value: [0u8; 256],
            used: false,
        }
    }
}

/// Rule entry
struct Rule {
    target: [u8; 128],
    deps: [[u8; 128]; MAX_DEPS],
    num_deps: usize,
    commands: [[u8; MAX_CMD]; 8],
    num_commands: usize,
    phony: bool,
}

impl Rule {
    const fn new() -> Self {
        const EMPTY_DEP: [u8; 128] = [0u8; 128];
        const EMPTY_CMD: [u8; MAX_CMD] = [0u8; MAX_CMD];
        Rule {
            target: [0u8; 128],
            deps: [EMPTY_DEP; MAX_DEPS],
            num_deps: 0,
            commands: [EMPTY_CMD; 8],
            num_commands: 0,
            phony: false,
        }
    }
}

/// Make state
struct Make {
    variables: [Variable; MAX_VARS],
    num_vars: usize,
    rules: [Rule; MAX_RULES],
    num_rules: usize,
    dry_run: bool,
    had_error: bool,
}

impl Make {
    fn new() -> Self {
        const EMPTY_VAR: Variable = Variable::new();
        const EMPTY_RULE: Rule = Rule::new();
        Make {
            variables: [EMPTY_VAR; MAX_VARS],
            num_vars: 0,
            rules: [EMPTY_RULE; MAX_RULES],
            num_rules: 0,
            dry_run: false,
            had_error: false,
        }
    }

    /// Set a variable
    fn set_var(&mut self, name: &[u8], value: &[u8]) {
        // Check if variable already exists
        for i in 0..self.num_vars {
            if self.variables[i].used && bytes_eq_trimmed(&self.variables[i].name, name) {
                copy_trimmed(&mut self.variables[i].value, value);
                return;
            }
        }

        // Add new variable
        if self.num_vars < MAX_VARS {
            self.variables[self.num_vars].used = true;
            copy_trimmed(&mut self.variables[self.num_vars].name, name);
            copy_trimmed(&mut self.variables[self.num_vars].value, value);
            self.num_vars += 1;
        }
    }

    /// Get a variable value
    fn get_var(&self, name: &[u8]) -> Option<&[u8]> {
        for i in 0..self.num_vars {
            if self.variables[i].used && bytes_eq_trimmed(&self.variables[i].name, name) {
                let len = self.variables[i]
                    .value
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(self.variables[i].value.len());
                return Some(&self.variables[i].value[..len]);
            }
        }
        None
    }

    /// Parse a Makefile
    fn parse(&mut self, data: &[u8]) {
        let mut pos = 0;
        let len = data.iter().position(|&c| c == 0).unwrap_or(data.len());
        let mut current_rule: Option<usize> = None;

        while pos < len {
            // Skip blank lines
            while pos < len && (data[pos] == b'\n' || data[pos] == b'\r') {
                pos += 1;
            }

            if pos >= len {
                break;
            }

            // Check for comment
            if data[pos] == b'#' {
                while pos < len && data[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }

            // Check for tab (recipe line)
            if data[pos] == b'\t' && current_rule.is_some() {
                pos += 1;
                let line_start = pos;
                while pos < len && data[pos] != b'\n' {
                    pos += 1;
                }

                let rule_idx = current_rule.unwrap();
                if self.rules[rule_idx].num_commands < 8 {
                    let cmd_idx = self.rules[rule_idx].num_commands;
                    copy_bytes(
                        &mut self.rules[rule_idx].commands[cmd_idx],
                        &data[line_start..pos],
                    );
                    self.rules[rule_idx].num_commands += 1;
                }
                continue;
            }

            // Get the line
            let line_start = pos;
            while pos < len && data[pos] != b'\n' {
                pos += 1;
            }
            let line = &data[line_start..pos];

            // Check for variable assignment
            if let Some(eq_pos) = line.iter().position(|&c| c == b'=') {
                // Check it's not part of := or +=
                let is_simple = eq_pos > 0 && line[eq_pos - 1] == b':';
                let name_end = if is_simple { eq_pos - 1 } else { eq_pos };
                let value_start = eq_pos + 1;

                self.set_var(&line[..name_end], &line[value_start..]);
                current_rule = None;
                continue;
            }

            // Check for rule (contains :)
            if let Some(colon_pos) = line.iter().position(|&c| c == b':') {
                // Check it's not :=
                if colon_pos + 1 < line.len() && line[colon_pos + 1] == b'=' {
                    // Simple assignment
                    self.set_var(&line[..colon_pos], &line[colon_pos + 2..]);
                    current_rule = None;
                    continue;
                }

                // It's a rule
                if self.num_rules < MAX_RULES {
                    let rule_idx = self.num_rules;
                    self.num_rules += 1;

                    // Parse target
                    copy_trimmed(&mut self.rules[rule_idx].target, &line[..colon_pos]);

                    // Parse dependencies
                    let deps_str = &line[colon_pos + 1..];
                    self.parse_deps(rule_idx, deps_str);

                    current_rule = Some(rule_idx);
                }
                continue;
            }

            // Unknown line, skip
            current_rule = None;
        }
    }

    /// Parse dependencies for a rule
    fn parse_deps(&mut self, rule_idx: usize, deps: &[u8]) {
        let mut pos = 0;
        let len = deps.iter().position(|&c| c == 0).unwrap_or(deps.len());

        while pos < len && self.rules[rule_idx].num_deps < MAX_DEPS {
            // Skip whitespace
            while pos < len && (deps[pos] == b' ' || deps[pos] == b'\t') {
                pos += 1;
            }

            if pos >= len {
                break;
            }

            // Get dependency name
            let start = pos;
            while pos < len && deps[pos] != b' ' && deps[pos] != b'\t' {
                pos += 1;
            }

            if pos > start {
                let dep_idx = self.rules[rule_idx].num_deps;
                copy_trimmed(&mut self.rules[rule_idx].deps[dep_idx], &deps[start..pos]);
                self.rules[rule_idx].num_deps += 1;
            }
        }
    }

    /// Find a rule by target name
    fn find_rule(&self, target: &[u8]) -> Option<usize> {
        for i in 0..self.num_rules {
            if bytes_eq_trimmed(&self.rules[i].target, target) {
                return Some(i);
            }
        }
        None
    }

    /// Build a target
    fn build(&mut self, target: &[u8]) -> bool {
        let rule_idx = match self.find_rule(target) {
            Some(idx) => idx,
            None => {
                // No rule - check if file exists
                if file_exists(target) {
                    return true;
                }
                eprints("make: *** No rule to make target '");
                prints(bytes_to_str(target));
                eprintlns("'");
                return false;
            }
        };

        // Copy dependencies to avoid borrow issues
        let num_deps = self.rules[rule_idx].num_deps;
        let mut deps: [[u8; 128]; MAX_DEPS] = [[0u8; 128]; MAX_DEPS];
        for i in 0..num_deps {
            deps[i] = self.rules[rule_idx].deps[i];
        }

        // Build dependencies first
        for i in 0..num_deps {
            if !self.build(&deps[i]) {
                return false;
            }
        }

        // Check if target needs rebuilding
        let needs_rebuild = self.needs_rebuild(rule_idx);

        if needs_rebuild {
            // Copy commands to avoid borrow issues
            let num_commands = self.rules[rule_idx].num_commands;
            let mut commands: [[u8; MAX_CMD]; 8] = [[0u8; MAX_CMD]; 8];
            for i in 0..num_commands {
                commands[i] = self.rules[rule_idx].commands[i];
            }

            // Execute commands
            for i in 0..num_commands {
                if !self.execute_command(rule_idx, &commands[i]) {
                    return false;
                }
            }
        }

        true
    }

    /// Check if a target needs rebuilding
    fn needs_rebuild(&self, rule_idx: usize) -> bool {
        let target = &self.rules[rule_idx].target;

        // Phony targets always need rebuilding
        if self.rules[rule_idx].phony {
            return true;
        }

        // If target doesn't exist, needs rebuild
        let target_mtime = match get_mtime(target) {
            Some(t) => t,
            None => return true,
        };

        // Check if any dependency is newer
        for i in 0..self.rules[rule_idx].num_deps {
            let dep = &self.rules[rule_idx].deps[i];
            if let Some(dep_mtime) = get_mtime(dep) {
                if dep_mtime > target_mtime {
                    return true;
                }
            }
        }

        false
    }

    /// Execute a command
    fn execute_command(&mut self, rule_idx: usize, cmd: &[u8]) -> bool {
        // Expand variables in command
        let mut expanded = [0u8; MAX_CMD];
        self.expand_vars(rule_idx, cmd, &mut expanded);

        let cmd_str = bytes_to_str(&expanded);
        let cmd_len = cmd_str.len();

        if cmd_len == 0 {
            return true;
        }

        // Check for @ prefix (silent)
        let (silent, cmd_start) = if expanded[0] == b'@' {
            (true, 1)
        } else {
            (false, 0)
        };

        let effective_cmd = &expanded[cmd_start..];
        let effective_str = bytes_to_str(effective_cmd);

        if !silent {
            printlns(effective_str);
        }

        if self.dry_run {
            return true;
        }

        // Execute via shell
        // Build shell command: /initramfs/bin/esh -c "command"
        // For now, we'll use a simpler approach: just execute the command directly
        // This means commands need to be full paths or shell built-ins won't work
        let pid = fork();
        if pid == 0 {
            // Child - try to execute the first word as a program
            let cmd_str = bytes_to_str(effective_cmd);

            // For simple commands without shell features, exec directly
            // For now, exec the shell with the command
            // The shell will need to support -c flag for this to work properly
            exec(cmd_str);
            _exit(127);
        } else if pid > 0 {
            // Parent - wait for child
            let mut status = 0;
            waitpid(pid, &mut status, 0);

            if wifexited(status) {
                let exit_code = wexitstatus(status);
                if exit_code != 0 {
                    eprints("make: *** [");
                    prints(bytes_to_str(&self.rules[rule_idx].target));
                    eprintlns("] Error");
                    self.had_error = true;
                    return false;
                }
            } else {
                self.had_error = true;
                return false;
            }
        } else {
            eprintlns("make: fork failed");
            return false;
        }

        true
    }

    /// Expand variables in a string
    fn expand_vars(&self, rule_idx: usize, src: &[u8], dst: &mut [u8]) {
        let mut src_pos = 0;
        let mut dst_pos = 0;
        let src_len = src.iter().position(|&c| c == 0).unwrap_or(src.len());

        while src_pos < src_len && dst_pos < dst.len() - 1 {
            if src[src_pos] == b'$' && src_pos + 1 < src_len {
                src_pos += 1;

                match src[src_pos] {
                    b'@' => {
                        // Target name
                        let target = &self.rules[rule_idx].target;
                        let len = target.iter().position(|&c| c == 0).unwrap_or(target.len());
                        let copy_len = len.min(dst.len() - 1 - dst_pos);
                        dst[dst_pos..dst_pos + copy_len].copy_from_slice(&target[..copy_len]);
                        dst_pos += copy_len;
                        src_pos += 1;
                    }
                    b'<' => {
                        // First dependency
                        if self.rules[rule_idx].num_deps > 0 {
                            let dep = &self.rules[rule_idx].deps[0];
                            let len = dep.iter().position(|&c| c == 0).unwrap_or(dep.len());
                            let copy_len = len.min(dst.len() - 1 - dst_pos);
                            dst[dst_pos..dst_pos + copy_len].copy_from_slice(&dep[..copy_len]);
                            dst_pos += copy_len;
                        }
                        src_pos += 1;
                    }
                    b'^' => {
                        // All dependencies
                        for i in 0..self.rules[rule_idx].num_deps {
                            if i > 0 && dst_pos < dst.len() - 1 {
                                dst[dst_pos] = b' ';
                                dst_pos += 1;
                            }
                            let dep = &self.rules[rule_idx].deps[i];
                            let len = dep.iter().position(|&c| c == 0).unwrap_or(dep.len());
                            let copy_len = len.min(dst.len() - 1 - dst_pos);
                            dst[dst_pos..dst_pos + copy_len].copy_from_slice(&dep[..copy_len]);
                            dst_pos += copy_len;
                        }
                        src_pos += 1;
                    }
                    b'(' => {
                        // Variable reference $(NAME)
                        src_pos += 1;
                        let var_start = src_pos;
                        while src_pos < src_len && src[src_pos] != b')' {
                            src_pos += 1;
                        }
                        let var_name = &src[var_start..src_pos];
                        if src_pos < src_len && src[src_pos] == b')' {
                            src_pos += 1;
                        }

                        if let Some(value) = self.get_var(var_name) {
                            let copy_len = value.len().min(dst.len() - 1 - dst_pos);
                            dst[dst_pos..dst_pos + copy_len].copy_from_slice(&value[..copy_len]);
                            dst_pos += copy_len;
                        }
                    }
                    b'$' => {
                        // Literal $
                        dst[dst_pos] = b'$';
                        dst_pos += 1;
                        src_pos += 1;
                    }
                    _ => {
                        // Unknown, keep as-is
                        dst[dst_pos] = b'$';
                        dst_pos += 1;
                    }
                }
            } else {
                dst[dst_pos] = src[src_pos];
                dst_pos += 1;
                src_pos += 1;
            }
        }

        dst[dst_pos] = 0;
    }
}

/// Check if file exists
fn file_exists(name: &[u8]) -> bool {
    let mut st = Stat::zeroed();
    stat(bytes_to_str(name), &mut st) == 0
}

/// Get file modification time
fn get_mtime(name: &[u8]) -> Option<u64> {
    let mut st = Stat::zeroed();
    if stat(bytes_to_str(name), &mut st) == 0 {
        Some(st.mtime)
    } else {
        None
    }
}

/// Copy bytes with trimming
fn copy_trimmed(dst: &mut [u8], src: &[u8]) {
    let src_len = src.iter().position(|&c| c == 0).unwrap_or(src.len());

    // Find start (skip leading whitespace)
    let mut start = 0;
    while start < src_len && (src[start] == b' ' || src[start] == b'\t') {
        start += 1;
    }

    // Find end (skip trailing whitespace)
    let mut end = src_len;
    while end > start && (src[end - 1] == b' ' || src[end - 1] == b'\t' || src[end - 1] == b'\r') {
        end -= 1;
    }

    let len = (end - start).min(dst.len() - 1);
    dst[..len].copy_from_slice(&src[start..start + len]);
    dst[len] = 0;
}

/// Copy bytes
fn copy_bytes(dst: &mut [u8], src: &[u8]) {
    let len = src
        .iter()
        .position(|&c| c == 0 || c == b'\n' || c == b'\r')
        .unwrap_or(src.len())
        .min(dst.len() - 1);
    dst[..len].copy_from_slice(&src[..len]);
    dst[len] = 0;
}

/// Compare trimmed byte slices
fn bytes_eq_trimmed(a: &[u8], b: &[u8]) -> bool {
    let a_len = a.iter().position(|&c| c == 0).unwrap_or(a.len());
    let b_len = b.iter().position(|&c| c == 0).unwrap_or(b.len());

    // Trim a
    let mut a_start = 0;
    while a_start < a_len && (a[a_start] == b' ' || a[a_start] == b'\t') {
        a_start += 1;
    }
    let mut a_end = a_len;
    while a_end > a_start && (a[a_end - 1] == b' ' || a[a_end - 1] == b'\t') {
        a_end -= 1;
    }

    // Trim b
    let mut b_start = 0;
    while b_start < b_len && (b[b_start] == b' ' || b[b_start] == b'\t') {
        b_start += 1;
    }
    let mut b_end = b_len;
    while b_end > b_start && (b[b_end - 1] == b' ' || b[b_end - 1] == b'\t') {
        b_end -= 1;
    }

    let a_trimmed = &a[a_start..a_end];
    let b_trimmed = &b[b_start..b_end];

    if a_trimmed.len() != b_trimmed.len() {
        return false;
    }

    for i in 0..a_trimmed.len() {
        if a_trimmed[i] != b_trimmed[i] {
            return false;
        }
    }
    true
}

/// Convert byte slice to str
fn bytes_to_str(s: &[u8]) -> &str {
    let len = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    unsafe { core::str::from_utf8_unchecked(&s[..len]) }
}

// Global make instance
use core::cell::UnsafeCell;

struct MakeCell(UnsafeCell<core::mem::MaybeUninit<Make>>);
unsafe impl Sync for MakeCell {}

static MAKE: MakeCell = MakeCell(UnsafeCell::new(core::mem::MaybeUninit::uninit()));
static mut MAKE_INITIALIZED: bool = false;

fn make() -> &'static mut Make {
    unsafe {
        let ptr = (*MAKE.0.get()).as_mut_ptr();
        if !MAKE_INITIALIZED {
            ptr.write(Make::new());
            MAKE_INITIALIZED = true;
        }
        &mut *ptr
    }
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let m = make();

    // Parse arguments
    let mut makefile = "Makefile";
    let mut targets: [[u8; 128]; MAX_TARGETS] = [[0u8; 128]; MAX_TARGETS];
    let mut num_targets = 0;

    let mut i = 1;
    while i < argc as usize {
        let arg = get_arg(argv, i);

        if bytes_eq_trimmed(arg, b"-f") {
            i += 1;
            if i < argc as usize {
                makefile = bytes_to_str(get_arg(argv, i));
            }
        } else if bytes_eq_trimmed(arg, b"-n") {
            m.dry_run = true;
        } else if arg[0] != b'-' && num_targets < MAX_TARGETS {
            copy_bytes(&mut targets[num_targets], arg);
            num_targets += 1;
        }
        i += 1;
    }

    // Read Makefile
    let fd = open2(makefile, O_RDONLY);
    if fd < 0 {
        eprints("make: ");
        prints(makefile);
        eprintlns(": No such file or directory");
        return 2;
    }

    let mut data = [0u8; MAX_MAKEFILE];
    let n = syscall::sys_read(fd, &mut data);
    close(fd);

    if n < 0 {
        eprintlns("make: read error");
        return 2;
    }

    // Parse Makefile
    m.parse(&data);

    // Build targets
    if num_targets == 0 {
        // Build first target
        if m.num_rules > 0 {
            if !m.build(&m.rules[0].target.clone()) {
                return 2;
            }
        } else {
            eprintlns("make: *** No targets");
            return 2;
        }
    } else {
        for t in 0..num_targets {
            if !m.build(&targets[t]) {
                return 2;
            }
        }
    }

    if m.had_error { 2 } else { 0 }
}

/// Get argument at index
fn get_arg(argv: *const *const u8, idx: usize) -> &'static [u8] {
    unsafe {
        let ptr = *argv.add(idx);
        if ptr.is_null() {
            return b"";
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::slice::from_raw_parts(ptr, len)
    }
}
