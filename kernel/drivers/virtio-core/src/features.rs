//! VirtIO Feature Bits
//!
//! Common feature flags shared across VirtIO devices.
//! Device-specific features are defined in their respective drivers.
//! — SableWire: negotiate or fail, there is no try

/// Device supports indirect descriptor tables (§5.1.5.3)
pub const INDIRECT_DESC: u64 = 1 << 28;

/// Device supports used buffer notifications suppression (§5.1.5.4.1)
pub const EVENT_IDX: u64 = 1 << 29;

/// Device operates in VirtIO 1.0+ mode (modern, not legacy)
pub const VERSION_1: u64 = 1 << 32;

/// Device can be accessed via IOMMU (memory access protection)
pub const ACCESS_PLATFORM: u64 = 1 << 33;

/// Device supports packed virtqueue layout (§2.7)
pub const RING_PACKED: u64 = 1 << 34;

/// Device supports in-order completion (§2.8)
pub const IN_ORDER: u64 = 1 << 35;

/// Device supports notification coalescing (§2.9)
pub const NOTIFICATION_DATA: u64 = 1 << 38;

/// Device supports selective reset (§2.5.1)
pub const RING_RESET: u64 = 1 << 40;
