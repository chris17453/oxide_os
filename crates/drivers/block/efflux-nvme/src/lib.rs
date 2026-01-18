//! NVMe Driver for EFFLUX OS
//!
//! Implements the NVMe 1.4 specification for NVM Express storage devices.

#![no_std]

extern crate alloc;

mod queue;
mod commands;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use efflux_block::{BlockDevice, BlockDeviceInfo, BlockError, BlockResult};

/// NVMe controller registers (memory-mapped)
#[repr(C)]
pub struct NvmeRegs {
    /// Controller Capabilities
    pub cap: u64,        // 0x00
    /// Version
    pub vs: u32,         // 0x08
    /// Interrupt Mask Set
    pub intms: u32,      // 0x0C
    /// Interrupt Mask Clear
    pub intmc: u32,      // 0x10
    /// Controller Configuration
    pub cc: u32,         // 0x14
    _reserved1: u32,     // 0x18
    /// Controller Status
    pub csts: u32,       // 0x1C
    /// NVM Subsystem Reset
    pub nssr: u32,       // 0x20
    /// Admin Queue Attributes
    pub aqa: u32,        // 0x24
    /// Admin Submission Queue Base Address
    pub asq: u64,        // 0x28
    /// Admin Completion Queue Base Address
    pub acq: u64,        // 0x30
    // More registers follow...
}

/// NVMe capability bits
mod cap {
    /// Maximum Queue Entries Supported (mask)
    pub const MQES_MASK: u64 = 0xFFFF;
    /// Contiguous Queues Required
    pub const CQR: u64 = 1 << 16;
    /// Doorbell Stride (4 << DSTRD)
    pub const DSTRD_SHIFT: u64 = 32;
    pub const DSTRD_MASK: u64 = 0xF << DSTRD_SHIFT;
    /// Memory Page Size Minimum
    pub const MPSMIN_SHIFT: u64 = 48;
    pub const MPSMIN_MASK: u64 = 0xF << MPSMIN_SHIFT;
    /// Memory Page Size Maximum
    pub const MPSMAX_SHIFT: u64 = 52;
    pub const MPSMAX_MASK: u64 = 0xF << MPSMAX_SHIFT;
}

/// NVMe controller configuration bits
mod cc {
    /// Enable
    pub const EN: u32 = 1 << 0;
    /// I/O Command Set Selected
    pub const CSS_NVM: u32 = 0 << 4;
    /// Memory Page Size
    pub const MPS_SHIFT: u32 = 7;
    /// Arbitration Mechanism
    pub const AMS_RR: u32 = 0 << 11;
    /// Shutdown Notification
    pub const SHN_NONE: u32 = 0 << 14;
    /// I/O Submission Queue Entry Size (6 = 64 bytes)
    pub const IOSQES_SHIFT: u32 = 16;
    /// I/O Completion Queue Entry Size (4 = 16 bytes)
    pub const IOCQES_SHIFT: u32 = 20;
}

/// NVMe controller status bits
mod csts {
    /// Ready
    pub const RDY: u32 = 1 << 0;
    /// Controller Fatal Status
    pub const CFS: u32 = 1 << 1;
    /// Shutdown Status
    pub const SHST_MASK: u32 = 3 << 2;
}

/// NVMe opcodes
mod opcode {
    // Admin commands
    pub const DELETE_SQ: u8 = 0x00;
    pub const CREATE_SQ: u8 = 0x01;
    pub const DELETE_CQ: u8 = 0x04;
    pub const CREATE_CQ: u8 = 0x05;
    pub const IDENTIFY: u8 = 0x06;
    pub const SET_FEATURES: u8 = 0x09;
    pub const GET_FEATURES: u8 = 0x0A;

    // NVM commands
    pub const FLUSH: u8 = 0x00;
    pub const WRITE: u8 = 0x01;
    pub const READ: u8 = 0x02;
}

/// NVMe submission queue entry (64 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NvmeSqe {
    /// Command Dword 0: Opcode, Fused, PSDT, CID
    pub cdw0: u32,
    /// Namespace ID
    pub nsid: u32,
    /// Reserved
    pub cdw2: u32,
    pub cdw3: u32,
    /// Metadata Pointer
    pub mptr: u64,
    /// Data Pointer (PRP1)
    pub prp1: u64,
    /// Data Pointer (PRP2)
    pub prp2: u64,
    /// Command-specific
    pub cdw10: u32,
    pub cdw11: u32,
    pub cdw12: u32,
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
}

impl NvmeSqe {
    /// Create a new SQE with opcode and command ID
    pub fn new(opcode: u8, cid: u16) -> Self {
        NvmeSqe {
            cdw0: (opcode as u32) | ((cid as u32) << 16),
            ..Default::default()
        }
    }

    /// Set PRP1 (first data pointer)
    pub fn set_prp1(&mut self, addr: u64) {
        self.prp1 = addr;
    }

    /// Set PRP2 (second data pointer or PRP list)
    pub fn set_prp2(&mut self, addr: u64) {
        self.prp2 = addr;
    }
}

/// NVMe completion queue entry (16 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NvmeCqe {
    /// Command-specific result
    pub result: u64,
    /// Submission Queue Head Pointer
    pub sq_head: u16,
    /// Submission Queue Identifier
    pub sq_id: u16,
    /// Command Identifier
    pub cid: u16,
    /// Phase Tag and Status
    pub status: u16,
}

impl NvmeCqe {
    /// Check if command completed successfully
    pub fn success(&self) -> bool {
        (self.status & 0xFFFE) == 0
    }

    /// Get status code
    pub fn status_code(&self) -> u8 {
        ((self.status >> 1) & 0xFF) as u8
    }

    /// Get phase tag
    pub fn phase(&self) -> bool {
        self.status & 1 != 0
    }
}

/// NVMe Identify Controller data structure
#[repr(C)]
pub struct IdentifyController {
    /// PCI Vendor ID
    pub vid: u16,
    /// PCI Subsystem Vendor ID
    pub ssvid: u16,
    /// Serial Number
    pub sn: [u8; 20],
    /// Model Number
    pub mn: [u8; 40],
    /// Firmware Revision
    pub fr: [u8; 8],
    // ... more fields
}

/// NVMe Identify Namespace data structure
#[repr(C)]
pub struct IdentifyNamespace {
    /// Namespace Size (in blocks)
    pub nsze: u64,
    /// Namespace Capacity
    pub ncap: u64,
    /// Namespace Utilization
    pub nuse: u64,
    /// Namespace Features
    pub nsfeat: u8,
    /// Number of LBA Formats
    pub nlbaf: u8,
    /// Formatted LBA Size
    pub flbas: u8,
    // ... more fields
    _reserved: [u8; 101],
    /// LBA Format support (up to 16)
    pub lbaf: [LbaFormat; 16],
    // ... rest of structure
}

/// LBA Format descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LbaFormat {
    /// Metadata Size
    pub ms: u16,
    /// LBA Data Size (2^n bytes)
    pub lbads: u8,
    /// Relative Performance
    pub rp: u8,
}

/// NVMe controller
pub struct NvmeController {
    /// MMIO base address
    mmio_base: u64,
    /// Doorbell stride (in bytes)
    doorbell_stride: u32,
    /// Maximum queue entries
    max_queue_entries: u16,
    /// Admin submission queue
    admin_sq: Mutex<Vec<NvmeSqe>>,
    /// Admin completion queue
    admin_cq: Mutex<Vec<NvmeCqe>>,
    /// Admin SQ tail
    admin_sq_tail: AtomicU32,
    /// Admin CQ head
    admin_cq_head: AtomicU32,
    /// Current admin CQ phase
    admin_cq_phase: AtomicU32,
    /// Command ID counter
    next_cid: AtomicU32,
    /// Namespaces
    namespaces: Mutex<Vec<NvmeNamespace>>,
}

/// NVMe namespace (a logical block device within the controller)
pub struct NvmeNamespace {
    /// Namespace ID
    nsid: u32,
    /// Block size in bytes
    block_size: u32,
    /// Number of blocks
    block_count: u64,
    /// Controller reference (MMIO base)
    mmio_base: u64,
    /// I/O submission queue
    io_sq: Mutex<Vec<NvmeSqe>>,
    /// I/O completion queue
    io_cq: Mutex<Vec<NvmeCqe>>,
    /// I/O SQ tail
    io_sq_tail: AtomicU32,
    /// I/O CQ head
    io_cq_head: AtomicU32,
    /// Current I/O CQ phase
    io_cq_phase: AtomicU32,
    /// Next command ID
    next_cid: AtomicU32,
}

impl NvmeController {
    // Register offsets
    const REG_CAP: usize = 0x00;
    const REG_VS: usize = 0x08;
    const REG_CC: usize = 0x14;
    const REG_CSTS: usize = 0x1C;
    const REG_AQA: usize = 0x24;

    /// Probe for an NVMe controller at the given PCI BAR0 address
    ///
    /// # Safety
    /// The MMIO address must be valid and mapped.
    pub unsafe fn probe(mmio_base: u64) -> Option<Self> {
        unsafe {
            let base = mmio_base as *mut u8;

            // Read capabilities
            let cap_ptr = base.add(Self::REG_CAP) as *const u64;
            let cap = core::ptr::read_volatile(cap_ptr);
            let max_queue_entries = ((cap & cap::MQES_MASK) + 1) as u16;
            let doorbell_stride = 4u32 << ((cap & cap::DSTRD_MASK) >> cap::DSTRD_SHIFT);

            // Read version
            let vs_ptr = base.add(Self::REG_VS) as *const u32;
            let version = core::ptr::read_volatile(vs_ptr);
            if version < 0x00010000 {
                // Require at least NVMe 1.0
                return None;
            }

            // Disable controller
            let cc_ptr = base.add(Self::REG_CC) as *mut u32;
            core::ptr::write_volatile(cc_ptr, 0);

            // Wait for not ready
            let csts_ptr = base.add(Self::REG_CSTS) as *const u32;
            for _ in 0..1000000 {
                let csts = core::ptr::read_volatile(csts_ptr);
                if csts & csts::RDY == 0 {
                    break;
                }
                core::hint::spin_loop();
            }

            // Allocate admin queues (simplified - would need proper DMA allocation)
            let admin_sq = Vec::with_capacity(64);
            let admin_cq = Vec::with_capacity(64);

            // Configure admin queue attributes
            let aqa_ptr = base.add(Self::REG_AQA) as *mut u32;
            let aqa = ((64 - 1) << 16) | (64 - 1); // ACQS and ASQS
            core::ptr::write_volatile(aqa_ptr, aqa);

            // Set admin queue base addresses (would need physical addresses)
            // For now, this is a stub

            // Configure and enable controller
            let cc = cc::EN
                | cc::CSS_NVM
                | (0 << cc::MPS_SHIFT)      // 4KB pages
                | (6 << cc::IOSQES_SHIFT)   // 64-byte SQE
                | (4 << cc::IOCQES_SHIFT);  // 16-byte CQE
            core::ptr::write_volatile(cc_ptr, cc);

            // Wait for ready
            for _ in 0..1000000 {
                let csts = core::ptr::read_volatile(csts_ptr);
                if csts & csts::RDY != 0 {
                    break;
                }
                if csts & csts::CFS != 0 {
                    // Fatal error
                    return None;
                }
                core::hint::spin_loop();
            }

            Some(NvmeController {
                mmio_base,
                doorbell_stride,
                max_queue_entries,
                admin_sq: Mutex::new(admin_sq),
                admin_cq: Mutex::new(admin_cq),
                admin_sq_tail: AtomicU32::new(0),
                admin_cq_head: AtomicU32::new(0),
                admin_cq_phase: AtomicU32::new(1),
                next_cid: AtomicU32::new(0),
                namespaces: Mutex::new(Vec::new()),
            })
        }
    }

    /// Get the doorbell address for a queue
    fn doorbell_addr(&self, qid: u16, is_cq: bool) -> u64 {
        let base = self.mmio_base + 0x1000;
        let offset = ((qid as u32) * 2 + if is_cq { 1 } else { 0 }) * self.doorbell_stride;
        base + offset as u64
    }

    /// Ring the submission queue doorbell
    unsafe fn ring_sq_doorbell(&self, qid: u16, tail: u32) {
        unsafe {
            let addr = self.doorbell_addr(qid, false);
            core::ptr::write_volatile(addr as *mut u32, tail);
        }
    }

    /// Ring the completion queue doorbell
    unsafe fn ring_cq_doorbell(&self, qid: u16, head: u32) {
        unsafe {
            let addr = self.doorbell_addr(qid, true);
            core::ptr::write_volatile(addr as *mut u32, head);
        }
    }

    /// Get next command ID
    fn alloc_cid(&self) -> u16 {
        (self.next_cid.fetch_add(1, Ordering::Relaxed) & 0xFFFF) as u16
    }
}

impl NvmeNamespace {
    /// Create a new namespace
    pub fn new(nsid: u32, block_size: u32, block_count: u64, mmio_base: u64) -> Self {
        NvmeNamespace {
            nsid,
            block_size,
            block_count,
            mmio_base,
            io_sq: Mutex::new(Vec::with_capacity(256)),
            io_cq: Mutex::new(Vec::with_capacity(256)),
            io_sq_tail: AtomicU32::new(0),
            io_cq_head: AtomicU32::new(0),
            io_cq_phase: AtomicU32::new(1),
            next_cid: AtomicU32::new(0),
        }
    }
}

impl BlockDevice for NvmeNamespace {
    fn read(&self, start_block: u64, buf: &mut [u8]) -> BlockResult<usize> {
        if start_block >= self.block_count {
            return Err(BlockError::InvalidBlock);
        }

        let blocks = buf.len() / self.block_size as usize;
        if start_block + blocks as u64 > self.block_count {
            return Err(BlockError::InvalidBlock);
        }

        // In a real implementation:
        // 1. Allocate command ID
        // 2. Build read command SQE
        // 3. Setup PRPs for DMA
        // 4. Submit to I/O queue
        // 5. Wait for completion
        // 6. Check status

        // Stub - return zeros
        buf.fill(0);
        Ok(buf.len())
    }

    fn write(&self, start_block: u64, buf: &[u8]) -> BlockResult<usize> {
        if start_block >= self.block_count {
            return Err(BlockError::InvalidBlock);
        }

        let blocks = buf.len() / self.block_size as usize;
        if start_block + blocks as u64 > self.block_count {
            return Err(BlockError::InvalidBlock);
        }

        // Similar to read but with write command

        Ok(buf.len())
    }

    fn flush(&self) -> BlockResult<()> {
        // Send flush command
        Ok(())
    }

    fn block_size(&self) -> u32 {
        self.block_size
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn info(&self) -> BlockDeviceInfo {
        BlockDeviceInfo {
            name: "nvme",
            block_size: self.block_size,
            block_count: self.block_count,
            read_only: false,
            removable: false,
            model: "NVMe Namespace",
        }
    }
}
