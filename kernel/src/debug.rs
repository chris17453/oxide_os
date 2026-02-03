//! Debug output macros
//!
//! Conditional debug output based on Cargo features.
//! Enable with: cargo build --features debug-all
//! Or specific: cargo build --features debug-fork,debug-cow
//!
//! BlackLatch: Uses lock-free ring buffer to prevent debug feedback loops

/// Helper to write formatted debug to ring buffer
#[macro_export]
macro_rules! debug_to_buffer {
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
            struct BufferWriter;
            impl Write for BufferWriter {
                fn write_str(&mut self, s: &str) -> core::fmt::Result {
                    $crate::debug_buffer::write_debug(s.as_bytes());
                    Ok(())
                }
            }
            let mut writer = BufferWriter;
            let _ = writeln!(writer, $($arg)*);
        }
    };
}

/// Debug print for syscall operations
#[macro_export]
macro_rules! debug_syscall {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-syscall")]
        {
            $crate::debug_to_buffer!($($arg)*);
        }
    };
}

/// Debug print for fork/exec operations
#[macro_export]
macro_rules! debug_fork {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-fork")]
        {
            $crate::debug_to_buffer!($($arg)*);
        }
    };
}

/// Debug print for COW page fault operations
#[macro_export]
macro_rules! debug_cow {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-cow")]
        {
            $crate::debug_to_buffer!($($arg)*);
        }
    };
}

/// Debug print for process management operations
#[macro_export]
macro_rules! debug_proc {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-proc")]
        {
            $crate::debug_to_buffer!($($arg)*);
        }
    };
}

/// Debug print for scheduler context switches
#[macro_export]
macro_rules! debug_sched {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-sched")]
        {
            $crate::debug_to_buffer!($($arg)*);
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
            unsafe {
                arch_x86_64::serial::write_str_unsafe($s);
            }
        }
    };
    ($s:expr, byte $b:expr) => {
        #[cfg(feature = "debug-sched")]
        {
            unsafe {
                arch_x86_64::serial::write_byte_unsafe($b);
            }
        }
    };
}

/// Debug print for mouse/input operations
#[macro_export]
macro_rules! debug_mouse {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-mouse")]
        {
            $crate::debug_to_buffer!($($arg)*);
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
            unsafe {
                arch_x86_64::serial::write_str_unsafe($s);
            }
        }
    };
}

/// Debug print for input event path (userspace /dev/input)
#[macro_export]
macro_rules! debug_input {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-input")]
        {
            $crate::debug_to_buffer!($($arg)*);
        }
    };
}

/// Debug print for console I/O path tracing
#[macro_export]
macro_rules! debug_console {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-console")]
        {
            $crate::debug_to_buffer!($($arg)*);
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
