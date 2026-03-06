//! ARM64 exception handling
//! — BlackLatch

/// ARM64 exception frame
///
/// This is the state saved when an exception occurs on ARM64.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ExceptionFrame {
    /// General purpose registers x0-x30
    pub x: [u64; 31],
    /// Stack pointer
    pub sp: u64,
    /// Program counter (ELR_EL1)
    pub elr: u64,
    /// Saved processor state (SPSR_EL1)
    pub spsr: u64,
}

impl Default for ExceptionFrame {
    fn default() -> Self {
        Self {
            x: [0; 31],
            sp: 0,
            elr: 0,
            spsr: 0,
        }
    }
}

// — NeonRoot: keyboard/mouse stubs so arch.rs doesn't need cfg gates.
// ARM uses different input (e.g., USB HID or device-tree GPIO).

/// Initialize PS/2 keyboard (no-op on ARM — uses USB HID or DT input)
pub fn init_ps2_keyboard() {}

/// Set keyboard IRQ callback (no-op on ARM)
pub unsafe fn set_keyboard_callback(_callback: fn()) {}

/// Get keyboard IRQ count (always 0 on ARM)
pub fn keyboard_irq_count() -> u64 { 0 }

/// Set mouse IRQ callback (no-op on ARM)
pub unsafe fn set_mouse_callback(_callback: fn()) {}

