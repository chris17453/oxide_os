//! C type aliases — making std's os/fd modules happy.
//!
//! — PulseForge: std's fd handling code expects c_int, c_char, etc.
//! These aliases map Rust's native types to C ABI conventions.
//! Nothing clever here, just type-level duct tape.

/// C int (32-bit signed on x86_64)
pub type c_int = i32;
/// C unsigned int
pub type c_uint = u32;
/// C long (64-bit on x86_64)
pub type c_long = i64;
/// C unsigned long
pub type c_ulong = u64;
/// C char (signed on x86_64)
pub type c_char = i8;
/// C unsigned char
pub type c_uchar = u8;
/// C short
pub type c_short = i16;
/// C unsigned short
pub type c_ushort = u16;
/// C size_t
pub type c_size_t = usize;
/// C ssize_t
pub type c_ssize_t = isize;
/// C void (actually u8 for pointer arithmetic)
pub type c_void = core::ffi::c_void;
/// C off_t (64-bit file offset)
pub type off_t = i64;
/// C mode_t (file mode)
pub type mode_t = u32;
/// C pid_t
pub type pid_t = i32;
/// C uid_t
pub type uid_t = u32;
/// C gid_t
pub type gid_t = u32;

/// Raw file descriptor type (used by std::os::fd)
pub type RawFd = c_int;

/// Standard file descriptors
pub const STDIN_FILENO: c_int = 0;
pub const STDOUT_FILENO: c_int = 1;
pub const STDERR_FILENO: c_int = 2;

/// F_DUPFD_CLOEXEC for fcntl (not used on oxide, but provided for compat)
pub const F_DUPFD_CLOEXEC: c_int = 1030;
