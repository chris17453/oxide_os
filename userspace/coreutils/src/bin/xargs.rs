//! xargs - build and execute command lines from standard input

#![no_std]
#![no_main]

use efflux_libc::*;

const MAX_ARGS: usize = 64;
const MAX_ARG_LEN: usize = 256;
const MAX_CMD_LEN: usize = 4096;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut one_at_a_time = false;
    let mut arg_idx = 1;

    // Parse flags
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        if arg == "-n" {
            arg_idx += 1;
            if arg_idx < argc {
                let n_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
                if n_arg == "1" {
                    one_at_a_time = true;
                }
            }
            arg_idx += 1;
        } else if arg.starts_with("-n") {
            if arg.len() > 2 && arg.as_bytes()[2] == b'1' {
                one_at_a_time = true;
            }
            arg_idx += 1;
        } else if !arg.starts_with('-') {
            break;
        } else {
            arg_idx += 1;
        }
    }

    // Get command to execute (default: echo)
    let mut cmd_parts: [[u8; MAX_ARG_LEN]; MAX_ARGS] = [[0; MAX_ARG_LEN]; MAX_ARGS];
    let mut cmd_lens: [usize; MAX_ARGS] = [0; MAX_ARGS];
    let mut cmd_count = 0;

    if arg_idx >= argc {
        // Default command is echo
        cmd_parts[0][..4].copy_from_slice(b"echo");
        cmd_lens[0] = 4;
        cmd_count = 1;
    } else {
        while arg_idx < argc && cmd_count < MAX_ARGS {
            let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            let len = arg.len().min(MAX_ARG_LEN - 1);
            cmd_parts[cmd_count][..len].copy_from_slice(&arg.as_bytes()[..len]);
            cmd_lens[cmd_count] = len;
            cmd_count += 1;
            arg_idx += 1;
        }
    }

    // Read arguments from stdin
    let mut buf = [0u8; 4096];
    let mut args: [[u8; MAX_ARG_LEN]; MAX_ARGS] = [[0; MAX_ARG_LEN]; MAX_ARGS];
    let mut arg_lens: [usize; MAX_ARGS] = [0; MAX_ARGS];
    let mut arg_count = 0;
    let mut current_arg = [0u8; MAX_ARG_LEN];
    let mut current_len = 0;
    let mut status = 0;

    loop {
        let n = read(STDIN_FILENO, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            let c = buf[i];

            if c == b' ' || c == b'\t' || c == b'\n' {
                if current_len > 0 {
                    // Completed an argument
                    if one_at_a_time {
                        // Execute immediately with one argument
                        if execute_cmd(&cmd_parts, &cmd_lens, cmd_count,
                                      &current_arg[..current_len]) != 0 {
                            status = 1;
                        }
                    } else {
                        // Accumulate argument
                        if arg_count < MAX_ARGS {
                            args[arg_count][..current_len].copy_from_slice(&current_arg[..current_len]);
                            arg_lens[arg_count] = current_len;
                            arg_count += 1;
                        }
                    }
                    current_len = 0;
                }
            } else if current_len < MAX_ARG_LEN - 1 {
                current_arg[current_len] = c;
                current_len += 1;
            }
        }
    }

    // Handle last argument
    if current_len > 0 {
        if one_at_a_time {
            if execute_cmd(&cmd_parts, &cmd_lens, cmd_count,
                          &current_arg[..current_len]) != 0 {
                status = 1;
            }
        } else {
            if arg_count < MAX_ARGS {
                args[arg_count][..current_len].copy_from_slice(&current_arg[..current_len]);
                arg_lens[arg_count] = current_len;
                arg_count += 1;
            }
        }
    }

    // Execute with all accumulated arguments (if not one_at_a_time)
    if !one_at_a_time && arg_count > 0 {
        status = execute_cmd_batch(&cmd_parts, &cmd_lens, cmd_count,
                                   &args, &arg_lens, arg_count);
    }

    status
}

fn execute_cmd(cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
               cmd_lens: &[usize; MAX_ARGS],
               cmd_count: usize,
               arg: &[u8]) -> i32 {
    // Build command line
    let mut cmd = [0u8; MAX_CMD_LEN];
    let mut pos = 0;

    // Add command parts
    for i in 0..cmd_count {
        if pos > 0 {
            cmd[pos] = b' ';
            pos += 1;
        }
        let len = cmd_lens[i];
        cmd[pos..pos + len].copy_from_slice(&cmd_parts[i][..len]);
        pos += len;
    }

    // Add argument
    if pos > 0 {
        cmd[pos] = b' ';
        pos += 1;
    }
    let arg_len = arg.len().min(MAX_CMD_LEN - pos - 1);
    cmd[pos..pos + arg_len].copy_from_slice(&arg[..arg_len]);
    pos += arg_len;

    let cmd_str = bytes_to_str(&cmd[..pos]);

    // Fork and exec
    let pid = fork();
    if pid == 0 {
        exec(cmd_str);
        exit(127);
    } else if pid > 0 {
        let mut status = 0;
        waitpid(pid, &mut status, 0);
        return status;
    }

    1
}

fn execute_cmd_batch(cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
                     cmd_lens: &[usize; MAX_ARGS],
                     cmd_count: usize,
                     args: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
                     arg_lens: &[usize; MAX_ARGS],
                     arg_count: usize) -> i32 {
    // Build command line
    let mut cmd = [0u8; MAX_CMD_LEN];
    let mut pos = 0;

    // Add command parts
    for i in 0..cmd_count {
        if pos > 0 && pos < MAX_CMD_LEN {
            cmd[pos] = b' ';
            pos += 1;
        }
        let len = cmd_lens[i].min(MAX_CMD_LEN - pos);
        cmd[pos..pos + len].copy_from_slice(&cmd_parts[i][..len]);
        pos += len;
    }

    // Add all arguments
    for i in 0..arg_count {
        if pos >= MAX_CMD_LEN - 1 {
            break;
        }
        cmd[pos] = b' ';
        pos += 1;
        let len = arg_lens[i].min(MAX_CMD_LEN - pos);
        cmd[pos..pos + len].copy_from_slice(&args[i][..len]);
        pos += len;
    }

    let cmd_str = bytes_to_str(&cmd[..pos]);

    // Fork and exec
    let pid = fork();
    if pid == 0 {
        exec(cmd_str);
        exit(127);
    } else if pid > 0 {
        let mut status = 0;
        waitpid(pid, &mut status, 0);
        return status;
    }

    1
}

fn bytes_to_str(bytes: &[u8]) -> &str {
    unsafe { core::str::from_utf8_unchecked(bytes) }
}

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
