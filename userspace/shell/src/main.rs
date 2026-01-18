//! EFFLUX Shell (esh)
//!
//! A simple shell for EFFLUX OS with:
//! - Command execution
//! - Builtin commands (cd, exit, echo, pwd, export)
//! - I/O redirection (<, >, >>)
//! - Pipes (|)
//! - Background jobs (&)

#![no_std]
#![no_main]

use efflux_libc::*;

/// Maximum command line length
const MAX_LINE: usize = 256;

/// Maximum number of arguments
const MAX_ARGS: usize = 32;

/// Maximum number of pipe stages
const MAX_PIPES: usize = 8;

/// Redirection types
#[derive(Clone, Copy)]
enum Redirect {
    None,
    /// Input redirection (<)
    Input,
    /// Output redirection (>)
    Output,
    /// Append output redirection (>>)
    Append,
}

/// A command stage (for pipes)
struct Command {
    /// Arguments
    args: [[u8; 64]; MAX_ARGS],
    /// Argument count
    argc: usize,
    /// Input redirection file
    input_file: [u8; 64],
    /// Output redirection file
    output_file: [u8; 64],
    /// Input redirect type
    input_redir: Redirect,
    /// Output redirect type
    output_redir: Redirect,
}

impl Command {
    fn new() -> Self {
        Command {
            args: [[0u8; 64]; MAX_ARGS],
            argc: 0,
            input_file: [0u8; 64],
            output_file: [0u8; 64],
            input_redir: Redirect::None,
            output_redir: Redirect::None,
        }
    }
}

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

/// Execute a command line (may contain pipes)
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

    // Split by pipes
    let mut commands: [Command; MAX_PIPES] = core::array::from_fn(|_| Command::new());
    let num_commands = split_pipes(line, &mut commands);

    if num_commands == 0 {
        return;
    }

    // Single command without pipes - check for builtin
    if num_commands == 1 && is_builtin(&commands[0].args[0]) {
        execute_builtin(&commands[0]);
        return;
    }

    // Execute pipeline
    execute_pipeline(&commands, num_commands, background);
}

/// Split line by pipes and parse redirections
fn split_pipes(line: &[u8], commands: &mut [Command; MAX_PIPES]) -> usize {
    let mut num_commands = 0;
    let mut start = 0;

    for i in 0..line.len() {
        if line[i] == 0 {
            break;
        }
        if line[i] == b'|' {
            if num_commands >= MAX_PIPES {
                eprintln("esh: too many pipes");
                return 0;
            }
            parse_command(&line[start..i], &mut commands[num_commands]);
            num_commands += 1;
            start = i + 1;
        }
    }

    // Parse last command
    if num_commands < MAX_PIPES {
        let end = line.iter().position(|&c| c == 0).unwrap_or(line.len());
        parse_command(&line[start..end], &mut commands[num_commands]);
        num_commands += 1;
    }

    num_commands
}

/// Parse a single command with redirections
fn parse_command(line: &[u8], cmd: &mut Command) {
    let line = trim(line);
    let mut i = 0;
    cmd.argc = 0;

    while i < line.len() && line[i] != 0 {
        // Skip whitespace
        while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
            i += 1;
        }
        if i >= line.len() || line[i] == 0 {
            break;
        }

        // Check for redirections
        if line[i] == b'<' {
            cmd.input_redir = Redirect::Input;
            i += 1;
            // Skip whitespace
            while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
                i += 1;
            }
            // Read filename
            let mut j = 0;
            while i < line.len() && line[i] != 0 && line[i] != b' ' &&
                  line[i] != b'\t' && line[i] != b'<' && line[i] != b'>' && j < 63 {
                cmd.input_file[j] = line[i];
                j += 1;
                i += 1;
            }
            cmd.input_file[j] = 0;
        } else if line[i] == b'>' {
            i += 1;
            if i < line.len() && line[i] == b'>' {
                cmd.output_redir = Redirect::Append;
                i += 1;
            } else {
                cmd.output_redir = Redirect::Output;
            }
            // Skip whitespace
            while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
                i += 1;
            }
            // Read filename
            let mut j = 0;
            while i < line.len() && line[i] != 0 && line[i] != b' ' &&
                  line[i] != b'\t' && line[i] != b'<' && line[i] != b'>' && j < 63 {
                cmd.output_file[j] = line[i];
                j += 1;
                i += 1;
            }
            cmd.output_file[j] = 0;
        } else {
            // Regular argument
            if cmd.argc >= MAX_ARGS {
                break;
            }
            let mut j = 0;
            while i < line.len() && line[i] != 0 && line[i] != b' ' &&
                  line[i] != b'\t' && line[i] != b'<' && line[i] != b'>' &&
                  line[i] != b'|' && j < 63 {
                cmd.args[cmd.argc][j] = line[i];
                j += 1;
                i += 1;
            }
            if j > 0 {
                cmd.args[cmd.argc][j] = 0;
                cmd.argc += 1;
            }
        }
    }
}

/// Execute a pipeline of commands
fn execute_pipeline(commands: &[Command; MAX_PIPES], num_commands: usize, background: bool) {
    // Create pipes
    let mut pipes: [[i32; 2]; MAX_PIPES] = [[0; 2]; MAX_PIPES];
    for i in 0..(num_commands - 1) {
        if pipe(&mut pipes[i]) < 0 {
            eprintln("esh: pipe failed");
            return;
        }
    }

    let mut pids = [0i32; MAX_PIPES];

    for i in 0..num_commands {
        let pid = fork();
        if pid == 0 {
            // Child process

            // Setup input
            if i > 0 {
                // Read from previous pipe
                dup2(pipes[i - 1][0], 0);
            } else if let Redirect::Input = commands[i].input_redir {
                // Redirect from file
                let path = bytes_to_str(&commands[i].input_file);
                let fd = open2(path, O_RDONLY);
                if fd < 0 {
                    eprint("esh: ");
                    print_bytes(&commands[i].input_file);
                    eprintln(": No such file");
                    _exit(1);
                }
                dup2(fd, 0);
                close(fd);
            }

            // Setup output
            if i < num_commands - 1 {
                // Write to next pipe
                dup2(pipes[i][1], 1);
            } else {
                match commands[i].output_redir {
                    Redirect::Output => {
                        let path = bytes_to_str(&commands[i].output_file);
                        let fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
                        if fd < 0 {
                            eprint("esh: ");
                            print_bytes(&commands[i].output_file);
                            eprintln(": Cannot create file");
                            _exit(1);
                        }
                        dup2(fd, 1);
                        close(fd);
                    }
                    Redirect::Append => {
                        let path = bytes_to_str(&commands[i].output_file);
                        let fd = open(path, O_WRONLY | O_CREAT | O_APPEND, 0o644);
                        if fd < 0 {
                            eprint("esh: ");
                            print_bytes(&commands[i].output_file);
                            eprintln(": Cannot create file");
                            _exit(1);
                        }
                        dup2(fd, 1);
                        close(fd);
                    }
                    Redirect::None | Redirect::Input => {}
                }
            }

            // Close all pipe fds in child
            for j in 0..(num_commands - 1) {
                close(pipes[j][0]);
                close(pipes[j][1]);
            }

            // Execute command
            execute_external(&commands[i]);
            _exit(127);
        } else if pid > 0 {
            pids[i] = pid;
        } else {
            eprintln("esh: fork failed");
        }
    }

    // Parent: close all pipe fds
    for i in 0..(num_commands - 1) {
        close(pipes[i][0]);
        close(pipes[i][1]);
    }

    // Wait for all children
    if !background {
        for i in 0..num_commands {
            let mut status = 0;
            waitpid(pids[i], &mut status, 0);
        }
    } else {
        print("[");
        print_i64(pids[num_commands - 1] as i64);
        println("]");
    }
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
fn execute_builtin(cmd: &Command) {
    if bytes_eq(&cmd.args[0], b"exit") {
        let code = if cmd.argc > 1 {
            parse_int_bytes(&cmd.args[1]).unwrap_or(0) as i32
        } else {
            0
        };
        _exit(code);
    } else if bytes_eq(&cmd.args[0], b"echo") {
        for i in 1..cmd.argc {
            if i > 1 {
                print(" ");
            }
            print_bytes(&cmd.args[i]);
        }
        println("");
    } else if bytes_eq(&cmd.args[0], b"cd") {
        if cmd.argc < 2 {
            eprintln("esh: cd: missing argument");
        } else {
            let path = bytes_to_str(&cmd.args[1]);
            if chdir(path) < 0 {
                eprint("esh: cd: ");
                print_bytes(&cmd.args[1]);
                eprintln(": No such directory");
            }
        }
    } else if bytes_eq(&cmd.args[0], b"pwd") {
        let mut buf = [0u8; 256];
        if getcwd(&mut buf) >= 0 {
            print_bytes(&buf);
            println("");
        } else {
            eprintln("esh: pwd: failed");
        }
    } else if bytes_eq(&cmd.args[0], b"help") {
        println("EFFLUX Shell (esh) - Built-in commands:");
        println("  echo [args...]  - Print arguments");
        println("  cd <dir>        - Change directory");
        println("  pwd             - Print working directory");
        println("  export VAR=val  - Set environment variable");
        println("  exit [code]     - Exit shell");
        println("  help            - Show this help");
        println("");
        println("I/O Redirection:");
        println("  cmd < file      - Read input from file");
        println("  cmd > file      - Write output to file");
        println("  cmd >> file     - Append output to file");
        println("");
        println("Pipes:");
        println("  cmd1 | cmd2     - Pipe output of cmd1 to cmd2");
        println("");
        println("Background:");
        println("  cmd &           - Run command in background");
    } else if bytes_eq(&cmd.args[0], b"true") {
        // Do nothing, exit 0
    } else if bytes_eq(&cmd.args[0], b"false") {
        // Note: this won't affect exit code in builtins
    } else if bytes_eq(&cmd.args[0], b"export") {
        if cmd.argc < 2 {
            eprintln("esh: export: usage: export VAR=value");
        } else {
            // Parse VAR=value
            let arg = &cmd.args[1];
            let mut eq_pos = None;
            for i in 0..64 {
                if arg[i] == 0 {
                    break;
                }
                if arg[i] == b'=' {
                    eq_pos = Some(i);
                    break;
                }
            }

            if let Some(pos) = eq_pos {
                let mut name = [0u8; 64];
                let mut value = [0u8; 64];
                name[..pos].copy_from_slice(&arg[..pos]);
                let mut i = pos + 1;
                let mut j = 0;
                while i < 64 && arg[i] != 0 && j < 63 {
                    value[j] = arg[i];
                    i += 1;
                    j += 1;
                }

                let name_str = bytes_to_str(&name);
                let value_str = bytes_to_str(&value);
                if setenv(name_str, value_str) < 0 {
                    eprintln("esh: export: failed");
                }
            } else {
                eprintln("esh: export: usage: export VAR=value");
            }
        }
    }
}

/// Execute an external command
fn execute_external(cmd: &Command) {
    let arg = &cmd.args[0];

    // Try direct path first if it starts with /
    if arg[0] == b'/' {
        let path = bytes_to_str(arg);
        let ret = exec(path);
        if ret < 0 {
            eprint("esh: ");
            print_bytes(arg);
            eprintln(": not found");
        }
        return;
    }

    // Search in /bin
    let mut path = [0u8; 128];
    path[..5].copy_from_slice(b"/bin/");
    let mut i = 0;
    while i < 63 && arg[i] != 0 {
        path[5 + i] = arg[i];
        i += 1;
    }
    path[5 + i] = 0;

    let path_str = bytes_to_str(&path);
    let ret = exec(path_str);
    if ret < 0 {
        eprint("esh: ");
        print_bytes(arg);
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
