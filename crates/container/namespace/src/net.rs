//! Network Namespace

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;
use crate::{alloc_ns_id, NsResult, NsError};

/// Network device in namespace
#[derive(Clone)]
pub struct NetDevice {
    /// Interface index
    pub ifindex: u32,
    /// Interface name
    pub name: String,
    /// MAC address
    pub mac: [u8; 6],
    /// MTU
    pub mtu: u32,
    /// Flags
    pub flags: u32,
    /// IPv4 addresses
    pub ipv4_addrs: Vec<Ipv4Addr>,
    /// IPv6 addresses
    pub ipv6_addrs: Vec<Ipv6Addr>,
}

/// IPv4 address
#[derive(Clone, Copy)]
pub struct Ipv4Addr {
    /// Address
    pub addr: u32,
    /// Prefix length
    pub prefix_len: u8,
}

/// IPv6 address
#[derive(Clone, Copy)]
pub struct Ipv6Addr {
    /// Address
    pub addr: [u8; 16],
    /// Prefix length
    pub prefix_len: u8,
}

/// Network namespace
pub struct NetNamespace {
    /// Unique namespace ID
    id: u64,
    /// Parent namespace
    parent: Option<Arc<NetNamespace>>,
    /// Network devices
    devices: RwLock<Vec<NetDevice>>,
    /// Next interface index
    next_ifindex: RwLock<u32>,
    /// Loopback configured
    loopback_up: RwLock<bool>,
}

impl NetNamespace {
    /// Create root network namespace
    pub fn root() -> Self {
        let ns = NetNamespace {
            id: alloc_ns_id(),
            parent: None,
            devices: RwLock::new(Vec::new()),
            next_ifindex: RwLock::new(1),
            loopback_up: RwLock::new(false),
        };

        // Add loopback device
        ns.add_loopback();
        ns
    }

    /// Create child network namespace
    pub fn new(parent: Option<Arc<NetNamespace>>) -> Self {
        let ns = NetNamespace {
            id: alloc_ns_id(),
            parent,
            devices: RwLock::new(Vec::new()),
            next_ifindex: RwLock::new(1),
            loopback_up: RwLock::new(false),
        };

        // Each new netns gets its own loopback
        ns.add_loopback();
        ns
    }

    /// Get namespace ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Add loopback device
    fn add_loopback(&self) {
        let ifindex = self.alloc_ifindex();
        let lo = NetDevice {
            ifindex,
            name: String::from("lo"),
            mac: [0; 6],
            mtu: 65536,
            flags: 0x0001, // IFF_UP
            ipv4_addrs: alloc::vec![Ipv4Addr {
                addr: 0x7F000001, // 127.0.0.1
                prefix_len: 8,
            }],
            ipv6_addrs: alloc::vec![Ipv6Addr {
                addr: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], // ::1
                prefix_len: 128,
            }],
        };
        self.devices.write().push(lo);
        *self.loopback_up.write() = true;
    }

    /// Allocate interface index
    fn alloc_ifindex(&self) -> u32 {
        let mut next = self.next_ifindex.write();
        let idx = *next;
        *next += 1;
        idx
    }

    /// Add a device to namespace
    pub fn add_device(&self, name: &str, mac: [u8; 6], mtu: u32) -> u32 {
        let ifindex = self.alloc_ifindex();
        let dev = NetDevice {
            ifindex,
            name: String::from(name),
            mac,
            mtu,
            flags: 0,
            ipv4_addrs: Vec::new(),
            ipv6_addrs: Vec::new(),
        };
        self.devices.write().push(dev);
        ifindex
    }

    /// Remove device from namespace
    pub fn remove_device(&self, ifindex: u32) -> NsResult<()> {
        let mut devices = self.devices.write();
        if let Some(pos) = devices.iter().position(|d| d.ifindex == ifindex) {
            // Don't allow removing loopback
            if devices[pos].name == "lo" {
                return Err(NsError::InvalidOperation);
            }
            devices.remove(pos);
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }

    /// Get device by index
    pub fn get_device(&self, ifindex: u32) -> Option<NetDevice> {
        self.devices.read()
            .iter()
            .find(|d| d.ifindex == ifindex)
            .cloned()
    }

    /// Get device by name
    pub fn get_device_by_name(&self, name: &str) -> Option<NetDevice> {
        self.devices.read()
            .iter()
            .find(|d| d.name == name)
            .cloned()
    }

    /// List all devices
    pub fn devices(&self) -> Vec<NetDevice> {
        self.devices.read().clone()
    }

    /// Set device up
    pub fn set_device_up(&self, ifindex: u32) -> NsResult<()> {
        let mut devices = self.devices.write();
        if let Some(dev) = devices.iter_mut().find(|d| d.ifindex == ifindex) {
            dev.flags |= 0x0001; // IFF_UP
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }

    /// Set device down
    pub fn set_device_down(&self, ifindex: u32) -> NsResult<()> {
        let mut devices = self.devices.write();
        if let Some(dev) = devices.iter_mut().find(|d| d.ifindex == ifindex) {
            dev.flags &= !0x0001;
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }

    /// Add IPv4 address to device
    pub fn add_ipv4(&self, ifindex: u32, addr: Ipv4Addr) -> NsResult<()> {
        let mut devices = self.devices.write();
        if let Some(dev) = devices.iter_mut().find(|d| d.ifindex == ifindex) {
            dev.ipv4_addrs.push(addr);
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }

    /// Move device to another namespace
    pub fn move_device(&self, ifindex: u32, target: &NetNamespace) -> NsResult<()> {
        let mut src_devices = self.devices.write();

        if let Some(pos) = src_devices.iter().position(|d| d.ifindex == ifindex) {
            // Don't move loopback
            if src_devices[pos].name == "lo" {
                return Err(NsError::InvalidOperation);
            }

            let mut dev = src_devices.remove(pos);
            // Assign new ifindex in target namespace
            dev.ifindex = target.alloc_ifindex();
            target.devices.write().push(dev);
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }
}
