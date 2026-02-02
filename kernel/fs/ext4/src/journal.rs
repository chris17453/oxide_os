//! ext4/JBD2 Journal Implementation
//!
//! Implements write-ahead logging for ext4 metadata integrity.
//!
//! The journal provides crash recovery by:
//! 1. Writing changes to journal first
//! 2. Only committing to main filesystem after journal commit
//! 3. Replaying uncommitted transactions on mount
//!
//! Journal block types:
//! - Descriptor: Lists blocks in a transaction
//! - Commit: Marks end of a transaction
//! - Revoke: Blocks to ignore during recovery
//! - Superblock: Journal metadata (v1 or v2)

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::error::{Ext4Error, Ext4Result};
use crate::extent;
use crate::group_desc::BlockGroupTable;
use crate::inode::{Ext4Inode, read_inode};
use crate::superblock::Ext4Superblock;
use block::BlockDevice;

/// JBD2 magic number
pub const JBD2_MAGIC: u32 = 0xC03B3998;

/// Journal block types
pub mod block_type {
    pub const DESCRIPTOR: u32 = 1;
    pub const COMMIT: u32 = 2;
    pub const SUPERBLOCK_V1: u32 = 3;
    pub const SUPERBLOCK_V2: u32 = 4;
    pub const REVOKE: u32 = 5;
}

/// Journal feature flags (compatible)
pub mod compat {
    pub const CHECKSUM: u32 = 0x0001;
}

/// Journal feature flags (incompatible)
pub mod incompat {
    pub const REVOKE: u32 = 0x0001;
    pub const _64BIT: u32 = 0x0002;
    pub const ASYNC_COMMIT: u32 = 0x0004;
    pub const CSUM_V2: u32 = 0x0008;
    pub const CSUM_V3: u32 = 0x0010;
}

/// Journal flags
pub mod flags {
    pub const ESCAPE: u32 = 1; // Block escaped (magic number replaced)
    pub const SAME_UUID: u32 = 2; // UUID same as previous
    pub const DELETED: u32 = 4; // Block deleted (for revoke)
    pub const LAST_TAG: u32 = 8; // Last tag in descriptor
}

/// Common header for all journal blocks
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct JournalHeader {
    pub h_magic: u32,     // JBD2_MAGIC
    pub h_blocktype: u32, // Block type (descriptor, commit, etc.)
    pub h_sequence: u32,  // Transaction sequence number
}

impl JournalHeader {
    pub fn new(blocktype: u32, sequence: u32) -> Self {
        JournalHeader {
            h_magic: JBD2_MAGIC.to_be(),
            h_blocktype: blocktype.to_be(),
            h_sequence: sequence.to_be(),
        }
    }

    pub fn magic(&self) -> u32 {
        u32::from_be(self.h_magic)
    }

    pub fn blocktype(&self) -> u32 {
        u32::from_be(self.h_blocktype)
    }

    pub fn sequence(&self) -> u32 {
        u32::from_be(self.h_sequence)
    }
}

/// Journal superblock (v2)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct JournalSuperblock {
    // Header (12 bytes)
    pub s_header: JournalHeader,

    // Static information (offset 12)
    pub s_blocksize: u32, // Journal block size
    pub s_maxlen: u32,    // Total blocks in journal
    pub s_first: u32,     // First block of log information

    // Dynamic information (offset 24)
    pub s_sequence: u32, // First commit ID in log
    pub s_start: u32,    // Block number of start of log
    pub s_errno: u32,    // Error value (if any)

    // V2 only fields (offset 36)
    pub s_feature_compat: u32,
    pub s_feature_incompat: u32,
    pub s_feature_ro_compat: u32,
    pub s_uuid: [u8; 16],       // UUID of journal
    pub s_nr_users: u32,        // Number of filesystems using journal
    pub s_dynsuper: u32,        // Dynamic superblock copy location
    pub s_max_transaction: u32, // Max blocks per transaction
    pub s_max_trans_data: u32,  // Max data blocks per transaction

    // Checksumming (v3)
    pub s_checksum_type: u8,
    pub s_padding2: [u8; 3],
    pub s_padding: [u32; 42],
    pub s_checksum: u32, // Superblock checksum

    // User list
    pub s_users: [[u8; 16]; 48],
}

impl JournalSuperblock {
    /// Read journal superblock from block device
    pub fn read(device: &dyn BlockDevice, block: u64, block_size: u64) -> Ext4Result<Self> {
        let mut buf = [0u8; 1024]; // Journal superblock is at most 1024 bytes
        let sectors_per_block = block_size / 512;
        let start_sector = block * sectors_per_block;

        // Read enough sectors
        for i in 0..2 {
            device
                .read(
                    start_sector + i,
                    &mut buf[i as usize * 512..(i as usize + 1) * 512],
                )
                .map_err(|_| Ext4Error::IoError)?;
        }

        let sb: JournalSuperblock =
            unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const JournalSuperblock) };

        // Validate magic
        if sb.s_header.magic() != JBD2_MAGIC {
            return Err(Ext4Error::InvalidJournal);
        }

        Ok(sb)
    }

    pub fn blocksize(&self) -> u32 {
        u32::from_be(self.s_blocksize)
    }

    pub fn maxlen(&self) -> u32 {
        u32::from_be(self.s_maxlen)
    }

    pub fn first(&self) -> u32 {
        u32::from_be(self.s_first)
    }

    pub fn sequence(&self) -> u32 {
        u32::from_be(self.s_sequence)
    }

    pub fn start(&self) -> u32 {
        u32::from_be(self.s_start)
    }

    pub fn is_64bit(&self) -> bool {
        u32::from_be(self.s_feature_incompat) & incompat::_64BIT != 0
    }

    pub fn has_revoke(&self) -> bool {
        u32::from_be(self.s_feature_incompat) & incompat::REVOKE != 0
    }
}

/// Tag in descriptor block (describes one journaled block)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct JournalBlockTag3 {
    pub t_blocknr: u32,      // Filesystem block number (low 32 bits)
    pub t_flags: u32,        // Flags
    pub t_blocknr_high: u32, // High 32 bits of block number (64-bit mode)
    pub t_checksum: u32,     // Block checksum
}

impl JournalBlockTag3 {
    pub fn blocknr(&self) -> u64 {
        let lo = u32::from_be(self.t_blocknr) as u64;
        let hi = u32::from_be(self.t_blocknr_high) as u64;
        lo | (hi << 32)
    }

    pub fn flags(&self) -> u32 {
        u32::from_be(self.t_flags)
    }

    pub fn is_last(&self) -> bool {
        self.flags() & flags::LAST_TAG != 0
    }

    pub fn is_escaped(&self) -> bool {
        self.flags() & flags::ESCAPE != 0
    }
}

/// Smaller tag without high bits (for non-64bit journals)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct JournalBlockTag {
    pub t_blocknr: u32,
    pub t_checksum: u16,
    pub t_flags: u16,
}

/// Revoke block header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct JournalRevokeHeader {
    pub r_header: JournalHeader,
    pub r_count: u32, // Number of bytes used in revoke block
}

/// Commit block (marks end of transaction)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct JournalCommitBlock {
    pub h_header: JournalHeader,
    pub h_chksum_type: u8,
    pub h_chksum_size: u8,
    pub h_padding: [u8; 2],
    pub h_chksum: [u32; 8], // Checksum(s)
    pub h_commit_sec: u64,  // Commit timestamp (seconds)
    pub h_commit_nsec: u32, // Commit timestamp (nanoseconds)
}

/// A block to be written in a transaction
#[derive(Clone)]
pub struct JournalBlock {
    /// Filesystem block number
    pub blocknr: u64,
    /// Block data
    pub data: Vec<u8>,
    /// Is metadata (vs data)
    pub is_metadata: bool,
}

/// Current transaction state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransactionState {
    Running,
    Locked,
    Flush,
    Commit,
    Finished,
}

/// A journal transaction
pub struct Transaction {
    /// Transaction ID (sequence number)
    pub tid: u32,
    /// Transaction state
    pub state: TransactionState,
    /// Blocks to be written
    pub blocks: Vec<JournalBlock>,
    /// Revoked blocks (don't replay these)
    pub revoked: Vec<u64>,
}

impl Transaction {
    pub fn new(tid: u32) -> Self {
        Transaction {
            tid,
            state: TransactionState::Running,
            blocks: Vec::new(),
            revoked: Vec::new(),
        }
    }

    /// Add a block to the transaction
    pub fn add_block(&mut self, blocknr: u64, data: Vec<u8>, is_metadata: bool) {
        // Check if block already in transaction
        for block in &mut self.blocks {
            if block.blocknr == blocknr {
                block.data = data;
                return;
            }
        }
        self.blocks.push(JournalBlock {
            blocknr,
            data,
            is_metadata,
        });
    }

    /// Add a block to revoke list
    pub fn revoke_block(&mut self, blocknr: u64) {
        if !self.revoked.contains(&blocknr) {
            self.revoked.push(blocknr);
        }
    }
}

/// Journal handle for a single operation
pub struct JournalHandle {
    /// Blocks modified by this handle
    pub blocks: Vec<JournalBlock>,
}

impl JournalHandle {
    pub fn new() -> Self {
        JournalHandle { blocks: Vec::new() }
    }

    /// Mark a block for journaling
    pub fn get_write_access(&mut self, blocknr: u64, data: Vec<u8>, is_metadata: bool) {
        self.blocks.push(JournalBlock {
            blocknr,
            data,
            is_metadata,
        });
    }
}

/// Journal state
pub struct Journal {
    /// Block device
    device: Arc<dyn BlockDevice>,
    /// Journal superblock
    jsb: JournalSuperblock,
    /// First journal block (absolute)
    first_block: u64,
    /// Block size
    block_size: u64,
    /// Number of journal blocks
    journal_len: u32,
    /// Current sequence number
    sequence: u32,
    /// Current write position in journal
    head: u32,
    /// Position of oldest valid transaction
    tail: u32,
    /// Current running transaction
    current_transaction: Option<Transaction>,
    /// Is journal clean (no replay needed)
    clean: bool,
    /// Blocks revoked during recovery (shouldn't be replayed)
    revoked_blocks: Vec<u64>,
}

impl Journal {
    /// Open the journal from an ext4 filesystem
    pub fn open(
        device: Arc<dyn BlockDevice>,
        sb: &Ext4Superblock,
        group_table: &BlockGroupTable,
    ) -> Ext4Result<Self> {
        // Journal inode number
        let journal_ino = sb.s_journal_inum;
        if journal_ino == 0 {
            return Err(Ext4Error::NoJournal);
        }

        // Read journal inode
        let journal_inode = read_inode(&*device, sb, group_table, journal_ino)?;

        // Get first block of journal file
        let first_block =
            extent::map_block(&*device, sb, &journal_inode, 0)?.ok_or(Ext4Error::InvalidJournal)?;

        let block_size = sb.block_size();

        // Read journal superblock
        let jsb = JournalSuperblock::read(&*device, first_block, block_size)?;

        // Validate journal
        if jsb.blocksize() as u64 != block_size {
            return Err(Ext4Error::InvalidJournal);
        }

        let journal = Journal {
            device,
            jsb,
            first_block,
            block_size,
            journal_len: jsb.maxlen(),
            sequence: jsb.sequence(),
            head: jsb.start(),
            tail: jsb.start(),
            current_transaction: None,
            clean: jsb.start() == 0,
            revoked_blocks: Vec::new(),
        };

        Ok(journal)
    }

    /// Check if journal needs recovery
    pub fn needs_recovery(&self) -> bool {
        !self.clean
    }

    /// Recover the journal (replay uncommitted transactions)
    pub fn recover(&mut self) -> Ext4Result<()> {
        if self.clean {
            return Ok(());
        }

        // Phase 1: Scan journal to find valid transactions
        let mut position = self.jsb.start();
        let mut expected_sequence = self.jsb.sequence();
        let first_block = self.jsb.first();

        // Collect blocks to replay and revoked blocks
        let mut replay_blocks: Vec<(u64, Vec<u8>)> = Vec::new();

        loop {
            // Read block at current position
            let journal_block = first_block + position;
            let absolute_block = self.first_block + journal_block as u64;

            let mut buf = vec![0u8; self.block_size as usize];
            self.read_journal_block(absolute_block, &mut buf)?;

            // Check header
            let header: JournalHeader =
                unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const JournalHeader) };

            if header.magic() != JBD2_MAGIC {
                // End of valid journal
                break;
            }

            if header.sequence() != expected_sequence {
                // Sequence mismatch - end of valid journal
                break;
            }

            match header.blocktype() {
                block_type::DESCRIPTOR => {
                    // Parse descriptor block and collect data blocks
                    let mut data_blocks = self.parse_descriptor(&buf, position)?;
                    for (fs_block, data) in data_blocks.drain(..) {
                        if !self.revoked_blocks.contains(&fs_block) {
                            replay_blocks.push((fs_block, data));
                        }
                    }
                    position = self.wrap_position(position + 1);
                }
                block_type::COMMIT => {
                    // Transaction committed - advance to next
                    expected_sequence += 1;
                    position = self.wrap_position(position + 1);
                }
                block_type::REVOKE => {
                    // Parse revoke block
                    self.parse_revoke_block(&buf)?;
                    position = self.wrap_position(position + 1);
                }
                block_type::SUPERBLOCK_V1 | block_type::SUPERBLOCK_V2 => {
                    // Skip superblock
                    position = self.wrap_position(position + 1);
                }
                _ => {
                    // Unknown block type - stop recovery
                    break;
                }
            }
        }

        // Phase 2: Replay blocks (filter out revoked)
        for (fs_block, data) in replay_blocks {
            if !self.revoked_blocks.contains(&fs_block) {
                self.write_fs_block(fs_block, &data)?;
            }
        }

        // Phase 3: Reset journal
        self.reset()?;

        self.clean = true;
        self.revoked_blocks.clear();

        Ok(())
    }

    /// Parse descriptor block and return (fs_block, data) pairs
    fn parse_descriptor(&self, buf: &[u8], mut position: u32) -> Ext4Result<Vec<(u64, Vec<u8>)>> {
        let mut result = Vec::new();
        let tag_size = if self.jsb.is_64bit() { 16 } else { 8 };
        let mut offset = core::mem::size_of::<JournalHeader>();

        position = self.wrap_position(position + 1); // Move past descriptor block

        loop {
            if offset + tag_size > buf.len() {
                break;
            }

            let (fs_block, tag_flags) = if self.jsb.is_64bit() {
                let tag: JournalBlockTag3 = unsafe {
                    core::ptr::read_unaligned(buf[offset..].as_ptr() as *const JournalBlockTag3)
                };
                (tag.blocknr(), tag.flags())
            } else {
                let tag: JournalBlockTag = unsafe {
                    core::ptr::read_unaligned(buf[offset..].as_ptr() as *const JournalBlockTag)
                };
                let blocknr = u32::from_be(tag.t_blocknr) as u64;
                let flags = u16::from_be(tag.t_flags) as u32;
                (blocknr, flags)
            };

            // Read the data block
            let journal_block = self.first_block + (self.jsb.first() + position) as u64;
            let mut data = vec![0u8; self.block_size as usize];
            self.read_journal_block(journal_block, &mut data)?;

            // Handle escaped blocks
            if tag_flags & flags::ESCAPE != 0 {
                // First word was the magic number - restore it
                let magic_bytes = JBD2_MAGIC.to_be_bytes();
                data[0..4].copy_from_slice(&magic_bytes);
            }

            result.push((fs_block, data));
            position = self.wrap_position(position + 1);

            if tag_flags & flags::LAST_TAG != 0 {
                break;
            }

            offset += tag_size;
        }

        Ok(result)
    }

    /// Parse revoke block and add to revoked list
    fn parse_revoke_block(&mut self, buf: &[u8]) -> Ext4Result<()> {
        let header: JournalRevokeHeader =
            unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const JournalRevokeHeader) };

        let count = u32::from_be(header.r_count) as usize;
        let entry_size = if self.jsb.is_64bit() { 8 } else { 4 };
        let mut offset = core::mem::size_of::<JournalRevokeHeader>();

        while offset + entry_size <= count && offset + entry_size <= buf.len() {
            let block = if self.jsb.is_64bit() {
                let val: u64 =
                    unsafe { core::ptr::read_unaligned(buf[offset..].as_ptr() as *const u64) };
                u64::from_be(val)
            } else {
                let val: u32 =
                    unsafe { core::ptr::read_unaligned(buf[offset..].as_ptr() as *const u32) };
                u32::from_be(val) as u64
            };

            if !self.revoked_blocks.contains(&block) {
                self.revoked_blocks.push(block);
            }
            offset += entry_size;
        }

        Ok(())
    }

    /// Start a new transaction
    pub fn start_transaction(&mut self) -> Ext4Result<()> {
        if self.current_transaction.is_some() {
            return Err(Ext4Error::JournalBusy);
        }

        let tid = self.sequence;
        self.current_transaction = Some(Transaction::new(tid));
        Ok(())
    }

    /// Add a block to current transaction
    pub fn journal_block(
        &mut self,
        blocknr: u64,
        data: Vec<u8>,
        is_metadata: bool,
    ) -> Ext4Result<()> {
        let txn = self
            .current_transaction
            .as_mut()
            .ok_or(Ext4Error::NoTransaction)?;
        txn.add_block(blocknr, data, is_metadata);
        Ok(())
    }

    /// Commit the current transaction
    pub fn commit_transaction(&mut self) -> Ext4Result<()> {
        let txn = self
            .current_transaction
            .take()
            .ok_or(Ext4Error::NoTransaction)?;

        if txn.blocks.is_empty() && txn.revoked.is_empty() {
            // Empty transaction - nothing to do
            return Ok(());
        }

        // Phase 1: Write descriptor block(s) and data blocks
        let descriptor_block = self.allocate_journal_block();
        let mut descriptor = vec![0u8; self.block_size as usize];

        // Write descriptor header
        let header = JournalHeader::new(block_type::DESCRIPTOR, txn.tid);
        let header_bytes: [u8; 12] = unsafe { core::mem::transmute(header) };
        descriptor[0..12].copy_from_slice(&header_bytes);

        let tag_size = if self.jsb.is_64bit() { 16 } else { 8 };
        let mut tag_offset = 12;
        let mut data_positions = Vec::new();

        for (i, block) in txn.blocks.iter().enumerate() {
            // Allocate journal block for data
            let data_block = self.allocate_journal_block();
            data_positions.push((block.blocknr, data_block));

            // Write tag
            let mut flags = 0u32;
            if i == txn.blocks.len() - 1 {
                flags |= flags::LAST_TAG;
            }

            // Check if block needs escaping (starts with magic number)
            let first_word = if block.data.len() >= 4 {
                u32::from_be_bytes([block.data[0], block.data[1], block.data[2], block.data[3]])
            } else {
                0
            };
            if first_word == JBD2_MAGIC {
                flags |= flags::ESCAPE;
            }

            if self.jsb.is_64bit() {
                let tag = JournalBlockTag3 {
                    t_blocknr: (block.blocknr as u32).to_be(),
                    t_flags: flags.to_be(),
                    t_blocknr_high: ((block.blocknr >> 32) as u32).to_be(),
                    t_checksum: 0,
                };
                let tag_bytes: [u8; 16] = unsafe { core::mem::transmute(tag) };
                descriptor[tag_offset..tag_offset + 16].copy_from_slice(&tag_bytes);
            } else {
                let tag = JournalBlockTag {
                    t_blocknr: (block.blocknr as u32).to_be(),
                    t_checksum: 0,
                    t_flags: (flags as u16).to_be(),
                };
                let tag_bytes: [u8; 8] = unsafe { core::mem::transmute(tag) };
                descriptor[tag_offset..tag_offset + 8].copy_from_slice(&tag_bytes);
            }
            tag_offset += tag_size;
        }

        // Write descriptor block
        self.write_journal_block(descriptor_block, &descriptor)?;

        // Write data blocks
        for (i, block) in txn.blocks.iter().enumerate() {
            let (_, journal_block) = data_positions[i];
            let mut data = block.data.clone();

            // Escape magic number if needed
            if data.len() >= 4 {
                let first_word = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                if first_word == JBD2_MAGIC {
                    data[0..4].copy_from_slice(&[0, 0, 0, 0]);
                }
            }

            self.write_journal_block(journal_block, &data)?;
        }

        // Phase 2: Write revoke block (if any)
        if !txn.revoked.is_empty() {
            self.write_revoke_block(&txn.revoked, txn.tid)?;
        }

        // Phase 3: Write commit block
        let commit_block = self.allocate_journal_block();
        let mut commit_data = vec![0u8; self.block_size as usize];
        let commit_header = JournalHeader::new(block_type::COMMIT, txn.tid);
        let commit_bytes: [u8; 12] = unsafe { core::mem::transmute(commit_header) };
        commit_data[0..12].copy_from_slice(&commit_bytes);
        self.write_journal_block(commit_block, &commit_data)?;

        // Phase 4: Write actual filesystem blocks
        for block in &txn.blocks {
            self.write_fs_block(block.blocknr, &block.data)?;
        }

        // Phase 5: Update journal superblock
        self.sequence += 1;
        self.update_superblock()?;

        Ok(())
    }

    /// Write a revoke block
    fn write_revoke_block(&mut self, blocks: &[u64], tid: u32) -> Ext4Result<()> {
        let revoke_block = self.allocate_journal_block();
        let mut data = vec![0u8; self.block_size as usize];

        // Header
        let header = JournalHeader::new(block_type::REVOKE, tid);
        let header_bytes: [u8; 12] = unsafe { core::mem::transmute(header) };
        data[0..12].copy_from_slice(&header_bytes);

        let entry_size = if self.jsb.is_64bit() { 8 } else { 4 };
        let mut offset = core::mem::size_of::<JournalRevokeHeader>();

        for &block in blocks {
            if offset + entry_size > self.block_size as usize {
                break;
            }

            if self.jsb.is_64bit() {
                // Big-endian encoding for journal block numbers
                // — WireSaint
                let bytes = block.to_be_bytes();
                data[offset..offset + 8].copy_from_slice(&bytes);
            } else {
                let bytes = (block as u32).to_be_bytes();
                data[offset..offset + 4].copy_from_slice(&bytes);
            }
            offset += entry_size;
        }

        // Set count field — big-endian encoding
        // — WireSaint
        let count_bytes = (offset as u32).to_be_bytes();
        data[12..16].copy_from_slice(&count_bytes);

        self.write_journal_block(revoke_block, &data)?;
        Ok(())
    }

    /// Allocate next journal block
    fn allocate_journal_block(&mut self) -> u64 {
        let block = self.first_block + (self.jsb.first() + self.head) as u64;
        self.head = self.wrap_position(self.head + 1);
        block
    }

    /// Wrap journal position
    fn wrap_position(&self, pos: u32) -> u32 {
        let log_size = self.journal_len - self.jsb.first();
        if pos >= log_size { pos - log_size } else { pos }
    }

    /// Read a journal block
    fn read_journal_block(&self, block: u64, buf: &mut [u8]) -> Ext4Result<()> {
        let sectors_per_block = self.block_size / 512;
        let start_sector = block * sectors_per_block;

        for i in 0..sectors_per_block {
            let offset = i as usize * 512;
            self.device
                .read(start_sector + i, &mut buf[offset..offset + 512])
                .map_err(|_| Ext4Error::IoError)?;
        }
        Ok(())
    }

    /// Write a journal block
    fn write_journal_block(&self, block: u64, buf: &[u8]) -> Ext4Result<()> {
        let sectors_per_block = self.block_size / 512;
        let start_sector = block * sectors_per_block;

        for i in 0..sectors_per_block {
            let offset = i as usize * 512;
            self.device
                .write(start_sector + i, &buf[offset..offset + 512])
                .map_err(|_| Ext4Error::IoError)?;
        }
        self.device.flush().map_err(|_| Ext4Error::IoError)?;
        Ok(())
    }

    /// Write a filesystem block
    fn write_fs_block(&self, block: u64, buf: &[u8]) -> Ext4Result<()> {
        let sectors_per_block = self.block_size / 512;
        let start_sector = block * sectors_per_block;

        for i in 0..sectors_per_block {
            let offset = i as usize * 512;
            self.device
                .write(start_sector + i, &buf[offset..offset + 512])
                .map_err(|_| Ext4Error::IoError)?;
        }
        Ok(())
    }

    /// Reset journal to clean state
    fn reset(&mut self) -> Ext4Result<()> {
        self.head = 0;
        self.tail = 0;
        self.update_superblock()?;
        Ok(())
    }

    /// Update journal superblock on disk
    fn update_superblock(&self) -> Ext4Result<()> {
        let mut jsb = self.jsb;
        jsb.s_sequence = self.sequence.to_be();
        jsb.s_start = self.tail.to_be();

        let mut buf = vec![0u8; self.block_size as usize];
        let jsb_bytes: &[u8] = unsafe {
            core::slice::from_raw_parts(
                &jsb as *const _ as *const u8,
                core::mem::size_of::<JournalSuperblock>(),
            )
        };
        buf[..jsb_bytes.len()].copy_from_slice(jsb_bytes);

        self.write_journal_block(self.first_block, &buf)?;
        Ok(())
    }

    /// Abort current transaction without committing
    pub fn abort_transaction(&mut self) {
        self.current_transaction = None;
    }
}

/// Thread-safe journal wrapper
pub struct SharedJournal(pub Mutex<Journal>);

impl SharedJournal {
    pub fn new(journal: Journal) -> Self {
        SharedJournal(Mutex::new(journal))
    }

    /// Execute a journaled operation
    pub fn with_transaction<F, R>(&self, f: F) -> Ext4Result<R>
    where
        F: FnOnce(&mut Journal) -> Ext4Result<R>,
    {
        let mut journal = self.0.lock();
        journal.start_transaction()?;

        match f(&mut journal) {
            Ok(result) => {
                journal.commit_transaction()?;
                Ok(result)
            }
            Err(e) => {
                journal.abort_transaction();
                Err(e)
            }
        }
    }
}
