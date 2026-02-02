//! Minimal zlib stubs for CPython binascii module
//!
//! CPython's binascii optionally uses zlib for CRC32 calculations.
//! We provide minimal stubs to allow compilation.

use core::ffi::{c_int, c_long, c_uchar, c_uint, c_ulong};

/// zlib version string
static ZLIB_VERSION: &[u8] = b"1.2.11-stub\0";

/// z_stream structure (opaque, minimal)
#[repr(C)]
pub struct z_stream {
    next_in: *const c_uchar,
    avail_in: c_uint,
    total_in: c_ulong,
    next_out: *mut c_uchar,
    avail_out: c_uint,
    total_out: c_ulong,
    msg: *const u8,
    state: *mut u8,
    zalloc: *mut u8,
    zfree: *mut u8,
    opaque: *mut u8,
    data_type: c_int,
    adler: c_ulong,
    reserved: c_ulong,
}

// Error codes
pub const Z_OK: c_int = 0;
pub const Z_STREAM_END: c_int = 1;
pub const Z_NEED_DICT: c_int = 2;
pub const Z_ERRNO: c_int = -1;
pub const Z_STREAM_ERROR: c_int = -2;
pub const Z_DATA_ERROR: c_int = -3;
pub const Z_MEM_ERROR: c_int = -4;
pub const Z_BUF_ERROR: c_int = -5;
pub const Z_VERSION_ERROR: c_int = -6;

/// CRC32 - compute CRC32 checksum
///
/// This is a stub implementation using a simple algorithm.
/// For production, you'd want the actual zlib CRC32 table.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crc32(crc: c_ulong, buf: *const c_uchar, len: c_uint) -> c_ulong {
    if buf.is_null() {
        return 0;
    }

    let mut c = (crc as u32) ^ 0xFFFFFFFF;
    let slice = core::slice::from_raw_parts(buf, len as usize);

    for &byte in slice {
        c = crc32_table()[((c ^ (byte as u32)) & 0xFF) as usize] ^ (c >> 8);
    }

    (c ^ 0xFFFFFFFF) as c_ulong
}

/// CRC32 lookup table
fn crc32_table() -> &'static [u32; 256] {
    static TABLE: [u32; 256] = generate_crc32_table();
    &TABLE
}

/// Generate CRC32 lookup table at compile time
const fn generate_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = 0xEDB88320 ^ (crc >> 1);
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// zlibVersion - return zlib version string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zlibVersion() -> *const u8 {
    ZLIB_VERSION.as_ptr()
}

/// Stubs for compression functions (not implemented)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn compress(
    _dest: *mut c_uchar,
    _destLen: *mut c_ulong,
    _source: *const c_uchar,
    _sourceLen: c_ulong,
) -> c_int {
    Z_MEM_ERROR // Not implemented
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uncompress(
    _dest: *mut c_uchar,
    _destLen: *mut c_ulong,
    _source: *const c_uchar,
    _sourceLen: c_ulong,
) -> c_int {
    Z_MEM_ERROR // Not implemented
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn deflate(_strm: *mut z_stream, _flush: c_int) -> c_int {
    Z_STREAM_ERROR
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inflate(_strm: *mut z_stream, _flush: c_int) -> c_int {
    Z_STREAM_ERROR
}
