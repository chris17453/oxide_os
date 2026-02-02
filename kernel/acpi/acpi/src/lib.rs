//! ACPI table parsing for OXIDE OS
//!
//! Parses RSDP → RSDT/XSDT → MADT to enumerate Local APIC entries
//! for SMP CPU discovery. Operates on physical memory via a caller-supplied
//! mapping base (PHYS_MAP_BASE), so no allocator is needed.
//!
//! — SableWire: bare-metal firmware table walker

#![no_std]

pub mod madt;
pub mod rsdp;
pub mod sdt;

pub use madt::{MadtEntry, MadtLocalApic, parse_madt};
pub use rsdp::Rsdp;
pub use sdt::{SdtHeader, find_table};
