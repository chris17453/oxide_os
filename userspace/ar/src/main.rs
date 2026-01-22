//! OXIDE Archive Tool (ar)
//!
//! Create and manipulate archive files (static libraries).
//!
//! Usage:
//!   ar r archive.a file...   - Insert/replace files in archive
//!   ar t archive.a           - List archive contents
//!   ar x archive.a [file...] - Extract files from archive
//!   ar d archive.a file...   - Delete files from archive

#![no_std]
#![no_main]

use libc::*;

/// Archive magic string
const AR_MAGIC: &[u8] = b"!<arch>\n";
const AR_MAGIC_LEN: usize = 8;

/// Archive member header size
const AR_HDR_SIZE: usize = 60;

/// Maximum file size (1MB)
const MAX_FILE_SIZE: usize = 1024 * 1024;

/// Maximum archive members
const MAX_MEMBERS: usize = 256;

/// Archive member header (60 bytes, ASCII)
#[repr(C)]
struct ArHeader {
    ar_name: [u8; 16], // Member name (/ terminated or space padded)
    ar_date: [u8; 12], // Modification time
    ar_uid: [u8; 6],   // Owner UID
    ar_gid: [u8; 6],   // Owner GID
    ar_mode: [u8; 8],  // File mode (octal)
    ar_size: [u8; 10], // File size (decimal)
    ar_fmag: [u8; 2],  // Magic: "`\n"
}

/// Archive member info
struct MemberInfo {
    name: [u8; 64],
    offset: usize, // Offset in archive file
    size: usize,
}

impl MemberInfo {
    const fn new() -> Self {
        MemberInfo {
            name: [0u8; 64],
            offset: 0,
            size: 0,
        }
    }
}

/// Main entry point
#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 3 {
        eprintlns("usage: ar {rtxd} archive [files...]");
        return 1;
    }

    let operation = get_arg(argv, 1);
    let archive = get_arg(argv, 2);

    if operation.is_empty() || archive.is_empty() {
        eprintlns("ar: invalid arguments");
        return 1;
    }

    let op = operation[0];

    match op {
        b'r' => {
            // Replace/insert files
            if argc < 4 {
                eprintlns("ar: no files specified");
                return 1;
            }
            let files: &[*const u8] =
                unsafe { core::slice::from_raw_parts(argv.add(3), (argc - 3) as usize) };
            ar_replace(archive, files)
        }
        b't' => {
            // List contents
            ar_list(archive)
        }
        b'x' => {
            // Extract files
            let files = if argc > 3 {
                unsafe { core::slice::from_raw_parts(argv.add(3), (argc - 3) as usize) }
            } else {
                &[]
            };
            ar_extract(archive, files)
        }
        b'd' => {
            // Delete files
            if argc < 4 {
                eprintlns("ar: no files specified");
                return 1;
            }
            let files = unsafe { core::slice::from_raw_parts(argv.add(3), (argc - 3) as usize) };
            ar_delete(archive, files)
        }
        _ => {
            eprints("ar: unknown operation '");
            putchar(op);
            eprintlns("'");
            1
        }
    }
}

/// Replace/insert files in archive
fn ar_replace(archive: &[u8], files: &[*const u8]) -> i32 {
    // Read existing archive if it exists
    let mut members: [MemberInfo; MAX_MEMBERS] = core::array::from_fn(|_| MemberInfo::new());
    let mut num_members = 0;
    let mut archive_data = [0u8; MAX_FILE_SIZE];
    let mut archive_len = 0;

    let archive_str = bytes_to_str(archive);
    let fd = open2(archive_str, O_RDONLY);
    if fd >= 0 {
        let n = syscall::sys_read(fd, &mut archive_data);
        close(fd);
        if n > 0 {
            archive_len = n as usize;
            num_members = read_archive_members(&archive_data, archive_len, &mut members);
        }
    }

    // Create output archive
    let out_fd = open(archive_str, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if out_fd < 0 {
        eprints("ar: cannot create ");
        eprintlns(archive_str);
        return 1;
    }

    // Write archive magic
    syscall::sys_write(out_fd, AR_MAGIC);

    // Track which existing members have been replaced
    let mut replaced = [false; MAX_MEMBERS];

    // First, write the new/updated files
    for &file_ptr in files {
        let file_name = ptr_to_slice(file_ptr);
        let base_name = get_basename(file_name);

        // Read file content
        let mut content = [0u8; MAX_FILE_SIZE];
        let content_len = read_file(file_name, &mut content);
        if content_len < 0 {
            eprints("ar: cannot read ");
            eprintlns(bytes_to_str(file_name));
            close(out_fd);
            return 1;
        }

        // Check if replacing existing member
        for i in 0..num_members {
            if bytes_eq_name(&members[i].name, base_name) {
                replaced[i] = true;
            }
        }

        // Write member
        write_member(out_fd, base_name, &content[..content_len as usize]);
    }

    // Write remaining existing members that weren't replaced
    for i in 0..num_members {
        if !replaced[i] {
            let start = members[i].offset;
            let size = members[i].size;
            if start + size <= archive_len {
                write_member(out_fd, &members[i].name, &archive_data[start..start + size]);
            }
        }
    }

    close(out_fd);
    0
}

/// List archive contents
fn ar_list(archive: &[u8]) -> i32 {
    let archive_str = bytes_to_str(archive);
    let fd = open2(archive_str, O_RDONLY);
    if fd < 0 {
        eprints("ar: cannot open ");
        eprintlns(archive_str);
        return 1;
    }

    let mut data = [0u8; MAX_FILE_SIZE];
    let n = syscall::sys_read(fd, &mut data);
    close(fd);

    if n <= AR_MAGIC_LEN as isize {
        eprintlns("ar: not an archive");
        return 1;
    }

    if &data[..AR_MAGIC_LEN] != AR_MAGIC {
        eprintlns("ar: not an archive");
        return 1;
    }

    let mut members: [MemberInfo; MAX_MEMBERS] = core::array::from_fn(|_| MemberInfo::new());
    let num_members = read_archive_members(&data, n as usize, &mut members);

    for i in 0..num_members {
        prints(bytes_to_str(&members[i].name));
        printlns("");
    }

    0
}

/// Extract files from archive
fn ar_extract(archive: &[u8], files: &[*const u8]) -> i32 {
    let archive_str = bytes_to_str(archive);
    let fd = open2(archive_str, O_RDONLY);
    if fd < 0 {
        eprints("ar: cannot open ");
        eprintlns(archive_str);
        return 1;
    }

    let mut data = [0u8; MAX_FILE_SIZE];
    let n = syscall::sys_read(fd, &mut data);
    close(fd);

    if n <= AR_MAGIC_LEN as isize {
        eprintlns("ar: not an archive");
        return 1;
    }

    if &data[..AR_MAGIC_LEN] != AR_MAGIC {
        eprintlns("ar: not an archive");
        return 1;
    }

    let mut members: [MemberInfo; MAX_MEMBERS] = core::array::from_fn(|_| MemberInfo::new());
    let num_members = read_archive_members(&data, n as usize, &mut members);

    let extract_all = files.is_empty();

    for i in 0..num_members {
        let should_extract = if extract_all {
            true
        } else {
            // Check if this file is in the list
            let mut found = false;
            for &file_ptr in files {
                let file_name = ptr_to_slice(file_ptr);
                if bytes_eq_name(&members[i].name, file_name) {
                    found = true;
                    break;
                }
            }
            found
        };

        if should_extract {
            let name = bytes_to_str(&members[i].name);
            let out_fd = open(name, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
            if out_fd < 0 {
                eprints("ar: cannot create ");
                eprintlns(name);
                continue;
            }

            let start = members[i].offset;
            let size = members[i].size;
            if start + size <= n as usize {
                syscall::sys_write(out_fd, &data[start..start + size]);
            }

            close(out_fd);
        }
    }

    0
}

/// Delete files from archive
fn ar_delete(archive: &[u8], files: &[*const u8]) -> i32 {
    let archive_str = bytes_to_str(archive);
    let fd = open2(archive_str, O_RDONLY);
    if fd < 0 {
        eprints("ar: cannot open ");
        eprintlns(archive_str);
        return 1;
    }

    let mut data = [0u8; MAX_FILE_SIZE];
    let n = syscall::sys_read(fd, &mut data);
    close(fd);

    if n <= AR_MAGIC_LEN as isize {
        eprintlns("ar: not an archive");
        return 1;
    }

    if &data[..AR_MAGIC_LEN] != AR_MAGIC {
        eprintlns("ar: not an archive");
        return 1;
    }

    let mut members: [MemberInfo; MAX_MEMBERS] = core::array::from_fn(|_| MemberInfo::new());
    let num_members = read_archive_members(&data, n as usize, &mut members);

    // Create output archive
    let out_fd = open(archive_str, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if out_fd < 0 {
        eprints("ar: cannot create ");
        eprintlns(archive_str);
        return 1;
    }

    // Write archive magic
    syscall::sys_write(out_fd, AR_MAGIC);

    // Write members that are not being deleted
    for i in 0..num_members {
        let mut should_delete = false;
        for &file_ptr in files {
            let file_name = ptr_to_slice(file_ptr);
            if bytes_eq_name(&members[i].name, file_name) {
                should_delete = true;
                break;
            }
        }

        if !should_delete {
            let start = members[i].offset;
            let size = members[i].size;
            if start + size <= n as usize {
                write_member(out_fd, &members[i].name, &data[start..start + size]);
            }
        }
    }

    close(out_fd);
    0
}

/// Read archive members into array
fn read_archive_members(data: &[u8], len: usize, members: &mut [MemberInfo]) -> usize {
    if len < AR_MAGIC_LEN || &data[..AR_MAGIC_LEN] != AR_MAGIC {
        return 0;
    }

    let mut pos = AR_MAGIC_LEN;
    let mut count = 0;

    while pos + AR_HDR_SIZE <= len && count < members.len() {
        let hdr = unsafe { &*(data.as_ptr().add(pos) as *const ArHeader) };

        // Verify header magic
        if hdr.ar_fmag != [0x60, 0x0A] {
            break;
        }

        // Parse size
        let size = parse_decimal(&hdr.ar_size);

        // Parse name (remove trailing / or spaces)
        let name = parse_name(&hdr.ar_name);
        copy_bytes(&mut members[count].name, name);

        members[count].offset = pos + AR_HDR_SIZE;
        members[count].size = size;

        count += 1;

        // Move to next member (aligned to 2 bytes)
        pos += AR_HDR_SIZE + size;
        if pos % 2 != 0 {
            pos += 1;
        }
    }

    count
}

/// Write a member to the archive
fn write_member(fd: i32, name: &[u8], content: &[u8]) {
    let mut hdr = ArHeader {
        ar_name: [b' '; 16],
        ar_date: [b' '; 12],
        ar_uid: [b' '; 6],
        ar_gid: [b' '; 6],
        ar_mode: [b' '; 8],
        ar_size: [b' '; 10],
        ar_fmag: [0x60, 0x0A],
    };

    // Copy name (max 15 chars + /)
    let name_len = name
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(name.len())
        .min(15);
    hdr.ar_name[..name_len].copy_from_slice(&name[..name_len]);
    hdr.ar_name[name_len] = b'/';

    // Set date to 0
    hdr.ar_date[0] = b'0';

    // Set uid/gid to 0
    hdr.ar_uid[0] = b'0';
    hdr.ar_gid[0] = b'0';

    // Set mode to 644
    copy_to_field(&mut hdr.ar_mode, b"100644");

    // Set size
    let mut size_buf = [0u8; 16];
    let size_str = format_decimal(content.len(), &mut size_buf);
    copy_to_field(&mut hdr.ar_size, size_str);

    // Write header
    let hdr_bytes =
        unsafe { core::slice::from_raw_parts(&hdr as *const ArHeader as *const u8, AR_HDR_SIZE) };
    syscall::sys_write(fd, hdr_bytes);

    // Write content
    syscall::sys_write(fd, content);

    // Pad to even boundary
    if content.len() % 2 != 0 {
        syscall::sys_write(fd, b"\n");
    }
}

/// Parse decimal number from space-padded field
fn parse_decimal(field: &[u8]) -> usize {
    let mut result = 0usize;
    for &c in field {
        if c >= b'0' && c <= b'9' {
            result = result * 10 + (c - b'0') as usize;
        } else if c == b' ' {
            break;
        }
    }
    result
}

/// Parse name from field (remove trailing / or spaces)
fn parse_name(field: &[u8]) -> &[u8] {
    let mut end = field.len();
    for i in (0..field.len()).rev() {
        if field[i] != b' ' && field[i] != b'/' {
            end = i + 1;
            break;
        }
    }
    &field[..end]
}

/// Format decimal number into buffer
fn format_decimal(mut n: usize, buf: &mut [u8]) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }

    let mut i = 0;
    while n > 0 && i < buf.len() {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    // Reverse
    for j in 0..i / 2 {
        buf.swap(j, i - 1 - j);
    }

    &buf[..i]
}

/// Copy to space-padded field
fn copy_to_field(field: &mut [u8], src: &[u8]) {
    let len = src.len().min(field.len());
    field[..len].copy_from_slice(&src[..len]);
}

/// Read file content
fn read_file(name: &[u8], buf: &mut [u8]) -> isize {
    let fd = open2(bytes_to_str(name), O_RDONLY);
    if fd < 0 {
        return -1;
    }

    let n = syscall::sys_read(fd, buf);
    close(fd);
    n
}

/// Copy bytes
fn copy_bytes(dst: &mut [u8], src: &[u8]) {
    let len = src
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(src.len())
        .min(dst.len() - 1);
    dst[..len].copy_from_slice(&src[..len]);
    if len < dst.len() {
        dst[len] = 0;
    }
}

/// Compare names (null-terminated or space-terminated)
fn bytes_eq_name(a: &[u8], b: &[u8]) -> bool {
    let a_len = a
        .iter()
        .position(|&c| c == 0 || c == b' ' || c == b'/')
        .unwrap_or(a.len());
    let b_len = b
        .iter()
        .position(|&c| c == 0 || c == b' ' || c == b'/')
        .unwrap_or(b.len());
    if a_len != b_len {
        return false;
    }
    for i in 0..a_len {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

/// Get basename of path
fn get_basename(path: &[u8]) -> &[u8] {
    let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    let last_slash = path[..len].iter().rposition(|&c| c == b'/');
    match last_slash {
        Some(pos) => &path[pos + 1..len],
        None => &path[..len],
    }
}

/// Convert pointer to slice
fn ptr_to_slice(ptr: *const u8) -> &'static [u8] {
    if ptr.is_null() {
        return b"";
    }
    unsafe {
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::slice::from_raw_parts(ptr, len)
    }
}

/// Get argument at index
fn get_arg(argv: *const *const u8, idx: usize) -> &'static [u8] {
    ptr_to_slice(unsafe { *argv.add(idx) })
}

/// Convert byte slice to str
fn bytes_to_str(s: &[u8]) -> &str {
    let len = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    unsafe { core::str::from_utf8_unchecked(&s[..len]) }
}
