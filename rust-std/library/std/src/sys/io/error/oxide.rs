//! — ByteRiot: OXIDE error mapping. Linux errno ABI — syscalls return -errno directly.
use crate::io;
use crate::sys::io::RawOsError;

pub fn errno() -> RawOsError {
    0 // OXIDE propagates errors via return values, not thread-local errno
}

pub fn is_interrupted(code: io::RawOsError) -> bool {
    code == 4 // EINTR
}

pub fn decode_error_kind(code: io::RawOsError) -> io::ErrorKind {
    match code {
        1 => io::ErrorKind::PermissionDenied,       // EPERM
        2 => io::ErrorKind::NotFound,               // ENOENT
        4 => io::ErrorKind::Interrupted,             // EINTR
        9 => io::ErrorKind::InvalidInput,            // EBADF
        11 => io::ErrorKind::WouldBlock,             // EAGAIN
        12 => io::ErrorKind::OutOfMemory,            // ENOMEM
        13 => io::ErrorKind::PermissionDenied,       // EACCES
        17 => io::ErrorKind::AlreadyExists,          // EEXIST
        20 => io::ErrorKind::NotADirectory,          // ENOTDIR
        21 => io::ErrorKind::IsADirectory,           // EISDIR
        22 => io::ErrorKind::InvalidInput,           // EINVAL
        28 => io::ErrorKind::StorageFull,            // ENOSPC
        32 => io::ErrorKind::BrokenPipe,             // EPIPE
        36 => io::ErrorKind::InvalidFilename,        // ENAMETOOLONG
        38 => io::ErrorKind::Unsupported,            // ENOSYS
        39 => io::ErrorKind::DirectoryNotEmpty,      // ENOTEMPTY
        110 => io::ErrorKind::TimedOut,              // ETIMEDOUT
        111 => io::ErrorKind::ConnectionRefused,     // ECONNREFUSED
        _ => io::ErrorKind::Uncategorized,
    }
}

pub fn error_string(errno: RawOsError) -> String {
    match errno {
        1 => "Operation not permitted".to_string(),
        2 => "No such file or directory".to_string(),
        4 => "Interrupted system call".to_string(),
        9 => "Bad file descriptor".to_string(),
        11 => "Resource temporarily unavailable".to_string(),
        12 => "Out of memory".to_string(),
        13 => "Permission denied".to_string(),
        17 => "File exists".to_string(),
        20 => "Not a directory".to_string(),
        21 => "Is a directory".to_string(),
        22 => "Invalid argument".to_string(),
        28 => "No space left on device".to_string(),
        32 => "Broken pipe".to_string(),
        36 => "File name too long".to_string(),
        38 => "Function not implemented".to_string(),
        39 => "Directory not empty".to_string(),
        110 => "Connection timed out".to_string(),
        111 => "Connection refused".to_string(),
        _ => format!("Unknown error {}", errno),
    }
}
