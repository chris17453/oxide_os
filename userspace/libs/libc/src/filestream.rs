//! FILE stream implementation for C stdio
//!
//! Provides fopen/fclose/fread/fwrite/fgets/fputs/fseek/ftell etc.

use crate::errno;
use crate::fcntl::*;
use crate::syscall;

/// Calculate the length of a C string (excluding null terminator)
unsafe fn cstr_len(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

const FILE_BUF_SIZE: usize = 4096;
const MAX_OPEN_FILES: usize = 64;

/// Internal file stream state
#[repr(C)]
pub struct FILE {
    fd: i32,
    flags: u32,
    buf: [u8; FILE_BUF_SIZE],
    buf_pos: usize,
    buf_len: usize,
    error: i32,
    eof: i32,
    buf_mode: i32,
    ungetc_buf: i32, // -1 if empty
}

const FILE_FLAG_READ: u32 = 1;
const FILE_FLAG_WRITE: u32 = 2;
const FILE_FLAG_APPEND: u32 = 4;
const FILE_FLAG_BINARY: u32 = 8;
const FILE_FLAG_ALLOCATED: u32 = 16;

pub const _IONBF: i32 = 2;
pub const _IOLBF: i32 = 1;
pub const _IOFBF: i32 = 0;

static mut FILE_POOL: [FILE; MAX_OPEN_FILES] = {
    const EMPTY: FILE = FILE {
        fd: -1,
        flags: 0,
        buf: [0; FILE_BUF_SIZE],
        buf_pos: 0,
        buf_len: 0,
        error: 0,
        eof: 0,
        buf_mode: _IOFBF,
        ungetc_buf: -1,
    };
    [EMPTY; MAX_OPEN_FILES]
};

// Standard streams - slots 0, 1, 2
static mut STDIN_FILE: FILE = FILE {
    fd: 0,
    flags: FILE_FLAG_READ | FILE_FLAG_ALLOCATED,
    buf: [0; FILE_BUF_SIZE],
    buf_pos: 0,
    buf_len: 0,
    error: 0,
    eof: 0,
    buf_mode: _IOLBF,
    ungetc_buf: -1,
};

static mut STDOUT_FILE: FILE = FILE {
    fd: 1,
    flags: FILE_FLAG_WRITE | FILE_FLAG_ALLOCATED,
    buf: [0; FILE_BUF_SIZE],
    buf_pos: 0,
    buf_len: 0,
    error: 0,
    eof: 0,
    buf_mode: _IOLBF,
    ungetc_buf: -1,
};

static mut STDERR_FILE: FILE = FILE {
    fd: 2,
    flags: FILE_FLAG_WRITE | FILE_FLAG_ALLOCATED,
    buf: [0; FILE_BUF_SIZE],
    buf_pos: 0,
    buf_len: 0,
    error: 0,
    eof: 0,
    buf_mode: _IONBF,
    ungetc_buf: -1,
};

fn alloc_file() -> *mut FILE {
    unsafe {
        for i in 0..MAX_OPEN_FILES {
            if FILE_POOL[i].flags & FILE_FLAG_ALLOCATED == 0 {
                FILE_POOL[i].flags = FILE_FLAG_ALLOCATED;
                FILE_POOL[i].fd = -1;
                FILE_POOL[i].buf_pos = 0;
                FILE_POOL[i].buf_len = 0;
                FILE_POOL[i].error = 0;
                FILE_POOL[i].eof = 0;
                FILE_POOL[i].buf_mode = _IOFBF;
                FILE_POOL[i].ungetc_buf = -1;
                return &raw mut FILE_POOL[i];
            }
        }
        core::ptr::null_mut()
    }
}

fn parse_mode(mode: *const u8) -> (u32, i32) {
    unsafe {
        let c0 = *mode;
        let mut flags = 0u32;
        let mut oflags: u32;

        match c0 {
            b'r' => {
                flags |= FILE_FLAG_READ;
                oflags = O_RDONLY;
            }
            b'w' => {
                flags |= FILE_FLAG_WRITE;
                oflags = O_WRONLY | O_CREAT | O_TRUNC;
            }
            b'a' => {
                flags |= FILE_FLAG_WRITE | FILE_FLAG_APPEND;
                oflags = O_WRONLY | O_CREAT | O_APPEND;
            }
            _ => return (0, -1),
        }

        let mut i = 1;
        loop {
            let c = *mode.add(i);
            if c == 0 {
                break;
            }
            match c {
                b'+' => {
                    flags |= FILE_FLAG_READ | FILE_FLAG_WRITE;
                    oflags = (oflags & !(O_RDONLY | O_WRONLY)) | O_RDWR;
                }
                b'b' => {
                    flags |= FILE_FLAG_BINARY;
                }
                _ => {}
            }
            i += 1;
        }

        (flags, oflags as i32)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fopen(path: *const u8, mode: *const u8) -> *mut FILE {
    let (flags, oflags) = parse_mode(mode);
    if oflags < 0 {
        return core::ptr::null_mut();
    }

    let fd = syscall::syscall4(
        syscall::nr::OPEN,
        path as usize,
        cstr_len(path),
        oflags as usize,
        0o666,
    ) as isize;
    if fd < 0 {
        crate::set_errno(-fd as i32);
        return core::ptr::null_mut();
    }

    let f = alloc_file();
    if f.is_null() {
        syscall::syscall1(syscall::nr::CLOSE, fd as usize);
        crate::set_errno(errno::ENOMEM);
        return core::ptr::null_mut();
    }

    (*f).fd = fd as i32;
    (*f).flags = flags | FILE_FLAG_ALLOCATED;
    f
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fdopen(fd: i32, mode: *const u8) -> *mut FILE {
    let (flags, _) = parse_mode(mode);

    let f = alloc_file();
    if f.is_null() {
        return core::ptr::null_mut();
    }

    (*f).fd = fd;
    (*f).flags = flags | FILE_FLAG_ALLOCATED;
    f
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn freopen(path: *const u8, mode: *const u8, stream: *mut FILE) -> *mut FILE {
    if !stream.is_null() {
        flush_write_buf(stream);
        if (*stream).fd >= 3 {
            syscall::syscall1(syscall::nr::CLOSE, (*stream).fd as usize);
        }
    }

    if path.is_null() {
        return stream;
    }

    let (flags, oflags) = parse_mode(mode);
    if oflags < 0 {
        return core::ptr::null_mut();
    }

    let fd = syscall::syscall4(
        syscall::nr::OPEN,
        path as usize,
        cstr_len(path),
        oflags as usize,
        0o666,
    ) as isize;
    if fd < 0 {
        return core::ptr::null_mut();
    }

    let f = if stream.is_null() {
        alloc_file()
    } else {
        stream
    };
    if f.is_null() {
        syscall::syscall1(syscall::nr::CLOSE, fd as usize);
        return core::ptr::null_mut();
    }

    (*f).fd = fd as i32;
    (*f).flags = flags | FILE_FLAG_ALLOCATED;
    (*f).buf_pos = 0;
    (*f).buf_len = 0;
    (*f).error = 0;
    (*f).eof = 0;
    (*f).ungetc_buf = -1;
    f
}

unsafe fn flush_write_buf(f: *mut FILE) -> i32 {
    if (*f).buf_pos > 0 && (*f).flags & FILE_FLAG_WRITE != 0 {
        let written = syscall::sys_write((*f).fd, &(&(*f).buf)[..(*f).buf_pos]);
        if written < 0 {
            (*f).error = 1;
            return -1;
        }
        (*f).buf_pos = 0;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fclose(stream: *mut FILE) -> i32 {
    if stream.is_null() {
        return -1;
    }

    flush_write_buf(stream);
    let ret = if (*stream).fd >= 3 {
        syscall::sys_close((*stream).fd)
    } else {
        0
    };

    (*stream).flags = 0;
    (*stream).fd = -1;
    ret
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fflush(stream: *mut FILE) -> i32 {
    if stream.is_null() {
        // Flush all streams
        flush_write_buf(&raw mut STDOUT_FILE);
        flush_write_buf(&raw mut STDERR_FILE);
        for i in 0..MAX_OPEN_FILES {
            if FILE_POOL[i].flags & FILE_FLAG_ALLOCATED != 0 {
                flush_write_buf(&raw mut FILE_POOL[i]);
            }
        }
        return 0;
    }
    flush_write_buf(stream)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fread(
    ptr: *mut u8,
    size: usize,
    nmemb: usize,
    stream: *mut FILE,
) -> usize {
    if stream.is_null() || ptr.is_null() || size == 0 || nmemb == 0 {
        return 0;
    }

    let total = size * nmemb;
    let mut read_total = 0usize;

    while read_total < total {
        // Check ungetc buffer first
        if (*stream).ungetc_buf >= 0 {
            *ptr.add(read_total) = (*stream).ungetc_buf as u8;
            (*stream).ungetc_buf = -1;
            read_total += 1;
            continue;
        }

        let n = syscall::sys_read(
            (*stream).fd,
            core::slice::from_raw_parts_mut(ptr.add(read_total), total - read_total),
        );

        if n <= 0 {
            if n == 0 {
                (*stream).eof = 1;
            } else {
                (*stream).error = 1;
            }
            break;
        }
        read_total += n as usize;
    }

    read_total / size
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fwrite(
    ptr: *const u8,
    size: usize,
    nmemb: usize,
    stream: *mut FILE,
) -> usize {
    if stream.is_null() || ptr.is_null() || size == 0 || nmemb == 0 {
        return 0;
    }

    let total = size * nmemb;
    let data = core::slice::from_raw_parts(ptr, total);

    // For unbuffered or line-buffered stderr, write directly
    if (*stream).buf_mode == _IONBF {
        let n = syscall::sys_write((*stream).fd, data);
        if n < 0 {
            (*stream).error = 1;
            return 0;
        }
        return n as usize / size;
    }

    // Buffered write
    let mut written = 0usize;
    while written < total {
        let space = FILE_BUF_SIZE - (*stream).buf_pos;
        let chunk = core::cmp::min(space, total - written);

        core::ptr::copy_nonoverlapping(
            data.as_ptr().add(written),
            (*stream).buf.as_mut_ptr().add((*stream).buf_pos),
            chunk,
        );
        (*stream).buf_pos += chunk;
        written += chunk;

        // Flush if buffer full or line-buffered and has newline
        if (*stream).buf_pos >= FILE_BUF_SIZE {
            if flush_write_buf(stream) < 0 {
                return written / size;
            }
        } else if (*stream).buf_mode == _IOLBF {
            // Check for newline in what we just wrote
            for j in ((*stream).buf_pos - chunk)..(*stream).buf_pos {
                if (*stream).buf[j] == b'\n' {
                    if flush_write_buf(stream) < 0 {
                        return written / size;
                    }
                    break;
                }
            }
        }
    }

    written / size
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetc(stream: *mut FILE) -> i32 {
    if stream.is_null() {
        return -1;
    }

    if (*stream).ungetc_buf >= 0 {
        let c = (*stream).ungetc_buf;
        (*stream).ungetc_buf = -1;
        return c;
    }

    let mut c = 0u8;
    // — GraveShift: retry on EINTR so signal delivery doesn't fake an EOF
    let n = loop {
        let r = syscall::sys_read((*stream).fd, core::slice::from_raw_parts_mut(&mut c, 1));
        if r != -4 { break r; } // -4 = EINTR, retry
    };
    if n <= 0 {
        if n == 0 {
            (*stream).eof = 1;
        } else {
            (*stream).error = 1;
        }
        return -1;
    }
    c as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgets(s: *mut u8, size: i32, stream: *mut FILE) -> *mut u8 {
    if s.is_null() || size <= 0 || stream.is_null() {
        return core::ptr::null_mut();
    }

    let mut i = 0i32;
    while i < size - 1 {
        let c = fgetc(stream);
        if c < 0 {
            if i == 0 {
                return core::ptr::null_mut();
            }
            break;
        }
        *s.add(i as usize) = c as u8;
        i += 1;
        if c == b'\n' as i32 {
            break;
        }
    }
    *s.add(i as usize) = 0;
    s
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputc(c: i32, stream: *mut FILE) -> i32 {
    let byte = c as u8;
    if fwrite(&byte as *const u8, 1, 1, stream) == 1 {
        c
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputs(s: *const u8, stream: *mut FILE) -> i32 {
    if s.is_null() || stream.is_null() {
        return -1;
    }

    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }

    if fwrite(s, 1, len, stream) == len {
        0
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ungetc(c: i32, stream: *mut FILE) -> i32 {
    if stream.is_null() || c < 0 {
        return -1;
    }
    (*stream).ungetc_buf = c;
    (*stream).eof = 0;
    c
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fseek(stream: *mut FILE, offset: i64, whence: i32) -> i32 {
    if stream.is_null() {
        return -1;
    }
    flush_write_buf(stream);
    (*stream).ungetc_buf = -1;
    (*stream).eof = 0;

    let ret = syscall::sys_lseek((*stream).fd, offset, whence);
    if ret < 0 {
        (*stream).error = 1;
        -1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fseeko(stream: *mut FILE, offset: i64, whence: i32) -> i32 {
    fseek(stream, offset, whence)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftell(stream: *mut FILE) -> i64 {
    if stream.is_null() {
        return -1;
    }
    flush_write_buf(stream);
    syscall::sys_lseek((*stream).fd, 0, 1) // SEEK_CUR
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftello(stream: *mut FILE) -> i64 {
    ftell(stream)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rewind(stream: *mut FILE) {
    if !stream.is_null() {
        fseek(stream, 0, 0);
        (*stream).error = 0;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn feof(stream: *mut FILE) -> i32 {
    if stream.is_null() {
        return 0;
    }
    (*stream).eof
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ferror(stream: *mut FILE) -> i32 {
    if stream.is_null() {
        return 0;
    }
    (*stream).error
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearerr(stream: *mut FILE) {
    if !stream.is_null() {
        (*stream).error = 0;
        (*stream).eof = 0;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileno(stream: *mut FILE) -> i32 {
    if stream.is_null() {
        return -1;
    }
    (*stream).fd
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setvbuf(stream: *mut FILE, _buf: *mut u8, mode: i32, _size: usize) -> i32 {
    if stream.is_null() {
        return -1;
    }
    (*stream).buf_mode = mode;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setbuf(stream: *mut FILE, buf: *mut u8) {
    if buf.is_null() {
        setvbuf(stream, core::ptr::null_mut(), _IONBF, 0);
    } else {
        setvbuf(stream, buf, _IOFBF, FILE_BUF_SIZE);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getc(stream: *mut FILE) -> i32 {
    fgetc(stream)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn putc(c: i32, stream: *mut FILE) -> i32 {
    fputc(c, stream)
}

// Global stdin/stdout/stderr pointers
#[unsafe(no_mangle)]
pub static mut stdin: *mut FILE = core::ptr::null_mut();
#[unsafe(no_mangle)]
pub static mut stdout: *mut FILE = core::ptr::null_mut();
#[unsafe(no_mangle)]
pub static mut stderr: *mut FILE = core::ptr::null_mut();

/// Initialize standard streams - must be called from _start
pub unsafe fn init_stdio() {
    stdin = &raw mut STDIN_FILE;
    stdout = &raw mut STDOUT_FILE;
    stderr = &raw mut STDERR_FILE;
}

// tmpfile/tmpnam stubs
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tmpfile() -> *mut FILE {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tmpnam(_s: *mut u8) -> *mut u8 {
    core::ptr::null_mut()
}

// remove/rename
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remove(path: *const u8) -> i32 {
    let path_len = cstr_len(path);
    let ret = syscall::syscall2(syscall::nr::UNLINK, path as usize, path_len) as isize;
    if ret < 0 {
        let ret2 = syscall::syscall2(syscall::nr::RMDIR, path as usize, path_len) as isize;
        if ret2 < 0 {
            crate::set_errno(-ret2 as i32);
            return -1;
        }
    }
    0
}

// fgetpos/fsetpos
#[repr(C)]
pub struct FposT {
    offset: i64,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetpos(stream: *mut FILE, pos: *mut FposT) -> i32 {
    let off = ftell(stream);
    if off < 0 {
        return -1;
    }
    (*pos).offset = off;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fsetpos(stream: *mut FILE, pos: *const FposT) -> i32 {
    fseek(stream, (*pos).offset, 0)
}

// getline for C (POSIX)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getline(lineptr: *mut *mut u8, n: *mut usize, stream: *mut FILE) -> isize {
    getdelim(lineptr, n, b'\n' as i32, stream)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getdelim(
    lineptr: *mut *mut u8,
    n: *mut usize,
    delim: i32,
    stream: *mut FILE,
) -> isize {
    if lineptr.is_null() || n.is_null() || stream.is_null() {
        return -1;
    }

    // Allocate initial buffer if needed
    if (*lineptr).is_null() || *n == 0 {
        *n = 128;
        *lineptr = crate::c_exports::malloc(*n) as *mut u8;
        if (*lineptr).is_null() {
            return -1;
        }
    }

    let mut i = 0usize;
    loop {
        let c = fgetc(stream);
        if c < 0 {
            if i == 0 {
                return -1;
            }
            break;
        }

        // Grow buffer if needed
        if i + 1 >= *n {
            let new_n = *n * 2;
            let new_ptr = crate::c_exports::realloc(*lineptr as *mut u8, new_n) as *mut u8;
            if new_ptr.is_null() {
                return -1;
            }
            *lineptr = new_ptr;
            *n = new_n;
        }

        *(*lineptr).add(i) = c as u8;
        i += 1;

        if c == delim {
            break;
        }
    }

    *(*lineptr).add(i) = 0;
    i as isize
}
