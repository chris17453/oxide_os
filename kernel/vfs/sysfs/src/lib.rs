//! System filesystem (sysfs) for OXIDE OS
//!
//! Provides /sys with kernel object hierarchy populated from real hardware.
//!
//! Structure:
//! - /sys/kernel/       — kernel subsystem info
//! - /sys/devices/      — device tree (PCI devices from pci::devices())
//! - /sys/bus/pci/devices/ — PCI device links
//! - /sys/class/        — device classes
//! - /sys/firmware/     — firmware interfaces
//!
//! — IronGhost: the machine's nervous system laid bare. Every file under
//! /sys is a window into hardware reality — vendor IDs, device classes,
//! BAR addresses, interrupt lines. No more guessing what's plugged in.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

// ============================================================================
// Helpers
// ============================================================================

/// — NightDoc: read-only sysfs attribute file. Returns a fixed string.
struct SysAttr {
    ino: u64,
    content: String,
}

impl SysAttr {
    fn new(ino: u64, content: String) -> Arc<Self> {
        Arc::new(Self { ino, content })
    }
}

impl VnodeOps for SysAttr {
    fn vtype(&self) -> VnodeType { VnodeType::File }
    fn lookup(&self, _: &str) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::NotDirectory) }
    fn readdir(&self, _: u64) -> VfsResult<Option<DirEntry>> { Err(VfsError::NotDirectory) }
    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let data = self.content.as_bytes();
        let off = offset as usize;
        if off >= data.len() { return Ok(0); }
        let len = core::cmp::min(buf.len(), data.len() - off);
        buf[..len].copy_from_slice(&data[off..off + len]);
        Ok(len)
    }
    fn write(&self, _: u64, _: &[u8]) -> VfsResult<usize> { Err(VfsError::ReadOnly) }
    fn stat(&self) -> VfsResult<Stat> {
        let mut s = Stat::new(VnodeType::File, Mode::new(0o444), 0, self.ino);
        s.size = self.content.len() as u64;
        Ok(s)
    }
    fn create(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn mkdir(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn rmdir(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn unlink(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn rename(&self, _: &str, _: &dyn VnodeOps, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn truncate(&self, _: u64) -> VfsResult<()> { Err(VfsError::ReadOnly) }
}

/// — NightDoc: helper macro for VnodeOps directory boilerplate
macro_rules! impl_dir_boilerplate {
    ($ty:ty, $ino_field:ident) => {
        impl $ty {
            fn dir_stat(&self) -> VfsResult<Stat> {
                Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.$ino_field))
            }
        }
    };
}

// ============================================================================
// Root: /sys
// ============================================================================

pub struct SysFs { ino: u64 }

impl SysFs {
    pub fn new() -> Arc<Self> { Arc::new(SysFs { ino: 1 }) }
}

const SYS_ENTRIES: &[&str] = &["kernel", "devices", "class", "bus", "firmware"];

impl VnodeOps for SysFs {
    fn vtype(&self) -> VnodeType { VnodeType::Directory }
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        match name {
            "kernel" => Ok(Arc::new(SysKernel { ino: 10 })),
            "devices" => Ok(Arc::new(SysDevices { ino: 20 })),
            "class" => Ok(Arc::new(SysEmpty { ino: 30 })),
            "bus" => Ok(Arc::new(SysBus { ino: 40 })),
            "firmware" => Ok(Arc::new(SysEmpty { ino: 50 })),
            _ => Err(VfsError::NotFound),
        }
    }
    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let idx = offset as usize;
        match idx {
            0 => Ok(Some(DirEntry { name: String::from("."), ino: self.ino, file_type: VnodeType::Directory })),
            1 => Ok(Some(DirEntry { name: String::from(".."), ino: self.ino, file_type: VnodeType::Directory })),
            _ => {
                let i = idx - 2;
                if i < SYS_ENTRIES.len() {
                    Ok(Some(DirEntry { name: String::from(SYS_ENTRIES[i]), ino: (i as u64 + 1) * 10, file_type: VnodeType::Directory }))
                } else { Ok(None) }
            }
        }
    }
    fn stat(&self) -> VfsResult<Stat> { Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino)) }
    fn read(&self, _: u64, _: &mut [u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn write(&self, _: u64, _: &[u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn create(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn mkdir(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn rmdir(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn unlink(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn rename(&self, _: &str, _: &dyn VnodeOps, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn truncate(&self, _: u64) -> VfsResult<()> { Err(VfsError::ReadOnly) }
}

// ============================================================================
// /sys/devices — dynamic PCI device enumeration
// — IronGhost: queries pci::devices() at readdir time. Each device gets a
// subdirectory named "0000:BB:DD.F" with attribute files for vendor, device,
// class, subsystem_vendor, subsystem_device, irq, resource (BARs).
// ============================================================================

struct SysDevices { ino: u64 }

/// Format a PCI address as "0000:BB:DD.F"
fn pci_addr_str(dev: &pci::PciDevice) -> String {
    format!("0000:{:02x}:{:02x}.{}", dev.address.bus, dev.address.device, dev.address.function)
}

impl VnodeOps for SysDevices {
    fn vtype(&self) -> VnodeType { VnodeType::Directory }
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        // — IronGhost: find the PCI device matching this name
        let devices = pci::devices();
        for (i, dev) in devices.iter().enumerate() {
            if pci_addr_str(dev) == name {
                return Ok(Arc::new(SysPciDevice { ino: 1000 + i as u64, dev: dev.clone() }));
            }
        }
        // Also check for "pci0000:00" (root bus)
        if name == "pci0000:00" {
            return Ok(Arc::new(SysDevices { ino: 21 })); // recurse — same listing
        }
        Err(VfsError::NotFound)
    }
    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let idx = offset as usize;
        match idx {
            0 => Ok(Some(DirEntry { name: String::from("."), ino: self.ino, file_type: VnodeType::Directory })),
            1 => Ok(Some(DirEntry { name: String::from(".."), ino: 1, file_type: VnodeType::Directory })),
            _ => {
                let devices = pci::devices();
                let i = idx - 2;
                if i < devices.len() {
                    Ok(Some(DirEntry {
                        name: pci_addr_str(&devices[i]),
                        ino: 1000 + i as u64,
                        file_type: VnodeType::Directory,
                    }))
                } else { Ok(None) }
            }
        }
    }
    fn stat(&self) -> VfsResult<Stat> { Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino)) }
    fn read(&self, _: u64, _: &mut [u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn write(&self, _: u64, _: &[u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn create(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn mkdir(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn rmdir(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn unlink(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn rename(&self, _: &str, _: &dyn VnodeOps, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn truncate(&self, _: u64) -> VfsResult<()> { Err(VfsError::ReadOnly) }
}

// ============================================================================
// /sys/devices/0000:BB:DD.F — individual PCI device attributes
// ============================================================================

struct SysPciDevice {
    ino: u64,
    dev: pci::PciDevice,
}

const PCI_DEV_ATTRS: &[&str] = &["vendor", "device", "class", "revision", "irq", "subsystem_vendor", "subsystem_device"];

impl VnodeOps for SysPciDevice {
    fn vtype(&self) -> VnodeType { VnodeType::Directory }
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        let base_ino = self.ino * 100;
        match name {
            "vendor" => Ok(SysAttr::new(base_ino + 1, format!("0x{:04x}\n", self.dev.vendor_id))),
            "device" => Ok(SysAttr::new(base_ino + 2, format!("0x{:04x}\n", self.dev.device_id))),
            "class" => Ok(SysAttr::new(base_ino + 3, format!("0x{:02x}{:02x}{:02x}\n", self.dev.class_code, self.dev.subclass, self.dev.prog_if))),
            "revision" => Ok(SysAttr::new(base_ino + 4, format!("0x{:02x}\n", self.dev.revision))),
            "irq" => Ok(SysAttr::new(base_ino + 5, format!("{}\n", self.dev.interrupt_line))),
            "subsystem_vendor" => Ok(SysAttr::new(base_ino + 6, format!("0x{:04x}\n", self.dev.vendor_id))),
            "subsystem_device" => Ok(SysAttr::new(base_ino + 7, format!("0x{:04x}\n", self.dev.device_id))),
            _ => Err(VfsError::NotFound),
        }
    }
    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let idx = offset as usize;
        match idx {
            0 => Ok(Some(DirEntry { name: String::from("."), ino: self.ino, file_type: VnodeType::Directory })),
            1 => Ok(Some(DirEntry { name: String::from(".."), ino: 20, file_type: VnodeType::Directory })),
            _ => {
                let i = idx - 2;
                if i < PCI_DEV_ATTRS.len() {
                    Ok(Some(DirEntry {
                        name: String::from(PCI_DEV_ATTRS[i]),
                        ino: self.ino * 100 + i as u64 + 1,
                        file_type: VnodeType::File,
                    }))
                } else { Ok(None) }
            }
        }
    }
    fn stat(&self) -> VfsResult<Stat> { Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino)) }
    fn read(&self, _: u64, _: &mut [u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn write(&self, _: u64, _: &[u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn create(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn mkdir(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn rmdir(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn unlink(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn rename(&self, _: &str, _: &dyn VnodeOps, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn truncate(&self, _: u64) -> VfsResult<()> { Err(VfsError::ReadOnly) }
}

// ============================================================================
// /sys/bus — bus hierarchy
// ============================================================================

struct SysBus { ino: u64 }

impl VnodeOps for SysBus {
    fn vtype(&self) -> VnodeType { VnodeType::Directory }
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        match name {
            "pci" => Ok(Arc::new(SysBusPci { ino: 41 })),
            _ => Err(VfsError::NotFound),
        }
    }
    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        match offset as usize {
            0 => Ok(Some(DirEntry { name: String::from("."), ino: self.ino, file_type: VnodeType::Directory })),
            1 => Ok(Some(DirEntry { name: String::from(".."), ino: 1, file_type: VnodeType::Directory })),
            2 => Ok(Some(DirEntry { name: String::from("pci"), ino: 41, file_type: VnodeType::Directory })),
            _ => Ok(None),
        }
    }
    fn stat(&self) -> VfsResult<Stat> { Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino)) }
    fn read(&self, _: u64, _: &mut [u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn write(&self, _: u64, _: &[u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn create(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn mkdir(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn rmdir(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn unlink(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn rename(&self, _: &str, _: &dyn VnodeOps, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn truncate(&self, _: u64) -> VfsResult<()> { Err(VfsError::ReadOnly) }
}

/// /sys/bus/pci — contains "devices" subdir listing all PCI devices
struct SysBusPci { ino: u64 }

impl VnodeOps for SysBusPci {
    fn vtype(&self) -> VnodeType { VnodeType::Directory }
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        match name {
            "devices" => Ok(Arc::new(SysDevices { ino: 42 })),
            _ => Err(VfsError::NotFound),
        }
    }
    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        match offset as usize {
            0 => Ok(Some(DirEntry { name: String::from("."), ino: self.ino, file_type: VnodeType::Directory })),
            1 => Ok(Some(DirEntry { name: String::from(".."), ino: 40, file_type: VnodeType::Directory })),
            2 => Ok(Some(DirEntry { name: String::from("devices"), ino: 42, file_type: VnodeType::Directory })),
            _ => Ok(None),
        }
    }
    fn stat(&self) -> VfsResult<Stat> { Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino)) }
    fn read(&self, _: u64, _: &mut [u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn write(&self, _: u64, _: &[u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn create(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn mkdir(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn rmdir(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn unlink(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn rename(&self, _: &str, _: &dyn VnodeOps, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn truncate(&self, _: u64) -> VfsResult<()> { Err(VfsError::ReadOnly) }
}

// ============================================================================
// /sys/kernel
// ============================================================================

struct SysKernel { ino: u64 }

impl VnodeOps for SysKernel {
    fn vtype(&self) -> VnodeType { VnodeType::Directory }
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        match name {
            "version" => Ok(SysAttr::new(11, String::from("OXIDE OS 0.1.0\n"))),
            "hostname" => Ok(SysAttr::new(12, String::from("oxide\n"))),
            _ => Err(VfsError::NotFound),
        }
    }
    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        match offset as usize {
            0 => Ok(Some(DirEntry { name: String::from("."), ino: self.ino, file_type: VnodeType::Directory })),
            1 => Ok(Some(DirEntry { name: String::from(".."), ino: 1, file_type: VnodeType::Directory })),
            2 => Ok(Some(DirEntry { name: String::from("version"), ino: 11, file_type: VnodeType::File })),
            3 => Ok(Some(DirEntry { name: String::from("hostname"), ino: 12, file_type: VnodeType::File })),
            _ => Ok(None),
        }
    }
    fn stat(&self) -> VfsResult<Stat> { Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino)) }
    fn read(&self, _: u64, _: &mut [u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn write(&self, _: u64, _: &[u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn create(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn mkdir(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn rmdir(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn unlink(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn rename(&self, _: &str, _: &dyn VnodeOps, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn truncate(&self, _: u64) -> VfsResult<()> { Err(VfsError::ReadOnly) }
}

// ============================================================================
// Empty placeholder directory
// ============================================================================

struct SysEmpty { ino: u64 }

impl VnodeOps for SysEmpty {
    fn vtype(&self) -> VnodeType { VnodeType::Directory }
    fn lookup(&self, _: &str) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::NotFound) }
    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        match offset as usize {
            0 => Ok(Some(DirEntry { name: String::from("."), ino: self.ino, file_type: VnodeType::Directory })),
            1 => Ok(Some(DirEntry { name: String::from(".."), ino: 1, file_type: VnodeType::Directory })),
            _ => Ok(None),
        }
    }
    fn stat(&self) -> VfsResult<Stat> { Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino)) }
    fn read(&self, _: u64, _: &mut [u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn write(&self, _: u64, _: &[u8]) -> VfsResult<usize> { Err(VfsError::IsDirectory) }
    fn create(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn mkdir(&self, _: &str, _: Mode) -> VfsResult<Arc<dyn VnodeOps>> { Err(VfsError::ReadOnly) }
    fn rmdir(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn unlink(&self, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn rename(&self, _: &str, _: &dyn VnodeOps, _: &str) -> VfsResult<()> { Err(VfsError::ReadOnly) }
    fn truncate(&self, _: u64) -> VfsResult<()> { Err(VfsError::ReadOnly) }
}
