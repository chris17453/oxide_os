//! UTS Namespace (hostname and domain name)

use alloc::string::String;
use alloc::sync::Arc;
use spin::RwLock;
use crate::{alloc_ns_id, NsResult, NsError};

/// Maximum hostname length
pub const HOST_NAME_MAX: usize = 64;
/// Maximum domain name length
pub const DOMAIN_NAME_MAX: usize = 64;

/// UTS namespace
pub struct UtsNamespace {
    /// Unique namespace ID
    id: u64,
    /// Parent namespace
    parent: Option<Arc<UtsNamespace>>,
    /// Hostname
    hostname: RwLock<String>,
    /// Domain name (NIS domain)
    domainname: RwLock<String>,
}

impl UtsNamespace {
    /// Create root UTS namespace
    pub fn root() -> Self {
        UtsNamespace {
            id: alloc_ns_id(),
            parent: None,
            hostname: RwLock::new(String::from("efflux")),
            domainname: RwLock::new(String::from("(none)")),
        }
    }

    /// Create child UTS namespace (copies parent's values)
    pub fn new(parent: Option<Arc<UtsNamespace>>) -> Self {
        let (hostname, domainname) = if let Some(ref p) = parent {
            (p.hostname.read().clone(), p.domainname.read().clone())
        } else {
            (String::from("efflux"), String::from("(none)"))
        };

        UtsNamespace {
            id: alloc_ns_id(),
            parent,
            hostname: RwLock::new(hostname),
            domainname: RwLock::new(domainname),
        }
    }

    /// Get namespace ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get hostname
    pub fn hostname(&self) -> String {
        self.hostname.read().clone()
    }

    /// Set hostname
    pub fn set_hostname(&self, name: &str) -> NsResult<()> {
        if name.len() > HOST_NAME_MAX {
            return Err(NsError::InvalidOperation);
        }
        *self.hostname.write() = String::from(name);
        Ok(())
    }

    /// Get domain name
    pub fn domainname(&self) -> String {
        self.domainname.read().clone()
    }

    /// Set domain name
    pub fn set_domainname(&self, name: &str) -> NsResult<()> {
        if name.len() > DOMAIN_NAME_MAX {
            return Err(NsError::InvalidOperation);
        }
        *self.domainname.write() = String::from(name);
        Ok(())
    }

    /// Get uname info
    pub fn uname(&self) -> UnameInfo {
        UnameInfo {
            sysname: String::from("Efflux"),
            nodename: self.hostname.read().clone(),
            release: String::from("0.1.0"),
            version: String::from("#1"),
            machine: String::from("x86_64"),
            domainname: self.domainname.read().clone(),
        }
    }
}

/// Uname information
#[derive(Clone)]
pub struct UnameInfo {
    /// OS name
    pub sysname: String,
    /// Network node name (hostname)
    pub nodename: String,
    /// OS release
    pub release: String,
    /// OS version
    pub version: String,
    /// Hardware type
    pub machine: String,
    /// NIS domain name
    pub domainname: String,
}
