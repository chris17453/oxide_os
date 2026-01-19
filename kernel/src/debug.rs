//! Debug output macros
//!
//! Conditional debug output based on Cargo features.
//! Enable with: cargo build --features debug-all
//! Or specific: cargo build --features debug-fork,debug-cow

/// Debug print for syscall operations
#[macro_export]
macro_rules! debug_syscall {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-syscall")]
        {
            use core::fmt::Write;
            let mut writer = $crate::serial_writer();
            let _ = writeln!(writer, $($arg)*);
        }
    };
}

/// Debug print for fork/exec operations
#[macro_export]
macro_rules! debug_fork {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-fork")]
        {
            use core::fmt::Write;
            let mut writer = $crate::serial_writer();
            let _ = writeln!(writer, $($arg)*);
        }
    };
}

/// Debug print for COW page fault operations
#[macro_export]
macro_rules! debug_cow {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-cow")]
        {
            use core::fmt::Write;
            let mut writer = $crate::serial_writer();
            let _ = writeln!(writer, $($arg)*);
        }
    };
}

/// Debug print for process management operations
#[macro_export]
macro_rules! debug_proc {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-proc")]
        {
            use core::fmt::Write;
            let mut writer = $crate::serial_writer();
            let _ = writeln!(writer, $($arg)*);
        }
    };
}
