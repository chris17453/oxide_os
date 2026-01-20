//! cut - remove sections from each line of files

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE: usize = 4096;
const MAX_FIELDS: usize = 64;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut delimiter = b'\t';
    let mut fields: [usize; MAX_FIELDS] = [0; MAX_FIELDS];
    let mut field_count = 0;
    let mut chars: [usize; MAX_FIELDS] = [0; MAX_FIELDS];
    let mut char_count = 0;
    let mut use_fields = false;
    let mut use_chars = false;
    let mut arg_idx = 1;

    // Parse arguments
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        if arg == "-d" {
            arg_idx += 1;
            if arg_idx < argc {
                let delim_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
                if !delim_arg.is_empty() {
                    delimiter = delim_arg.as_bytes()[0];
                }
            }
            arg_idx += 1;
        } else if arg.starts_with("-d") {
            if arg.len() > 2 {
                delimiter = arg.as_bytes()[2];
            }
            arg_idx += 1;
        } else if arg == "-f" {
            arg_idx += 1;
            if arg_idx < argc {
                let fields_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
                field_count = parse_list(fields_arg, &mut fields);
                use_fields = true;
            }
            arg_idx += 1;
        } else if arg.starts_with("-f") {
            let fields_arg = &arg[2..];
            field_count = parse_list(fields_arg, &mut fields);
            use_fields = true;
            arg_idx += 1;
        } else if arg == "-c" {
            arg_idx += 1;
            if arg_idx < argc {
                let chars_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
                char_count = parse_list(chars_arg, &mut chars);
                use_chars = true;
            }
            arg_idx += 1;
        } else if arg.starts_with("-c") {
            let chars_arg = &arg[2..];
            char_count = parse_list(chars_arg, &mut chars);
            use_chars = true;
            arg_idx += 1;
        } else if !arg.starts_with('-') {
            break;
        } else {
            arg_idx += 1;
        }
    }

    if !use_fields && !use_chars {
        eprintlns("cut: you must specify a list of fields or characters");
        return 1;
    }

    // Process files or stdin
    if arg_idx >= argc {
        process_fd(STDIN_FILENO, delimiter, &fields[..field_count], &chars[..char_count], use_fields);
    } else {
        for i in arg_idx..argc {
            let path = unsafe { cstr_to_str(*argv.add(i as usize)) };
            if path == "-" {
                process_fd(STDIN_FILENO, delimiter, &fields[..field_count], &chars[..char_count], use_fields);
            } else {
                let fd = open2(path, O_RDONLY);
                if fd < 0 {
                    eprints("cut: ");
                    print(path);
                    eprintlns(": No such file");
                    continue;
                }
                process_fd(fd, delimiter, &fields[..field_count], &chars[..char_count], use_fields);
                close(fd);
            }
        }
    }

    0
}

fn parse_list(s: &str, out: &mut [usize; MAX_FIELDS]) -> usize {
    let mut count = 0;
    let mut current = 0usize;
    let mut in_range = false;
    let mut range_start = 0usize;

    for c in s.bytes() {
        if c >= b'0' && c <= b'9' {
            current = current * 10 + (c - b'0') as usize;
        } else if c == b',' {
            if in_range {
                // End of range
                for n in range_start..=current {
                    if count < MAX_FIELDS {
                        out[count] = n;
                        count += 1;
                    }
                }
                in_range = false;
            } else if current > 0 {
                if count < MAX_FIELDS {
                    out[count] = current;
                    count += 1;
                }
            }
            current = 0;
        } else if c == b'-' {
            range_start = if current > 0 { current } else { 1 };
            in_range = true;
            current = 0;
        }
    }

    // Handle last item
    if in_range {
        let end = if current > 0 { current } else { 1000 };
        for n in range_start..=end {
            if count < MAX_FIELDS {
                out[count] = n;
                count += 1;
            }
        }
    } else if current > 0 {
        if count < MAX_FIELDS {
            out[count] = current;
            count += 1;
        }
    }

    count
}

fn process_fd(fd: i32, delimiter: u8, fields: &[usize], chars: &[usize], use_fields: bool) {
    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == b'\n' {
                if use_fields {
                    cut_fields(&line[..line_len], delimiter, fields);
                } else {
                    cut_chars(&line[..line_len], chars);
                }
                putchar(b'\n');
                line_len = 0;
            } else if line_len < MAX_LINE - 1 {
                line[line_len] = buf[i];
                line_len += 1;
            }
        }
    }

    // Handle last line without newline
    if line_len > 0 {
        if use_fields {
            cut_fields(&line[..line_len], delimiter, fields);
        } else {
            cut_chars(&line[..line_len], chars);
        }
        putchar(b'\n');
    }
}

fn cut_fields(line: &[u8], delimiter: u8, fields: &[usize]) {
    // Split line into fields
    let mut field_starts: [usize; MAX_FIELDS] = [0; MAX_FIELDS];
    let mut field_ends: [usize; MAX_FIELDS] = [0; MAX_FIELDS];
    let mut num_fields = 0;

    let mut start = 0;
    for i in 0..line.len() {
        if line[i] == delimiter {
            if num_fields < MAX_FIELDS {
                field_starts[num_fields] = start;
                field_ends[num_fields] = i;
                num_fields += 1;
            }
            start = i + 1;
        }
    }
    // Last field
    if num_fields < MAX_FIELDS {
        field_starts[num_fields] = start;
        field_ends[num_fields] = line.len();
        num_fields += 1;
    }

    // Output selected fields
    let mut first = true;
    for &f in fields {
        if f > 0 && f <= num_fields {
            if !first {
                putchar(delimiter);
            }
            for i in field_starts[f - 1]..field_ends[f - 1] {
                putchar(line[i]);
            }
            first = false;
        }
    }
}

fn cut_chars(line: &[u8], chars: &[usize]) {
    for &c in chars {
        if c > 0 && c <= line.len() {
            putchar(line[c - 1]);
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
