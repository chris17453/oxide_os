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
/// — PatchBay: Routes through os_log → console. No more serial.
#[macro_export]
macro_rules! debug_sched_unsafe {
    ($s:expr) => {
        #[cfg(feature = "debug-sched")]
        {
            unsafe {
                os_log::write_str_raw($s);
            }
        }
    };
    ($s:expr, byte $b:expr) => {
        #[cfg(feature = "debug-sched")]
        {
            unsafe {
                os_log::write_byte_raw($b);
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
/// — PatchBay: Routes through os_log → console. No more serial.
#[macro_export]
macro_rules! debug_mouse_unsafe {
    ($s:expr) => {
        #[cfg(feature = "debug-mouse")]
        {
            unsafe {
                os_log::write_str_raw($s);
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
/// — PatchBay: Routes through os_log → console. No more serial.
#[macro_export]
macro_rules! debug_lock_contention {
    ($lock_name:expr) => {
        #[cfg(feature = "debug-lock")]
        {
            unsafe {
                os_log::write_str_raw("[LOCK] contention: ");
                os_log::write_str_raw($lock_name);
                os_log::write_str_raw(" (ISR try_lock failed)\n");
            }
        }
    };
}
