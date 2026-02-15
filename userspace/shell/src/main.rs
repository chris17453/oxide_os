//! OXIDE Shell (esh)
//!
//! A simple shell for OXIDE OS with:
//! - Command execution
//! - Builtin commands (cd, exit, echo, pwd, export, unset, help, theme)
//! - I/O redirection (<, >, >>)
//! - Pipes (|)
//! - Background jobs (&)
//! - Tab completion for commands and files
//! - Color themes (VIM-style)

#![no_std]
#![no_main]
#![allow(unused)]

mod theme;

use libc::*;
use theme::{
    Color, ColorSpec, THEME_NAMES, Theme, colors_enabled, current_theme, load_theme,
    set_colors_enabled, set_theme,
};

/// Maximum command line length
const MAX_LINE: usize = 256;

/// Maximum number of arguments
const MAX_ARGS: usize = 32;

/// Maximum number of pipe stages
const MAX_PIPES: usize = 8;

/// Maximum completions to show
const MAX_COMPLETIONS: usize = 64;

/// Maximum number of aliases
const MAX_ALIASES: usize = 64;

/// Default prompt config paths
const GLOBAL_PROMPT_PATH: &str = "/etc/prompt";
const USER_PROMPT_FILENAME: &str = ".esh_prompt";
/// Fallback prompt string (POSIX-ish)
const DEFAULT_PROMPT: &str = "\\u@\\h:\\w$ ";

/// Alias entry
struct Alias {
    name: [u8; 32],
    value: [u8; 128],
    used: bool,
}

impl Alias {
    const fn new() -> Self {
        Alias {
            name: [0u8; 32],
            value: [0u8; 128],
            used: false,
        }
    }
}

/// Cached prompt template — determined once at startup via file I/O,
/// then reused on every prompt to avoid repeated ext4 directory lookups
/// (which are expensive: synchronous VirtIO polling with no block cache).
static mut PROMPT_TEMPLATE: [u8; 256] = [0; 256];
static mut PROMPT_TEMPLATE_LEN: usize = 0;
static mut PROMPT_TEMPLATE_LOADED: bool = false;

/// Determine the prompt template string (file I/O, runs once).
///
/// Hierarchy:
/// 1. $ESH_PROMPT environment variable
/// 2. ~/.esh_prompt if readable
/// 3. /etc/prompt if readable
/// 4. DEFAULT_PROMPT
fn load_prompt_template() {
    fn load_file(path: &str, buf: &mut [u8]) -> Option<usize> {
        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            return None;
        }
        let n = read(fd, buf);
        close(fd);
        if n > 0 { Some(n as usize) } else { None }
    }

    let tpl = unsafe { &raw mut PROMPT_TEMPLATE };
    let tpl_buf: &mut [u8; 256] = unsafe { &mut *tpl };

    // 1) ESH_PROMPT env
    if let Some(prompt) = getenv("ESH_PROMPT") {
        let len = prompt.len().min(255);
        tpl_buf[..len].copy_from_slice(&prompt.as_bytes()[..len]);
        unsafe {
            PROMPT_TEMPLATE_LEN = len;
        }
        return;
    }

    // 2) User prompt file (~/.esh_prompt)
    let home = getenv("HOME");
    if let Some(home_str) = home {
        if !home_str.is_empty() {
            let mut path_buf = [0u8; 256];
            let mut idx = 0;
            for b in home_str.as_bytes() {
                if idx < path_buf.len() - 1 {
                    path_buf[idx] = *b;
                    idx += 1;
                }
            }
            if idx < path_buf.len() - 1 {
                path_buf[idx] = b'/';
                idx += 1;
            }
            for b in USER_PROMPT_FILENAME.as_bytes() {
                if idx < path_buf.len() - 1 {
                    path_buf[idx] = *b;
                    idx += 1;
                }
            }
            path_buf[idx] = 0;

            let path = bytes_to_str(&path_buf);
            let mut file_buf = [0u8; 256];
            if let Some(n) = load_file(path, &mut file_buf) {
                let len = n.min(255);
                tpl_buf[..len].copy_from_slice(&file_buf[..len]);
                unsafe {
                    PROMPT_TEMPLATE_LEN = len;
                }
                return;
            }
        }
    }

    // 3) Global prompt file (/etc/prompt)
    let mut file_buf = [0u8; 256];
    if let Some(n) = load_file(GLOBAL_PROMPT_PATH, &mut file_buf) {
        let len = n.min(255);
        tpl_buf[..len].copy_from_slice(&file_buf[..len]);
        unsafe {
            PROMPT_TEMPLATE_LEN = len;
        }
        return;
    }

    // 4) Fallback
    let len = DEFAULT_PROMPT.len().min(255);
    tpl_buf[..len].copy_from_slice(&DEFAULT_PROMPT.as_bytes()[..len]);
    unsafe {
        PROMPT_TEMPLATE_LEN = len;
    }
}

/// Build a NUL-terminated prompt string.
///
/// The template is cached after the first call (file I/O happens once).
/// Only render_prompt() runs each iteration (in-memory substitution + getcwd).
fn get_prompt_string() -> &'static [u8] {
    static mut PROMPT_RESULT: [u8; 256] = [0; 256];

    // Load template once (all file I/O happens here)
    unsafe {
        if !PROMPT_TEMPLATE_LOADED {
            load_prompt_template();
            PROMPT_TEMPLATE_LOADED = true;
        }
    }

    let template = unsafe { &PROMPT_TEMPLATE[..PROMPT_TEMPLATE_LEN] };
    let s = render_prompt(template);
    unsafe {
        let len = s.len().min(255);
        PROMPT_RESULT[..len].copy_from_slice(&s.as_bytes()[..len]);
        PROMPT_RESULT[len] = 0;
        &PROMPT_RESULT[..len + 1]
    }
}

/// Render a prompt template supporting minimal escapes:
/// \u user, \h host, \w cwd, \\ literal backslash, \$ root/user sign
fn render_prompt(template: &[u8]) -> &str {
    // Static scratch buffer
    static mut PROMPT_OUT: [u8; 256] = [0; 256];
    let out_ptr = &raw mut PROMPT_OUT;
    let out: &mut [u8] = unsafe { &mut *out_ptr };
    let mut len = 0usize;

    let user = getenv("USER").unwrap_or("user");
    let host = getenv("HOSTNAME").unwrap_or("oxide");
    let cwd = {
        static mut CWD_BUF: [u8; 128] = [0; 128];
        let buf_ptr = &raw mut CWD_BUF;
        let buf: &mut [u8; 128] = unsafe { &mut *buf_ptr };
        let res = getcwd(buf);
        if res < 0 { "/" } else { bytes_to_str(buf) }
    };
    let is_root = geteuid() == 0;

    let mut i = 0;
    while i < template.len() && len + 1 < out.len() {
        let b = template[i];
        if b == b'\\' && i + 1 < template.len() {
            let esc = template[i + 1];
            match esc {
                b'u' => len += push_str(out, len, user),
                b'h' => len += push_str(out, len, host),
                b'w' => len += push_str(out, len, cwd),
                b'\\' => {
                    out[len] = b'\\';
                    len += 1;
                }
                b'$' => {
                    out[len] = if is_root { b'#' } else { b'$' };
                    len += 1;
                }
                _ => {
                    // Unknown escape, emit literally
                    out[len] = b'\\';
                    len += 1;
                    if len < out.len() {
                        out[len] = esc;
                        len += 1;
                    }
                }
            }
            i += 2;
        } else {
            out[len] = b;
            len += 1;
            i += 1;
        }
    }
    if len >= out.len() {
        len = out.len() - 1;
    }
    out[len] = 0;
    unsafe { core::str::from_utf8_unchecked(&out[..len]) }
}

/// Copy a string into buffer starting at offset, returns bytes written
fn push_str(out: &mut [u8], offset: usize, s: &str) -> usize {
    let mut written = 0;
    for &b in s.as_bytes() {
        if offset + written >= out.len() - 1 {
            break;
        }
        out[offset + written] = b;
        written += 1;
    }
    written
}

/// Print a C-style string without allocation
unsafe fn prints_raw(ptr: *const i8) {
    unsafe {
        let mut p = ptr;
        while *p != 0 {
            putchar(*p as u8);
            p = p.add(1);
        }
    }
}

/// Convert C string pointer to &str
unsafe fn cstr_to_str<'a>(ptr: *const i8) -> &'a str {
    unsafe {
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr as *const u8, len))
    }
}

/// Expand leading ~ or ~/ using $HOME into the provided buffer and return &str
fn expand_tilde_to_buf<'a>(input: &[u8], out: &'a mut [u8]) -> &'a str {
    // Determine input length (stop at NUL)
    let mut in_len = 0usize;
    while in_len < input.len() && input[in_len] != 0 {
        in_len += 1;
    }

    let tilde_prefix = in_len > 0 && input[0] == b'~' && (in_len == 1 || input[1] == b'/');

    if tilde_prefix {
        let home = getenv("HOME").unwrap_or("/");

        let mut o = 0usize;
        for &b in home.as_bytes() {
            if o >= out.len() - 1 {
                break;
            }
            out[o] = b;
            o += 1;
        }

        if in_len > 1 {
            if o > 0 && out[o - 1] != b'/' && o < out.len() - 1 {
                out[o] = b'/';
                o += 1;
            }
            for i in 2..in_len {
                if o >= out.len() - 1 {
                    break;
                }
                out[o] = input[i];
                o += 1;
            }
        }

        if o >= out.len() {
            o = out.len() - 1;
        }
        out[o] = 0;
        return bytes_to_str(out);
    }

    // Fallback: copy input as-is
    let mut o = 0usize;
    for i in 0..in_len {
        if o >= out.len() - 1 {
            break;
        }
        out[o] = input[i];
        o += 1;
    }
    out[o] = 0;
    bytes_to_str(out)
}

/// Shell state
struct ShellState {
    /// Command aliases
    aliases: [Alias; MAX_ALIASES],
    /// Last exit status
    last_status: i32,
    /// Current umask
    umask: u32,
    /// Positional parameters ($1, $2, etc.)
    positional: [[u8; 64]; MAX_ARGS],
    /// Number of positional parameters
    positional_count: usize,
}

impl ShellState {
    const fn new() -> Self {
        const EMPTY_ALIAS: Alias = Alias::new();
        const EMPTY_ARG: [u8; 64] = [0u8; 64];
        ShellState {
            aliases: [EMPTY_ALIAS; MAX_ALIASES],
            last_status: 0,
            umask: 0o022,
            positional: [EMPTY_ARG; MAX_ARGS],
            positional_count: 0,
        }
    }
}

/// Global shell state
static mut SHELL: ShellState = ShellState::new();

/// Get shell state
fn shell() -> &'static mut ShellState {
    unsafe { &mut *core::ptr::addr_of_mut!(SHELL) }
}

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

/// Directory entry header (matches kernel's UserDirEntry)
/// MUST be packed to match kernel's packed struct (19 bytes, not 24)
#[repr(C, packed)]
struct DirEntry {
    d_ino: u64,
    d_off: u64,
    d_reclen: u16,
    d_type: u8,
}

const DT_DIR: u8 = 4;

/// Shell completion callback for readline.
///
/// Called by readline when the user presses TAB. Returns a NULL-terminated
/// array of matches (first element = longest common prefix), or NULL.
unsafe extern "C" fn shell_completion(text: *const u8, start: i32, _end: i32) -> *mut *mut u8 {
    // SAFETY: This is a readline C callback; all pointer/raw operations here are part of
    // readline's completion protocol. Caller (readline) ensures text pointer validity.
    // — ThreadRogue
    unsafe {
        // Suppress default filename completion — we handle it ourselves
        libc::readline::rl_attempted_completion_over = 1;

        let text_len = libc::string::strlen(text);
        let prefix = core::slice::from_raw_parts(text, text_len);

        // Determine if we're completing a command (first word) or a path
        let is_first_word = start == 0;

        // Check if prefix contains a / (path completion even for first word)
        let has_slash = prefix.iter().any(|&c| c == b'/');

        let mut completions: [[u8; 64]; MAX_COMPLETIONS] = [[0u8; 64]; MAX_COMPLETIONS];
        let num_completions;

        if is_first_word && !has_slash {
            num_completions = complete_commands(prefix, text_len, &mut completions);
        } else {
            num_completions = complete_paths(prefix, text_len, &mut completions);
        }

        if num_completions == 0 {
            return core::ptr::null_mut();
        }

        // Build the matches array in readline's format:
        // matches[0] = longest common prefix (LCD)
        // matches[1..N] = individual matches
        // matches[N+1] = NULL

        let array_size = (num_completions + 2) * core::mem::size_of::<*mut u8>();
        let array = libc::c_exports::malloc(array_size) as *mut *mut u8;
        if array.is_null() {
            return core::ptr::null_mut();
        }

        // Find LCD (longest common prefix)
        let first_len = bytes_len(&completions[0]);
        let mut lcd_len = first_len;
        for i in 1..num_completions {
            let other_len = bytes_len(&completions[i]);
            let mut j = 0;
            while j < lcd_len && j < other_len && completions[0][j] == completions[i][j] {
                j += 1;
            }
            lcd_len = j;
        }

        // Allocate LCD string
        let lcd = libc::c_exports::malloc(lcd_len + 1);
        if !lcd.is_null() {
            core::ptr::copy_nonoverlapping(completions[0].as_ptr(), lcd, lcd_len);
            *lcd.add(lcd_len) = 0;
        }
        *array.add(0) = lcd;

        // Copy individual matches
        for i in 0..num_completions {
            let mlen = bytes_len(&completions[i]);
            // Check if this is a directory — byte 63 stores d_type
            let is_dir = completions[i][63] == DT_DIR;

            let alloc_len = if is_dir { mlen + 2 } else { mlen + 1 };
            let m = libc::c_exports::malloc(alloc_len);
            if !m.is_null() {
                core::ptr::copy_nonoverlapping(completions[i].as_ptr(), m, mlen);
                if is_dir {
                    *m.add(mlen) = b'/';
                    *m.add(mlen + 1) = 0;
                } else {
                    *m.add(mlen) = 0;
                }
            }
            *array.add(i + 1) = m;
        }
        *array.add(num_completions + 1) = core::ptr::null_mut();

        // For single completions, readline appends a space by default which is fine
        // For directory completions we already added '/', suppress the extra space
        if num_completions == 1 && completions[0][63] == DT_DIR {
            libc::readline::rl_completion_suppress_append = 1;
        } else {
            libc::readline::rl_completion_suppress_append = 0;
        }

        array
    }
}

/// Main shell entry point
#[unsafe(no_mangle)]
fn main() -> i32 {
    // Put shell in its own process group
    setpgid(0, 0);

    // Make this shell the foreground process group of the controlling terminal
    tcsetpgrp(0, getpid());

    // Ignore SIGINT (Ctrl+C) in the shell itself
    // Child processes will inherit default SIGINT behavior
    signal(SIGINT, SIG_IGN);

    // — GraveShift: cursor escape codes removed. The GOP framebuffer shows the
    // terminal cursor via paint_cursor() — no need for DEC Private Mode toggles
    // that can trigger blink timer contention with the TERMINAL spinlock.

    // Register tab completion callback with readline
    unsafe {
        libc::readline::rl_attempted_completion_function = Some(shell_completion);
    }

    // Source system profile to set PATH and other environment variables
    source_profile(b"/etc/profile\0");

    // Load theme from environment variable if set
    if let Some(theme_name) = getenv("ESH_THEME") {
        load_theme(theme_name.as_bytes());
    }

    // Check if NO_COLOR is set (standard for disabling colors)
    if getenv("NO_COLOR").is_some() {
        set_colors_enabled(false);
    }

    // Print welcome message
    printlns("OXIDE Shell (esh)");
    printlns("Type 'help' for available commands");
    printlns("");

    // Main shell loop
    loop {
        // Build prompt string (NUL-terminated)
        let prompt = get_prompt_string();

        // Read a line using readline (handles raw mode, editing, history nav)
        let line_ptr = unsafe { libc::readline::readline(prompt.as_ptr()) };

        if line_ptr.is_null() {
            // EOF (Ctrl-D on empty line)
            printlns("");
            break;
        }

        // Compute length using volatile reads. With opt-level=z + LTO, the
        // compiler can inline through readline/malloc/strlen and determine that
        // the buffer came from zeroed memory, optimizing strlen to return 0.
        // Volatile reads prevent this by forcing actual memory loads.
        let line_len = unsafe {
            let mut n = 0usize;
            while core::ptr::read_volatile(line_ptr.add(n)) != 0 {
                n += 1;
            }
            n
        };
        let line_slice = unsafe { core::slice::from_raw_parts(line_ptr, line_len) };

        // Skip empty lines
        let cmd = trim(line_slice);
        if cmd.is_empty() {
            unsafe { libc::c_exports::free(line_ptr) };
            continue;
        }

        // Add to readline's history (deduplication handled internally)
        unsafe { libc::readline::add_history(line_ptr) };

        // Execute command
        execute_line(cmd);

        // Free the malloc'd line
        unsafe { libc::c_exports::free(line_ptr) };
    }

    0
}

/// Get length of null-terminated byte string
fn bytes_len(s: &[u8]) -> usize {
    for i in 0..s.len() {
        if s[i] == 0 {
            return i;
        }
    }
    s.len()
}

/// Check if a prefix matches start of a name
fn prefix_matches(prefix: &[u8], prefix_len: usize, name: &[u8]) -> bool {
    for i in 0..prefix_len {
        if i >= name.len() || name[i] == 0 || prefix[i] != name[i] {
            return false;
        }
    }
    true
}

/// Builtin command names for tab completion
const BUILTINS: &[&[u8]] = &[
    b".",
    b":",
    b"[",
    b"alias",
    b"bg",
    b"builtin",
    b"cd",
    b"colors",
    b"command",
    b"declare",
    b"echo",
    b"eval",
    b"exec",
    b"exit",
    b"export",
    b"false",
    b"fg",
    b"getopts",
    b"help",
    b"history",
    b"jobs",
    b"kill",
    b"let",
    b"local",
    b"printf",
    b"pwd",
    b"read",
    b"readonly",
    b"set",
    b"shift",
    b"source",
    b"test",
    b"theme",
    b"true",
    b"type",
    b"umask",
    b"unalias",
    b"unset",
    b"wait",
];

/// Complete commands from /bin directory and builtins
fn complete_commands(
    prefix: &[u8],
    prefix_len: usize,
    completions: &mut [[u8; 64]; MAX_COMPLETIONS],
) -> usize {
    let mut count = 0;

    // First, add matching builtins
    for builtin in BUILTINS {
        if count >= MAX_COMPLETIONS {
            break;
        }
        if prefix_matches(prefix, prefix_len, builtin) {
            let mut i = 0;
            while i < 63 && i < builtin.len() && builtin[i] != 0 {
                completions[count][i] = builtin[i];
                i += 1;
            }
            completions[count][i] = 0;
            // Mark as non-directory (byte 63 = 0, which != DT_DIR)
            completions[count][63] = 0;
            count += 1;
        }
    }

    // Then add commands from /bin
    let fd = open2("/bin", O_RDONLY | O_DIRECTORY);
    if fd < 0 {
        return count;
    }

    let mut dir_buf = [0u8; 2048];

    loop {
        let n = sys_getdents(fd, &mut dir_buf);
        if n <= 0 {
            break;
        }

        let mut offset = 0;
        while offset < n as usize && count < MAX_COMPLETIONS {
            let entry_ptr = dir_buf.as_ptr().wrapping_add(offset) as *const DirEntry;
            let entry = unsafe { &*entry_ptr };

            let name_offset = offset + core::mem::size_of::<DirEntry>();
            let name = &dir_buf[name_offset..];

            // Skip . and ..
            if name[0] == b'.' {
                offset += entry.d_reclen as usize;
                continue;
            }

            // Check if prefix matches
            if prefix_matches(prefix, prefix_len, name) {
                // Check if already added as builtin
                let mut already_added = false;
                for i in 0..count {
                    if bytes_eq_raw(&completions[i], name) {
                        already_added = true;
                        break;
                    }
                }

                if !already_added {
                    // Copy name to completions
                    let mut i = 0;
                    while i < 63 && name[i] != 0 {
                        completions[count][i] = name[i];
                        i += 1;
                    }
                    completions[count][i] = 0;
                    // Mark as non-directory
                    completions[count][63] = 0;
                    count += 1;
                }
            }

            offset += entry.d_reclen as usize;
        }
    }

    close(fd);
    count
}

/// Compare two byte slices for equality (raw, null-terminated)
fn bytes_eq_raw(a: &[u8], b: &[u8]) -> bool {
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

/// Complete file paths
fn complete_paths(
    prefix: &[u8],
    prefix_len: usize,
    completions: &mut [[u8; 64]; MAX_COMPLETIONS],
) -> usize {
    // Find directory and filename parts
    let mut last_slash = 0;
    let mut has_slash = false;
    for i in 0..prefix_len {
        if prefix[i] == b'/' {
            last_slash = i;
            has_slash = true;
        }
    }

    let (dir_path, file_prefix, file_prefix_len) = if has_slash {
        let dir_end = last_slash + 1;
        (
            &prefix[..dir_end],
            &prefix[dir_end..prefix_len],
            prefix_len - dir_end,
        )
    } else {
        (b".".as_slice(), prefix, prefix_len)
    };

    // Build null-terminated directory path
    let mut dir_buf = [0u8; 128];
    let dir_len = dir_path.len().min(127);
    dir_buf[..dir_len].copy_from_slice(&dir_path[..dir_len]);
    dir_buf[dir_len] = 0;

    let dir_str = bytes_to_str(&dir_buf);
    let fd = open2(dir_str, O_RDONLY | O_DIRECTORY);
    if fd < 0 {
        return 0;
    }

    let mut count = 0;
    let mut read_buf = [0u8; 2048];

    loop {
        let n = sys_getdents(fd, &mut read_buf);
        if n <= 0 {
            break;
        }

        let mut offset = 0;
        while offset < n as usize && count < MAX_COMPLETIONS {
            let entry_ptr = read_buf.as_ptr().wrapping_add(offset) as *const DirEntry;
            let entry = unsafe { &*entry_ptr };

            let name_offset = offset + core::mem::size_of::<DirEntry>();
            let name = &read_buf[name_offset..];

            // Skip . and ..
            if name[0] == b'.' && (name[1] == 0 || (name[1] == b'.' && name[2] == 0)) {
                offset += entry.d_reclen as usize;
                continue;
            }

            // Check if file prefix matches
            if prefix_matches(file_prefix, file_prefix_len, name) {
                // Build full path for completion
                let mut i = 0;

                // Copy directory path if present
                if has_slash {
                    for j in 0..dir_len {
                        if i < 63 {
                            completions[count][i] = dir_buf[j];
                            i += 1;
                        }
                    }
                }

                // Copy filename
                let mut j = 0;
                while j < name.len() && name[j] != 0 && i < 63 {
                    completions[count][i] = name[j];
                    i += 1;
                    j += 1;
                }
                completions[count][i] = 0;

                // Mark directories with trailing indicator in d_type
                // Store d_type at position 63 (last byte) for later checking
                completions[count][63] = entry.d_type;

                count += 1;
            }

            offset += entry.d_reclen as usize;
        }
    }

    close(fd);
    count
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

    // Single command without pipes - check for builtin (apply redirections)
    if num_commands == 1 && is_builtin(&commands[0].args[0]) {
        execute_builtin_with_redirs(&commands[0]);
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
                eprintlns("esh: too many pipes");
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
            while i < line.len()
                && line[i] != 0
                && line[i] != b' '
                && line[i] != b'\t'
                && line[i] != b'<'
                && line[i] != b'>'
                && j < 63
            {
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
            while i < line.len()
                && line[i] != 0
                && line[i] != b' '
                && line[i] != b'\t'
                && line[i] != b'<'
                && line[i] != b'>'
                && j < 63
            {
                cmd.output_file[j] = line[i];
                j += 1;
                i += 1;
            }
            cmd.output_file[j] = 0;
        } else {
            // Regular argument (handle quotes)
            if cmd.argc >= MAX_ARGS {
                break;
            }
            let mut j = 0;
            let mut in_quote = false;
            let mut quote_char = 0u8;

            while i < line.len() && line[i] != 0 && j < 63 {
                let ch = line[i];

                // Check for quote start/end
                if !in_quote && (ch == b'"' || ch == b'\'') {
                    // Start quoted section
                    in_quote = true;
                    quote_char = ch;
                    i += 1;
                    continue;
                } else if in_quote && ch == quote_char {
                    // End quoted section
                    in_quote = false;
                    i += 1;
                    continue;
                }

                // If not in quote, check for terminators
                if !in_quote {
                    if ch == b' ' || ch == b'\t' || ch == b'<' || ch == b'>' || ch == b'|' {
                        break;
                    }
                }

                // Copy character
                cmd.args[cmd.argc][j] = ch;
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
            eprintlns("esh: pipe failed");
            return;
        }
    }

    let mut pids = [0i32; MAX_PIPES];
    let mut pgid: i32 = 0; // Process group for the pipeline

    for i in 0..num_commands {
        let pid = fork();
        if pid == 0 {
            // Child process

            // Put child in its own process group (first child becomes leader)
            if i == 0 {
                setpgid(0, 0); // Make this child the process group leader
            } else {
                setpgid(0, pids[0]); // Join the first child's process group
            }

            // Make this process group the foreground group (for signals like Ctrl+C)
            if !background {
                tcsetpgrp(0, getpid()); // stdin is fd 0
            }

            // Setup input
            if i > 0 {
                // Read from previous pipe
                dup2(pipes[i - 1][0], 0);
            } else if let Redirect::Input = commands[i].input_redir {
                // Redirect from file
                let expanded = expand_path(&commands[i].input_file);
                let path = bytes_to_str(&expanded);
                let fd = open2(path, O_RDONLY);
                if fd < 0 {
                    eprints("esh: ");
                    print_bytes(&commands[i].input_file);
                    eprintlns(": No such file");
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
                        let expanded = expand_path(&commands[i].output_file);
                        let path = bytes_to_str(&expanded);
                        let fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
                        if fd < 0 {
                            eprints("esh: ");
                            print_bytes(&commands[i].output_file);
                            eprintlns(": Cannot create file");
                            _exit(1);
                        }
                        dup2(fd, 1);
                        close(fd);
                    }
                    Redirect::Append => {
                        let expanded = expand_path(&commands[i].output_file);
                        let path = bytes_to_str(&expanded);
                        let fd = open(path, O_WRONLY | O_CREAT | O_APPEND, 0o644);
                        if fd < 0 {
                            eprints("esh: ");
                            print_bytes(&commands[i].output_file);
                            eprintlns(": Cannot create file");
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

            // Parent: set up process group
            if i == 0 {
                pgid = pid; // First child is the process group leader
                setpgid(pid, pid); // Ensure it's in its own group

                // Make this the foreground process group if not background
                if !background {
                    tcsetpgrp(0, pgid);
                }
            } else {
                setpgid(pid, pgid); // Join the process group
            }
        } else {
            eprintlns("esh: fork failed");
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

        // Restore shell as foreground process group
        tcsetpgrp(0, getpid());
    } else {
        prints("[");
        print_i64(pids[num_commands - 1] as i64);
        printlns("]");
    }
}

/// Check if command is a builtin
fn is_builtin(cmd: &[u8]) -> bool {
    bytes_eq(cmd, b"cd")
        || bytes_eq(cmd, b"exit")
        || bytes_eq(cmd, b"echo")
        || bytes_eq(cmd, b"pwd")
        || bytes_eq(cmd, b"help")
        || bytes_eq(cmd, b"export")
        || bytes_eq(cmd, b"unset")
        || bytes_eq(cmd, b"true")
        || bytes_eq(cmd, b"false")
        || bytes_eq(cmd, b":")
        || bytes_eq(cmd, b"[")
        || bytes_eq(cmd, b"test")
        || bytes_eq(cmd, b"source")
        || bytes_eq(cmd, b".")
        || bytes_eq(cmd, b"read")
        || bytes_eq(cmd, b"printf")
        || bytes_eq(cmd, b"alias")
        || bytes_eq(cmd, b"unalias")
        || bytes_eq(cmd, b"type")
        || bytes_eq(cmd, b"command")
        || bytes_eq(cmd, b"builtin")
        || bytes_eq(cmd, b"set")
        || bytes_eq(cmd, b"shift")
        || bytes_eq(cmd, b"local")
        || bytes_eq(cmd, b"declare")
        || bytes_eq(cmd, b"readonly")
        || bytes_eq(cmd, b"let")
        || bytes_eq(cmd, b"exec")
        || bytes_eq(cmd, b"eval")
        || bytes_eq(cmd, b"umask")
        || bytes_eq(cmd, b"jobs")
        || bytes_eq(cmd, b"fg")
        || bytes_eq(cmd, b"bg")
        || bytes_eq(cmd, b"wait")
        || bytes_eq(cmd, b"kill")
        || bytes_eq(cmd, b"history")
        || bytes_eq(cmd, b"getopts")
        || bytes_eq(cmd, b"theme")
        || bytes_eq(cmd, b"colors")
}

/// Execute a builtin command with optional redirections applied
fn execute_builtin_with_redirs(cmd: &Command) {
    // Handle input redirection
    let mut saved_stdin: Option<i32> = None;
    if let Redirect::Input = cmd.input_redir {
        let path = bytes_to_str(&cmd.input_file);
        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprints("esh: ");
            print_bytes(&cmd.input_file);
            eprintlns(": No such file");
            shell().last_status = 1;
            return;
        }
        let backup = dup(0);
        if backup >= 0 {
            saved_stdin = Some(backup);
            dup2(fd, 0);
        }
        close(fd);
    }

    // Handle output redirection
    let mut saved_stdout: Option<i32> = None;
    match cmd.output_redir {
        Redirect::Output => {
            let expanded = expand_path(&cmd.output_file);
            let path = bytes_to_str(&expanded);
            let fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
            if fd < 0 {
                eprints("esh: ");
                print_bytes(&cmd.output_file);
                eprintlns(": Cannot create file");
                shell().last_status = 1;
                return;
            }
            let backup = dup(1);
            if backup >= 0 {
                saved_stdout = Some(backup);
                dup2(fd, 1);
            }
            close(fd);
        }
        Redirect::Append => {
            let expanded = expand_path(&cmd.output_file);
            let path = bytes_to_str(&expanded);
            let fd = open(path, O_WRONLY | O_CREAT | O_APPEND, 0o644);
            if fd < 0 {
                eprints("esh: ");
                print_bytes(&cmd.output_file);
                eprintlns(": Cannot create file");
                shell().last_status = 1;
                return;
            }
            let backup = dup(1);
            if backup >= 0 {
                saved_stdout = Some(backup);
                dup2(fd, 1);
            }
            close(fd);
        }
        _ => {}
    }

    execute_builtin(cmd);

    // Restore fds
    if let Some(fd) = saved_stdin {
        dup2(fd, 0);
        close(fd);
    }
    if let Some(fd) = saved_stdout {
        dup2(fd, 1);
        close(fd);
    }
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
                prints(" ");
            }
            print_bytes(&cmd.args[i]);
        }
        printlns("");
    } else if bytes_eq(&cmd.args[0], b"cd") {
        if cmd.argc < 2 {
            eprintlns("esh: cd: missing argument");
        } else {
            let expanded = expand_path(&cmd.args[1]);
            let path = bytes_to_str(&expanded);
            if chdir(path) < 0 {
                eprints("esh: cd: ");
                print_bytes(&cmd.args[1]);
                eprintlns(": No such directory");
            }
        }
    } else if bytes_eq(&cmd.args[0], b"pwd") {
        let mut buf = [0u8; 256];
        if getcwd(&mut buf) >= 0 {
            print_bytes(&buf);
            printlns("");
        } else {
            eprintlns("esh: pwd: failed");
        }
    } else if bytes_eq(&cmd.args[0], b"help") {
        printlns("OXIDE Shell (esh) - Built-in commands:");
        printlns("  echo [args...]  - Print arguments");
        printlns("  cd <dir>        - Change directory");
        printlns("  pwd             - Print working directory");
        printlns("  export          - List all environment variables");
        printlns("  export VAR=val  - Set environment variable");
        printlns("  export VAR      - Export variable (set empty if unset)");
        printlns("  unset VAR...    - Unset environment variable(s)");
        printlns("  exit [code]     - Exit shell");
        printlns("  help            - Show this help");
        printlns("");
        printlns("Color Themes:");
        printlns("  theme           - Show current theme");
        printlns("  theme <name>    - Set color theme");
        printlns("  theme list      - List available themes");
        printlns("  colors on|off   - Enable/disable colors");
        printlns("  colors test     - Show color test pattern");
        printlns("");
        printlns("I/O Redirection:");
        printlns("  cmd < file      - Read input from file");
        printlns("  cmd > file      - Write output to file");
        printlns("  cmd >> file     - Append output to file");
        printlns("");
        printlns("Pipes:");
        printlns("  cmd1 | cmd2     - Pipe output of cmd1 to cmd2");
        printlns("");
        printlns("Background:");
        printlns("  cmd &           - Run command in background");
        printlns("");
        printlns("Tab Completion:");
        printlns("  Press TAB to complete commands and file paths");
    } else if bytes_eq(&cmd.args[0], b"true") {
        // Do nothing, exit 0
    } else if bytes_eq(&cmd.args[0], b"false") {
        // Note: this won't affect exit code in builtins
    } else if bytes_eq(&cmd.args[0], b"export") {
        if cmd.argc < 2 {
            // List all environment variables (like Linux export without args)
            env_iter(|name, value| {
                prints("export ");
                print_bytes(name);
                prints("=\"");
                print_bytes(value);
                printlns("\"");
            });
        } else {
            // Parse VAR=value or just VAR
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
                // VAR=value form
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
                    eprintlns("esh: export: failed");
                }
            } else {
                // Just VAR - check if it exists, if not set to empty
                let name_str = bytes_to_str(arg);
                if getenv(name_str).is_none() {
                    // Set to empty string (like bash does for export VAR)
                    if setenv(name_str, "") < 0 {
                        eprintlns("esh: export: failed");
                    }
                }
                // If it exists, it's already exported (we don't have separate export flag)
            }
        }
    } else if bytes_eq(&cmd.args[0], b"unset") {
        if cmd.argc < 2 {
            eprintlns("esh: unset: not enough arguments");
        } else {
            for i in 1..cmd.argc {
                let name_str = bytes_to_str(&cmd.args[i]);
                unsetenv(name_str);
            }
        }
    } else if bytes_eq(&cmd.args[0], b":") {
        // Null command - do nothing, success
        shell().last_status = 0;
    } else if bytes_eq(&cmd.args[0], b"[") || bytes_eq(&cmd.args[0], b"test") {
        shell().last_status = builtin_test(cmd);
    } else if bytes_eq(&cmd.args[0], b"source") || bytes_eq(&cmd.args[0], b".") {
        if cmd.argc < 2 {
            eprintlns("esh: source: filename argument required");
            shell().last_status = 1;
        } else {
            shell().last_status = builtin_source(&cmd.args[1]);
        }
    } else if bytes_eq(&cmd.args[0], b"read") {
        shell().last_status = builtin_read(cmd);
    } else if bytes_eq(&cmd.args[0], b"printf") {
        shell().last_status = builtin_printf(cmd);
    } else if bytes_eq(&cmd.args[0], b"alias") {
        shell().last_status = builtin_alias(cmd);
    } else if bytes_eq(&cmd.args[0], b"unalias") {
        shell().last_status = builtin_unalias(cmd);
    } else if bytes_eq(&cmd.args[0], b"type") {
        shell().last_status = builtin_type(cmd);
    } else if bytes_eq(&cmd.args[0], b"command") {
        // Run command bypassing aliases and functions
        if cmd.argc < 2 {
            shell().last_status = 0;
        } else {
            // Create a new command without the 'command' prefix
            let mut new_cmd = Command::new();
            for i in 1..cmd.argc {
                new_cmd.args[i - 1] = cmd.args[i];
            }
            new_cmd.argc = cmd.argc - 1;
            execute_external(&new_cmd);
        }
    } else if bytes_eq(&cmd.args[0], b"builtin") {
        // Run builtin directly
        if cmd.argc < 2 {
            shell().last_status = 0;
        } else {
            let mut new_cmd = Command::new();
            for i in 1..cmd.argc {
                new_cmd.args[i - 1] = cmd.args[i];
            }
            new_cmd.argc = cmd.argc - 1;
            if is_builtin(&new_cmd.args[0]) {
                execute_builtin(&new_cmd);
            } else {
                eprints("esh: builtin: ");
                print_bytes(&cmd.args[1]);
                eprintlns(": not a shell builtin");
                shell().last_status = 1;
            }
        }
    } else if bytes_eq(&cmd.args[0], b"set") {
        shell().last_status = builtin_set(cmd);
    } else if bytes_eq(&cmd.args[0], b"shift") {
        shell().last_status = builtin_shift(cmd);
    } else if bytes_eq(&cmd.args[0], b"local") || bytes_eq(&cmd.args[0], b"declare") {
        // For now, local/declare just sets variables (no function scope yet)
        shell().last_status = builtin_declare(cmd);
    } else if bytes_eq(&cmd.args[0], b"readonly") {
        // Mark variables as read-only (simplified: just set them)
        shell().last_status = builtin_declare(cmd);
    } else if bytes_eq(&cmd.args[0], b"let") {
        shell().last_status = builtin_let(cmd);
    } else if bytes_eq(&cmd.args[0], b"exec") {
        builtin_exec(cmd);
    } else if bytes_eq(&cmd.args[0], b"eval") {
        shell().last_status = builtin_eval(cmd);
    } else if bytes_eq(&cmd.args[0], b"umask") {
        shell().last_status = builtin_umask(cmd);
    } else if bytes_eq(&cmd.args[0], b"jobs") {
        printlns("esh: jobs: job control not yet implemented");
        shell().last_status = 1;
    } else if bytes_eq(&cmd.args[0], b"fg") || bytes_eq(&cmd.args[0], b"bg") {
        printlns("esh: job control not yet implemented");
        shell().last_status = 1;
    } else if bytes_eq(&cmd.args[0], b"wait") {
        shell().last_status = builtin_wait(cmd);
    } else if bytes_eq(&cmd.args[0], b"kill") {
        shell().last_status = builtin_kill(cmd);
    } else if bytes_eq(&cmd.args[0], b"history") {
        shell().last_status = builtin_history(cmd);
    } else if bytes_eq(&cmd.args[0], b"getopts") {
        eprintlns("esh: getopts: not yet implemented");
        shell().last_status = 1;
    } else if bytes_eq(&cmd.args[0], b"theme") {
        shell().last_status = builtin_theme(cmd);
    } else if bytes_eq(&cmd.args[0], b"colors") {
        shell().last_status = builtin_colors(cmd);
    }
}

/// test / [ builtin - evaluate conditional expressions
fn builtin_test(cmd: &Command) -> i32 {
    let is_bracket = bytes_eq(&cmd.args[0], b"[");
    let mut argc = cmd.argc;

    // For [, check for closing ]
    if is_bracket {
        if argc < 2 || !bytes_eq(&cmd.args[argc - 1], b"]") {
            eprintlns("esh: [: missing ']'");
            return 2;
        }
        argc -= 1; // Don't include ] in argument processing
    }

    // No arguments = false
    if argc < 2 {
        return 1;
    }

    // Single argument: true if non-empty
    if argc == 2 {
        let arg = &cmd.args[1];
        return if arg[0] != 0 { 0 } else { 1 };
    }

    // Two arguments: unary operators
    if argc == 3 {
        let op = &cmd.args[1];
        let arg = &cmd.args[2];

        if bytes_eq(op, b"-n") {
            // Non-empty string
            return if arg[0] != 0 { 0 } else { 1 };
        } else if bytes_eq(op, b"-z") {
            // Empty string
            return if arg[0] == 0 { 0 } else { 1 };
        } else if bytes_eq(op, b"-e") || bytes_eq(op, b"-a") {
            // File exists
            let path = bytes_to_str(arg);
            let fd = open2(path, O_RDONLY);
            if fd >= 0 {
                close(fd);
                return 0;
            }
            return 1;
        } else if bytes_eq(op, b"-f") {
            // Regular file (simplified: just check exists and not directory)
            let path = bytes_to_str(arg);
            let fd = open2(path, O_RDONLY);
            if fd >= 0 {
                close(fd);
                // Check if it's a directory by trying to open as directory
                let dfd = open(path, O_RDONLY | O_DIRECTORY, 0);
                if dfd >= 0 {
                    close(dfd);
                    return 1; // It's a directory, not a regular file
                }
                return 0;
            }
            return 1;
        } else if bytes_eq(op, b"-d") {
            // Directory
            let path = bytes_to_str(arg);
            let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
            if fd >= 0 {
                close(fd);
                return 0;
            }
            return 1;
        } else if bytes_eq(op, b"-r") || bytes_eq(op, b"-w") || bytes_eq(op, b"-x") {
            // Readable/writable/executable (simplified: just check exists)
            let path = bytes_to_str(arg);
            let fd = open2(path, O_RDONLY);
            if fd >= 0 {
                close(fd);
                return 0;
            }
            return 1;
        } else if bytes_eq(op, b"-s") {
            // File has size > 0 (simplified: check exists)
            let path = bytes_to_str(arg);
            let fd = open2(path, O_RDONLY);
            if fd >= 0 {
                close(fd);
                return 0; // Assume non-empty if exists
            }
            return 1;
        } else if bytes_eq(op, b"!") {
            // Negation of single arg
            return if arg[0] != 0 { 1 } else { 0 };
        }
    }

    // Three arguments: binary operators
    if argc == 4 {
        let left = &cmd.args[1];
        let op = &cmd.args[2];
        let right = &cmd.args[3];

        // String comparisons
        if bytes_eq(op, b"=") || bytes_eq(op, b"==") {
            return if bytes_eq(left, right) { 0 } else { 1 };
        } else if bytes_eq(op, b"!=") {
            return if !bytes_eq(left, right) { 0 } else { 1 };
        }

        // Integer comparisons
        if let (Some(l), Some(r)) = (parse_int_bytes(left), parse_int_bytes(right)) {
            if bytes_eq(op, b"-eq") {
                return if l == r { 0 } else { 1 };
            } else if bytes_eq(op, b"-ne") {
                return if l != r { 0 } else { 1 };
            } else if bytes_eq(op, b"-lt") {
                return if l < r { 0 } else { 1 };
            } else if bytes_eq(op, b"-le") {
                return if l <= r { 0 } else { 1 };
            } else if bytes_eq(op, b"-gt") {
                return if l > r { 0 } else { 1 };
            } else if bytes_eq(op, b"-ge") {
                return if l >= r { 0 } else { 1 };
            }
        }
    }

    // Unsupported expression
    eprintlns("esh: test: unsupported expression");
    2
}

/// Source a profile file silently (no error if file doesn't exist)
fn source_profile(filename: &[u8]) {
    let path = bytes_to_str(filename);
    let fd = open2(path, O_RDONLY);
    if fd < 0 {
        return; // Silently ignore missing profile
    }

    // Read and execute file line by line
    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_pos = 0;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            let c = buf[i];
            if c == b'\n' {
                line[line_pos] = 0;
                if line_pos > 0 {
                    let trimmed = trim(&line);
                    if !trimmed.is_empty() && trimmed[0] != b'#' {
                        execute_line(trimmed);
                    }
                }
                line_pos = 0;
            } else if line_pos < MAX_LINE - 1 {
                line[line_pos] = c;
                line_pos += 1;
            }
        }
    }

    // Handle last line without newline
    if line_pos > 0 {
        line[line_pos] = 0;
        let trimmed = trim(&line);
        if !trimmed.is_empty() && trimmed[0] != b'#' {
            execute_line(trimmed);
        }
    }

    close(fd);
}

/// source / . builtin - execute commands from file
fn builtin_source(filename: &[u8]) -> i32 {
    let path = bytes_to_str(filename);
    let fd = open2(path, O_RDONLY);
    if fd < 0 {
        eprints("esh: source: ");
        print_bytes(filename);
        eprintlns(": No such file");
        return 1;
    }

    // Read and execute file line by line
    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_pos = 0;
    let mut last_status = 0;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            let c = buf[i];
            if c == b'\n' {
                line[line_pos] = 0;
                if line_pos > 0 {
                    let trimmed = trim(&line);
                    if !trimmed.is_empty() && trimmed[0] != b'#' {
                        execute_line(trimmed);
                        last_status = shell().last_status;
                    }
                }
                line_pos = 0;
            } else if line_pos < MAX_LINE - 1 {
                line[line_pos] = c;
                line_pos += 1;
            }
        }
    }

    // Handle last line without newline
    if line_pos > 0 {
        line[line_pos] = 0;
        let trimmed = trim(&line);
        if !trimmed.is_empty() && trimmed[0] != b'#' {
            execute_line(trimmed);
            last_status = shell().last_status;
        }
    }

    close(fd);
    last_status
}

/// read builtin - read a line from stdin
fn builtin_read(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        eprintlns("esh: read: variable name required");
        return 1;
    }

    // Read a line from stdin
    let mut buf = [0u8; MAX_LINE];
    let mut pos = 0;

    loop {
        let c = getchar();
        if c < 0 {
            if pos == 0 {
                return 1; // EOF with no input
            }
            break;
        }
        let c = c as u8;
        if c == b'\n' {
            break;
        }
        if pos < MAX_LINE - 1 {
            buf[pos] = c;
            pos += 1;
        }
    }
    buf[pos] = 0;

    // Set the variable
    let name = bytes_to_str(&cmd.args[1]);
    let value = bytes_to_str(&buf);
    if setenv(name, value) < 0 {
        return 1;
    }
    0
}

/// printf builtin - formatted output
fn builtin_printf(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        return 0;
    }

    let format = &cmd.args[1];
    let mut arg_idx = 2usize;
    let mut i = 0;

    while i < format.len() && format[i] != 0 {
        if format[i] == b'%' && i + 1 < format.len() && format[i + 1] != 0 {
            i += 1;
            match format[i] {
                b's' => {
                    if arg_idx < cmd.argc {
                        print_bytes(&cmd.args[arg_idx]);
                        arg_idx += 1;
                    }
                }
                b'd' | b'i' => {
                    if arg_idx < cmd.argc {
                        if let Some(n) = parse_int_bytes(&cmd.args[arg_idx]) {
                            print_i64(n);
                        }
                        arg_idx += 1;
                    }
                }
                b'x' => {
                    if arg_idx < cmd.argc {
                        if let Some(n) = parse_int_bytes(&cmd.args[arg_idx]) {
                            print_hex(n as u64);
                        }
                        arg_idx += 1;
                    }
                }
                b'c' => {
                    if arg_idx < cmd.argc && cmd.args[arg_idx][0] != 0 {
                        putchar(cmd.args[arg_idx][0]);
                        arg_idx += 1;
                    }
                }
                b'%' => {
                    putchar(b'%');
                }
                b'n' => {
                    putchar(b'\n');
                }
                _ => {
                    putchar(b'%');
                    putchar(format[i]);
                }
            }
        } else if format[i] == b'\\' && i + 1 < format.len() && format[i + 1] != 0 {
            i += 1;
            match format[i] {
                b'n' => putchar(b'\n'),
                b't' => putchar(b'\t'),
                b'r' => putchar(b'\r'),
                b'\\' => putchar(b'\\'),
                _ => {
                    putchar(b'\\');
                    putchar(format[i]);
                }
            }
        } else {
            putchar(format[i]);
        }
        i += 1;
    }
    0
}

/// alias builtin - define or list aliases
fn builtin_alias(cmd: &Command) -> i32 {
    let state = shell();

    if cmd.argc < 2 {
        // List all aliases
        for alias in state.aliases.iter() {
            if alias.used {
                prints("alias ");
                print_bytes(&alias.name);
                prints("='");
                print_bytes(&alias.value);
                printlns("'");
            }
        }
        return 0;
    }

    // Parse name=value
    for i in 1..cmd.argc {
        let arg = &cmd.args[i];
        let mut eq_pos = None;
        for j in 0..64 {
            if arg[j] == 0 {
                break;
            }
            if arg[j] == b'=' {
                eq_pos = Some(j);
                break;
            }
        }

        if let Some(pos) = eq_pos {
            let mut name = [0u8; 32];
            let mut value = [0u8; 128];
            let name_len = pos.min(31);
            name[..name_len].copy_from_slice(&arg[..name_len]);

            let mut j = pos + 1;
            let mut k = 0;
            while j < 64 && arg[j] != 0 && k < 127 {
                value[k] = arg[j];
                j += 1;
                k += 1;
            }

            // Find existing or empty slot
            let mut slot = None;
            for (idx, alias) in state.aliases.iter_mut().enumerate() {
                if alias.used && bytes_eq(&alias.name, &name) {
                    slot = Some(idx);
                    break;
                }
            }
            if slot.is_none() {
                for (idx, alias) in state.aliases.iter().enumerate() {
                    if !alias.used {
                        slot = Some(idx);
                        break;
                    }
                }
            }

            if let Some(idx) = slot {
                state.aliases[idx].name = name;
                state.aliases[idx].value = value;
                state.aliases[idx].used = true;
            } else {
                eprintlns("esh: alias: too many aliases");
                return 1;
            }
        } else {
            // Just name - print that alias
            let mut found = false;
            for alias in state.aliases.iter() {
                if alias.used && bytes_eq(&alias.name, arg) {
                    prints("alias ");
                    print_bytes(&alias.name);
                    prints("='");
                    print_bytes(&alias.value);
                    printlns("'");
                    found = true;
                    break;
                }
            }
            if !found {
                eprints("esh: alias: ");
                print_bytes(arg);
                eprintlns(": not found");
                return 1;
            }
        }
    }
    0
}

/// unalias builtin - remove aliases
fn builtin_unalias(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        eprintlns("esh: unalias: argument required");
        return 1;
    }

    let state = shell();
    let mut status = 0;

    // Check for -a flag (remove all)
    if bytes_eq(&cmd.args[1], b"-a") {
        for alias in state.aliases.iter_mut() {
            alias.used = false;
        }
        return 0;
    }

    for i in 1..cmd.argc {
        let mut found = false;
        for alias in state.aliases.iter_mut() {
            if alias.used && bytes_eq(&alias.name, &cmd.args[i]) {
                alias.used = false;
                found = true;
                break;
            }
        }
        if !found {
            eprints("esh: unalias: ");
            print_bytes(&cmd.args[i]);
            eprintlns(": not found");
            status = 1;
        }
    }
    status
}

/// type builtin - show how a command would be interpreted
fn builtin_type(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        return 0;
    }

    let state = shell();
    let mut status = 0;

    for i in 1..cmd.argc {
        let name = &cmd.args[i];

        // Check if it's an alias
        let mut found = false;
        for alias in state.aliases.iter() {
            if alias.used && bytes_eq(&alias.name, name) {
                print_bytes(name);
                prints(" is aliased to '");
                print_bytes(&alias.value);
                printlns("'");
                found = true;
                break;
            }
        }

        if !found && is_builtin(name) {
            print_bytes(name);
            printlns(" is a shell builtin");
            found = true;
        }

        if !found {
            // Check if command exists in /bin
            // "/bin/" is 5 characters
            let mut path = [0u8; 128];
            path[..5].copy_from_slice(b"/bin/");
            let mut j = 0;
            while j < 63 && name[j] != 0 {
                path[5 + j] = name[j];
                j += 1;
            }
            path[5 + j] = 0;

            let path_str = bytes_to_str(&path);
            let fd = open2(path_str, O_RDONLY);
            if fd >= 0 {
                close(fd);
                print_bytes(name);
                prints(" is ");
                print_bytes(&path);
                printlns("");
                found = true;
            }
        }

        if !found {
            eprints("esh: type: ");
            print_bytes(name);
            eprintlns(": not found");
            status = 1;
        }
    }
    status
}

/// set builtin - set shell options and positional parameters
fn builtin_set(cmd: &Command) -> i32 {
    let state = shell();

    if cmd.argc < 2 {
        // Print all variables
        env_iter(|name, value| {
            print_bytes(name);
            prints("=");
            print_bytes(value);
            printlns("");
        });
        return 0;
    }

    // Set positional parameters
    state.positional_count = 0;
    for i in 1..cmd.argc {
        if state.positional_count < MAX_ARGS {
            state.positional[state.positional_count] = cmd.args[i];
            state.positional_count += 1;
        }
    }
    0
}

/// shift builtin - shift positional parameters
fn builtin_shift(cmd: &Command) -> i32 {
    let state = shell();
    let n = if cmd.argc > 1 {
        parse_int_bytes(&cmd.args[1]).unwrap_or(1) as usize
    } else {
        1
    };

    if n > state.positional_count {
        eprintlns("esh: shift: shift count out of range");
        return 1;
    }

    // Shift parameters
    for i in 0..(state.positional_count - n) {
        state.positional[i] = state.positional[i + n];
    }
    state.positional_count -= n;
    0
}

/// declare builtin - declare variables
fn builtin_declare(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        // List all variables
        env_iter(|name, value| {
            prints("declare -- ");
            print_bytes(name);
            prints("=\"");
            print_bytes(value);
            printlns("\"");
        });
        return 0;
    }

    for i in 1..cmd.argc {
        let arg = &cmd.args[i];

        // Skip flags like -x, -r, etc. (simplified)
        if arg[0] == b'-' {
            continue;
        }

        // Parse name=value
        let mut eq_pos = None;
        for j in 0..64 {
            if arg[j] == 0 {
                break;
            }
            if arg[j] == b'=' {
                eq_pos = Some(j);
                break;
            }
        }

        if let Some(pos) = eq_pos {
            let mut name = [0u8; 64];
            let mut value = [0u8; 64];
            name[..pos].copy_from_slice(&arg[..pos]);

            let mut j = pos + 1;
            let mut k = 0;
            while j < 64 && arg[j] != 0 && k < 63 {
                value[k] = arg[j];
                j += 1;
                k += 1;
            }

            let name_str = bytes_to_str(&name);
            let value_str = bytes_to_str(&value);
            setenv(name_str, value_str);
        } else {
            // Just declare the variable with empty value if not exists
            let name_str = bytes_to_str(arg);
            if getenv(name_str).is_none() {
                setenv(name_str, "");
            }
        }
    }
    0
}

/// let builtin - evaluate arithmetic expressions
fn builtin_let(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        eprintlns("esh: let: expression expected");
        return 1;
    }

    let mut result = 0i64;

    for i in 1..cmd.argc {
        let expr = &cmd.args[i];

        // Simple expression parser: VAR=expr or just expr
        let mut eq_pos = None;
        for j in 0..64 {
            if expr[j] == 0 {
                break;
            }
            if expr[j] == b'=' {
                eq_pos = Some(j);
                break;
            }
        }

        if let Some(pos) = eq_pos {
            // Assignment: VAR=expr
            let mut name = [0u8; 64];
            name[..pos].copy_from_slice(&expr[..pos]);

            let value_part = &expr[pos + 1..];
            result = eval_arithmetic(value_part);

            let name_str = bytes_to_str(&name);
            let mut value_buf = [0u8; 32];
            int_to_bytes(result, &mut value_buf);
            let value_str = bytes_to_str(&value_buf);
            setenv(name_str, value_str);
        } else {
            result = eval_arithmetic(expr);
        }
    }

    // let returns 1 if result is 0, 0 otherwise
    if result == 0 { 1 } else { 0 }
}

/// Simple arithmetic evaluator
fn eval_arithmetic(expr: &[u8]) -> i64 {
    // Very simplified: just parse integer or do simple operations
    let mut i = 0;
    let mut result = 0i64;
    let mut current_num = 0i64;
    let mut op = b'+';
    let mut negative = false;

    while i < expr.len() && expr[i] != 0 {
        let c = expr[i];

        if c == b'-' && (i == 0 || !expr[i - 1].is_ascii_digit()) {
            negative = true;
            i += 1;
            continue;
        }

        if c.is_ascii_digit() {
            current_num = current_num * 10 + (c - b'0') as i64;
        } else if c == b'+' || c == b'-' || c == b'*' || c == b'/' || c == b'%' {
            if negative {
                current_num = -current_num;
                negative = false;
            }
            result = apply_op(result, current_num, op);
            current_num = 0;
            op = c;
        }
        i += 1;
    }

    if negative {
        current_num = -current_num;
    }
    apply_op(result, current_num, op)
}

fn apply_op(left: i64, right: i64, op: u8) -> i64 {
    match op {
        b'+' => left + right,
        b'-' => left - right,
        b'*' => left * right,
        b'/' => {
            if right != 0 {
                left / right
            } else {
                0
            }
        }
        b'%' => {
            if right != 0 {
                left % right
            } else {
                0
            }
        }
        _ => right,
    }
}

fn int_to_bytes(mut n: i64, buf: &mut [u8]) {
    let negative = n < 0;
    if negative {
        n = -n;
    }

    let mut tmp = [0u8; 20];
    let mut i = 0;

    if n == 0 {
        tmp[0] = b'0';
        i = 1;
    } else {
        while n > 0 {
            tmp[i] = b'0' + (n % 10) as u8;
            n /= 10;
            i += 1;
        }
    }

    let mut j = 0;
    if negative && j < buf.len() {
        buf[j] = b'-';
        j += 1;
    }

    while i > 0 && j < buf.len() - 1 {
        i -= 1;
        buf[j] = tmp[i];
        j += 1;
    }
    buf[j] = 0;
}

/// exec builtin - replace shell with command
fn builtin_exec(cmd: &Command) {
    if cmd.argc < 2 {
        return;
    }

    let mut new_cmd = Command::new();
    for i in 1..cmd.argc {
        new_cmd.args[i - 1] = cmd.args[i];
    }
    new_cmd.argc = cmd.argc - 1;
    execute_external(&new_cmd);
    // If exec returns, the command failed
    shell().last_status = 127;
}

/// eval builtin - evaluate arguments as shell command
fn builtin_eval(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        return 0;
    }

    // Concatenate all arguments with spaces
    let mut line = [0u8; MAX_LINE];
    let mut pos = 0;

    for i in 1..cmd.argc {
        if i > 1 && pos < MAX_LINE - 1 {
            line[pos] = b' ';
            pos += 1;
        }
        let arg = &cmd.args[i];
        let mut j = 0;
        while j < 64 && arg[j] != 0 && pos < MAX_LINE - 1 {
            line[pos] = arg[j];
            pos += 1;
            j += 1;
        }
    }
    line[pos] = 0;

    execute_line(&line);
    shell().last_status
}

/// umask builtin - set file creation mask
fn builtin_umask(cmd: &Command) -> i32 {
    let state = shell();

    if cmd.argc < 2 {
        // Print current umask
        prints("0");
        print_u64((state.umask >> 6 & 7) as u64);
        print_u64((state.umask >> 3 & 7) as u64);
        print_u64((state.umask & 7) as u64);
        printlns("");
        return 0;
    }

    // Parse octal umask
    let arg = &cmd.args[1];
    let mut mask = 0u32;
    let mut i = 0;

    while i < arg.len() && arg[i] != 0 {
        let c = arg[i];
        if c >= b'0' && c <= b'7' {
            mask = mask * 8 + (c - b'0') as u32;
        } else {
            eprintlns("esh: umask: invalid octal number");
            return 1;
        }
        i += 1;
    }

    state.umask = mask & 0o777;
    0
}

/// wait builtin - wait for background jobs
fn builtin_wait(cmd: &Command) -> i32 {
    let mut status = 0i32;

    if cmd.argc > 1 {
        // Wait for specific PID
        if let Some(pid) = parse_int_bytes(&cmd.args[1]) {
            waitpid(pid as i32, &mut status, 0);
        }
    } else {
        // Wait for all children
        loop {
            let ret = waitpid(-1, &mut status, WNOHANG);
            if ret <= 0 {
                break;
            }
        }
    }
    wexitstatus(status)
}

/// kill builtin - send signal to process
fn builtin_kill(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        eprintlns("esh: kill: usage: kill [-signal] pid");
        return 1;
    }

    let mut signal = 15; // SIGTERM
    let mut pid_arg = 1;

    // Check for signal argument
    if cmd.args[1][0] == b'-' {
        if cmd.argc < 3 {
            eprintlns("esh: kill: pid required");
            return 1;
        }
        // Parse signal number
        let sig_str = &cmd.args[1][1..];
        if let Some(s) = parse_int_bytes(sig_str) {
            signal = s as i32;
        } else {
            // Named signal (simplified)
            if bytes_eq(&cmd.args[1], b"-TERM") || bytes_eq(&cmd.args[1], b"-15") {
                signal = 15;
            } else if bytes_eq(&cmd.args[1], b"-KILL") || bytes_eq(&cmd.args[1], b"-9") {
                signal = 9;
            } else if bytes_eq(&cmd.args[1], b"-INT") || bytes_eq(&cmd.args[1], b"-2") {
                signal = 2;
            } else if bytes_eq(&cmd.args[1], b"-HUP") || bytes_eq(&cmd.args[1], b"-1") {
                signal = 1;
            }
        }
        pid_arg = 2;
    }

    // Send signal to each PID
    let mut status = 0;
    for i in pid_arg..cmd.argc {
        if let Some(pid) = parse_int_bytes(&cmd.args[i]) {
            if sys_kill(pid as i32, signal) < 0 {
                eprints("esh: kill: ");
                print_bytes(&cmd.args[i]);
                eprintlns(": no such process");
                status = 1;
            }
        } else {
            eprints("esh: kill: ");
            print_bytes(&cmd.args[i]);
            eprintlns(": invalid pid");
            status = 1;
        }
    }
    status
}

/// history builtin - display command history
fn builtin_history(_cmd: &Command) -> i32 {
    let total = unsafe { libc::readline::history_length } as usize;
    let count = if _cmd.argc > 1 {
        parse_int_bytes(&_cmd.args[1]).unwrap_or(total as i64) as usize
    } else {
        total
    };

    let start = if count > total { 0 } else { total - count };

    for i in start..total {
        // history_get uses 1-based offset
        let entry = unsafe { libc::readline::history_get((i + 1) as i32) };
        if !entry.is_null() {
            prints("  ");
            print_i64((i + 1) as i64);
            prints("  ");
            let line = unsafe { (*entry).line };
            if !line.is_null() {
                let len = unsafe { libc::string::strlen(line) };
                let s = unsafe { core::slice::from_raw_parts(line, len) };
                print_bytes(s);
            }
            printlns("");
        }
    }
    0
}

/// theme builtin - manage color themes
fn builtin_theme(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        // Show current theme and list available
        prints("Current theme: ");
        print_bytes(&current_theme().name);
        printlns("");
        printlns("");
        printlns("Available themes:");
        for name in THEME_NAMES {
            prints("  ");
            print_bytes(name);
            printlns("");
        }
        printlns("");
        printlns("Usage: theme <name>   - Set theme");
        printlns("       theme list     - List themes");
        printlns("       theme show     - Show current theme colors");
        return 0;
    }

    let arg = &cmd.args[1];

    if bytes_eq(arg, b"list") {
        printlns("Available themes:");
        for name in THEME_NAMES {
            prints("  ");
            print_bytes(name);
            // Mark current
            if bytes_eq(name, &current_theme().name) {
                prints(" (current)");
            }
            printlns("");
        }
        return 0;
    }

    if bytes_eq(arg, b"show") {
        // Show theme colors with samples
        let theme = current_theme();
        prints("Theme: ");
        print_bytes(&theme.name);
        printlns("");
        printlns("");

        // Show prompt colors
        printlns("Prompt colors:");
        print_colored(b"  user    ", &theme.prompt_user);
        print_colored(b"  host    ", &theme.prompt_host);
        print_colored(b"  path    ", &theme.prompt_path);
        print_colored(b"  symbol  ", &theme.prompt_symbol);
        printlns("");

        // Show output colors
        printlns("Output colors:");
        print_colored(b"  error   ", &theme.error);
        print_colored(b"  warning ", &theme.warning);
        print_colored(b"  info    ", &theme.info);
        print_colored(b"  success ", &theme.success);
        printlns("");

        // Show completion colors
        printlns("Completion colors:");
        print_colored(b"  directory  ", &theme.comp_directory);
        print_colored(b"  executable ", &theme.comp_executable);
        print_colored(b"  file       ", &theme.comp_file);
        print_colored(b"  symlink    ", &theme.comp_symlink);
        printlns("");

        return 0;
    }

    // Set theme by name
    if load_theme(arg) {
        prints("Theme set to: ");
        print_bytes(&current_theme().name);
        printlns("");

        // Save to ESH_THEME env var for persistence hint
        let name_str = bytes_to_str(arg);
        setenv("ESH_THEME", name_str);
        0
    } else {
        eprints("esh: theme: unknown theme: ");
        print_bytes(arg);
        eprintlns("");
        printlns("Use 'theme list' to see available themes");
        1
    }
}

/// colors builtin - enable/disable colors
fn builtin_colors(cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        // Show current status
        if colors_enabled() {
            printlns("Colors: enabled");
        } else {
            printlns("Colors: disabled");
        }
        printlns("");
        printlns("Usage: colors on|off   - Enable/disable colors");
        printlns("       colors test     - Show color test pattern");
        return 0;
    }

    let arg = &cmd.args[1];

    if bytes_eq(arg, b"on") || bytes_eq(arg, b"enable") || bytes_eq(arg, b"1") {
        set_colors_enabled(true);
        printlns("Colors enabled");
        return 0;
    }

    if bytes_eq(arg, b"off") || bytes_eq(arg, b"disable") || bytes_eq(arg, b"0") {
        set_colors_enabled(false);
        printlns("Colors disabled");
        return 0;
    }

    if bytes_eq(arg, b"test") {
        // Show color test pattern
        printlns("Color test pattern:");
        printlns("");

        // Standard colors
        prints("Standard:  ");
        for i in 0..8 {
            let spec = ColorSpec::fg_only(color_from_index(i));
            print_colored(b"##", &spec);
            prints(" ");
        }
        printlns("");

        // Bright colors
        prints("Bright:    ");
        for i in 8..16 {
            let spec = ColorSpec::fg_only(color_from_index(i));
            print_colored(b"##", &spec);
            prints(" ");
        }
        printlns("");

        // Background colors
        prints("Backgrounds: ");
        for i in 0..8 {
            let spec = ColorSpec::new(Color::White, color_from_index(i), theme::Style::none());
            print_colored(b"  ", &spec);
        }
        printlns("");

        return 0;
    }

    eprints("esh: colors: unknown option: ");
    print_bytes(arg);
    eprintlns("");
    1
}

/// Print text with color specification
fn print_colored(text: &[u8], spec: &ColorSpec) {
    if colors_enabled() {
        let mut buf = [0u8; 32];
        let len = spec.write_escape(&mut buf);
        for i in 0..len {
            putchar(buf[i]);
        }
    }
    print_bytes(text);
    if colors_enabled() {
        prints("\x1b[0m");
    }
    printlns("");
}

/// Convert index to Color enum
fn color_from_index(i: u8) -> Color {
    match i {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::White,
        8 => Color::BrightBlack,
        9 => Color::BrightRed,
        10 => Color::BrightGreen,
        11 => Color::BrightYellow,
        12 => Color::BrightBlue,
        13 => Color::BrightMagenta,
        14 => Color::BrightCyan,
        15 => Color::BrightWhite,
        _ => Color::Default,
    }
}

/// Execute an external command
fn execute_external(cmd: &Command) {
    // Reset SIGINT to default behavior for child process
    // (shell ignores it, but child should be interruptible)
    signal(SIGINT, SIG_DFL);

    let arg = &cmd.args[0];

    // Build argv array (NULL-terminated array of pointers)
    let mut argv_ptrs: [*const u8; MAX_ARGS + 1] = [core::ptr::null(); MAX_ARGS + 1];
    for i in 0..cmd.argc {
        argv_ptrs[i] = cmd.args[i].as_ptr();
    }
    argv_ptrs[cmd.argc] = core::ptr::null(); // NULL terminator

    // Try direct path first if it starts with / or ~
    if arg[0] == b'/' || arg[0] == b'~' {
        let expanded = expand_path(arg);
        let path = bytes_to_str(&expanded);
        let ret = execv(path, argv_ptrs.as_ptr());
        if ret < 0 {
            eprints("esh: ");
            print_bytes(&expanded);
            eprintlns(": not found");
        }
        return;
    }

    // Get command name length
    let mut cmd_len = 0;
    while cmd_len < arg.len() && arg[cmd_len] != 0 {
        cmd_len += 1;
    }

    // Get PATH environment variable
    let path_env = getenv("PATH").unwrap_or("/bin:/usr/bin");
    let path_bytes = path_env.as_bytes();

    // Search each directory in PATH (colon-separated)
    let mut start = 0;
    let path_len = path_bytes.len();

    while start < path_len {
        // Find end of this path component (next colon or end)
        let mut end = start;
        while end < path_len && path_bytes[end] != b':' {
            end += 1;
        }

        // Build full path: directory + "/" + command
        let dir_len = end - start;
        if dir_len > 0 && dir_len + cmd_len + 2 < 256 {
            let mut full_path = [0u8; 256];

            // Copy directory
            full_path[..dir_len].copy_from_slice(&path_bytes[start..end]);

            // Add trailing slash if needed
            let mut pos = dir_len;
            if dir_len > 0 && full_path[dir_len - 1] != b'/' {
                full_path[pos] = b'/';
                pos += 1;
            }

            // Copy command name
            let mut i = 0;
            while i < cmd_len {
                full_path[pos + i] = arg[i];
                i += 1;
            }
            full_path[pos + i] = 0;

            let path_str = bytes_to_str(&full_path);

            // Try to execute - if it succeeds, execv doesn't return
            let ret = execv(path_str, argv_ptrs.as_ptr());
            if ret >= 0 {
                return;
            }
        }

        // Move to next path component
        start = end + 1;
    }

    // None of the paths worked
    eprints("esh: ");
    print_bytes(arg);
    eprintlns(": command not found");
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

/// Convert a path with optional leading ~ or ~/ into absolute path using HOME
fn expand_path(bytes: &[u8]) -> [u8; 128] {
    let mut out = [0u8; 128];
    let expanded = expand_tilde_to_buf(bytes, &mut out);
    let mut final_buf = [0u8; 128];
    let bytes = expanded.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && i < final_buf.len() - 1 {
        final_buf[i] = bytes[i];
        i += 1;
    }
    final_buf[i] = 0;
    final_buf
}
