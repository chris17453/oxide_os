//! Network Interface Management

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::{IpAddr, Ipv4Addr, Ipv6Addr, MacAddress, NetError, NetResult, NetworkDevice};

/// Interface configuration
#[derive(Debug, Clone)]
pub struct InterfaceConfig {
    /// Interface name
    pub name: String,
    /// IPv4 address
    pub ipv4_addr: Option<Ipv4Addr>,
    /// IPv4 netmask
    pub ipv4_netmask: Option<Ipv4Addr>,
    /// IPv4 broadcast
    pub ipv4_broadcast: Option<Ipv4Addr>,
    /// IPv4 gateway
    pub ipv4_gateway: Option<Ipv4Addr>,
    /// IPv6 addresses
    pub ipv6_addrs: Vec<Ipv6Addr>,
    /// MTU
    pub mtu: usize,
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        InterfaceConfig {
            name: String::new(),
            ipv4_addr: None,
            ipv4_netmask: None,
            ipv4_broadcast: None,
            ipv4_gateway: None,
            ipv6_addrs: Vec::new(),
            mtu: 1500,
        }
    }
}

/// Network interface
pub struct NetworkInterface {
    /// Device
    pub device: Arc<dyn NetworkDevice>,
    /// Configuration
    config: Mutex<InterfaceConfig>,
}

impl NetworkInterface {
    /// Create a new network interface
    pub fn new(device: Arc<dyn NetworkDevice>) -> Self {
        let config = InterfaceConfig {
            name: String::from(device.name()),
            mtu: device.mtu(),
            ..Default::default()
        };

        NetworkInterface {
            device,
            config: Mutex::new(config),
        }
    }

    /// Get interface name
    pub fn name(&self) -> String {
        self.config.lock().name.clone()
    }

    /// Get MAC address
    pub fn mac_address(&self) -> MacAddress {
        self.device.mac_address()
    }

    /// Get MTU
    pub fn mtu(&self) -> usize {
        self.config.lock().mtu
    }

    /// Set MTU
    pub fn set_mtu(&self, mtu: usize) -> NetResult<()> {
        if mtu < 68 || mtu > 65535 {
            return Err(NetError::InvalidArgument);
        }
        self.config.lock().mtu = mtu;
        Ok(())
    }

    /// Get IPv4 address
    pub fn ipv4_addr(&self) -> Option<Ipv4Addr> {
        self.config.lock().ipv4_addr
    }

    /// Set IPv4 address
    pub fn set_ipv4_addr(&self, addr: Ipv4Addr, netmask: Ipv4Addr) -> NetResult<()> {
        let mut config = self.config.lock();
        config.ipv4_addr = Some(addr);
        config.ipv4_netmask = Some(netmask);

        // Calculate broadcast
        let addr_u32 = addr.to_u32();
        let mask_u32 = netmask.to_u32();
        let broadcast = Ipv4Addr::from_u32(addr_u32 | !mask_u32);
        config.ipv4_broadcast = Some(broadcast);

        Ok(())
    }

    /// Get IPv4 netmask
    pub fn ipv4_netmask(&self) -> Option<Ipv4Addr> {
        self.config.lock().ipv4_netmask
    }

    /// Get IPv4 gateway
    pub fn ipv4_gateway(&self) -> Option<Ipv4Addr> {
        self.config.lock().ipv4_gateway
    }

    /// Set IPv4 gateway
    pub fn set_ipv4_gateway(&self, gateway: Ipv4Addr) -> NetResult<()> {
        self.config.lock().ipv4_gateway = Some(gateway);
        Ok(())
    }

    /// Add IPv6 address
    pub fn add_ipv6_addr(&self, addr: Ipv6Addr) -> NetResult<()> {
        self.config.lock().ipv6_addrs.push(addr);
        Ok(())
    }

    /// Get IPv6 addresses
    pub fn ipv6_addrs(&self) -> Vec<Ipv6Addr> {
        self.config.lock().ipv6_addrs.clone()
    }

    /// Check if link is up
    pub fn link_up(&self) -> bool {
        self.device.link_up()
    }

    /// Get configuration
    pub fn config(&self) -> InterfaceConfig {
        self.config.lock().clone()
    }

    /// Check if address belongs to this interface
    pub fn has_addr(&self, addr: IpAddr) -> bool {
        let config = self.config.lock();
        match addr {
            IpAddr::V4(v4) => config.ipv4_addr == Some(v4),
            IpAddr::V6(v6) => config.ipv6_addrs.contains(&v6),
        }
    }

    /// Check if address is on the same network
    pub fn same_network(&self, addr: Ipv4Addr) -> bool {
        let config = self.config.lock();
        if let (Some(my_addr), Some(netmask)) = (config.ipv4_addr, config.ipv4_netmask) {
            let my_net = my_addr.to_u32() & netmask.to_u32();
            let their_net = addr.to_u32() & netmask.to_u32();
            my_net == their_net
        } else {
            false
        }
    }
}

/// Global interface list
static INTERFACES: Mutex<Vec<Arc<NetworkInterface>>> = Mutex::new(Vec::new());

/// Add an interface
pub fn add_interface(iface: Arc<NetworkInterface>) {
    INTERFACES.lock().push(iface);
}

/// Get interface by name
pub fn get_interface(name: &str) -> Option<Arc<NetworkInterface>> {
    INTERFACES.lock().iter().find(|i| i.name() == name).cloned()
}

/// Get interface by index
pub fn get_interface_by_index(index: usize) -> Option<Arc<NetworkInterface>> {
    INTERFACES.lock().get(index).cloned()
}

/// Get all interfaces
pub fn interfaces() -> Vec<Arc<NetworkInterface>> {
    INTERFACES.lock().clone()
}

/// Find interface for destination
pub fn find_route(dest: Ipv4Addr) -> Option<Arc<NetworkInterface>> {
    let ifaces = INTERFACES.lock();

    // Check if destination is on a directly connected network
    for iface in ifaces.iter() {
        if iface.same_network(dest) {
            return Some(iface.clone());
        }
    }

    // Use default gateway
    for iface in ifaces.iter() {
        if iface.ipv4_gateway().is_some() {
            return Some(iface.clone());
        }
    }

    None
}
