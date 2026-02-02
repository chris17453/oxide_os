//! MIPS64 (SGI) architecture-specific implementations
//!
//! ⚠️ BIG-ENDIAN architecture

pub mod start;
pub mod syscall;

pub use start::_start;
