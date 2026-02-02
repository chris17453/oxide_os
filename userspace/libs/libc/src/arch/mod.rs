//! Architecture-specific implementations
//!
//! This module provides conditional compilation to select the correct
//! architecture-specific code at compile time.
//!
//! Supported architectures:
//! - x86_64: Intel/AMD 64-bit (little-endian, coherent caches)
//! - aarch64: ARM64 (little-endian, coherent caches)
//! - mips64: SGI MIPS64 (BIG-ENDIAN, non-coherent caches)
//!
//! — NeonRoot

// ============================================================================
// x86_64 Architecture
// ============================================================================

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use x86_64::syscall;

// ============================================================================
// ARM64 (aarch64) Architecture
// ============================================================================

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[cfg(target_arch = "aarch64")]
pub use aarch64::syscall;

// ============================================================================
// MIPS64 Architecture (SGI)
// ============================================================================

#[cfg(target_arch = "mips64")]
pub mod mips64;

#[cfg(target_arch = "mips64")]
pub use mips64::syscall;
