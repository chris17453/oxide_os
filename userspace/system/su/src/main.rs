//! su — Switch User for OXIDE OS
//!
//! — ColdCipher: changes the effective UID/GID and execs a shell (or command).
//! Usage:
//!   su              — switch to root
//!   su username     — switch to username
//!   su -c command   — run command as root
//!
//! Also provides sudo functionality when invoked as "sudo":
//!   sudo command    — run command as root
//!
//! Password authentication: checks /etc/shadow (TODO — currently no password check)
//! For now, allows any switch if caller is root (euid=0). Non-root gets EPERM.

#![no_std]
#![no_main]

use libc::*;

/// Parse /etc/passwd to find a user's UID and GID by name.
/// Returns (uid, gid, home_dir, shell) or None.
fn lookup_user(name: &str) -> Option<(u32, u32)> {
    // — ColdCipher: read /etc/passwd, format: name:x:uid:gid:gecos:home:shell
    let fd = open2("/etc/passwd", 0); // O_RDONLY
    if fd < 0 { return None; }

    let mut buf = [0u8; 4096];
    let n = read(fd, &mut buf);
    close(fd);
    if n <= 0 { return None; }

    let data = &buf[..n as usize];
    let mut line_start = 0;

    while line_start < data.len() {
        // Find end of line
        let mut line_end = line_start;
        while line_end < data.len() && data[line_end] != b'\n' {
            line_end += 1;
        }

        let line = &data[line_start..line_end];
        // Parse name:x:uid:gid:...
        let mut fields = [0usize; 7]; // field start offsets
        let mut field_count = 0;
        fields[0] = 0;
        field_count = 1;

        for i in 0..line.len() {
            if line[i] == b':' && field_count < 7 {
                fields[field_count] = i + 1;
                field_count += 1;
            }
        }

        if field_count >= 4 {
            let field_name = &line[fields[0]..line.iter().position(|&c| c == b':').unwrap_or(line.len())];

            if field_name == name.as_bytes() {
                // Parse UID (field 2) and GID (field 3)
                let uid_end = if field_count > 3 { fields[3] - 1 } else { line.len() };
                let gid_end = if field_count > 4 { fields[4] - 1 } else { line.len() };

                let uid = parse_u32(&line[fields[2]..uid_end]);
                let gid = parse_u32(&line[fields[3]..gid_end]);

                if let (Some(u), Some(g)) = (uid, gid) {
                    return Some((u, g));
                }
            }
        }

        line_start = line_end + 1;
    }

    None
}

fn parse_u32(s: &[u8]) -> Option<u32> {
    let mut val = 0u32;
    for &b in s {
        if b < b'0' || b > b'9' { return None; }
        val = val * 10 + (b - b'0') as u32;
    }
    Some(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    let args: &[*const u8] = if argc > 0 && !argv.is_null() {
        unsafe { core::slice::from_raw_parts(argv, argc as usize) }
    } else {
        &[]
    };

    // — ColdCipher: determine if invoked as "su" or "sudo"
    let is_sudo = if args.len() > 0 {
        let arg0 = unsafe { cstr_to_str(args[0]) };
        arg0.ends_with("sudo")
    } else {
        false
    };

    // — ColdCipher: determine target user and command
    let (target_uid, target_gid) = if is_sudo {
        // sudo: always runs as root
        (0u32, 0u32)
    } else if args.len() > 1 {
        let target_name = unsafe { cstr_to_str(args[1]) };
        if target_name == "-c" {
            // su -c command: run as root
            (0, 0)
        } else {
            match lookup_user(target_name) {
                Some((u, g)) => (u, g),
                None => {
                    prints("su: user '");
                    prints(target_name);
                    prints("' not found\n");
                    return 1;
                }
            }
        }
    } else {
        // su with no args: switch to root
        (0, 0)
    };

    // — ColdCipher: check if we're allowed (must be root or correct password)
    // For now: only root can su. TODO: password authentication.
    let my_euid = syscall::sys_geteuid();
    if my_euid != 0 && target_uid != my_euid as u32 {
        prints("su: permission denied (must be root)\n");
        return 1;
    }

    // — ColdCipher: set UID/GID
    if syscall::sys_setgid(target_gid) < 0 {
        prints("su: setgid failed\n");
        return 1;
    }
    if syscall::sys_setuid(target_uid) < 0 {
        prints("su: setuid failed\n");
        return 1;
    }

    // — ColdCipher: exec the target command or shell
    if is_sudo && args.len() > 1 {
        // sudo command args...
        let cmd = unsafe { cstr_to_str(args[1]) };
        // Build argv for exec
        let exec_argv = &args[1..];
        let null_envp: [*const u8; 1] = [core::ptr::null()];
        syscall::sys_execve(cmd, exec_argv.as_ptr(), null_envp.as_ptr());
        prints("sudo: exec failed: ");
        prints(cmd);
        prints("\n");
        return 127;
    } else if !is_sudo && args.len() > 2 {
        let flag = unsafe { cstr_to_str(args[1]) };
        if flag == "-c" && args.len() > 2 {
            // su -c "command"
            let cmd = unsafe { cstr_to_str(args[2]) };
            let shell_argv: [*const u8; 4] = [
                b"/bin/esh\0".as_ptr(),
                b"-c\0".as_ptr(),
                args[2],
                core::ptr::null(),
            ];
            let null_envp: [*const u8; 1] = [core::ptr::null()];
            syscall::sys_execve("/bin/esh", shell_argv.as_ptr(), null_envp.as_ptr());
            prints("su: exec failed\n");
            return 127;
        }
    }

    // Default: exec a shell
    let shell = b"/bin/esh\0";
    let shell_argv: [*const u8; 2] = [shell.as_ptr(), core::ptr::null()];
    let null_envp: [*const u8; 1] = [core::ptr::null()];
    syscall::sys_execve("/bin/esh", shell_argv.as_ptr(), null_envp.as_ptr());
    prints("su: exec shell failed\n");
    127
}

unsafe fn cstr_to_str(p: *const u8) -> &'static str {
    if p.is_null() { return ""; }
    let mut len = 0;
    while *p.add(len) != 0 { len += 1; }
    core::str::from_utf8_unchecked(core::slice::from_raw_parts(p, len))
}
