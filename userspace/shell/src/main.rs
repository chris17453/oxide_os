//! EFFLUX Shell (esh)
//!
//! A simple shell for EFFLUX OS with:
//! - Command execution
//! - Builtin commands (cd, exit, echo, pwd)
//! - Basic I/O redirection
//! - Background jobs (&)

#![no_std]
#![no_main]

use efflux_libc::*;

/// Maximum command line length
const MAX_LINE: usize = 256;

/// Maximum number of arguments
const MAX_ARGS: usize = 32;

/// Main shell entry point
#[unsafe(no_mangle)]
fn main() -> i32 {
    // Print welcome message
    println("EFFLUX Shell (esh)");
    println("Type 'help' for available commands");
    println("");

    // Main shell loop
    let mut line = [0u8; MAX_LINE];

    loop {
        // Print prompt
        print("esh> ");

        // Read command line
        let len = getline(&mut line);
        if len == 0 {
            // EOF
            println("");
            break;
        }

        // Remove trailing newline
        if len > 0 && line[len - 1] == b'\n' {
            line[len - 1] = 0;
        }

        // Skip empty lines
        let cmd = trim(&line);
        if cmd.is_empty() {
            continue;
        }

        // Execute command
        execute_line(cmd);
    }

    0
}

/// Trim whitespace from string
fn trim(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();

    // Find start
    while start < s.len() && (s[start] == b' ' || s[start] == b'\t' || s[start] == 0) {
        start += 1;
    }

    // Find end (null terminator or actual content end)
    for i in start..s.len() {
        if s[i] == 0 {
            end = i;
            break;
        }
    }

    // Trim trailing whitespace
    while end > start && (s[end - 1] == b' ' || s[end - 1] == b'\t') {
        end -= 1;
    }

    &s[start..end]
}

/// Execute a command line
fn execute_line(line: &[u8]) {
    // Check for background execution
    let (line, background) = if line.last() == Some(&b'&') {
        (&line[..line.len() - 1], true)
    } else {
        (line, false)
    };

    let line = trim(line);
    if line.is_empty() {
        return;
    }

    // Parse into command and arguments
    let mut args = [[0u8; 64]; MAX_ARGS];
    let argc = parse_args(line, &mut args);

    if argc == 0 {
        return;
    }

    // Get command name
    let cmd = &args[0];

    // Check for builtin commands
    if is_builtin(cmd) {
        execute_builtin(&args, argc);
        return;
    }

    // External command - fork and exec
    let pid = fork();
    if pid == 0 {
        // Child process
        execute_external(&args, argc);
        _exit(127); // Command not found
    } else if pid > 0 {
        // Parent process
        if !background {
            let mut status = 0;
            waitpid(pid, &mut status, 0);
        } else {
            print("[");
            print_i64(pid as i64);
            println("]");
        }
    } else {
        eprintln("esh: fork failed");
    }
}

/// Parse command line into arguments
fn parse_args(line: &[u8], args: &mut [[u8; 64]; MAX_ARGS]) -> usize {
    let mut argc = 0;
    let mut i = 0;
    let mut in_arg = false;
    let mut arg_pos = 0;

    while i < line.len() && line[i] != 0 && argc < MAX_ARGS {
        let c = line[i];

        if c == b' ' || c == b'\t' {
            if in_arg {
                // End of argument
                args[argc][arg_pos] = 0;
                argc += 1;
                in_arg = false;
                arg_pos = 0;
            }
        } else {
            // Part of argument
            if !in_arg {
                in_arg = true;
                arg_pos = 0;
            }
            if arg_pos < 63 {
                args[argc][arg_pos] = c;
                arg_pos += 1;
            }
        }

        i += 1;
    }

    // Handle last argument
    if in_arg {
        args[argc][arg_pos] = 0;
        argc += 1;
    }

    argc
}

/// Check if command is a builtin
fn is_builtin(cmd: &[u8]) -> bool {
    bytes_eq(cmd, b"cd") ||
    bytes_eq(cmd, b"exit") ||
    bytes_eq(cmd, b"echo") ||
    bytes_eq(cmd, b"pwd") ||
    bytes_eq(cmd, b"help") ||
    bytes_eq(cmd, b"export") ||
    bytes_eq(cmd, b"true") ||
    bytes_eq(cmd, b"false")
}

/// Execute a builtin command
fn execute_builtin(args: &[[u8; 64]; MAX_ARGS], argc: usize) {
    let cmd = &args[0];

    if bytes_eq(cmd, b"exit") {
        let code = if argc > 1 {
            parse_int_bytes(&args[1]).unwrap_or(0) as i32
        } else {
            0
        };
        _exit(code);
    } else if bytes_eq(cmd, b"echo") {
        for i in 1..argc {
            if i > 1 {
                print(" ");
            }
            print_bytes(&args[i]);
        }
        println("");
    } else if bytes_eq(cmd, b"cd") {
        if argc < 2 {
            eprintln("esh: cd: missing argument");
        } else {
            // Note: chdir syscall not implemented yet
            eprintln("esh: cd: not implemented");
        }
    } else if bytes_eq(cmd, b"pwd") {
        // Note: getcwd syscall not implemented yet
        eprintln("esh: pwd: not implemented");
    } else if bytes_eq(cmd, b"help") {
        println("EFFLUX Shell (esh) - Built-in commands:");
        println("  echo [args...]  - Print arguments");
        println("  cd <dir>        - Change directory");
        println("  pwd             - Print working directory");
        println("  exit [code]     - Exit shell");
        println("  help            - Show this help");
        println("");
        println("External commands are searched in /bin");
    } else if bytes_eq(cmd, b"true") {
        // Do nothing, exit 0
    } else if bytes_eq(cmd, b"false") {
        // Note: this won't affect exit code in builtins
    } else if bytes_eq(cmd, b"export") {
        eprintln("esh: export: not implemented");
    }
}

/// Execute an external command
fn execute_external(args: &[[u8; 64]; MAX_ARGS], _argc: usize) {
    let cmd = &args[0];

    // Try direct path first if it starts with /
    if cmd[0] == b'/' {
        let path = bytes_to_str(cmd);
        let ret = exec(path);
        if ret < 0 {
            print("esh: ");
            print_bytes(cmd);
            eprintln(": not found");
        }
        return;
    }

    // Search in /bin
    let mut path = [0u8; 128];
    path[..5].copy_from_slice(b"/bin/");
    let mut i = 0;
    while i < 63 && cmd[i] != 0 {
        path[5 + i] = cmd[i];
        i += 1;
    }
    path[5 + i] = 0;

    let path_str = bytes_to_str(&path);
    let ret = exec(path_str);
    if ret < 0 {
        print("esh: ");
        print_bytes(cmd);
        eprintln(": command not found");
    }
}

/// Compare byte slices for equality
fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    let mut i = 0;
    loop {
        let a_end = i >= a.len() || a[i] == 0;
        let b_end = i >= b.len() || b[i] == 0;

        if a_end && b_end {
            return true;
        }
        if a_end || b_end {
            return false;
        }
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
}

/// Print bytes until null terminator
fn print_bytes(s: &[u8]) {
    let mut i = 0;
    while i < s.len() && s[i] != 0 {
        putchar(s[i]);
        i += 1;
    }
}

/// Parse integer from bytes
fn parse_int_bytes(s: &[u8]) -> Option<i64> {
    let mut i = 0;
    let negative = if i < s.len() && s[i] == b'-' {
        i += 1;
        true
    } else {
        false
    };

    let mut result: i64 = 0;
    while i < s.len() && s[i] != 0 {
        let c = s[i];
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as i64;
        i += 1;
    }

    Some(if negative { -result } else { result })
}

/// Convert bytes to str (assuming valid UTF-8 ASCII subset)
fn bytes_to_str(bytes: &[u8]) -> &str {
    let mut len = 0;
    while len < bytes.len() && bytes[len] != 0 {
        len += 1;
    }
    unsafe { core::str::from_utf8_unchecked(&bytes[..len]) }
}
