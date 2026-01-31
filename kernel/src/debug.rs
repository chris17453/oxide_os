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

/// Debug print for scheduler context switches
///
/// Uses unsafe serial writes to avoid deadlock in interrupt context.
#[macro_export]
macro_rules! debug_sched {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-sched")]
        {
            use core::fmt::Write;
            let mut writer = $crate::serial_writer();
            let _ = writeln!(writer, $($arg)*);
        }
    };
}

/// Debug print for scheduler context switches (interrupt-safe)
///
/// Uses unsafe direct serial byte writes to avoid lock contention
/// in interrupt handlers where the normal serial writer may deadlock.
#[macro_export]
macro_rules! debug_sched_unsafe {
    ($s:expr) => {
        #[cfg(feature = "debug-sched")]
        {
            unsafe { arch_x86_64::serial::write_str_unsafe($s); }
        }
    };
    ($s:expr, byte $b:expr) => {
        #[cfg(feature = "debug-sched")]
        {
            unsafe { arch_x86_64::serial::write_byte_unsafe($b); }
        }
    };
}

/// Debug print for mouse/input operations
#[macro_export]
macro_rules! debug_mouse {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-mouse")]
        {
            use core::fmt::Write;
            let mut writer = $crate::serial_writer();
            let _ = writeln!(writer, $($arg)*);
        }
    };
}

/// Debug print for mouse/input operations (interrupt-safe)
///
/// Uses unsafe direct serial byte writes to avoid lock contention
/// in interrupt handlers where the normal serial writer may deadlock.
#[macro_export]
macro_rules! debug_mouse_unsafe {
    ($s:expr) => {
        #[cfg(feature = "debug-mouse")]
        {
            unsafe { arch_x86_64::serial::write_str_unsafe($s); }
        }
    };
}

/// Warn about lock contention in ISR context
///
/// Emits a serial warning when try_lock fails in an interrupt handler.
/// Uses unsafe direct serial writes since we're in ISR context.
#[macro_export]
macro_rules! debug_lock_contention {
    ($lock_name:expr) => {
        #[cfg(feature = "debug-lock")]
        {
            unsafe {
                arch_x86_64::serial::write_str_unsafe("[LOCK] contention: ");
                arch_x86_64::serial::write_str_unsafe($lock_name);
                arch_x86_64::serial::write_str_unsafe(" (ISR try_lock failed)\n");
            }
        }
    };
}
