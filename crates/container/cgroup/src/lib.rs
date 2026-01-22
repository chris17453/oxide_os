//! Cgroups v2 Implementation
//!
//! Provides resource limits for CPU, memory, and other resources.

#![no_std]

extern crate alloc;

pub mod cpu;
pub mod io;
pub mod memory;
pub mod pids;

use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};

pub use cpu::CpuController;
pub use io::IoController;
pub use memory::MemoryController;
pub use pids::PidsController;

/// Cgroup error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgroupError {
    /// Permission denied
    PermissionDenied,
    /// Cgroup not found
    NotFound,
    /// Resource limit exceeded
    ResourceLimit,
    /// Invalid operation
    InvalidOperation,
    /// Controller not available
    ControllerNotAvailable,
}

/// Cgroup result type
pub type CgroupResult<T> = Result<T, CgroupError>;

/// Available cgroup controllers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Controller {
    /// CPU controller
    Cpu,
    /// Memory controller
    Memory,
    /// IO controller
    Io,
    /// PIDs controller
    Pids,
}

impl Controller {
    /// Get controller name
    pub fn name(&self) -> &'static str {
        match self {
            Controller::Cpu => "cpu",
            Controller::Memory => "memory",
            Controller::Io => "io",
            Controller::Pids => "pids",
        }
    }
}

/// Cgroup
pub struct Cgroup {
    /// Cgroup ID
    id: u64,
    /// Path in hierarchy
    path: String,
    /// Parent cgroup
    parent: Option<Weak<Cgroup>>,
    /// Child cgroups
    children: RwLock<Vec<Arc<Cgroup>>>,
    /// Processes in this cgroup
    processes: RwLock<Vec<u32>>,
    /// Enabled controllers
    controllers: RwLock<Vec<Controller>>,
    /// Controllers enabled for subtree
    subtree_control: RwLock<Vec<Controller>>,
    /// CPU controller
    cpu: Mutex<Option<CpuController>>,
    /// Memory controller
    memory: Mutex<Option<MemoryController>>,
    /// IO controller
    io: Mutex<Option<IoController>>,
    /// PIDs controller
    pids: Mutex<Option<PidsController>>,
}

impl Cgroup {
    /// Create root cgroup
    pub fn root() -> Arc<Self> {
        Arc::new(Cgroup {
            id: alloc_cgroup_id(),
            path: String::from("/"),
            parent: None,
            children: RwLock::new(Vec::new()),
            processes: RwLock::new(Vec::new()),
            controllers: RwLock::new(alloc::vec![
                Controller::Cpu,
                Controller::Memory,
                Controller::Io,
                Controller::Pids,
            ]),
            subtree_control: RwLock::new(Vec::new()),
            cpu: Mutex::new(Some(CpuController::new())),
            memory: Mutex::new(Some(MemoryController::new())),
            io: Mutex::new(Some(IoController::new())),
            pids: Mutex::new(Some(PidsController::new())),
        })
    }

    /// Create child cgroup
    pub fn create_child(self: &Arc<Self>, name: &str) -> CgroupResult<Arc<Self>> {
        let path = if self.path == "/" {
            alloc::format!("/{}", name)
        } else {
            alloc::format!("{}/{}", self.path, name)
        };

        // Check if name already exists
        {
            let children = self.children.read();
            if children.iter().any(|c| c.path == path) {
                return Err(CgroupError::InvalidOperation);
            }
        }

        // Inherit enabled controllers from subtree_control
        let subtree = self.subtree_control.read().clone();

        let child = Arc::new(Cgroup {
            id: alloc_cgroup_id(),
            path,
            parent: Some(Arc::downgrade(self)),
            children: RwLock::new(Vec::new()),
            processes: RwLock::new(Vec::new()),
            controllers: RwLock::new(subtree.clone()),
            subtree_control: RwLock::new(Vec::new()),
            cpu: Mutex::new(if subtree.contains(&Controller::Cpu) {
                Some(CpuController::new())
            } else {
                None
            }),
            memory: Mutex::new(if subtree.contains(&Controller::Memory) {
                Some(MemoryController::new())
            } else {
                None
            }),
            io: Mutex::new(if subtree.contains(&Controller::Io) {
                Some(IoController::new())
            } else {
                None
            }),
            pids: Mutex::new(if subtree.contains(&Controller::Pids) {
                Some(PidsController::new())
            } else {
                None
            }),
        });

        self.children.write().push(child.clone());
        Ok(child)
    }

    /// Get cgroup ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Add process to cgroup
    pub fn add_process(&self, pid: u32) -> CgroupResult<()> {
        // Check PIDs limit
        if let Some(ref pids) = *self.pids.lock() {
            if !pids.can_fork() {
                return Err(CgroupError::ResourceLimit);
            }
            pids.add_task();
        }

        self.processes.write().push(pid);
        Ok(())
    }

    /// Remove process from cgroup
    pub fn remove_process(&self, pid: u32) {
        self.processes.write().retain(|&p| p != pid);

        if let Some(ref pids) = *self.pids.lock() {
            pids.remove_task();
        }
    }

    /// Get all processes
    pub fn processes(&self) -> Vec<u32> {
        self.processes.read().clone()
    }

    /// Enable controller for subtree
    pub fn enable_subtree_controller(&self, controller: Controller) -> CgroupResult<()> {
        // Can only enable if we have access to it
        if !self.controllers.read().contains(&controller) {
            return Err(CgroupError::ControllerNotAvailable);
        }

        // Can't change if we have processes (must be in leaf)
        if !self.processes.read().is_empty() {
            return Err(CgroupError::InvalidOperation);
        }

        let mut subtree = self.subtree_control.write();
        if !subtree.contains(&controller) {
            subtree.push(controller);
        }
        Ok(())
    }

    /// Disable controller for subtree
    pub fn disable_subtree_controller(&self, controller: Controller) -> CgroupResult<()> {
        self.subtree_control.write().retain(|&c| c != controller);
        Ok(())
    }

    /// Get CPU controller
    pub fn cpu(&self) -> Option<CpuController> {
        self.cpu.lock().clone()
    }

    /// Get memory controller
    pub fn memory(&self) -> Option<MemoryController> {
        self.memory.lock().clone()
    }

    /// Get IO controller
    pub fn io(&self) -> Option<IoController> {
        self.io.lock().clone()
    }

    /// Get PIDs controller
    pub fn pids(&self) -> Option<PidsController> {
        self.pids.lock().clone()
    }

    /// Set CPU quota (microseconds per period)
    pub fn set_cpu_quota(&self, quota_us: u64, period_us: u64) -> CgroupResult<()> {
        let mut cpu = self.cpu.lock();
        if let Some(ref mut controller) = *cpu {
            controller.set_quota(quota_us, period_us);
            Ok(())
        } else {
            Err(CgroupError::ControllerNotAvailable)
        }
    }

    /// Set memory limit
    pub fn set_memory_max(&self, max_bytes: u64) -> CgroupResult<()> {
        let mut memory = self.memory.lock();
        if let Some(ref mut controller) = *memory {
            controller.set_max(max_bytes);
            Ok(())
        } else {
            Err(CgroupError::ControllerNotAvailable)
        }
    }

    /// Set PIDs limit
    pub fn set_pids_max(&self, max_pids: u64) -> CgroupResult<()> {
        let mut pids = self.pids.lock();
        if let Some(ref mut controller) = *pids {
            controller.set_max(max_pids);
            Ok(())
        } else {
            Err(CgroupError::ControllerNotAvailable)
        }
    }

    /// Check if can allocate memory
    pub fn can_charge_memory(&self, bytes: u64) -> bool {
        if let Some(ref memory) = *self.memory.lock() {
            memory.can_charge(bytes)
        } else {
            true
        }
    }

    /// Charge memory usage
    pub fn charge_memory(&self, bytes: u64) -> CgroupResult<()> {
        let mut memory = self.memory.lock();
        if let Some(ref mut controller) = *memory {
            if controller.can_charge(bytes) {
                controller.charge(bytes);
                Ok(())
            } else {
                Err(CgroupError::ResourceLimit)
            }
        } else {
            Ok(())
        }
    }

    /// Uncharge memory usage
    pub fn uncharge_memory(&self, bytes: u64) {
        if let Some(ref mut memory) = *self.memory.lock() {
            memory.uncharge(bytes);
        }
    }
}

/// Global cgroup ID counter
static CGROUP_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate cgroup ID
fn alloc_cgroup_id() -> u64 {
    CGROUP_ID.fetch_add(1, Ordering::SeqCst)
}

/// Root cgroup
static ROOT_CGROUP: RwLock<Option<Arc<Cgroup>>> = RwLock::new(None);

/// Initialize cgroup subsystem
pub fn init() {
    let mut root = ROOT_CGROUP.write();
    if root.is_none() {
        *root = Some(Cgroup::root());
    }
}

/// Get root cgroup
pub fn root_cgroup() -> Option<Arc<Cgroup>> {
    ROOT_CGROUP.read().clone()
}

/// Find cgroup by path
pub fn find_cgroup(path: &str) -> Option<Arc<Cgroup>> {
    let root = root_cgroup()?;

    if path == "/" {
        return Some(root);
    }

    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let mut current = root;

    for part in parts {
        let child_path = if current.path == "/" {
            alloc::format!("/{}", part)
        } else {
            alloc::format!("{}/{}", current.path, part)
        };

        let next = {
            let children = current.children.read();
            children.iter().find(|c| c.path == child_path).cloned()
        };

        if let Some(child) = next {
            current = child;
        } else {
            return None;
        }
    }

    Some(current)
}
