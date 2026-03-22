//! Shell Builtins — commands that MUST run in the shell process
//!
//! — ByteRiot: the sacred few. Only commands that need to modify the
//! shell's own process state live here. Everything else is an external
//! binary in /bin or /usr/bin. If it doesn't touch the shell's memory,
//! environment, working directory, or file descriptors — it doesn't
//! belong here.
//!
//! Builtins:
//!   cd       — change working directory
//!   exit     — terminate shell
//!   export   — set environment variable
//!   unset    — remove environment variable
//!   source/. — execute script in current shell
//!   exec     — replace shell with command
//!   eval     — evaluate string as command
//!   set      — set shell options / positional params
//!   shift    — shift positional parameters
//!   alias    — define command alias
//!   unalias  — remove alias
//!   umask    — set file creation mask
//!   read     — read line from stdin into variable
//!   true     — exit 0
//!   false    — exit 1
//!   :        — no-op, exit 0
//!   [ / test — conditional expression
//!   builtin  — force builtin execution
//!   command  — force external execution
//!   type     — show command type

extern crate alloc;
use alloc::vec::Vec;
use libc::*;
use libc::dirent::{opendir, closedir, readdir};

use crate::ast::Redirect;
use crate::eval::Evaluator;

/// Try to execute a command as a builtin. Returns Some(exit_status) if
/// the command is a builtin, None if it should be handled externally.
/// — ByteRiot: the gatekeeper. Checks the name, runs it in-process if
/// it's one of ours, or waves it through to fork+exec.
pub fn try_exec_builtin(argv: &[Vec<u8>], redirs: &[Redirect], eval: &mut Evaluator) -> Option<i32> {
    if argv.is_empty() { return Some(0); }

    let cmd = &argv[0];

    // — ByteRiot: fast dispatch table. No hash map needed — the builtin
    // count is small enough that a match chain is faster.
    // — ByteRiot: fast dispatch table. No hash map needed — the builtin
    // count is small enough that a match chain is faster than hashing.
    let is_builtin = match cmd.as_slice() {
        b"cd" | b"exit" | b"export" | b"unset" | b"source" | b"." |
        b"exec" | b"eval" | b"set" | b"shift" | b"alias" | b"unalias" |
        b"umask" | b"read" | b"true" | b"false" | b":" | b"[" | b"test" |
        b"builtin" | b"command" | b"type" | b"echo" | b"pwd" |
        b"jobs" | b"fg" | b"bg" | b"wait" | b"kill" |
        b"history" | b"theme" | b"colors" | b"help" |
        b"local" | b"declare" | b"readonly" | b"let" | b"getopts" |
        b"printf" | b"complete" | b"compgen" | b"mapfile" | b"readarray" | b"shopt" |
        b"return" | b"break" | b"continue" | b"trap" => true,
        _ => false,
    };

    if !is_builtin { return None; }

    // Apply redirections (save/restore fds)
    let saved = apply_redirections(redirs);

    let status = exec_builtin(argv, eval);

    // Restore fds
    restore_redirections(&saved);

    Some(status)
}

/// File descriptor save entry
struct SavedFd {
    original_fd: i32,
    backup_fd: i32,
}

/// Apply redirections, saving original fds for restore
fn apply_redirections(redirs: &[Redirect]) -> Vec<SavedFd> {
    let mut saved = Vec::new();

    for redir in redirs {
        let target = bytes_to_str(&redir.target);
        match redir.rtype {
            crate::ast::RedirectType::Input => {
                let fd = open2(target, O_RDONLY);
                if fd >= 0 {
                    let backup = dup(redir.fd);
                    if backup >= 0 {
                        saved.push(SavedFd { original_fd: redir.fd, backup_fd: backup });
                    }
                    dup2(fd, redir.fd);
                    close(fd);
                }
            }
            crate::ast::RedirectType::Output => {
                let fd = open(target, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
                if fd >= 0 {
                    let backup = dup(redir.fd);
                    if backup >= 0 {
                        saved.push(SavedFd { original_fd: redir.fd, backup_fd: backup });
                    }
                    dup2(fd, redir.fd);
                    close(fd);
                }
            }
            crate::ast::RedirectType::Append => {
                let fd = open(target, O_WRONLY | O_CREAT | O_APPEND, 0o644);
                if fd >= 0 {
                    let backup = dup(redir.fd);
                    if backup >= 0 {
                        saved.push(SavedFd { original_fd: redir.fd, backup_fd: backup });
                    }
                    dup2(fd, redir.fd);
                    close(fd);
                }
            }
            crate::ast::RedirectType::DupOut | crate::ast::RedirectType::DupIn => {
                if let Some(&b) = redir.target.first() {
                    let target_fd = (b - b'0') as i32;
                    let backup = dup(redir.fd);
                    if backup >= 0 {
                        saved.push(SavedFd { original_fd: redir.fd, backup_fd: backup });
                    }
                    dup2(target_fd, redir.fd);
                }
            }
            // — ByteRiot: heredoc/herestring redirections for builtins — pipe body to stdin
            crate::ast::RedirectType::HereDoc | crate::ast::RedirectType::HereDocStrip => {
                let mut pipefd = [0i32; 2];
                if pipe(&mut pipefd) == 0 {
                    let backup = dup(redir.fd);
                    if backup >= 0 {
                        saved.push(SavedFd { original_fd: redir.fd, backup_fd: backup });
                    }
                    let body = &redir.target;
                    let _ = libc::write(pipefd[1], body);
                    close(pipefd[1]);
                    dup2(pipefd[0], redir.fd);
                    close(pipefd[0]);
                }
            }
            crate::ast::RedirectType::HereString => {
                let mut pipefd = [0i32; 2];
                if pipe(&mut pipefd) == 0 {
                    let backup = dup(redir.fd);
                    if backup >= 0 {
                        saved.push(SavedFd { original_fd: redir.fd, backup_fd: backup });
                    }
                    let _ = libc::write(pipefd[1], &redir.target);
                    let _ = libc::write(pipefd[1], b"\n");
                    close(pipefd[1]);
                    dup2(pipefd[0], redir.fd);
                    close(pipefd[0]);
                }
            }
        }
    }

    saved
}

/// Restore saved file descriptors
fn restore_redirections(saved: &[SavedFd]) {
    for s in saved.iter().rev() {
        dup2(s.backup_fd, s.original_fd);
        close(s.backup_fd);
    }
}

/// Execute a builtin command, return exit status
fn exec_builtin(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    let cmd = &argv[0];

    match cmd.as_slice() {
        b"true" | b":" => 0,
        b"false" => 1,
        b"exit" => {
            // — ByteRiot: fire the EXIT trap before we flatline.
            // Last chance to run cleanup code. Make it count.
            eval.fire_exit_trap();
            let code = if argv.len() > 1 {
                parse_int(&argv[1]).unwrap_or(0) as i32
            } else {
                0
            };
            _exit(code);
        }
        b"cd" => builtin_cd(argv),
        b"pwd" => { print_pwd(); 0 }
        b"echo" => builtin_echo(argv),
        b"export" => builtin_export(argv),
        b"unset" => builtin_unset(argv, eval),
        b"source" | b"." => builtin_source(argv, eval),
        b"exec" => builtin_exec(argv),
        b"eval" => builtin_eval(argv, eval),
        b"set" => builtin_set(argv, eval),
        b"shift" => builtin_shift(argv, eval),
        b"read" => builtin_read(argv),
        b"test" | b"[" => builtin_test(argv),
        b"type" => builtin_type(argv, eval),
        b"command" => builtin_command(argv, eval),
        b"builtin" => builtin_builtin(argv, eval),
        b"umask" => builtin_umask(argv),
        b"alias" => builtin_alias(argv),
        b"unalias" => builtin_unalias(argv),
        b"jobs" => builtin_jobs(eval),
        b"fg" => builtin_fg(argv, eval),
        b"bg" => builtin_bg(argv, eval),
        b"wait" => builtin_wait(argv),
        b"kill" => builtin_kill(argv),
        b"history" => builtin_history(argv),
        b"help" => { print_help(); 0 }
        b"theme" | b"colors" => { /* — ByteRiot: theme/colors handled by main.rs */ 0 }
        b"printf" => builtin_printf(argv),
        b"local" => builtin_local(argv, eval),
        b"declare" | b"readonly" => builtin_declare(argv, eval),
        b"let" => builtin_let(argv),
        b"getopts" => builtin_getopts(argv),
        b"return" => builtin_return(argv, eval),
        b"break" => builtin_break(argv, eval),
        b"continue" => builtin_continue(argv, eval),
        b"trap" => builtin_trap(argv, eval),
        b"complete" => builtin_complete(argv),
        b"compgen" => builtin_compgen(argv),
        b"mapfile" | b"readarray" => builtin_mapfile(argv, eval),
        b"shopt" => builtin_shopt(argv, eval),
        _ => { eprintlns("esh: unknown builtin"); 1 }
    }
}

/// cd — change directory
/// — ByteRiot: the one builtin everyone understands. Can't be external
/// because chdir() only affects the calling process.
fn builtin_cd(argv: &[Vec<u8>]) -> i32 {
    let target = if argv.len() > 1 {
        let arg = &argv[1];
        if arg == b"-" {
            // cd - → go to OLDPWD
            match getenv("OLDPWD") {
                Some(old) => old,
                None => { eprintlns("esh: cd: OLDPWD not set"); return 1; }
            }
        } else if arg.first() == Some(&b'~') {
            // Tilde expansion
            let home = getenv("HOME").unwrap_or("/");
            if arg.len() == 1 {
                home
            } else {
                // ~/ prefix — need to build path
                // Use static buffer since we need &str
                static mut CD_BUF: [u8; 256] = [0; 256];
                let buf = unsafe { &mut *core::ptr::addr_of_mut!(CD_BUF) };
                let hb = home.as_bytes();
                let mut o = 0;
                for &b in hb { if o < 255 { buf[o] = b; o += 1; } }
                for &b in &arg[1..] { if o < 255 && b != 0 { buf[o] = b; o += 1; } }
                buf[o] = 0;
                unsafe { core::str::from_utf8_unchecked(&buf[..o]) }
            }
        } else {
            bytes_to_str(arg)
        }
    } else {
        // cd with no args → HOME
        match getenv("HOME") {
            Some(home) => home,
            None => { eprintlns("esh: cd: HOME not set"); return 1; }
        }
    };

    // Save current dir as OLDPWD
    let mut cwd_buf = [0u8; 128];
    if getcwd(&mut cwd_buf) >= 0 {
        let cwd = bytes_to_str(&cwd_buf);
        setenv("OLDPWD", cwd);
    }

    if chdir(target) < 0 {
        eprints("esh: cd: ");
        prints(target);
        eprintlns(": No such file or directory");
        return 1;
    }

    // Update PWD
    if getcwd(&mut cwd_buf) >= 0 {
        let cwd = bytes_to_str(&cwd_buf);
        setenv("PWD", cwd);
    }

    0
}

/// pwd — print working directory
fn print_pwd() {
    let mut buf = [0u8; 256];
    if getcwd(&mut buf) >= 0 {
        printlns(bytes_to_str(&buf));
    }
}

/// echo — print arguments
/// — ByteRiot: technically could be external, but every shell since
/// the Bourne shell has it as a builtin for performance. Millions of
/// scripts depend on echo being instant.
fn builtin_echo(argv: &[Vec<u8>]) -> i32 {
    let mut newline = true;
    let mut start = 1;

    // Handle -n flag
    if argv.len() > 1 && argv[1] == b"-n" {
        newline = false;
        start = 2;
    }

    for i in start..argv.len() {
        if i > start { prints(" "); }
        print_bytes(&argv[i]);
    }

    if newline { prints("\n"); }
    fflush_stdout();
    0
}

/// export — set environment variable
fn builtin_export(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 {
        // List all exported vars — simplified
        printlns("esh: export: listing not implemented");
        return 0;
    }

    for i in 1..argv.len() {
        let arg = &argv[i];
        // Find = sign
        if let Some(eq_pos) = arg.iter().position(|&b| b == b'=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            if let (Ok(n), Ok(v)) = (core::str::from_utf8(name), core::str::from_utf8(value)) {
                setenv(n, v);
            }
        } else {
            // export VAR (without =) — mark for export (no-op in our impl,
            // all vars are exported)
        }
    }
    0
}

/// unset — remove environment variable or array element
/// — IronGhost: now handles `unset arr[n]` for removing array elements
/// and `unset arr` for removing the whole array.
fn builtin_unset(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    for i in 1..argv.len() {
        let arg = &argv[i];
        // Check for arr[n] syntax
        if let Some(bracket_pos) = arg.iter().position(|&b| b == b'[') {
            if arg.ends_with(b"]") {
                let arr_name = &arg[..bracket_pos];
                let idx_str = &arg[bracket_pos + 1..arg.len() - 1];
                let idx = parse_int_from_bytes(idx_str) as usize;
                eval.unset_array_element(arr_name, idx);
                continue;
            }
        }
        // Regular variable unset + array unset
        if let Ok(name) = core::str::from_utf8(arg) {
            libc::unsetenv(name);
        }
        eval.unset_array(arg);
    }
    0
}

/// Parse int from raw bytes (no null termination concerns)
fn parse_int_from_bytes(s: &[u8]) -> i64 {
    let mut i = 0;
    while i < s.len() && s[i] == b' ' { i += 1; }
    let neg = if i < s.len() && s[i] == b'-' { i += 1; true } else { false };
    let mut result: i64 = 0;
    while i < s.len() && s[i] >= b'0' && s[i] <= b'9' {
        result = result * 10 + (s[i] - b'0') as i64;
        i += 1;
    }
    if neg { -result } else { result }
}

/// source / . — execute file in current shell context
/// — ByteRiot: now supports positional params. `source script.sh arg1 arg2`
/// sets $1, $2 in the sourced script, then restores the caller's params.
/// Public alias for main.rs to call directly.
pub fn source_file(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    builtin_source(argv, eval)
}

fn builtin_source(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if argv.len() < 2 {
        eprintlns("esh: source: filename argument required");
        return 1;
    }

    let path = bytes_to_str(&argv[1]);
    let fd = open2(path, O_RDONLY);
    if fd < 0 {
        eprints("esh: source: ");
        print_bytes(&argv[1]);
        eprintlns(": No such file");
        return 1;
    }

    // Read file into buffer
    let mut content = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 { break; }
        content.extend_from_slice(&buf[..n as usize]);
    }
    close(fd);

    // — ByteRiot: save and set positional params if extra args provided
    let saved_positional = if argv.len() > 2 {
        let saved = core::mem::take(&mut eval.positional);
        eval.positional = argv[2..].to_vec();
        Some(saved)
    } else {
        None
    };

    // Parse and evaluate
    let tokens = crate::token::tokenize(&content);
    match crate::parser::parse(tokens) {
        Ok(prog) => eval.eval_program(&prog),
        Err(e) => {
            eprints("esh: source: parse error: ");
            eprintlns(e.message);
        }
    }

    // — ByteRiot: restore positional params
    if let Some(saved) = saved_positional {
        eval.positional = saved;
    }

    eval.last_status
}

/// exec — replace shell with command
fn builtin_exec(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 { return 0; }

    let mut c_argv: Vec<*const u8> = Vec::new();
    let mut owned: Vec<Vec<u8>> = Vec::new();
    for arg in &argv[1..] {
        let mut a = arg.clone();
        if a.last() != Some(&0) { a.push(0); }
        owned.push(a);
    }
    for arg in &owned {
        c_argv.push(arg.as_ptr());
    }
    c_argv.push(core::ptr::null());

    let cmd = bytes_to_str(&argv[1]);
    if cmd.starts_with('/') || cmd.starts_with('.') {
        execv(cmd, c_argv.as_ptr());
    } else {
        // PATH search
        let path_env = getenv("PATH").unwrap_or("/bin:/usr/bin");
        for dir in path_env.split(':') {
            if dir.is_empty() { continue; }
            let mut full = Vec::new();
            full.extend_from_slice(dir.as_bytes());
            if full.last() != Some(&b'/') { full.push(b'/'); }
            full.extend_from_slice(&argv[1]);
            full.push(0);
            c_argv[0] = full.as_ptr();
            execv(bytes_to_str(&full), c_argv.as_ptr());
        }
    }

    eprints("esh: exec: ");
    print_bytes(&argv[1]);
    eprintlns(": not found");
    1
}

/// eval — evaluate string as shell command
fn builtin_eval(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if argv.len() < 2 { return 0; }

    // Concatenate all args with spaces
    let mut cmd = Vec::new();
    for i in 1..argv.len() {
        if i > 1 { cmd.push(b' '); }
        cmd.extend_from_slice(&argv[i]);
    }

    let tokens = crate::token::tokenize(&cmd);
    match crate::parser::parse(tokens) {
        Ok(prog) => {
            eval.eval_program(&prog);
            eval.last_status
        }
        Err(e) => {
            eprints("esh: eval: ");
            eprintlns(e.message);
            1
        }
    }
}

/// set — set shell options or positional parameters
/// — ByteRiot: the configurator. -e makes you live dangerously (exit on error),
/// -x makes you transparent (print commands), -u makes you paranoid (unset = error),
/// -o pipefail makes pipelines honest (any stage can fail the whole thing).
fn builtin_set(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if argv.len() < 2 {
        // Print all variables — simplified
        return 0;
    }

    let mut i = 1;
    while i < argv.len() {
        let arg = &argv[i];
        if arg == b"--" {
            // set -- arg1 arg2 ... → set positional parameters
            eval.positional.clear();
            for j in (i + 1)..argv.len() {
                eval.positional.push(argv[j].clone());
            }
            return 0;
        }

        if arg.len() >= 2 && (arg[0] == b'-' || arg[0] == b'+') {
            let enable = arg[0] == b'-';
            if arg[1] == b'o' {
                // -o option / +o option
                i += 1;
                if i < argv.len() {
                    match argv[i].as_slice() {
                        b"pipefail" => eval.opts.pipefail = enable,
                        b"errexit" => eval.opts.errexit = enable,
                        b"xtrace" => eval.opts.xtrace = enable,
                        b"nounset" => eval.opts.nounset = enable,
                        _ => {
                            eprints("esh: set: unknown option: ");
                            print_bytes(&argv[i]);
                            eprintlns("");
                        }
                    }
                }
            } else {
                // Parse individual flags: -e, -x, -u, -exu, etc.
                for j in 1..arg.len() {
                    match arg[j] {
                        b'e' => eval.opts.errexit = enable,
                        b'x' => eval.opts.xtrace = enable,
                        b'u' => eval.opts.nounset = enable,
                        _ => {}
                    }
                }
            }
        }
        i += 1;
    }
    0
}

/// shift — shift positional parameters
fn builtin_shift(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    let n = if argv.len() > 1 {
        parse_int(&argv[1]).unwrap_or(1) as usize
    } else {
        1
    };

    if n > eval.positional.len() {
        eprintlns("esh: shift: shift count out of range");
        return 1;
    }

    eval.positional = eval.positional[n..].to_vec();
    0
}

/// read — read line from stdin into variable
fn builtin_read(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 {
        eprintlns("esh: read: variable name required");
        return 1;
    }

    let mut line = [0u8; 1024];
    let mut pos = 0;
    loop {
        let mut ch = [0u8; 1];
        let n = read(0, &mut ch);
        if n <= 0 || ch[0] == b'\n' { break; }
        if pos < line.len() - 1 {
            line[pos] = ch[0];
            pos += 1;
        }
    }
    line[pos] = 0;

    let var_name = bytes_to_str(&argv[1]);
    let value = bytes_to_str(&line[..pos]);
    setenv(var_name, value);
    0
}

/// test / [ — conditional expression
/// — ByteRiot: the POSIX test command. Handles -f, -d, -e, -z, -n,
/// string comparisons, and integer comparisons.
fn builtin_test(argv: &[Vec<u8>]) -> i32 {
    let is_bracket = argv[0] == b"[";
    let args = if is_bracket {
        // Strip [ and ]
        let end = argv.len();
        if end < 2 || argv[end - 1] != b"]" {
            eprintlns("esh: [: missing ]");
            return 2;
        }
        &argv[1..end - 1]
    } else {
        &argv[1..]
    };

    if args.is_empty() { return 1; } // empty test = false

    // Unary operators
    if args.len() == 2 {
        let op = &args[0];
        let arg = &args[1];
        let arg_str = bytes_to_str(arg);

        // — CrashBloom: full POSIX test unary operators. The old version
        // had -f, -d, -e and called it a day. Now we stat the file and
        // check mode bits like a real shell. — CrashBloom
        return match op.as_slice() {
            b"-z" => if arg.is_empty() || (arg.len() == 1 && arg[0] == 0) { 0 } else { 1 },
            b"-n" => if !arg.is_empty() && arg[0] != 0 { 0 } else { 1 },
            b"-f" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_file() { 0 } else { 1 } },
            b"-d" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_dir() { 0 } else { 1 } },
            b"-e" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 { 0 } else { 1 } },
            b"-s" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.size > 0 { 0 } else { 1 } },
            b"-r" | b"-w" | b"-x" => {
                // — CrashBloom: permission tests. Without proper access() we
                // just check file exists. Good enough for scripts that test -r
                // before reading. Better than returning false for everything.
                let mut st = libc::stat::Stat::zeroed();
                if libc::stat::stat(arg_str, &mut st) == 0 { 0 } else { 1 }
            },
            b"-L" | b"-h" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::lstat(arg_str, &mut st) == 0 && st.is_symlink() { 0 } else { 1 } },
            b"-p" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_fifo() { 0 } else { 1 } },
            b"-b" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_block_device() { 0 } else { 1 } },
            b"-c" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_char_device() { 0 } else { 1 } },
            b"-S" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_socket() { 0 } else { 1 } },
            b"!" => if builtin_test_inner(&args[1..]) == 0 { 1 } else { 0 },
            _ => 1,
        };
    }

    // Single arg — true if non-empty string
    if args.len() == 1 {
        return if args[0].is_empty() || args[0][0] == 0 { 1 } else { 0 };
    }

    // Binary operators
    if args.len() == 3 {
        return builtin_test_binary(&args[0], &args[1], &args[2]);
    }

    // Negation
    if args.len() >= 2 && args[0] == b"!" {
        let inner_result = builtin_test_inner(&args[1..]);
        return if inner_result == 0 { 1 } else { 0 };
    }

    1 // unknown expression = false
}

fn builtin_test_inner(args: &[Vec<u8>]) -> i32 {
    if args.is_empty() { return 1; }
    if args.len() == 1 {
        return if args[0].is_empty() || args[0][0] == 0 { 1 } else { 0 };
    }
    if args.len() == 2 {
        let op = &args[0];
        let arg = &args[1];
        let arg_str = bytes_to_str(arg);
        // — CrashBloom: delegate to the full unary operator set
        return match op.as_slice() {
            b"-z" => if arg.is_empty() { 0 } else { 1 },
            b"-n" => if !arg.is_empty() { 0 } else { 1 },
            b"-f" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_file() { 0 } else { 1 } },
            b"-d" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_dir() { 0 } else { 1 } },
            b"-e" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 { 0 } else { 1 } },
            b"-s" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.size > 0 { 0 } else { 1 } },
            b"-r" | b"-w" | b"-x" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 { 0 } else { 1 } },
            b"-L" | b"-h" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::lstat(arg_str, &mut st) == 0 && st.is_symlink() { 0 } else { 1 } },
            b"-p" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_fifo() { 0 } else { 1 } },
            b"-b" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_block_device() { 0 } else { 1 } },
            b"-c" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_char_device() { 0 } else { 1 } },
            b"-S" => { let mut st = libc::stat::Stat::zeroed(); if libc::stat::stat(arg_str, &mut st) == 0 && st.is_socket() { 0 } else { 1 } },
            _ => 1,
        };
    }
    if args.len() == 3 {
        return builtin_test_binary(&args[0], &args[1], &args[2]);
    }
    1
}

fn builtin_test_binary(left: &[u8], op: &[u8], right: &[u8]) -> i32 {
    match op {
        b"=" | b"==" => if left == right { 0 } else { 1 },
        b"!=" => if left != right { 0 } else { 1 },
        b"-eq" => { int_cmp(left, right, |a, b| a == b) }
        b"-ne" => { int_cmp(left, right, |a, b| a != b) }
        b"-lt" => { int_cmp(left, right, |a, b| a < b) }
        b"-le" => { int_cmp(left, right, |a, b| a <= b) }
        b"-gt" => { int_cmp(left, right, |a, b| a > b) }
        b"-ge" => { int_cmp(left, right, |a, b| a >= b) }
        _ => 1,
    }
}

fn int_cmp(a: &[u8], b: &[u8], f: fn(i64, i64) -> bool) -> i32 {
    match (parse_int(a), parse_int(b)) {
        (Some(a), Some(b)) => if f(a, b) { 0 } else { 1 },
        _ => 2,
    }
}

/// type — show command type
fn builtin_type(argv: &[Vec<u8>], eval: &Evaluator) -> i32 {
    if argv.len() < 2 { return 1; }

    for i in 1..argv.len() {
        let name = &argv[i];
        let name_str = bytes_to_str(name);

        // Check if builtin
        let is_b = matches!(name.as_slice(),
            b"cd" | b"exit" | b"export" | b"unset" | b"source" | b"." |
            b"exec" | b"eval" | b"set" | b"shift" | b"alias" | b"unalias" |
            b"umask" | b"read" | b"true" | b"false" | b":" | b"[" | b"test" |
            b"builtin" | b"command" | b"type" | b"echo" | b"pwd" |
            b"help" | b"printf" | b"let" | b"declare" | b"readonly"
        );

        if is_b {
            prints(name_str);
            printlns(" is a shell builtin");
        } else if eval.functions.iter().any(|(fname, _)| fname == name) {
            // — ByteRiot: user-defined functions get reported before PATH search.
            // You defined it, we found it. What more do you want?
            prints(name_str);
            printlns(" is a function");
        } else {
            // Search PATH
            let path_env = getenv("PATH").unwrap_or("/bin:/usr/bin");
            let mut found = false;
            for dir in path_env.split(':') {
                if dir.is_empty() { continue; }
                let mut full = Vec::new();
                full.extend_from_slice(dir.as_bytes());
                if full.last() != Some(&b'/') { full.push(b'/'); }
                full.extend_from_slice(name);
                full.push(0);
                let path = bytes_to_str(&full);
                let fd = open2(path, O_RDONLY);
                if fd >= 0 {
                    close(fd);
                    prints(name_str);
                    prints(" is ");
                    printlns(path);
                    found = true;
                    break;
                }
            }
            if !found {
                eprints("esh: type: ");
                prints(name_str);
                eprintlns(": not found");
            }
        }
    }
    0
}

/// command — force PATH lookup (skip builtins/aliases)
fn builtin_command(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if argv.len() < 2 { return 0; }
    // Re-invoke as external — skip builtin check
    let sub_argv = &argv[1..];
    eval.exec_external(sub_argv, &[], &[])
}

/// builtin — force builtin execution
fn builtin_builtin(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if argv.len() < 2 { return 0; }
    exec_builtin(&argv[1..].to_vec(), eval)
}

/// umask — set file creation mask
fn builtin_umask(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 {
        // Print current umask — simplified
        printlns("0022");
        return 0;
    }

    // Parse octal
    let arg = &argv[1];
    let mut val: u32 = 0;
    for &b in arg.iter() {
        if b == 0 { break; }
        if b < b'0' || b > b'7' {
            eprintlns("esh: umask: invalid octal number");
            return 1;
        }
        val = val * 8 + (b - b'0') as u32;
    }

    unsafe { libc::c_exports::umask(val); }
    0
}

/// Alias storage — simple static array
static mut ALIASES: [([u8; 32], [u8; 128], bool); 64] = [([0u8; 32], [0u8; 128], false); 64];

/// alias — define command alias
fn builtin_alias(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 {
        // List all aliases
        let aliases = unsafe { &*core::ptr::addr_of!(ALIASES) };
        for (name, value, used) in aliases.iter() {
            if *used {
                prints("alias ");
                print_bytes(name);
                prints("='");
                print_bytes(value);
                printlns("'");
            }
        }
        return 0;
    }

    for i in 1..argv.len() {
        let arg = &argv[i];
        if let Some(eq_pos) = arg.iter().position(|&b| b == b'=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];

            let aliases = unsafe { &mut *core::ptr::addr_of_mut!(ALIASES) };
            // Find existing or empty slot
            let mut slot = None;
            for j in 0..64 {
                if aliases[j].2 && &aliases[j].0[..name.len()] == name && aliases[j].0[name.len()] == 0 {
                    slot = Some(j);
                    break;
                }
            }
            if slot.is_none() {
                for j in 0..64 {
                    if !aliases[j].2 { slot = Some(j); break; }
                }
            }

            if let Some(j) = slot {
                aliases[j].0 = [0u8; 32];
                aliases[j].1 = [0u8; 128];
                let nlen = name.len().min(31);
                aliases[j].0[..nlen].copy_from_slice(&name[..nlen]);
                let vlen = value.len().min(127);
                aliases[j].1[..vlen].copy_from_slice(&value[..vlen]);
                aliases[j].2 = true;
            }
        }
    }
    0
}

/// unalias — remove alias
fn builtin_unalias(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 { return 1; }

    let aliases = unsafe { &mut *core::ptr::addr_of_mut!(ALIASES) };
    for i in 1..argv.len() {
        let name = &argv[i];
        for j in 0..64 {
            if aliases[j].2 {
                let nlen = bytes_len(&aliases[j].0);
                if nlen == name.len() && &aliases[j].0[..nlen] == name.as_slice() {
                    aliases[j].2 = false;
                    break;
                }
            }
        }
    }
    0
}

/// Look up an alias by name
pub fn lookup_alias(name: &[u8]) -> Option<Vec<u8>> {
    let aliases = unsafe { &*core::ptr::addr_of!(ALIASES) };
    for (aname, avalue, used) in aliases.iter() {
        if *used {
            let nlen = bytes_len(aname);
            if nlen == name.len() && &aname[..nlen] == name {
                let vlen = bytes_len(avalue);
                return Some(avalue[..vlen].to_vec());
            }
        }
    }
    None
}

/// wait — wait for background processes
fn builtin_wait(argv: &[Vec<u8>]) -> i32 {
    if argv.len() > 1 {
        if let Some(pid) = parse_int(&argv[1]) {
            let mut status = 0;
            waitpid(pid as i32, &mut status, 0);
            return (status >> 8) & 0xFF;
        }
    }
    // Wait for all children
    let mut status = 0;
    loop {
        let ret = waitpid(-1, &mut status, 0);
        if ret <= 0 { break; }
    }
    0
}

/// kill — send signal to process
fn builtin_kill(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 {
        eprintlns("esh: kill: usage: kill [-SIGNAL] PID");
        return 1;
    }

    let mut sig = SIGTERM;
    let mut pid_idx = 1;

    // Check for -SIGNAL
    if argv.len() > 2 && argv[1].first() == Some(&b'-') {
        let sig_str = &argv[1][1..];
        sig = match sig_str {
            s if s == b"9" || s == b"KILL" => SIGKILL,
            s if s == b"15" || s == b"TERM" => SIGTERM,
            s if s == b"2" || s == b"INT" => SIGINT,
            s if s == b"1" || s == b"HUP" => SIGHUP,
            s if s == b"0" => 0,
            _ => {
                if let Some(n) = parse_int(sig_str) { n as i32 } else { SIGTERM }
            }
        };
        pid_idx = 2;
    }

    for i in pid_idx..argv.len() {
        if let Some(pid) = parse_int(&argv[i]) {
            let ret = syscall::sys_kill(pid as i32, sig);
            if ret < 0 {
                eprints("esh: kill: ");
                eprintlns("failed to send signal");
                return 1;
            }
        }
    }
    0
}

/// printf — formatted output (simplified)
fn builtin_printf(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 { return 0; }

    let fmt = &argv[1];
    let mut arg_idx = 2;
    let mut i = 0;

    while i < fmt.len() && fmt[i] != 0 {
        if fmt[i] == b'\\' && i + 1 < fmt.len() {
            match fmt[i + 1] {
                b'n' => { putchar(b'\n'); }
                b't' => { putchar(b'\t'); }
                b'\\' => { putchar(b'\\'); }
                _ => { putchar(fmt[i + 1]); }
            }
            i += 2;
        } else if fmt[i] == b'%' && i + 1 < fmt.len() {
            match fmt[i + 1] {
                b's' => {
                    if arg_idx < argv.len() {
                        print_bytes(&argv[arg_idx]);
                        arg_idx += 1;
                    }
                }
                b'd' => {
                    if arg_idx < argv.len() {
                        if let Some(n) = parse_int(&argv[arg_idx]) {
                            print_i64(n);
                        }
                        arg_idx += 1;
                    }
                }
                b'%' => { putchar(b'%'); }
                _ => { putchar(fmt[i]); putchar(fmt[i + 1]); }
            }
            i += 2;
        } else {
            putchar(fmt[i]);
            i += 1;
        }
    }

    fflush_stdout();
    0
}

/// local — create function-scoped variables
/// — IronGhost: saves current value of VAR in the top local frame,
/// then sets VAR to the new value. On function exit, the old value
/// is restored. If not in a function, error.
fn builtin_local(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if !eval.in_function {
        eprintlns("esh: local: can only be used in a function");
        return 1;
    }

    for i in 1..argv.len() {
        let arg = &argv[i];
        if arg.first() == Some(&b'-') { continue; } // skip flags
        if let Some(eq_pos) = arg.iter().position(|&b| b == b'=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            // Save current value before overwriting
            eval.save_local(name);
            if let (Ok(n), Ok(v)) = (core::str::from_utf8(name), core::str::from_utf8(value)) {
                setenv(n, v);
            }
        } else {
            // `local VAR` without value — save current and leave as-is
            eval.save_local(arg);
        }
    }
    0
}

/// declare/readonly — variable declaration
/// — IronGhost: `declare -a name` creates empty array, otherwise setenv.
fn builtin_declare(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    let mut is_array = false;
    let mut is_assoc = false;
    for i in 1..argv.len() {
        let arg = &argv[i];
        if arg == b"-a" { is_array = true; continue; }
        if arg == b"-A" { is_assoc = true; continue; }
        if arg.first() == Some(&b'-') { continue; } // skip other flags

        if is_assoc {
            // — IronGhost: declare -A name — create associative array.
            // declare -A name=([key1]=val1 [key2]=val2)
            if !arg.iter().any(|&b| b == b'=') {
                eval.create_assoc(arg);
            } else if let Some(eq_pos) = arg.iter().position(|&b| b == b'=') {
                let name = &arg[..eq_pos];
                let value = &arg[eq_pos + 1..];
                eval.create_assoc(name);
                if value.starts_with(b"(") && value.ends_with(b")") {
                    let inner = &value[1..value.len() - 1];
                    // Parse [key]=value pairs
                    parse_assoc_init(inner, name, eval);
                }
            }
        } else if is_array {
            // declare -a name — create empty indexed array
            if !arg.iter().any(|&b| b == b'=') {
                eval.set_array(arg, Vec::new());
            } else if let Some(eq_pos) = arg.iter().position(|&b| b == b'=') {
                let name = &arg[..eq_pos];
                let value = &arg[eq_pos + 1..];
                if value.starts_with(b"(") && value.ends_with(b")") {
                    let inner = &value[1..value.len() - 1];
                    let elements = eval.split_array_elements(inner);
                    eval.set_array(name, elements);
                } else {
                    eval.set_array(name, Vec::new());
                }
            }
        } else if let Some(eq_pos) = arg.iter().position(|&b| b == b'=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            if let (Ok(n), Ok(v)) = (core::str::from_utf8(name), core::str::from_utf8(value)) {
                setenv(n, v);
            }
        }
    }
    0
}

/// let — arithmetic evaluation (simplified)
fn builtin_let(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 { return 1; }

    let mut result: i64 = 0;
    for i in 1..argv.len() {
        result = eval_arithmetic(&argv[i]);
    }

    if result != 0 { 0 } else { 1 }
}

/// Simple arithmetic evaluator for let
/// — ByteRiot: handles basic expressions: VAR=EXPR, NUM+NUM, NUM-NUM, etc.
fn eval_arithmetic(expr: &[u8]) -> i64 {
    // Check for assignment: VAR=EXPR
    if let Some(eq_pos) = expr.iter().position(|&b| b == b'=') {
        let name = &expr[..eq_pos];
        let value_expr = &expr[eq_pos + 1..];
        let val = eval_arithmetic(value_expr);
        let val_str = format_i64(val);
        if let (Ok(n), Ok(v)) = (core::str::from_utf8(name), core::str::from_utf8(&val_str)) {
            setenv(n, v);
        }
        return val;
    }

    // Simple binary ops: find last + or - (lowest precedence)
    let mut depth = 0i32;
    let mut last_add = None;
    for j in (0..expr.len()).rev() {
        if expr[j] == b')' { depth += 1; }
        if expr[j] == b'(' { depth -= 1; }
        if depth == 0 && (expr[j] == b'+' || expr[j] == b'-') && j > 0 {
            last_add = Some(j);
            break;
        }
    }

    if let Some(pos) = last_add {
        let left = eval_arithmetic(&expr[..pos]);
        let right = eval_arithmetic(&expr[pos + 1..]);
        return if expr[pos] == b'+' { left + right } else { left - right };
    }

    // Find last * or /
    let mut last_mul = None;
    let mut depth = 0i32;
    for j in (0..expr.len()).rev() {
        if expr[j] == b')' { depth += 1; }
        if expr[j] == b'(' { depth -= 1; }
        if depth == 0 && (expr[j] == b'*' || expr[j] == b'/' || expr[j] == b'%') && j > 0 {
            last_mul = Some(j);
            break;
        }
    }

    if let Some(pos) = last_mul {
        let left = eval_arithmetic(&expr[..pos]);
        let right = eval_arithmetic(&expr[pos + 1..]);
        return match expr[pos] {
            b'*' => left * right,
            b'/' => if right != 0 { left / right } else { 0 },
            b'%' => if right != 0 { left % right } else { 0 },
            _ => 0,
        };
    }

    // Atom: number or variable reference
    if let Some(n) = parse_int(expr) {
        return n;
    }

    // Try as variable name
    if let Ok(name) = core::str::from_utf8(expr) {
        if let Some(val) = getenv(name) {
            if let Some(n) = parse_int(val.as_bytes()) {
                return n;
            }
        }
    }

    0
}

fn format_i64(mut val: i64) -> Vec<u8> {
    if val == 0 { return alloc::vec![b'0']; }
    let neg = val < 0;
    if neg { val = -val; }
    let mut digits = Vec::new();
    while val > 0 {
        digits.push(b'0' + (val % 10) as u8);
        val /= 10;
    }
    if neg { digits.push(b'-'); }
    digits.reverse();
    digits
}

/// return — return from function with optional status
/// — ByteRiot: the escape hatch. Sets return_requested flag so the
/// evaluator unwinds back to the function call site.
fn builtin_return(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if !eval.in_function {
        eprintlns("esh: return: can only return from a function");
        return 1;
    }
    let status = if argv.len() > 1 {
        parse_int(&argv[1]).unwrap_or(eval.last_status as i64) as i32
    } else {
        eval.last_status
    };
    eval.return_requested = true;
    eval.return_status = status;
    status
}

/// break — exit from loop
/// — ByteRiot: break [n] — exit n levels of loop nesting. Default n=1.
/// Error if not inside a loop. `break 2` propagates through nested loops.
fn builtin_break(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if eval.loop_depth == 0 {
        eprintlns("esh: break: only meaningful in a loop");
        return 1;
    }
    let n = if argv.len() > 1 {
        let v = parse_int(&argv[1]).unwrap_or(1);
        if v < 1 { 1 } else { v as i32 }
    } else {
        1
    };
    eval.break_count = n;
    0
}

/// continue — skip to next loop iteration
/// — ByteRiot: continue [n] — skip to next iteration of nth enclosing loop.
fn builtin_continue(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if eval.loop_depth == 0 {
        eprintlns("esh: continue: only meaningful in a loop");
        return 1;
    }
    let n = if argv.len() > 1 {
        let v = parse_int(&argv[1]).unwrap_or(1);
        if v < 1 { 1 } else { v as i32 }
    } else {
        1
    };
    eval.continue_count = n;
    0
}

/// trap — set signal handlers
/// — ByteRiot: trap 'cmd' SIG — run cmd when SIG is caught.
/// trap '' SIG — ignore signal. trap - SIG — reset to default.
/// trap (no args) — list current traps.
fn builtin_trap(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if argv.len() < 2 {
        // List traps
        for (i, trap) in eval.traps.iter().enumerate() {
            if let Some(cmd) = trap {
                prints("trap -- '");
                print_bytes(cmd);
                prints("' ");
                print_signal_name(i);
                printlns("");
            }
        }
        return 0;
    }

    if argv.len() < 3 {
        eprintlns("esh: trap: usage: trap 'cmd' SIGNAL...");
        return 1;
    }

    let cmd = &argv[1];
    for i in 2..argv.len() {
        let sig = parse_signal_name(&argv[i]);
        if sig < 32 {
            if cmd == b"-" {
                eval.traps[sig] = None; // reset to default
            } else {
                eval.traps[sig] = Some(cmd.clone());
            }
        } else {
            eprints("esh: trap: unknown signal: ");
            print_bytes(&argv[i]);
            eprintlns("");
        }
    }
    0
}

/// Parse signal name to number
fn parse_signal_name(name: &[u8]) -> usize {
    match name {
        b"EXIT" | b"0" => 0,
        b"HUP" | b"SIGHUP" | b"1" => 1,
        b"INT" | b"SIGINT" | b"2" => 2,
        b"QUIT" | b"SIGQUIT" | b"3" => 3,
        b"TERM" | b"SIGTERM" | b"15" => 15,
        b"USR1" | b"SIGUSR1" | b"10" => 10,
        b"USR2" | b"SIGUSR2" | b"12" => 12,
        b"ALRM" | b"SIGALRM" | b"14" => 14,
        b"PIPE" | b"SIGPIPE" | b"13" => 13,
        b"CHLD" | b"SIGCHLD" | b"17" => 17,
        _ => {
            if let Some(n) = parse_int(name) {
                n as usize
            } else {
                99
            }
        }
    }
}

/// Print signal name for trap listing
fn print_signal_name(sig: usize) {
    match sig {
        0 => prints("EXIT"),
        1 => prints("HUP"),
        2 => prints("INT"),
        3 => prints("QUIT"),
        15 => prints("TERM"),
        _ => {
            let mut buf = [0u8; 4];
            let s = sig;
            if s >= 10 { buf[0] = b'0' + (s / 10) as u8; buf[1] = b'0' + (s % 10) as u8; }
            else { buf[0] = b'0' + s as u8; }
            print_bytes(&buf);
        }
    }
}

/// getopts — POSIX option parsing with bundled option support
/// — ThreadRogue: the option parser nobody loves but everybody uses.
/// Tracks state via OPTIND (argument index, 1-based) and OPTERR_CHARPOS
/// (character position within current arg, for -abc bundling).
/// Returns 0 while options remain, 1 when done.
/// `:` prefix = silent errors, trailing `:` = requires argument.
///
/// Bundled options: `-abc` is parsed as `-a`, `-b`, `-c` across successive
/// getopts calls. OPTERR_CHARPOS tracks where we are within the arg.
/// When an option needs an argument and more chars remain, the remainder
/// becomes OPTARG (e.g., `-ffile.txt` → opt=f, OPTARG=file.txt). — ThreadRogue
fn builtin_getopts(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 3 {
        eprintlns("esh: getopts: usage: getopts optstring name [args]");
        return 1;
    }

    let optstring = &argv[1];
    let var_name = bytes_to_str(&argv[2]);

    // Get OPTIND (1-based index into args)
    let optind = getenv("OPTIND")
        .and_then(|s| parse_int(s.as_bytes()))
        .unwrap_or(1) as usize;

    // — ThreadRogue: char position within current arg for bundled opts.
    // 0 means start fresh from position 1 (skip the '-').
    let charpos = getenv("OPTERR_CHARPOS")
        .and_then(|s| parse_int(s.as_bytes()))
        .unwrap_or(0) as usize;

    // Determine args to parse
    let args: Vec<&[u8]> = if argv.len() > 3 {
        argv[3..].iter().map(|a| a.as_slice()).collect()
    } else {
        return 1;
    };

    if optind > args.len() {
        setenv(var_name, "?");
        return 1;
    }

    let arg = args[optind - 1];
    if arg.is_empty() || arg[0] != b'-' || arg == b"-" {
        setenv(var_name, "?");
        return 1;
    }

    if arg == b"--" {
        let mut buf = [0u8; 16];
        let len = format_usize_buf(optind + 1, &mut buf);
        setenv("OPTIND", bytes_to_str(&buf[..len]));
        setenv(var_name, "?");
        setenv("OPTERR_CHARPOS", "0");
        return 1;
    }

    let silent = !optstring.is_empty() && optstring[0] == b':';

    // — ThreadRogue: determine which character to process.
    // charpos=0 means we haven't started this arg yet → start at position 1.
    let pos_in_arg = if charpos > 0 { charpos } else { 1 };

    if pos_in_arg >= arg.len() {
        // Exhausted this arg — advance to next
        let mut buf = [0u8; 16];
        let len = format_usize_buf(optind + 1, &mut buf);
        setenv("OPTIND", bytes_to_str(&buf[..len]));
        setenv("OPTERR_CHARPOS", "0");
        setenv(var_name, "?");
        return 1;
    }

    let opt_char = arg[pos_in_arg];

    // Check if this option is valid
    if let Some(opos) = optstring.iter().position(|&b| b == opt_char) {
        let needs_arg = opos + 1 < optstring.len() && optstring[opos + 1] == b':';

        if needs_arg {
            // — ThreadRogue: if more chars remain in this arg, they ARE the argument.
            // e.g., -ffile.txt → opt=f, OPTARG=file.txt
            if pos_in_arg + 1 < arg.len() {
                let remainder = &arg[pos_in_arg + 1..];
                setenv("OPTARG", bytes_to_str(remainder));
                // Advance to next arg
                let mut buf = [0u8; 16];
                let len = format_usize_buf(optind + 1, &mut buf);
                setenv("OPTIND", bytes_to_str(&buf[..len]));
                setenv("OPTERR_CHARPOS", "0");
            } else if optind < args.len() {
                // Argument is the next word
                let opt_arg = args[optind];
                setenv("OPTARG", bytes_to_str(opt_arg));
                let mut buf = [0u8; 16];
                let len = format_usize_buf(optind + 2, &mut buf);
                setenv("OPTIND", bytes_to_str(&buf[..len]));
                setenv("OPTERR_CHARPOS", "0");
            } else {
                // Missing argument
                if silent {
                    setenv(var_name, ":");
                    setenv("OPTARG", bytes_to_str(&[opt_char]));
                } else {
                    eprints("esh: getopts: option requires argument -- ");
                    putchar(opt_char);
                    eprintlns("");
                    setenv(var_name, "?");
                }
                let mut buf = [0u8; 16];
                let len = format_usize_buf(optind + 1, &mut buf);
                setenv("OPTIND", bytes_to_str(&buf[..len]));
                setenv("OPTERR_CHARPOS", "0");
                return 0;
            }
        } else {
            libc::unsetenv("OPTARG");
            // — ThreadRogue: advance within the bundled arg, or to next arg
            let next_pos = pos_in_arg + 1;
            if next_pos < arg.len() {
                // More options in this arg — stay on same OPTIND, advance charpos
                let mut buf = [0u8; 16];
                let len = format_usize_buf(next_pos, &mut buf);
                setenv("OPTERR_CHARPOS", bytes_to_str(&buf[..len]));
            } else {
                // Exhausted this arg — advance OPTIND
                let mut buf = [0u8; 16];
                let len = format_usize_buf(optind + 1, &mut buf);
                setenv("OPTIND", bytes_to_str(&buf[..len]));
                setenv("OPTERR_CHARPOS", "0");
            }
        }
        let opt_str = [opt_char];
        setenv(var_name, bytes_to_str(&opt_str));
        return 0;
    }

    // Invalid option
    if silent {
        setenv(var_name, "?");
        setenv("OPTARG", bytes_to_str(&[opt_char]));
    } else {
        eprints("esh: getopts: illegal option -- ");
        putchar(opt_char);
        eprintlns("");
        setenv(var_name, "?");
    }
    // Advance past this option character (might be bundled)
    let next_pos = pos_in_arg + 1;
    if next_pos < arg.len() {
        let mut buf = [0u8; 16];
        let len = format_usize_buf(next_pos, &mut buf);
        setenv("OPTERR_CHARPOS", bytes_to_str(&buf[..len]));
    } else {
        let mut buf = [0u8; 16];
        let len = format_usize_buf(optind + 1, &mut buf);
        setenv("OPTIND", bytes_to_str(&buf[..len]));
        setenv("OPTERR_CHARPOS", "0");
    }
    0
}

/// Format usize into buffer, return length
fn format_usize_buf(mut n: usize, buf: &mut [u8]) -> usize {
    if n == 0 { buf[0] = b'0'; return 1; }
    let mut digits = [0u8; 10];
    let mut len = 0;
    while n > 0 {
        digits[len] = b'0' + (n % 10) as u8;
        n /= 10;
        len += 1;
    }
    for i in 0..len {
        buf[i] = digits[len - 1 - i];
    }
    len
}

/// jobs — list background jobs
/// — ThreadRogue: the job inspector. Shows what's running in the background.
fn builtin_jobs(eval: &mut Evaluator) -> i32 {
    // Reap any completed jobs first
    reap_background_jobs(eval);

    let jobs = eval.job_table.list();
    if jobs.is_empty() {
        return 0;
    }
    for job in &jobs {
        prints("[");
        print_i64(job.id as i64);
        prints("]  ");
        match job.state {
            crate::jobs::JobState::Running => prints("Running    "),
            crate::jobs::JobState::Stopped => prints("Stopped    "),
            crate::jobs::JobState::Done => prints("Done       "),
        }
        print_bytes(&job.command);
        printlns("");
    }
    0
}

/// fg — bring background job to foreground
/// — ThreadRogue: pull a background job back into the spotlight.
/// tcsetpgrp gives it the terminal, waitpid blocks until it's done.
fn builtin_fg(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    let job_id = if argv.len() > 1 {
        // Parse %N or just N
        let arg = &argv[1];
        let s = if arg.first() == Some(&b'%') { &arg[1..] } else { &arg[..] };
        parse_int(s).unwrap_or(0) as usize
    } else {
        // Find the most recent job
        let jobs = eval.job_table.list();
        if let Some(last) = jobs.last() { last.id } else { 0 }
    };

    if job_id == 0 {
        eprintlns("esh: fg: no current job");
        return 1;
    }

    let job = match eval.job_table.get(job_id) {
        Some(j) => j,
        None => {
            eprintlns("esh: fg: no such job");
            return 1;
        }
    };

    let pgid = job.pgid;

    // Give the job the terminal
    tcsetpgrp(0, pgid);

    // Send SIGCONT in case it was stopped
    syscall::sys_kill(-pgid, SIGCONT);

    // Wait for it
    let mut status = 0;
    loop {
        let ret = waitpid(-pgid, &mut status, 0);
        if ret > 0 || (ret < 0 && ret != -(libc::errno::EINTR as i32)) { break; }
    }

    // Reclaim terminal
    for _ in 0..8 {
        if tcsetpgrp(0, getpid()) == 0 { break; }
        sched_yield();
    }

    eval.job_table.mark_done(pgid);
    eval.last_status = (status >> 8) & 0xFF;
    eval.last_status
}

/// bg — resume stopped job in background
/// — ThreadRogue: wake up a stopped job but keep it in the background.
fn builtin_bg(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    let job_id = if argv.len() > 1 {
        let arg = &argv[1];
        let s = if arg.first() == Some(&b'%') { &arg[1..] } else { &arg[..] };
        parse_int(s).unwrap_or(0) as usize
    } else {
        let jobs = eval.job_table.list();
        if let Some(last) = jobs.last() { last.id } else { 0 }
    };

    if job_id == 0 {
        eprintlns("esh: bg: no current job");
        return 1;
    }

    let job = match eval.job_table.get(job_id) {
        Some(j) => j,
        None => {
            eprintlns("esh: bg: no such job");
            return 1;
        }
    };

    let pgid = job.pgid;
    syscall::sys_kill(-pgid, SIGCONT);
    0
}

/// Reap background jobs — check for completed background processes
/// — ThreadRogue: called before prompt and after job commands to clean up.
fn reap_background_jobs(eval: &mut Evaluator) {
    // Collect done pgids first to avoid borrow conflict
    let mut done_pgids = Vec::new();
    let mut done_info = Vec::new();
    {
        let jobs = eval.job_table.list();
        for job in &jobs {
            for &pid in &job.pids {
                let mut status = 0;
                let ret = waitpid(pid, &mut status, WNOHANG);
                if ret > 0 {
                    done_pgids.push(job.pgid);
                    done_info.push((job.id, job.command.clone()));
                }
            }
        }
    }
    for pgid in &done_pgids {
        eval.job_table.mark_done(*pgid);
    }
    for (id, cmd) in &done_info {
        prints("[");
        print_i64(*id as i64);
        prints("] Done       ");
        print_bytes(cmd);
        printlns("");
    }
    eval.job_table.reap();
}

/// history — list command history
/// — ThreadRogue: walks readline's internal history list. `history` prints all,
/// `history N` prints last N, `history -c` clears.
fn builtin_history(argv: &[Vec<u8>]) -> i32 {
    // Access readline history via C API
    let hist_len = unsafe { libc::readline::history_length };
    if hist_len <= 0 && argv.len() < 2 {
        return 0;
    }

    if argv.len() > 1 {
        if argv[1] == b"-c" {
            // Clear history
            unsafe { libc::readline::clear_history(); }
            return 0;
        }
        // history N — show last N entries
        if let Some(n) = parse_int(&argv[1]) {
            let n = n as i32;
            let start = if hist_len > n { hist_len - n } else { 0 };
            for i in start..hist_len {
                let entry = unsafe { libc::readline::history_get(i + 1) }; // 1-based
                if !entry.is_null() {
                    let line = unsafe { (*entry).line };
                    if !line.is_null() {
                        print_i64((i + 1) as i64);
                        prints("  ");
                        unsafe {
                            let mut p = line;
                            while *p != 0 {
                                putchar(*p);
                                p = p.add(1);
                            }
                        }
                        printlns("");
                    }
                }
            }
            return 0;
        }
    }

    // Print all history
    for i in 0..hist_len {
        let entry = unsafe { libc::readline::history_get(i + 1) };
        if !entry.is_null() {
            let line = unsafe { (*entry).line };
            if !line.is_null() {
                print_i64((i + 1) as i64);
                prints("  ");
                unsafe {
                    let mut p = line;
                    while *p != 0 {
                        putchar(*p);
                        p = p.add(1);
                    }
                }
                printlns("");
            }
        }
    }
    0
}

/// Programmable completion storage — static table of completion specs
/// — ThreadRogue: `complete -W "words" cmd` registers word completions,
/// `complete -f cmd` for files, `complete -d cmd` for dirs.
static mut COMPLETION_SPECS: [([u8; 32], [u8; 256], u8, bool); 32] =
    [([0u8; 32], [0u8; 256], 0, false); 32];

/// Completion spec types
const COMP_WORDS: u8 = 1;    // -W "word list"
const COMP_FILES: u8 = 2;    // -f
const COMP_DIRS: u8 = 3;     // -d
const COMP_COMMANDS: u8 = 4; // -c
const COMP_BUILTINS: u8 = 5; // -b

/// complete — register programmable completion specs
/// — ThreadRogue: `complete -W "words" cmd`, `complete -f cmd`, `complete -d cmd`
fn builtin_complete(argv: &[Vec<u8>]) -> i32 {
    if argv.len() < 2 {
        // List all completions
        let specs = unsafe { &*core::ptr::addr_of!(COMPLETION_SPECS) };
        for (name, words, ctype, used) in specs.iter() {
            if *used {
                prints("complete ");
                match ctype {
                    &COMP_WORDS => { prints("-W '"); print_bytes(words); prints("' "); }
                    &COMP_FILES => prints("-f "),
                    &COMP_DIRS => prints("-d "),
                    _ => {}
                }
                print_bytes(name);
                printlns("");
            }
        }
        return 0;
    }

    let mut i = 1;
    let mut comp_type = 0u8;
    let mut wordlist = [0u8; 256];

    while i < argv.len() {
        let arg = &argv[i];
        if arg == b"-W" && i + 1 < argv.len() {
            i += 1;
            comp_type = COMP_WORDS;
            let wl = &argv[i];
            let len = wl.len().min(255);
            wordlist[..len].copy_from_slice(&wl[..len]);
        } else if arg == b"-f" {
            comp_type = COMP_FILES;
        } else if arg == b"-d" {
            comp_type = COMP_DIRS;
        } else if arg == b"-c" {
            comp_type = COMP_COMMANDS;
        } else if arg == b"-b" {
            comp_type = COMP_BUILTINS;
        } else if comp_type != 0 {
            // This is the command name
            let specs = unsafe { &mut *core::ptr::addr_of_mut!(COMPLETION_SPECS) };
            for j in 0..32 {
                if !specs[j].3 {
                    let nlen = arg.len().min(31);
                    specs[j].0[..nlen].copy_from_slice(&arg[..nlen]);
                    specs[j].1 = wordlist;
                    specs[j].2 = comp_type;
                    specs[j].3 = true;
                    break;
                }
            }
        }
        i += 1;
    }
    0
}

/// compgen — generate completions (bash extension)
/// — ThreadRogue: produces completions on stdout, one per line.
/// Used by completion functions to generate candidates.
/// compgen -W "word1 word2 word3" prefix → matching words
/// compgen -f prefix → matching filenames
/// compgen -d prefix → matching directories
/// compgen -b prefix → matching builtins
/// compgen -c prefix → matching commands (builtins + PATH)
fn builtin_compgen(argv: &[Vec<u8>]) -> i32 {
    let mut comp_type = 0u8;
    let mut wordlist = Vec::new();
    let mut prefix = Vec::new();

    let mut i = 1;
    while i < argv.len() {
        let arg = &argv[i];
        if arg == b"-W" && i + 1 < argv.len() {
            i += 1;
            comp_type = COMP_WORDS;
            wordlist = argv[i].clone();
        } else if arg == b"-f" {
            comp_type = COMP_FILES;
        } else if arg == b"-d" {
            comp_type = COMP_DIRS;
        } else if arg == b"-c" {
            comp_type = COMP_COMMANDS;
        } else if arg == b"-b" {
            comp_type = COMP_BUILTINS;
        } else if arg.first() != Some(&b'-') {
            prefix = arg.clone();
        }
        i += 1;
    }

    match comp_type {
        COMP_WORDS => {
            // Split wordlist by spaces and match prefix
            for word in wordlist.split(|&b| b == b' ') {
                if !word.is_empty() && (prefix.is_empty() || word.starts_with(&prefix)) {
                    print_bytes(word);
                    printlns("");
                }
            }
        }
        COMP_BUILTINS => {
            // — ThreadRogue: list all builtins matching prefix
            let builtins: &[&[u8]] = &[
                b"cd", b"exit", b"echo", b"export", b"unset", b"set", b"shift",
                b"source", b".", b"eval", b"exec", b"alias", b"unalias",
                b"umask", b"read", b"true", b"false", b"test", b"[",
                b"builtin", b"command", b"type", b"pwd", b"jobs", b"fg", b"bg",
                b"wait", b"kill", b"history", b"help", b"local", b"declare",
                b"readonly", b"let", b"getopts", b"printf", b"complete", b"compgen",
                b"mapfile", b"readarray", b"shopt", b"return", b"break", b"continue", b"trap",
            ];
            for &b in builtins {
                if prefix.is_empty() || b.starts_with(&prefix) {
                    print_bytes(b);
                    printlns("");
                }
            }
        }
        COMP_FILES | COMP_DIRS | COMP_COMMANDS => {
            // — ThreadRogue: file/dir completion — list directory entries matching prefix
            let dir = if prefix.contains(&b'/') {
                // Has directory component
                let last_slash = prefix.iter().rposition(|&b| b == b'/').unwrap_or(0);
                let dir_part = &prefix[..=last_slash];
                let mut d = dir_part.to_vec();
                d.push(0);
                d
            } else {
                b".\0".to_vec()
            };
            let dir_str = bytes_to_str(&dir);
            if let Some(mut d) = opendir(dir_str) {
                while let Some(entry) = readdir(&mut d) {
                    let name = entry.name();
                    if name == "." || name == ".." { continue; }

                    let mut full = Vec::new();
                    if prefix.contains(&b'/') {
                        let last_slash = prefix.iter().rposition(|&b| b == b'/').unwrap_or(0);
                        full.extend_from_slice(&prefix[..=last_slash]);
                    }
                    full.extend_from_slice(name.as_bytes());

                    if prefix.is_empty() || full.starts_with(&prefix) {
                        print_bytes(&full);
                        printlns("");
                    }
                }
                closedir(d);
            }
        }
        _ => {}
    }
    0
}

/// Look up a programmable completion spec for a command name
pub fn lookup_completion_spec(cmd: &[u8]) -> Option<(u8, &'static [u8])> {
    let specs = unsafe { &*core::ptr::addr_of!(COMPLETION_SPECS) };
    for (name, words, ctype, used) in specs.iter() {
        if *used {
            let nlen = bytes_len(name);
            if nlen == cmd.len() && &name[..nlen] == cmd {
                let wlen = bytes_len(words);
                return Some((*ctype, &words[..wlen]));
            }
        }
    }
    None
}

/// Print help
fn print_help() {
    printlns("OXIDE Shell (esh) — Builtins:");
    printlns("  cd [dir]         Change directory");
    printlns("  pwd              Print working directory");
    printlns("  echo [args]      Print arguments");
    printlns("  export VAR=val   Set environment variable");
    printlns("  unset VAR        Remove environment variable");
    printlns("  source file      Execute script in current shell");
    printlns("  exec cmd         Replace shell with command");
    printlns("  eval str         Evaluate string as command");
    printlns("  set [-- args]    Set positional parameters");
    printlns("  shift [n]        Shift positional parameters");
    printlns("  read VAR         Read line into variable");
    printlns("  test / [         Conditional expression");
    printlns("  alias name=val   Define alias");
    printlns("  unalias name     Remove alias");
    printlns("  umask [mode]     Set file creation mask");
    printlns("  type cmd         Show command type");
    printlns("  true / false     Exit with 0 / 1");
    printlns("  exit [code]      Exit shell");
    printlns("");
    printlns("Operators: | && || ; & > >> < 2>&1");
    printlns("Control: if/then/elif/else/fi for/in/do/done while/do/done");
    printlns("Expansion: $VAR ${VAR:-default} $(cmd) ~ *");
}

/// Parse integer from byte slice
/// shopt — shell options (bash extension)
/// — ByteRiot: the knob rack. Controls glob behavior, pattern matching,
/// and other shell features that bash scripts assume exist.
/// `shopt -s opt` enables, `shopt -u opt` disables, bare `shopt` lists all.
fn builtin_shopt(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    if argv.len() == 1 {
        // List all shopt options
        let opts = &eval.opts;
        let print_opt = |name: &str, val: bool| {
            prints(name);
            prints("\t");
            if val { printlns("on"); } else { printlns("off"); }
        };
        print_opt("nullglob", opts.nullglob);
        print_opt("dotglob", opts.dotglob);
        print_opt("nocaseglob", opts.nocaseglob);
        print_opt("failglob", opts.failglob);
        print_opt("globstar", opts.globstar);
        print_opt("extglob", opts.extglob);
        return 0;
    }

    let mode = &argv[1];
    let set_on = mode == b"-s";
    let set_off = mode == b"-u";

    if !set_on && !set_off {
        // Query single option
        let name = &argv[1];
        let val = match name.as_slice() {
            b"nullglob" => Some(eval.opts.nullglob),
            b"dotglob" => Some(eval.opts.dotglob),
            b"nocaseglob" => Some(eval.opts.nocaseglob),
            b"failglob" => Some(eval.opts.failglob),
            b"globstar" => Some(eval.opts.globstar),
            b"extglob" => Some(eval.opts.extglob),
            _ => None,
        };
        if let Some(v) = val {
            prints(bytes_to_str(name));
            prints("\t");
            if v { printlns("on"); } else { printlns("off"); }
            return 0;
        }
        eprints("esh: shopt: ");
        print_bytes(name);
        eprintlns(": invalid shell option name");
        return 1;
    }

    // Set/unset options
    for i in 2..argv.len() {
        let name = &argv[i];
        let target = if set_on { true } else { false };
        match name.as_slice() {
            b"nullglob" => eval.opts.nullglob = target,
            b"dotglob" => eval.opts.dotglob = target,
            b"nocaseglob" => eval.opts.nocaseglob = target,
            b"failglob" => eval.opts.failglob = target,
            b"globstar" => eval.opts.globstar = target,
            b"extglob" => eval.opts.extglob = target,
            _ => {
                eprints("esh: shopt: ");
                print_bytes(name);
                eprintlns(": invalid shell option name");
                return 1;
            }
        }
    }
    0
}

/// mapfile/readarray — read lines from stdin into an indexed array
/// — IronGhost: the bulk loader. Reads stdin line by line into an array variable.
/// Without this, scripts do `while read line; do arr+=("$line"); done` which is
/// painfully slow for large inputs. mapfile does it in one shot.
///
/// Usage: mapfile [-t] [-n count] [-s count] [array_name]
///   -t: strip trailing newline from each line
///   -n count: read at most count lines (0 = all)
///   -s count: skip first count lines
///   Default array name: MAPFILE
fn builtin_mapfile(argv: &[Vec<u8>], eval: &mut Evaluator) -> i32 {
    let mut strip_newline = false;
    let mut max_lines: usize = 0; // 0 = unlimited
    let mut skip_lines: usize = 0;
    let mut arr_name: &[u8] = b"MAPFILE";

    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_slice() {
            b"-t" => strip_newline = true,
            b"-n" => {
                i += 1;
                if i < argv.len() {
                    max_lines = parse_int(&argv[i]).unwrap_or(0) as usize;
                }
            }
            b"-s" => {
                i += 1;
                if i < argv.len() {
                    skip_lines = parse_int(&argv[i]).unwrap_or(0) as usize;
                }
            }
            _ => {
                if argv[i].first() != Some(&b'-') {
                    arr_name = &argv[i];
                }
            }
        }
        i += 1;
    }

    // Read from stdin line by line
    let mut elements: Vec<Vec<u8>> = Vec::new();
    let mut lines_read: usize = 0;
    let mut lines_skipped: usize = 0;
    let mut line_buf = Vec::with_capacity(256);

    loop {
        let mut byte = [0u8; 1];
        let n = libc::read(0, &mut byte);
        if n <= 0 {
            // EOF — flush remaining buffer as last line
            if !line_buf.is_empty() {
                if lines_skipped < skip_lines {
                    lines_skipped += 1;
                } else {
                    if strip_newline {
                        while line_buf.last() == Some(&b'\n') { line_buf.pop(); }
                    }
                    elements.push(core::mem::take(&mut line_buf));
                    lines_read += 1;
                }
            }
            break;
        }

        if byte[0] == b'\n' {
            if !strip_newline { line_buf.push(b'\n'); }
            if lines_skipped < skip_lines {
                lines_skipped += 1;
                line_buf.clear();
                continue;
            }
            elements.push(core::mem::take(&mut line_buf));
            lines_read += 1;
            if max_lines > 0 && lines_read >= max_lines { break; }
        } else {
            line_buf.push(byte[0]);
        }
    }

    eval.set_array(arr_name, elements);
    0
}

/// — IronGhost: parse [key]=value pairs from associative array initializer.
/// Input is the content between ( and ), e.g. `[foo]=bar [baz]=quux`
fn parse_assoc_init(inner: &[u8], name: &[u8], eval: &mut Evaluator) {
    let mut i = 0;
    while i < inner.len() {
        // Skip whitespace
        while i < inner.len() && inner[i] == b' ' { i += 1; }
        if i >= inner.len() { break; }

        // Expect [key]=value
        if inner[i] == b'[' {
            i += 1;
            let key_start = i;
            while i < inner.len() && inner[i] != b']' { i += 1; }
            if i >= inner.len() { break; }
            let key = &inner[key_start..i];
            i += 1; // skip ]

            if i < inner.len() && inner[i] == b'=' {
                i += 1;
                let val_start = i;
                // Value ends at space or end — handle quoting
                if i < inner.len() && (inner[i] == b'"' || inner[i] == b'\'') {
                    let quote = inner[i];
                    i += 1;
                    let qs = i;
                    while i < inner.len() && inner[i] != quote { i += 1; }
                    let val = &inner[qs..i];
                    if i < inner.len() { i += 1; } // skip closing quote
                    eval.set_assoc(name, key, val.to_vec());
                } else {
                    while i < inner.len() && inner[i] != b' ' { i += 1; }
                    let val = &inner[val_start..i];
                    eval.set_assoc(name, key, val.to_vec());
                }
            }
        } else {
            // Skip non-bracket content
            while i < inner.len() && inner[i] != b' ' { i += 1; }
        }
    }
}

fn parse_int(s: &[u8]) -> Option<i64> {
    let mut i = 0;
    while i < s.len() && s[i] == b' ' { i += 1; }
    let negative = if i < s.len() && s[i] == b'-' { i += 1; true } else { false };
    let mut result: i64 = 0;
    let mut any_digit = false;
    while i < s.len() && s[i] != 0 {
        let c = s[i];
        if c < b'0' || c > b'9' { break; }
        result = result * 10 + (c - b'0') as i64;
        any_digit = true;
        i += 1;
    }
    if !any_digit { return None; }
    Some(if negative { -result } else { result })
}

/// Print bytes until null
fn print_bytes(s: &[u8]) {
    for &b in s {
        if b == 0 { break; }
        putchar(b);
    }
}

/// Get length of null-terminated bytes
fn bytes_len(s: &[u8]) -> usize {
    for i in 0..s.len() {
        if s[i] == 0 { return i; }
    }
    s.len()
}

/// Bytes to str
fn bytes_to_str(bytes: &[u8]) -> &str {
    let len = bytes_len(bytes);
    unsafe { core::str::from_utf8_unchecked(&bytes[..len]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_int() {
        assert_eq!(parse_int(b"42"), Some(42));
        assert_eq!(parse_int(b"-7"), Some(-7));
        assert_eq!(parse_int(b"0"), Some(0));
        assert_eq!(parse_int(b"abc"), None);
    }

    #[test]
    fn test_eval_arithmetic_simple() {
        assert_eq!(eval_arithmetic(b"42"), 42);
        assert_eq!(eval_arithmetic(b"3+4"), 7);
        assert_eq!(eval_arithmetic(b"10-3"), 7);
        assert_eq!(eval_arithmetic(b"6*7"), 42);
        assert_eq!(eval_arithmetic(b"10/3"), 3);
        assert_eq!(eval_arithmetic(b"10%3"), 1);
    }

    #[test]
    fn test_format_i64() {
        assert_eq!(format_i64(42), b"42");
        assert_eq!(format_i64(-7), b"-7");
        assert_eq!(format_i64(0), b"0");
    }

    #[test]
    fn test_bytes_len() {
        assert_eq!(bytes_len(b"hello\0world"), 5);
        assert_eq!(bytes_len(b"hello"), 5);
        assert_eq!(bytes_len(b"\0"), 0);
    }

    #[test]
    fn test_test_string_eq() {
        let result = builtin_test_binary(b"foo", b"=", b"foo");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_test_string_ne() {
        let result = builtin_test_binary(b"foo", b"!=", b"bar");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_test_int_eq() {
        let result = builtin_test_binary(b"42", b"-eq", b"42");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_test_int_lt() {
        let result = builtin_test_binary(b"5", b"-lt", b"10");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_test_int_gt() {
        let result = builtin_test_binary(b"10", b"-gt", b"5");
        assert_eq!(result, 0);
    }
}
