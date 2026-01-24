//! ls - list directory contents
//!
//! POSIX-compliant implementation matching Linux ls behavior:
//! - Default: multi-column output, hide dotfiles
//! - Long format (-l) with permissions, links, owner, group, size, date, name
//! - Show all files (-a) including . and ..
//! - Almost all (-A) excludes . and ..
//! - Human-readable sizes (-h)
//! - Recursive listing (-R)
//! - One entry per line (-1)
//! - Append indicator (-F): / for dirs, * for executable, @ for symlink
//! - List directory entry itself (-d)

#![no_std]
#![no_main]

use libc::*;

/// Directory entry header (matches kernel's UserDirEntry)
/// MUST be packed to match kernel's packed struct (19 bytes, not 24)
#[repr(C, packed)]
struct DirEntry {
    d_ino: u64,
    d_off: u64,
    d_reclen: u16,
    d_type: u8,
}

// File type constants
const DT_UNKNOWN: u8 = 0;
const DT_FIFO: u8 = 1;
const DT_CHR: u8 = 2;
const DT_DIR: u8 = 4;
const DT_BLK: u8 = 6;
const DT_REG: u8 = 8;
const DT_LNK: u8 = 10;
const DT_SOCK: u8 = 12;

// Permission bits
const S_IRUSR: u32 = 0o400;
const S_IWUSR: u32 = 0o200;
const S_IXUSR: u32 = 0o100;
const S_IRGRP: u32 = 0o040;
const S_IWGRP: u32 = 0o020;
const S_IXGRP: u32 = 0o010;
const S_IROTH: u32 = 0o004;
const S_IWOTH: u32 = 0o002;
const S_IXOTH: u32 = 0o001;

/// Entry storage for sorting and display
#[derive(Clone, Copy)]
struct Entry {
    name: [u8; 256],
    d_type: u8,
    d_ino: u64,
    size: u64,
    mode: u32,
}

impl Entry {
    fn new() -> Self {
        Entry {
            name: [0; 256],
            d_type: 0,
            d_ino: 0,
            size: 0,
            mode: 0,
        }
    }

    fn name_len(&self) -> usize {
        self.name.iter().position(|&c| c == 0).unwrap_or(256)
    }

    fn is_hidden(&self) -> bool {
        self.name[0] == b'.'
    }

    fn is_dot_or_dotdot(&self) -> bool {
        (self.name[0] == b'.' && self.name[1] == 0)
            || (self.name[0] == b'.' && self.name[1] == b'.' && self.name[2] == 0)
    }
}

/// Print a null-terminated string from a byte slice
fn print_name(name: &[u8]) {
    for &b in name {
        if b == 0 {
            break;
        }
        putchar(b);
    }
}

/// Get file type character for long format
fn type_char(d_type: u8) -> u8 {
    match d_type {
        DT_DIR => b'd',
        DT_REG => b'-',
        DT_LNK => b'l',
        DT_CHR => b'c',
        DT_BLK => b'b',
        DT_FIFO => b'p',
        DT_SOCK => b's',
        _ => b'?',
    }
}

/// Get indicator character for -F flag
fn indicator_char(d_type: u8, mode: u32) -> Option<u8> {
    match d_type {
        DT_DIR => Some(b'/'),
        DT_LNK => Some(b'@'),
        DT_FIFO => Some(b'|'),
        DT_SOCK => Some(b'='),
        DT_REG if mode & S_IXUSR != 0 => Some(b'*'),
        _ => None,
    }
}

/// Format permission bits
fn format_permissions(mode: u32, buf: &mut [u8; 9]) {
    buf[0] = if mode & S_IRUSR != 0 { b'r' } else { b'-' };
    buf[1] = if mode & S_IWUSR != 0 { b'w' } else { b'-' };
    buf[2] = if mode & S_IXUSR != 0 { b'x' } else { b'-' };
    buf[3] = if mode & S_IRGRP != 0 { b'r' } else { b'-' };
    buf[4] = if mode & S_IWGRP != 0 { b'w' } else { b'-' };
    buf[5] = if mode & S_IXGRP != 0 { b'x' } else { b'-' };
    buf[6] = if mode & S_IROTH != 0 { b'r' } else { b'-' };
    buf[7] = if mode & S_IWOTH != 0 { b'w' } else { b'-' };
    buf[8] = if mode & S_IXOTH != 0 { b'x' } else { b'-' };
}

/// Format number right-aligned in buffer, returns start position
fn format_number(n: u64, buf: &mut [u8], width: usize) -> usize {
    let mut num = n;
    let mut pos = buf.len();

    if num == 0 {
        pos -= 1;
        buf[pos] = b'0';
    } else {
        while num > 0 && pos > 0 {
            pos -= 1;
            buf[pos] = b'0' + (num % 10) as u8;
            num /= 10;
        }
    }

    // Pad with spaces
    let start = if width > buf.len() - pos {
        0
    } else {
        buf.len() - width
    };
    for i in start..pos {
        buf[i] = b' ';
    }

    start
}

/// Format file size in human-readable format
fn format_size_human(size: u64, buf: &mut [u8]) -> usize {
    let units = [b'B', b'K', b'M', b'G', b'T'];
    let mut s = size;
    let mut unit_idx = 0;

    while s >= 1024 && unit_idx < units.len() - 1 {
        s /= 1024;
        unit_idx += 1;
    }

    let mut pos = 0;

    // Write number
    if s == 0 {
        buf[pos] = b'0';
        pos += 1;
    } else {
        let mut temp = [0u8; 20];
        let mut temp_len = 0;
        let mut n = s;
        while n > 0 {
            temp[temp_len] = b'0' + (n % 10) as u8;
            n /= 10;
            temp_len += 1;
        }
        for i in (0..temp_len).rev() {
            buf[pos] = temp[i];
            pos += 1;
        }
    }

    buf[pos] = units[unit_idx];
    pos += 1;

    pos
}

/// Parse command line arguments
struct Args {
    long_format: bool,      // -l
    show_all: bool,         // -a (show . and ..)
    show_almost_all: bool,  // -A (hide . and ..)
    human_readable: bool,   // -h
    recursive: bool,        // -R
    one_per_line: bool,     // -1
    classify: bool,         // -F
    directory_itself: bool, // -d
    paths: [[u8; 256]; 16],
    path_count: usize,
}

impl Args {
    fn new() -> Self {
        Args {
            long_format: false,
            show_all: false,
            show_almost_all: false,
            human_readable: false,
            recursive: false,
            one_per_line: false,
            classify: false,
            directory_itself: false,
            paths: [[0; 256]; 16],
            path_count: 0,
        }
    }

    fn add_path(&mut self, path: &[u8]) {
        if self.path_count < 16 {
            let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
            let copy_len = if len > 255 { 255 } else { len };
            self.paths[self.path_count][..copy_len].copy_from_slice(&path[..copy_len]);
            self.path_count += 1;
        }
    }

    fn should_show(&self, entry: &Entry) -> bool {
        if self.show_all {
            return true;
        }
        if self.show_almost_all {
            return !entry.is_dot_or_dotdot();
        }
        !entry.is_hidden()
    }
}

fn parse_args(argc: i32, argv: *const *const u8) -> Args {
    let mut args = Args::new();

    for i in 1..argc {
        let arg = unsafe { *argv.add(i as usize) };
        if arg.is_null() {
            continue;
        }

        let first = unsafe { *arg };
        if first == b'-' {
            // Parse flags
            let mut j = 1;
            loop {
                let c = unsafe { *arg.add(j) };
                if c == 0 {
                    break;
                }
                match c {
                    b'l' => args.long_format = true,
                    b'a' => args.show_all = true,
                    b'A' => args.show_almost_all = true,
                    b'h' => args.human_readable = true,
                    b'R' => args.recursive = true,
                    b'1' => args.one_per_line = true,
                    b'F' => args.classify = true,
                    b'd' => args.directory_itself = true,
                    _ => {}
                }
                j += 1;
            }
        } else {
            // Path argument
            let mut path = [0u8; 256];
            let mut j = 0;
            while j < 255 {
                let c = unsafe { *arg.add(j) };
                if c == 0 {
                    break;
                }
                path[j] = c;
                j += 1;
            }
            args.add_path(&path);
        }
    }

    // -l implies one per line
    if args.long_format {
        args.one_per_line = true;
    }

    args
}

/// Print entry in long format
fn print_long_entry(entry: &Entry, args: &Args) {
    // Type character
    putchar(type_char(entry.d_type));

    // Permissions
    let mut perms = [0u8; 9];
    format_permissions(entry.mode, &mut perms);
    for &c in &perms {
        putchar(c);
    }

    // Link count (hardcoded to 1 for now)
    prints("  1 ");

    // Owner and group (hardcoded for now)
    prints("root root ");

    // Size
    if args.human_readable {
        let mut size_buf = [0u8; 8];
        let len = format_size_human(entry.size, &mut size_buf);
        // Right-align in 5 chars
        for _ in 0..(5usize.saturating_sub(len)) {
            putchar(b' ');
        }
        for i in 0..len {
            putchar(size_buf[i]);
        }
    } else {
        let mut size_buf = [b' '; 8];
        let start = format_number(entry.size, &mut size_buf, 8);
        for i in start..8 {
            putchar(size_buf[i]);
        }
    }

    prints(" ");

    // Date (hardcoded for now - would need stat)
    prints("Jan  1 00:00 ");

    // Name
    print_name(&entry.name);

    // Indicator if -F
    if args.classify {
        if let Some(c) = indicator_char(entry.d_type, entry.mode) {
            putchar(c);
        }
    }

    printlns("");
}

/// Print entries in columns (like default ls)
fn print_columns(entries: &[Entry], count: usize, args: &Args) {
    if count == 0 {
        return;
    }

    // Find max name length
    let mut max_len = 0;
    for i in 0..count {
        let len = entries[i].name_len();
        let extra = if args.classify && indicator_char(entries[i].d_type, entries[i].mode).is_some()
        {
            1
        } else {
            0
        };
        if len + extra > max_len {
            max_len = len + extra;
        }
    }

    // Column width (name + 2 spaces minimum)
    let col_width = max_len + 2;

    // Assume 80 column terminal
    let term_width = 80;
    let num_cols = if col_width > 0 {
        term_width / col_width
    } else {
        1
    };
    let num_cols = if num_cols == 0 { 1 } else { num_cols };

    // Print in columns (row-major order like Linux ls)
    let num_rows = (count + num_cols - 1) / num_cols;

    for row in 0..num_rows {
        for col in 0..num_cols {
            let idx = row + col * num_rows;
            if idx >= count {
                break;
            }

            let entry = &entries[idx];
            let name_len = entry.name_len();

            print_name(&entry.name);

            // Print indicator if -F
            let indicator = if args.classify {
                indicator_char(entry.d_type, entry.mode)
            } else {
                None
            };

            if let Some(c) = indicator {
                putchar(c);
            }

            let printed_len = name_len + if indicator.is_some() { 1 } else { 0 };

            // Pad to column width (except last column)
            if col < num_cols - 1 && idx + num_rows < count {
                for _ in printed_len..col_width {
                    putchar(b' ');
                }
            }
        }
        printlns("");
    }
}

fn list_directory(path: &[u8], args: &Args, depth: usize, show_header: bool) -> i32 {
    let path_len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    let path_str = unsafe { core::str::from_utf8_unchecked(&path[..path_len]) };

    // If -d, just show the directory name itself
    if args.directory_itself {
        if args.long_format {
            let mut entry = Entry::new();
            entry.name[..path_len].copy_from_slice(&path[..path_len]);
            entry.d_type = DT_DIR;
            entry.mode = 0o755;
            print_long_entry(&entry, args);
        } else {
            print_name(path);
            if args.classify {
                putchar(b'/');
            }
            printlns("");
        }
        return 0;
    }

    // Show directory name if recursive or multiple paths
    if show_header {
        if depth > 0 {
            printlns("");
        }
        print_name(path);
        printlns(":");
    }

    let fd = open(path_str, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        eprints("ls: cannot access '");
        print_name(path);
        eprintlns("': No such file or directory");
        return 1;
    }

    // Collect entries
    let mut entries: [Entry; 256] = unsafe { core::mem::zeroed() };
    let mut entry_count = 0;

    // For storing subdirectories to recurse into
    let mut subdirs: [[u8; 512]; 64] = [[0; 512]; 64];
    let mut subdir_count = 0;

    // Read directory entries
    let mut buf = [0u8; 4096];
    loop {
        let n = sys_getdents(fd, &mut buf);
        if n <= 0 {
            break;
        }

        let mut offset = 0;
        while offset < n as usize && entry_count < 256 {
            let entry_ptr = buf.as_ptr().wrapping_add(offset) as *const DirEntry;
            let dirent = unsafe { &*entry_ptr };

            let name_offset = offset + core::mem::size_of::<DirEntry>();
            let name = &buf[name_offset..];

            // Create entry
            let mut entry = Entry::new();
            entry.d_type = dirent.d_type;
            entry.d_ino = dirent.d_ino;

            // Copy name
            let mut i = 0;
            while i < 255 && name[i] != 0 {
                entry.name[i] = name[i];
                i += 1;
            }

            // Get stat info for size and mode
            let mut full_path = [0u8; 512];
            let mut pos = 0;
            for j in 0..path_len {
                full_path[pos] = path[j];
                pos += 1;
            }
            if pos > 0 && full_path[pos - 1] != b'/' {
                full_path[pos] = b'/';
                pos += 1;
            }
            let name_len = entry.name_len();
            for j in 0..name_len {
                full_path[pos] = entry.name[j];
                pos += 1;
            }

            let full_path_str = unsafe { core::str::from_utf8_unchecked(&full_path[..pos]) };

            // Try to stat the file
            let mut stat_buf = Stat::zeroed();
            if stat(full_path_str, &mut stat_buf) == 0 {
                entry.size = stat_buf.size;
                entry.mode = stat_buf.mode;
            } else {
                // Default permissions based on type
                entry.mode = if entry.d_type == DT_DIR { 0o755 } else { 0o644 };
            }

            // Filter based on args
            if args.should_show(&entry) {
                entries[entry_count] = entry;
                entry_count += 1;

                // Save subdirectory for recursion (skip . and ..)
                if args.recursive && dirent.d_type == DT_DIR && subdir_count < 64 {
                    let e = &entries[entry_count - 1];
                    if !e.is_dot_or_dotdot() {
                        subdirs[subdir_count] = full_path;
                        subdir_count += 1;
                    }
                }
            }

            offset += dirent.d_reclen as usize;
        }
    }

    close(fd);

    // Sort entries alphabetically (simple bubble sort)
    for i in 0..entry_count {
        for j in 0..entry_count - 1 - i {
            let cmp = compare_names(&entries[j].name, &entries[j + 1].name);
            if cmp > 0 {
                // Swap
                let tmp = entries[j];
                entries[j] = entries[j + 1];
                entries[j + 1] = tmp;
            }
        }
    }

    // Print entries
    if args.long_format {
        for i in 0..entry_count {
            print_long_entry(&entries[i], args);
        }
    } else if args.one_per_line {
        for i in 0..entry_count {
            print_name(&entries[i].name);
            if args.classify {
                if let Some(c) = indicator_char(entries[i].d_type, entries[i].mode) {
                    putchar(c);
                }
            }
            printlns("");
        }
    } else {
        print_columns(&entries, entry_count, args);
    }

    // Recurse into subdirectories
    if args.recursive {
        for i in 0..subdir_count {
            list_directory(&subdirs[i], args, depth + 1, true);
        }
    }

    0
}

/// Compare two null-terminated names (case-insensitive for sorting like ls)
fn compare_names(a: &[u8], b: &[u8]) -> i32 {
    let mut i = 0;
    loop {
        let ca = if a[i] >= b'A' && a[i] <= b'Z' {
            a[i] + 32
        } else {
            a[i]
        };
        let cb = if b[i] >= b'A' && b[i] <= b'Z' {
            b[i] + 32
        } else {
            b[i]
        };

        if ca == 0 && cb == 0 {
            return 0;
        }
        if ca == 0 {
            return -1;
        }
        if cb == 0 {
            return 1;
        }
        if ca < cb {
            return -1;
        }
        if ca > cb {
            return 1;
        }
        i += 1;
    }
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let args = parse_args(argc, argv);

    // If no paths specified, use current directory
    if args.path_count == 0 {
        return list_directory(b".\0", &args, 0, false);
    }

    // Determine if we should show headers (multiple paths or recursive)
    let show_headers = args.path_count > 1 || args.recursive;

    // List all specified paths
    let mut ret = 0;
    for i in 0..args.path_count {
        if i > 0 && !args.long_format {
            printlns("");
        }
        if list_directory(&args.paths[i], &args, 0, show_headers) != 0 {
            ret = 1;
        }
    }

    ret
}
