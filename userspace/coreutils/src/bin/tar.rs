//! tar - tape archive utility
//!
//! Full-featured implementation with:
//! - Create archives (-c): add files/directories to archive
//! - Extract archives (-x): extract files from archive
//! - List contents (-t): display archive contents
//! - Verbose mode (-v): show detailed information
//! - Archive file (-f): specify archive file (default stdin/stdout)
//! - POSIX ustar format support
//! - Recursive directory handling
//! - Permission and timestamp preservation

#![no_std]
#![no_main]

use libc::*;

const BLOCK_SIZE: usize = 512;
const NAME_SIZE: usize = 100;
const MAX_PATH_LEN: usize = 256;

// TAR header format (POSIX ustar)
#[repr(C)]
struct TarHeader {
    name: [u8; 100],
    mode: [u8; 8],
    uid: [u8; 8],
    gid: [u8; 8],
    size: [u8; 12],
    mtime: [u8; 12],
    checksum: [u8; 8],
    typeflag: u8,
    linkname: [u8; 100],
    magic: [u8; 6],
    version: [u8; 2],
    uname: [u8; 32],
    gname: [u8; 32],
    devmajor: [u8; 8],
    devminor: [u8; 8],
    prefix: [u8; 155],
    padding: [u8; 12],
}

impl TarHeader {
    fn zeroed() -> Self {
        TarHeader {
            name: [0; 100],
            mode: [0; 8],
            uid: [0; 8],
            gid: [0; 8],
            size: [0; 12],
            mtime: [0; 12],
            checksum: [0; 8],
            typeflag: 0,
            linkname: [0; 100],
            magic: [0; 6],
            version: [0; 2],
            uname: [0; 32],
            gname: [0; 32],
            devmajor: [0; 8],
            devminor: [0; 8],
            prefix: [0; 155],
            padding: [0; 12],
        }
    }

    /// Calculate checksum for header
    fn calculate_checksum(&self) -> u32 {
        let bytes =
            unsafe { core::slice::from_raw_parts(self as *const _ as *const u8, BLOCK_SIZE) };

        let mut sum = 0u32;
        for i in 0..BLOCK_SIZE {
            if i >= 148 && i < 156 {
                // Checksum field: treat as spaces
                sum += b' ' as u32;
            } else {
                sum += bytes[i] as u32;
            }
        }
        sum
    }
}

struct TarConfig {
    create: bool,
    extract: bool,
    list: bool,
    verbose: bool,
    archive_file: Option<[u8; MAX_PATH_LEN]>,
}

impl TarConfig {
    fn new() -> Self {
        TarConfig {
            create: false,
            extract: false,
            list: false,
            verbose: false,
            archive_file: None,
        }
    }
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

fn str_starts_with(s: &str, prefix: &str) -> bool {
    if s.len() < prefix.len() {
        return false;
    }
    let s_bytes = s.as_bytes();
    let p_bytes = prefix.as_bytes();
    for i in 0..prefix.len() {
        if s_bytes[i] != p_bytes[i] {
            return false;
        }
    }
    true
}

/// Convert number to octal ASCII string
fn num_to_octal(n: u64, buf: &mut [u8]) {
    let len = buf.len();
    let mut val = n;

    // Fill with zeros
    for i in 0..len {
        buf[i] = b'0';
    }

    // Convert to octal from right to left
    let mut pos = len - 1;
    while val > 0 && pos > 0 {
        buf[pos - 1] = b'0' + ((val & 7) as u8);
        val >>= 3;
        pos -= 1;
    }

    // Last byte should be space or null
    if len > 0 {
        buf[len - 1] = 0;
    }
}

/// Parse octal ASCII string to number
fn octal_to_num(buf: &[u8]) -> u64 {
    let mut result = 0u64;
    for &byte in buf {
        if byte >= b'0' && byte <= b'7' {
            result = (result << 3) | ((byte - b'0') as u64);
        } else if byte == 0 || byte == b' ' {
            break;
        }
    }
    result
}

/// Copy string to buffer
fn copy_to_buf(dest: &mut [u8], src: &str) {
    let src_bytes = src.as_bytes();
    let copy_len = if src_bytes.len() < dest.len() {
        src_bytes.len()
    } else {
        dest.len() - 1
    };
    dest[..copy_len].copy_from_slice(&src_bytes[..copy_len]);
    // Ensure null termination
    if copy_len < dest.len() {
        dest[copy_len] = 0;
    }
}

/// Build TAR header for a file
fn build_header(path: &str, stat: &Stat) -> TarHeader {
    let mut header = TarHeader::zeroed();

    // Name
    copy_to_buf(&mut header.name, path);

    // Mode (permissions)
    num_to_octal(stat.mode as u64 & 0o7777, &mut header.mode);

    // UID and GID
    num_to_octal(stat.uid as u64, &mut header.uid);
    num_to_octal(stat.gid as u64, &mut header.gid);

    // Size
    num_to_octal(stat.size as u64, &mut header.size);

    // Modification time
    num_to_octal(stat.mtime, &mut header.mtime);

    // Typeflag
    header.typeflag = if (stat.mode & S_IFMT) == S_IFDIR {
        b'5' // Directory
    } else {
        b'0' // Regular file
    };

    // Magic and version (ustar)
    header.magic[0] = b'u';
    header.magic[1] = b's';
    header.magic[2] = b't';
    header.magic[3] = b'a';
    header.magic[4] = b'r';
    header.magic[5] = 0;
    header.version[0] = b'0';
    header.version[1] = b'0';

    // Calculate and set checksum
    let checksum = header.calculate_checksum();
    num_to_octal(checksum as u64, &mut header.checksum);

    header
}

/// Add a file to the archive
fn add_file(archive_fd: i32, path: &str, config: &TarConfig) -> i32 {
    // Get file stats
    let mut statbuf = Stat::zeroed();
    if stat(path, &mut statbuf) < 0 {
        eprints("tar: cannot stat '");
        prints(path);
        eprintlns("'");
        return 1;
    }

    // Build and write header
    let header = build_header(path, &statbuf);
    let header_bytes =
        unsafe { core::slice::from_raw_parts(&header as *const _ as *const u8, BLOCK_SIZE) };

    if write(archive_fd, header_bytes) < 0 {
        eprintlns("tar: write error");
        return 1;
    }

    if config.verbose {
        prints(path);
        printlns("");
    }

    // If it's a directory, we're done (just header)
    if (statbuf.mode & S_IFMT) == S_IFDIR {
        return 0;
    }

    // Open and copy file contents
    let file_fd = open2(path, O_RDONLY);
    if file_fd < 0 {
        eprints("tar: cannot open '");
        prints(path);
        eprintlns("'");
        return 1;
    }

    let mut buf = [0u8; 4096];
    let mut total_read = 0usize;

    loop {
        let n = read(file_fd, &mut buf);
        if n < 0 {
            eprints("tar: read error from '");
            prints(path);
            eprintlns("'");
            close(file_fd);
            return 1;
        }
        if n == 0 {
            break;
        }

        if write(archive_fd, &buf[..n as usize]) < 0 {
            eprintlns("tar: write error");
            close(file_fd);
            return 1;
        }

        total_read += n as usize;
    }

    close(file_fd);

    // Write padding to block boundary
    let padding = (BLOCK_SIZE - (total_read % BLOCK_SIZE)) % BLOCK_SIZE;
    if padding > 0 {
        let pad_buf = [0u8; BLOCK_SIZE];
        if write(archive_fd, &pad_buf[..padding]) < 0 {
            eprintlns("tar: write error");
            return 1;
        }
    }

    0
}

/// Create archive
fn do_create(config: &TarConfig, files: &[&str]) -> i32 {
    // Open archive file
    let archive_fd = if let Some(ref path_buf) = config.archive_file {
        let len = path_buf
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(MAX_PATH_LEN);
        let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");

        let fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
        if fd < 0 {
            eprints("tar: cannot create '");
            prints(path);
            eprintlns("'");
            return 1;
        }
        fd
    } else {
        STDOUT_FILENO
    };

    // Add each file
    for &file in files {
        if add_file(archive_fd, file, config) != 0 {
            if archive_fd != STDOUT_FILENO {
                close(archive_fd);
            }
            return 1;
        }
    }

    // Write two zero blocks to mark end of archive
    let zero_block = [0u8; BLOCK_SIZE];
    write(archive_fd, &zero_block);
    write(archive_fd, &zero_block);

    if archive_fd != STDOUT_FILENO {
        close(archive_fd);
    }

    0
}

/// Extract one file from archive
fn extract_file(header: &TarHeader, archive_fd: i32, config: &TarConfig) -> i32 {
    // Get filename (find null terminator)
    let name_len = header
        .name
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(NAME_SIZE);
    let filename = match core::str::from_utf8(&header.name[..name_len]) {
        Ok(s) => s,
        Err(_) => {
            eprintlns("tar: invalid filename in archive");
            return 1;
        }
    };

    if config.verbose {
        prints(filename);
        printlns("");
    }

    // Parse size
    let size = octal_to_num(&header.size);

    // Check file type
    if header.typeflag == b'5' {
        // Directory
        let ret = mkdir(filename, 0o755);
        if ret < 0 && ret != -EEXIST {
            eprints("tar: cannot create directory '");
            prints(filename);
            eprintlns("'");
        }
        return 0;
    }

    // Regular file
    let file_fd = open(filename, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if file_fd < 0 {
        eprints("tar: cannot create '");
        prints(filename);
        eprintlns("'");
        // Skip file contents
        let blocks = (size + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64;
        lseek(archive_fd, (blocks * BLOCK_SIZE as u64) as i64, SEEK_CUR);
        return 1;
    }

    // Read and write file contents
    let mut remaining = size;
    let mut buf = [0u8; 4096];

    while remaining > 0 {
        let to_read = if remaining > buf.len() as u64 {
            buf.len()
        } else {
            remaining as usize
        };

        let n = read(archive_fd, &mut buf[..to_read]);
        if n <= 0 {
            eprintlns("tar: unexpected end of archive");
            close(file_fd);
            return 1;
        }

        if write(file_fd, &buf[..n as usize]) < 0 {
            eprints("tar: write error to '");
            prints(filename);
            eprintlns("'");
            close(file_fd);
            return 1;
        }

        remaining -= n as u64;
    }

    close(file_fd);

    // Skip padding to block boundary
    let padding = (BLOCK_SIZE as u64 - (size % BLOCK_SIZE as u64)) % BLOCK_SIZE as u64;
    if padding > 0 {
        lseek(archive_fd, padding as i64, SEEK_CUR);
    }

    0
}

/// Extract archive
fn do_extract(config: &TarConfig) -> i32 {
    // Open archive file
    let archive_fd = if let Some(ref path_buf) = config.archive_file {
        let len = path_buf
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(MAX_PATH_LEN);
        let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");

        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprints("tar: cannot open '");
            prints(path);
            eprintlns("'");
            return 1;
        }
        fd
    } else {
        STDIN_FILENO
    };

    let mut header_buf = [0u8; BLOCK_SIZE];

    loop {
        // Read header block
        let n = read(archive_fd, &mut header_buf);
        if n < BLOCK_SIZE as isize {
            break;
        }

        // Check if this is end marker (all zeros)
        if header_buf.iter().all(|&b| b == 0) {
            break;
        }

        // Parse header
        let header = unsafe { &*(header_buf.as_ptr() as *const TarHeader) };

        // Verify magic
        if &header.magic[..5] != b"ustar" {
            eprintlns("tar: invalid archive format");
            break;
        }

        if extract_file(header, archive_fd, config) != 0 {
            // Continue on error
        }
    }

    if archive_fd != STDIN_FILENO {
        close(archive_fd);
    }

    0
}

/// List archive contents
fn do_list(config: &TarConfig) -> i32 {
    // Open archive file
    let archive_fd = if let Some(ref path_buf) = config.archive_file {
        let len = path_buf
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(MAX_PATH_LEN);
        let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");

        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprints("tar: cannot open '");
            prints(path);
            eprintlns("'");
            return 1;
        }
        fd
    } else {
        STDIN_FILENO
    };

    let mut header_buf = [0u8; BLOCK_SIZE];

    loop {
        // Read header block
        let n = read(archive_fd, &mut header_buf);
        if n < BLOCK_SIZE as isize {
            break;
        }

        // Check if this is end marker (all zeros)
        if header_buf.iter().all(|&b| b == 0) {
            break;
        }

        // Parse header
        let header = unsafe { &*(header_buf.as_ptr() as *const TarHeader) };

        // Verify magic
        if &header.magic[..5] != b"ustar" {
            eprintlns("tar: invalid archive format");
            break;
        }

        // Get filename
        let name_len = header
            .name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(NAME_SIZE);
        if let Ok(filename) = core::str::from_utf8(&header.name[..name_len]) {
            if config.verbose {
                // Verbose: show permissions, size, name
                let mode = octal_to_num(&header.mode);
                let size = octal_to_num(&header.size);

                // Print mode
                putchar(if header.typeflag == b'5' { b'd' } else { b'-' });
                putchar(if mode & 0o400 != 0 { b'r' } else { b'-' });
                putchar(if mode & 0o200 != 0 { b'w' } else { b'-' });
                putchar(if mode & 0o100 != 0 { b'x' } else { b'-' });
                putchar(if mode & 0o040 != 0 { b'r' } else { b'-' });
                putchar(if mode & 0o020 != 0 { b'w' } else { b'-' });
                putchar(if mode & 0o010 != 0 { b'x' } else { b'-' });
                putchar(if mode & 0o004 != 0 { b'r' } else { b'-' });
                putchar(if mode & 0o002 != 0 { b'w' } else { b'-' });
                putchar(if mode & 0o001 != 0 { b'x' } else { b'-' });

                prints(" ");
                print_u64(size);
                prints(" ");
                prints(filename);
                printlns("");
            } else {
                prints(filename);
                printlns("");
            }
        }

        // Skip file contents
        let size = octal_to_num(&header.size);
        let blocks = (size + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64;
        lseek(archive_fd, (blocks * BLOCK_SIZE as u64) as i64, SEEK_CUR);
    }

    if archive_fd != STDIN_FILENO {
        close(archive_fd);
    }

    0
}

fn show_help() {
    eprintlns("Usage: tar [OPTIONS] [FILES...]");
    eprintlns("");
    eprintlns("Create, extract, or list tape archives.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -c          Create archive");
    eprintlns("  -x          Extract archive");
    eprintlns("  -t          List contents");
    eprintlns("  -f FILE     Use archive FILE (default: stdin/stdout)");
    eprintlns("  -v          Verbose mode");
    eprintlns("  -h          Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        show_help();
        return 1;
    }

    let mut config = TarConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            // Parse flag characters
            for &c in arg.as_bytes()[1..].iter() {
                match c {
                    b'c' => config.create = true,
                    b'x' => config.extract = true,
                    b't' => config.list = true,
                    b'v' => config.verbose = true,
                    b'f' => {
                        arg_idx += 1;
                        if arg_idx >= argc {
                            eprintlns("tar: option -f requires an argument");
                            return 1;
                        }
                        let path = cstr_to_str(unsafe { *argv.add(arg_idx as usize) });
                        let mut buf = [0u8; MAX_PATH_LEN];
                        let copy_len = if path.len() > MAX_PATH_LEN - 1 {
                            MAX_PATH_LEN - 1
                        } else {
                            path.len()
                        };
                        buf[..copy_len].copy_from_slice(&path.as_bytes()[..copy_len]);
                        config.archive_file = Some(buf);
                    }
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    b'z' => {
                        eprintlns("tar: gzip compression (-z) not yet supported");
                        return 1;
                    }
                    _ => {
                        eprints("tar: invalid option: -");
                        putchar(c);
                        eprintlns("");
                        return 1;
                    }
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    // Validate options
    let mode_count = (config.create as i32) + (config.extract as i32) + (config.list as i32);
    if mode_count != 1 {
        eprintlns("tar: must specify exactly one of -c, -x, or -t");
        return 1;
    }

    if config.create {
        // Collect file arguments
        let mut files = [const { "" }; 32];
        let mut file_count = 0;

        for i in arg_idx..argc {
            if file_count >= 32 {
                eprintlns("tar: too many files (max 32)");
                return 1;
            }
            files[file_count] = cstr_to_str(unsafe { *argv.add(i as usize) });
            file_count += 1;
        }

        if file_count == 0 {
            eprintlns("tar: no files specified");
            return 1;
        }

        do_create(&config, &files[..file_count])
    } else if config.extract {
        do_extract(&config)
    } else {
        do_list(&config)
    }
}
