//! Linux-compatible Namespaces for Container Isolation
//!
//! Implements PID, Mount, Network, User, UTS, IPC, and Cgroup namespaces.

#![no_std]

extern crate alloc;

pub mod ipc;
pub mod mount;
pub mod net;
pub mod pid;
pub mod user;
pub mod uts;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

pub use ipc::IpcNamespace;
pub use mount::MountNamespace;
pub use net::NetNamespace;
pub use pid::PidNamespace;
pub use user::UserNamespace;
pub use uts::UtsNamespace;

/// Namespace clone flags
pub mod flags {
    pub const CLONE_NEWPID: u64 = 0x20000000;
    pub const CLONE_NEWNS: u64 = 0x00020000;
    pub const CLONE_NEWNET: u64 = 0x40000000;
    pub const CLONE_NEWUSER: u64 = 0x10000000;
    pub const CLONE_NEWUTS: u64 = 0x04000000;
    pub const CLONE_NEWIPC: u64 = 0x08000000;
    pub const CLONE_NEWCGROUP: u64 = 0x02000000;
}

/// Namespace error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NsError {
    /// Permission denied
    PermissionDenied,
    /// No such namespace
    NotFound,
    /// Resource limit reached
    ResourceLimit,
    /// Invalid operation
    InvalidOperation,
    /// Namespace busy
    Busy,
}

/// Namespace result type
pub type NsResult<T> = Result<T, NsError>;

/// Namespace set for a process
#[derive(Clone)]
pub struct NamespaceSet {
    /// PID namespace
    pub pid: Arc<PidNamespace>,
    /// Mount namespace
    pub mount: Arc<MountNamespace>,
    /// Network namespace
    pub net: Arc<NetNamespace>,
    /// User namespace
    pub user: Arc<UserNamespace>,
    /// UTS namespace
    pub uts: Arc<UtsNamespace>,
    /// IPC namespace
    pub ipc: Arc<IpcNamespace>,
}

impl NamespaceSet {
    /// Create initial (root) namespace set
    pub fn init() -> Self {
        NamespaceSet {
            pid: Arc::new(PidNamespace::root()),
            mount: Arc::new(MountNamespace::root()),
            net: Arc::new(NetNamespace::root()),
            user: Arc::new(UserNamespace::root()),
            uts: Arc::new(UtsNamespace::root()),
            ipc: Arc::new(IpcNamespace::root()),
        }
    }

    /// Create new namespace set from parent with specified new namespaces
    pub fn unshare(&self, flags: u64) -> NsResult<Self> {
        let mut new = self.clone();

        if flags & flags::CLONE_NEWPID != 0 {
            new.pid = Arc::new(PidNamespace::new(Some(self.pid.clone())));
        }

        if flags & flags::CLONE_NEWNS != 0 {
            new.mount = Arc::new(MountNamespace::new(Some(self.mount.clone())));
        }

        if flags & flags::CLONE_NEWNET != 0 {
            new.net = Arc::new(NetNamespace::new(Some(self.net.clone())));
        }

        if flags & flags::CLONE_NEWUSER != 0 {
            new.user = Arc::new(UserNamespace::new(Some(self.user.clone())));
        }

        if flags & flags::CLONE_NEWUTS != 0 {
            new.uts = Arc::new(UtsNamespace::new(Some(self.uts.clone())));
        }

        if flags & flags::CLONE_NEWIPC != 0 {
            new.ipc = Arc::new(IpcNamespace::new(Some(self.ipc.clone())));
        }

        Ok(new)
    }
}

/// Global namespace ID counter
static NAMESPACE_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a new namespace ID
pub fn alloc_ns_id() -> u64 {
    NAMESPACE_ID.fetch_add(1, Ordering::SeqCst)
}

/// Root namespace set
static ROOT_NAMESPACES: RwLock<Option<NamespaceSet>> = RwLock::new(None);

/// Initialize root namespaces
pub fn init() {
    let mut root = ROOT_NAMESPACES.write();
    if root.is_none() {
        *root = Some(NamespaceSet::init());
    }
}

/// Get root namespace set
pub fn root_namespaces() -> Option<NamespaceSet> {
    ROOT_NAMESPACES.read().clone()
}
