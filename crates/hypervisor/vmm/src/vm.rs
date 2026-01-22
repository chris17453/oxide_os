//! Virtual Machine Management

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

use crate::{
    GpaRange, GuestMemory, GuestMemoryRegion, Vcpu, VcpuId, VcpuRegs, VcpuState, VirtioDevice,
    VmmBackend, VmmError, VmmResult,
};

/// VM identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VmId(pub u64);

/// VM state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    /// VM created but not started
    Created,
    /// VM is running
    Running,
    /// VM is paused
    Paused,
    /// VM is shutting down
    Shutdown,
}

/// Virtual machine
pub struct VirtualMachine {
    /// VM ID
    id: VmId,
    /// Backend
    backend: Arc<dyn VmmBackend>,
    /// State
    state: RwLock<VmState>,
    /// VCPUs
    vcpus: RwLock<Vec<Arc<Vcpu>>>,
    /// Next VCPU ID
    next_vcpu_id: AtomicU32,
    /// Guest memory
    memory: RwLock<GuestMemory>,
    /// Emulated devices
    devices: Mutex<Vec<Box<dyn VirtioDevice>>>,
}

impl VirtualMachine {
    /// Create new VM
    pub fn new(id: VmId, backend: Arc<dyn VmmBackend>) -> VmmResult<Self> {
        Ok(VirtualMachine {
            id,
            backend,
            state: RwLock::new(VmState::Created),
            vcpus: RwLock::new(Vec::new()),
            next_vcpu_id: AtomicU32::new(0),
            memory: RwLock::new(GuestMemory::new()),
            devices: Mutex::new(Vec::new()),
        })
    }

    /// Get VM ID
    pub fn id(&self) -> VmId {
        self.id
    }

    /// Get VM state
    pub fn state(&self) -> VmState {
        *self.state.read()
    }

    /// Create VCPU
    pub fn create_vcpu(self: &Arc<Self>) -> VmmResult<Arc<Vcpu>> {
        let vcpu_id = VcpuId(self.next_vcpu_id.fetch_add(1, Ordering::SeqCst));
        let state = self.backend.create_vcpu_state(self, vcpu_id)?;
        let vcpu = Arc::new(Vcpu::new(vcpu_id, self.clone(), state)?);
        self.vcpus.write().push(vcpu.clone());
        Ok(vcpu)
    }

    /// Get VCPU by ID
    pub fn get_vcpu(&self, id: VcpuId) -> Option<Arc<Vcpu>> {
        self.vcpus.read().iter().find(|v| v.id() == id).cloned()
    }

    /// Get VCPU count
    pub fn vcpu_count(&self) -> usize {
        self.vcpus.read().len()
    }

    /// Add memory region
    pub fn add_memory_region(&self, region: GuestMemoryRegion) -> VmmResult<()> {
        if *self.state.read() != VmState::Created {
            return Err(VmmError::InvalidState);
        }
        self.memory.write().add_region(region)
    }

    /// Get guest memory
    pub fn memory(&self) -> &RwLock<GuestMemory> {
        &self.memory
    }

    /// Add emulated device
    pub fn add_device(&self, device: Box<dyn VirtioDevice>) -> VmmResult<()> {
        self.devices.lock().push(device);
        Ok(())
    }

    /// Get device by type
    pub fn get_device(&self, device_type: u32) -> Option<usize> {
        self.devices
            .lock()
            .iter()
            .position(|d| d.device_type() == device_type)
    }

    /// Start VM
    pub fn start(&self) -> VmmResult<()> {
        let mut state = self.state.write();
        if *state != VmState::Created && *state != VmState::Paused {
            return Err(VmmError::InvalidState);
        }
        *state = VmState::Running;
        Ok(())
    }

    /// Pause VM
    pub fn pause(&self) -> VmmResult<()> {
        let mut state = self.state.write();
        if *state != VmState::Running {
            return Err(VmmError::InvalidState);
        }
        *state = VmState::Paused;
        Ok(())
    }

    /// Resume VM
    pub fn resume(&self) -> VmmResult<()> {
        let mut state = self.state.write();
        if *state != VmState::Paused {
            return Err(VmmError::InvalidState);
        }
        *state = VmState::Running;
        Ok(())
    }

    /// Shutdown VM
    pub fn shutdown(&self) -> VmmResult<()> {
        let mut state = self.state.write();
        *state = VmState::Shutdown;

        // Stop all VCPUs
        for vcpu in self.vcpus.read().iter() {
            vcpu.stop()?;
        }

        // Reset devices
        for device in self.devices.lock().iter_mut() {
            device.reset();
        }

        Ok(())
    }

    /// Handle device IO
    pub fn handle_io(&self, port: u16, is_write: bool, data: &mut [u8]) -> VmmResult<()> {
        // Check emulated devices
        for device in self.devices.lock().iter_mut() {
            // virtio MMIO base detection (simplified)
            if port >= 0x1000 && port < 0x2000 {
                let offset = (port - 0x1000) as u64;
                if is_write {
                    device.write_config(offset, data);
                } else {
                    device.read_config(offset, data);
                }
                return Ok(());
            }
        }

        // Unhandled IO - ignore
        if !is_write {
            data.fill(0xFF);
        }
        Ok(())
    }

    /// Handle MMIO access
    pub fn handle_mmio(&self, gpa: u64, is_write: bool, data: &mut [u8]) -> VmmResult<()> {
        // Check virtio MMIO regions
        for (idx, device) in self.devices.lock().iter_mut().enumerate() {
            let base = 0xFEB0_0000 + (idx as u64 * 0x1000);
            if gpa >= base && gpa < base + 0x1000 {
                let offset = gpa - base;
                if is_write {
                    device.write_config(offset, data);
                } else {
                    device.read_config(offset, data);
                }
                return Ok(());
            }
        }

        // Unhandled MMIO
        if !is_write {
            data.fill(0xFF);
        }
        Ok(())
    }
}
