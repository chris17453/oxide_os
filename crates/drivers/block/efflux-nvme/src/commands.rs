//! NVMe Command Builders

use crate::{opcode, NvmeSqe};

/// Build an Identify Controller command
pub fn identify_controller(cid: u16, prp1: u64) -> NvmeSqe {
    let mut sqe = NvmeSqe::new(opcode::IDENTIFY, cid);
    sqe.prp1 = prp1;
    sqe.cdw10 = 1; // CNS = 1 (Controller)
    sqe
}

/// Build an Identify Namespace command
pub fn identify_namespace(cid: u16, nsid: u32, prp1: u64) -> NvmeSqe {
    let mut sqe = NvmeSqe::new(opcode::IDENTIFY, cid);
    sqe.nsid = nsid;
    sqe.prp1 = prp1;
    sqe.cdw10 = 0; // CNS = 0 (Namespace)
    sqe
}

/// Build a Create I/O Submission Queue command
pub fn create_io_sq(cid: u16, qid: u16, size: u16, prp1: u64, cqid: u16) -> NvmeSqe {
    let mut sqe = NvmeSqe::new(opcode::CREATE_SQ, cid);
    sqe.prp1 = prp1;
    sqe.cdw10 = ((size - 1) as u32) << 16 | qid as u32;
    sqe.cdw11 = (cqid as u32) << 16 | 1; // Physically Contiguous
    sqe
}

/// Build a Create I/O Completion Queue command
pub fn create_io_cq(cid: u16, qid: u16, size: u16, prp1: u64, iv: u16) -> NvmeSqe {
    let mut sqe = NvmeSqe::new(opcode::CREATE_CQ, cid);
    sqe.prp1 = prp1;
    sqe.cdw10 = ((size - 1) as u32) << 16 | qid as u32;
    sqe.cdw11 = (iv as u32) << 16 | 1; // Physically Contiguous, Interrupts Enabled
    sqe
}

/// Build a Read command
pub fn read(cid: u16, nsid: u32, lba: u64, blocks: u16, prp1: u64, prp2: u64) -> NvmeSqe {
    let mut sqe = NvmeSqe::new(opcode::READ, cid);
    sqe.nsid = nsid;
    sqe.prp1 = prp1;
    sqe.prp2 = prp2;
    sqe.cdw10 = lba as u32;
    sqe.cdw11 = (lba >> 32) as u32;
    sqe.cdw12 = (blocks - 1) as u32;
    sqe
}

/// Build a Write command
pub fn write(cid: u16, nsid: u32, lba: u64, blocks: u16, prp1: u64, prp2: u64) -> NvmeSqe {
    let mut sqe = NvmeSqe::new(opcode::WRITE, cid);
    sqe.nsid = nsid;
    sqe.prp1 = prp1;
    sqe.prp2 = prp2;
    sqe.cdw10 = lba as u32;
    sqe.cdw11 = (lba >> 32) as u32;
    sqe.cdw12 = (blocks - 1) as u32;
    sqe
}

/// Build a Flush command
pub fn flush(cid: u16, nsid: u32) -> NvmeSqe {
    let mut sqe = NvmeSqe::new(opcode::FLUSH, cid);
    sqe.nsid = nsid;
    sqe
}

/// Build a Set Features command
pub fn set_features(cid: u16, fid: u8, value: u32) -> NvmeSqe {
    let mut sqe = NvmeSqe::new(opcode::SET_FEATURES, cid);
    sqe.cdw10 = fid as u32;
    sqe.cdw11 = value;
    sqe
}

/// Build a Get Features command
pub fn get_features(cid: u16, fid: u8) -> NvmeSqe {
    let mut sqe = NvmeSqe::new(opcode::GET_FEATURES, cid);
    sqe.cdw10 = fid as u32;
    sqe
}

/// Feature IDs
pub mod features {
    pub const ARBITRATION: u8 = 0x01;
    pub const POWER_MANAGEMENT: u8 = 0x02;
    pub const LBA_RANGE_TYPE: u8 = 0x03;
    pub const TEMPERATURE_THRESHOLD: u8 = 0x04;
    pub const ERROR_RECOVERY: u8 = 0x05;
    pub const VOLATILE_WRITE_CACHE: u8 = 0x06;
    pub const NUMBER_OF_QUEUES: u8 = 0x07;
    pub const INTERRUPT_COALESCING: u8 = 0x08;
    pub const INTERRUPT_VECTOR_CONFIG: u8 = 0x09;
    pub const WRITE_ATOMICITY: u8 = 0x0A;
    pub const ASYNC_EVENT_CONFIG: u8 = 0x0B;
}
