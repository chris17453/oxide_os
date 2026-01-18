//! EFFLUXFS Journal for crash recovery
//!
//! Simple write-ahead logging journal.

use alloc::vec::Vec;

use crate::{EffluxfsError, EffluxfsResult};
use efflux_block::BlockDevice;

/// Journal magic number
const JOURNAL_MAGIC: u32 = 0x4A524E4C; // "JRNL"

/// Journal block types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum JournalBlockType {
    /// Transaction start
    Start = 1,
    /// Data block
    Data = 2,
    /// Transaction commit
    Commit = 3,
    /// Revoke block
    Revoke = 4,
}

/// Journal header (in superblock)
#[derive(Debug, Clone)]
pub struct JournalHeader {
    /// Journal magic
    pub magic: u32,
    /// Journal size in blocks
    pub size: u32,
    /// Current head position
    pub head: u32,
    /// Current tail position
    pub tail: u32,
    /// Next transaction ID
    pub next_tid: u64,
}

impl JournalHeader {
    /// Parse from bytes
    pub fn parse(data: &[u8]) -> EffluxfsResult<Self> {
        if data.len() < 24 {
            return Err(EffluxfsError::IoError);
        }

        Ok(JournalHeader {
            magic: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            size: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            head: u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
            tail: u32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            next_tid: u64::from_le_bytes([
                data[16], data[17], data[18], data[19],
                data[20], data[21], data[22], data[23],
            ]),
        })
    }

    /// Serialize to bytes
    pub fn serialize(&self, buf: &mut [u8]) {
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4..8].copy_from_slice(&self.size.to_le_bytes());
        buf[8..12].copy_from_slice(&self.head.to_le_bytes());
        buf[12..16].copy_from_slice(&self.tail.to_le_bytes());
        buf[16..24].copy_from_slice(&self.next_tid.to_le_bytes());
    }
}

/// Journal transaction descriptor block
#[derive(Debug, Clone)]
pub struct TransactionDesc {
    /// Magic number
    pub magic: u32,
    /// Block type
    pub block_type: u32,
    /// Transaction ID
    pub tid: u64,
    /// Number of blocks in this transaction
    pub num_blocks: u32,
}

/// A pending journal transaction
pub struct Transaction {
    /// Transaction ID
    pub tid: u64,
    /// Blocks to write (block number, data)
    pub blocks: Vec<(u64, Vec<u8>)>,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(tid: u64) -> Self {
        Transaction {
            tid,
            blocks: Vec::new(),
        }
    }

    /// Add a block to the transaction
    pub fn add_block(&mut self, block: u64, data: Vec<u8>) {
        self.blocks.push((block, data));
    }
}

/// Journal instance
pub struct Journal {
    /// Journal start block
    start_block: u64,
    /// Journal size in blocks
    size: u32,
    /// Current head
    head: u32,
    /// Current tail
    tail: u32,
    /// Next transaction ID
    next_tid: u64,
    /// Block size
    block_size: u32,
}

impl Journal {
    /// Initialize a new journal
    pub fn init(device: &dyn BlockDevice, start_block: u64, size: u32) -> EffluxfsResult<Self> {
        let block_size = device.block_size();

        // Write journal header
        let header = JournalHeader {
            magic: JOURNAL_MAGIC,
            size,
            head: 0,
            tail: 0,
            next_tid: 1,
        };

        let mut buf = alloc::vec![0u8; block_size as usize];
        header.serialize(&mut buf);
        device.write(start_block, &buf)?;

        Ok(Journal {
            start_block,
            size,
            head: 0,
            tail: 0,
            next_tid: 1,
            block_size,
        })
    }

    /// Load existing journal
    pub fn load(device: &dyn BlockDevice, start_block: u64) -> EffluxfsResult<Self> {
        let block_size = device.block_size();

        let mut buf = alloc::vec![0u8; block_size as usize];
        device.read(start_block, &mut buf)?;

        let header = JournalHeader::parse(&buf)?;
        if header.magic != JOURNAL_MAGIC {
            return Err(EffluxfsError::IoError);
        }

        Ok(Journal {
            start_block,
            size: header.size,
            head: header.head,
            tail: header.tail,
            next_tid: header.next_tid,
            block_size,
        })
    }

    /// Begin a new transaction
    pub fn begin(&mut self) -> Transaction {
        let tid = self.next_tid;
        self.next_tid += 1;
        Transaction::new(tid)
    }

    /// Commit a transaction
    pub fn commit(&mut self, device: &dyn BlockDevice, tx: Transaction) -> EffluxfsResult<()> {
        // 1. Write transaction start block
        let start_block = self.start_block + 1 + self.head as u64;
        let mut buf = alloc::vec![0u8; self.block_size as usize];

        // Transaction start descriptor
        buf[0..4].copy_from_slice(&JOURNAL_MAGIC.to_le_bytes());
        buf[4..8].copy_from_slice(&(JournalBlockType::Start as u32).to_le_bytes());
        buf[8..16].copy_from_slice(&tx.tid.to_le_bytes());
        buf[16..20].copy_from_slice(&(tx.blocks.len() as u32).to_le_bytes());

        device.write(start_block, &buf)?;
        self.advance_head(1);

        // 2. Write data blocks with their destinations
        for (dest_block, data) in &tx.blocks {
            // Write block descriptor + destination
            let journal_block = self.start_block + 1 + self.head as u64;
            buf.fill(0);
            buf[0..4].copy_from_slice(&JOURNAL_MAGIC.to_le_bytes());
            buf[4..8].copy_from_slice(&(JournalBlockType::Data as u32).to_le_bytes());
            buf[8..16].copy_from_slice(&dest_block.to_le_bytes());
            device.write(journal_block, &buf)?;
            self.advance_head(1);

            // Write actual data
            let journal_block = self.start_block + 1 + self.head as u64;
            device.write(journal_block, data)?;
            self.advance_head(1);
        }

        // 3. Write commit block
        let commit_block = self.start_block + 1 + self.head as u64;
        buf.fill(0);
        buf[0..4].copy_from_slice(&JOURNAL_MAGIC.to_le_bytes());
        buf[4..8].copy_from_slice(&(JournalBlockType::Commit as u32).to_le_bytes());
        buf[8..16].copy_from_slice(&tx.tid.to_le_bytes());
        device.write(commit_block, &buf)?;
        self.advance_head(1);

        // 4. Sync journal
        device.flush()?;

        // 5. Write actual data to disk
        for (dest_block, data) in &tx.blocks {
            device.write(*dest_block, data)?;
        }
        device.flush()?;

        // 6. Update tail (checkpoint)
        self.tail = self.head;
        self.sync_header(device)?;

        Ok(())
    }

    /// Advance head pointer with wrap-around
    fn advance_head(&mut self, count: u32) {
        self.head = (self.head + count) % (self.size - 1);
    }

    /// Sync journal header to disk
    fn sync_header(&self, device: &dyn BlockDevice) -> EffluxfsResult<()> {
        let header = JournalHeader {
            magic: JOURNAL_MAGIC,
            size: self.size,
            head: self.head,
            tail: self.tail,
            next_tid: self.next_tid,
        };

        let mut buf = alloc::vec![0u8; self.block_size as usize];
        header.serialize(&mut buf);
        device.write(self.start_block, &buf)?;
        device.flush()?;

        Ok(())
    }

    /// Recover from journal after crash
    pub fn recover(&mut self, device: &dyn BlockDevice) -> EffluxfsResult<()> {
        // Scan journal for committed transactions that weren't checkpointed
        let mut pos = self.tail;

        while pos != self.head {
            let block = self.start_block + 1 + pos as u64;
            let mut buf = alloc::vec![0u8; self.block_size as usize];
            device.read(block, &mut buf)?;

            let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            if magic != JOURNAL_MAGIC {
                break;
            }

            let block_type = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);

            match block_type {
                t if t == JournalBlockType::Start as u32 => {
                    // Start of transaction - replay it
                    let tid = u64::from_le_bytes([
                        buf[8], buf[9], buf[10], buf[11],
                        buf[12], buf[13], buf[14], buf[15],
                    ]);
                    let num_blocks = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);

                    // Read data blocks and replay
                    for _ in 0..num_blocks {
                        pos = (pos + 1) % (self.size - 1);
                        // Read descriptor
                        let block = self.start_block + 1 + pos as u64;
                        device.read(block, &mut buf)?;
                        let dest = u64::from_le_bytes([
                            buf[8], buf[9], buf[10], buf[11],
                            buf[12], buf[13], buf[14], buf[15],
                        ]);

                        pos = (pos + 1) % (self.size - 1);
                        // Read data
                        let block = self.start_block + 1 + pos as u64;
                        device.read(block, &mut buf)?;

                        // Write to destination
                        device.write(dest, &buf)?;
                    }
                }
                t if t == JournalBlockType::Commit as u32 => {
                    // Transaction committed
                }
                _ => break,
            }

            pos = (pos + 1) % (self.size - 1);
        }

        device.flush()?;
        self.tail = self.head;
        self.sync_header(device)?;

        Ok(())
    }
}
