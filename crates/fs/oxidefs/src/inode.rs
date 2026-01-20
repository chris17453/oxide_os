//! OXIDEFS Inode structures

use crate::superblock::Superblock;
use crate::{OxidefsError, OxidefsResult, INODE_SIZE};
use block::BlockDevice;

/// Inode data structure (256 bytes)
#[derive(Debug, Clone)]
pub struct InodeData {
    /// File mode and type
    pub mode: u32,
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
    /// File size
    pub size: u64,
    /// Access time
    pub atime: u64,
    /// Modification time
    pub mtime: u64,
    /// Status change time
    pub ctime: u64,
    /// Link count
    pub links: u32,
    /// Block count (in filesystem blocks)
    pub blocks: u64,
    /// Flags
    pub flags: u32,
    /// Direct block pointers (12)
    pub direct: [u64; 12],
    /// Single indirect block pointer
    pub indirect: u64,
    /// Double indirect block pointer
    pub double_indirect: u64,
    /// Triple indirect block pointer
    pub triple_indirect: u64,
    /// Checksum
    pub checksum: u32,
}

impl Default for InodeData {
    fn default() -> Self {
        InodeData {
            mode: 0,
            uid: 0,
            gid: 0,
            size: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            links: 0,
            blocks: 0,
            flags: 0,
            direct: [0; 12],
            indirect: 0,
            double_indirect: 0,
            triple_indirect: 0,
            checksum: 0,
        }
    }
}

impl InodeData {
    /// Parse inode from bytes
    pub fn parse(data: &[u8]) -> OxidefsResult<Self> {
        if data.len() < INODE_SIZE as usize {
            return Err(OxidefsError::CorruptedInode);
        }

        let mut direct = [0u64; 12];
        for i in 0..12 {
            let offset = 48 + i * 8;
            direct[i] = u64::from_le_bytes([
                data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
                data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7],
            ]);
        }

        Ok(InodeData {
            mode: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            uid: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            gid: u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
            size: u64::from_le_bytes([
                data[12], data[13], data[14], data[15],
                data[16], data[17], data[18], data[19],
            ]),
            atime: u64::from_le_bytes([
                data[20], data[21], data[22], data[23],
                data[24], data[25], data[26], data[27],
            ]),
            mtime: u64::from_le_bytes([
                data[28], data[29], data[30], data[31],
                data[32], data[33], data[34], data[35],
            ]),
            ctime: u64::from_le_bytes([
                data[36], data[37], data[38], data[39],
                data[40], data[41], data[42], data[43],
            ]),
            links: u32::from_le_bytes([data[44], data[45], data[46], data[47]]),
            blocks: u64::from_le_bytes([
                data[144], data[145], data[146], data[147],
                data[148], data[149], data[150], data[151],
            ]),
            flags: u32::from_le_bytes([data[152], data[153], data[154], data[155]]),
            direct,
            indirect: u64::from_le_bytes([
                data[156], data[157], data[158], data[159],
                data[160], data[161], data[162], data[163],
            ]),
            double_indirect: u64::from_le_bytes([
                data[164], data[165], data[166], data[167],
                data[168], data[169], data[170], data[171],
            ]),
            triple_indirect: u64::from_le_bytes([
                data[172], data[173], data[174], data[175],
                data[176], data[177], data[178], data[179],
            ]),
            checksum: u32::from_le_bytes([data[180], data[181], data[182], data[183]]),
        })
    }

    /// Serialize inode to bytes
    pub fn serialize(&self, buf: &mut [u8]) {
        buf[0..4].copy_from_slice(&self.mode.to_le_bytes());
        buf[4..8].copy_from_slice(&self.uid.to_le_bytes());
        buf[8..12].copy_from_slice(&self.gid.to_le_bytes());
        buf[12..20].copy_from_slice(&self.size.to_le_bytes());
        buf[20..28].copy_from_slice(&self.atime.to_le_bytes());
        buf[28..36].copy_from_slice(&self.mtime.to_le_bytes());
        buf[36..44].copy_from_slice(&self.ctime.to_le_bytes());
        buf[44..48].copy_from_slice(&self.links.to_le_bytes());

        for i in 0..12 {
            let offset = 48 + i * 8;
            buf[offset..offset + 8].copy_from_slice(&self.direct[i].to_le_bytes());
        }

        buf[144..152].copy_from_slice(&self.blocks.to_le_bytes());
        buf[152..156].copy_from_slice(&self.flags.to_le_bytes());
        buf[156..164].copy_from_slice(&self.indirect.to_le_bytes());
        buf[164..172].copy_from_slice(&self.double_indirect.to_le_bytes());
        buf[172..180].copy_from_slice(&self.triple_indirect.to_le_bytes());
        buf[180..184].copy_from_slice(&self.checksum.to_le_bytes());
    }

    /// Check if this is a directory
    pub fn is_dir(&self) -> bool {
        (self.mode & 0o170000) == 0o040000
    }

    /// Check if this is a regular file
    pub fn is_file(&self) -> bool {
        (self.mode & 0o170000) == 0o100000
    }

    /// Check if this is a symlink
    pub fn is_symlink(&self) -> bool {
        (self.mode & 0o170000) == 0o120000
    }
}

/// Read an inode from disk
pub fn read_inode(device: &dyn BlockDevice, sb: &Superblock, ino: u64) -> OxidefsResult<InodeData> {
    let block_size = sb.block_size as usize;
    let inodes_per_block = block_size / INODE_SIZE as usize;

    let inode_block = sb.inode_table_start + (ino / inodes_per_block as u64);
    let inode_offset = ((ino % inodes_per_block as u64) * INODE_SIZE as u64) as usize;

    let mut buf = alloc::vec![0u8; block_size];
    device.read(inode_block, &mut buf)?;

    InodeData::parse(&buf[inode_offset..])
}

/// Write an inode to disk
pub fn write_inode(device: &dyn BlockDevice, sb: &Superblock, ino: u64, inode: &InodeData) -> OxidefsResult<()> {
    let block_size = sb.block_size as usize;
    let inodes_per_block = block_size / INODE_SIZE as usize;

    let inode_block = sb.inode_table_start + (ino / inodes_per_block as u64);
    let inode_offset = ((ino % inodes_per_block as u64) * INODE_SIZE as u64) as usize;

    let mut buf = alloc::vec![0u8; block_size];
    device.read(inode_block, &mut buf)?;

    inode.serialize(&mut buf[inode_offset..inode_offset + INODE_SIZE as usize]);

    device.write(inode_block, &buf)?;

    Ok(())
}

/// Cached inode with filesystem reference
pub struct Inode {
    /// Inode number
    pub ino: u64,
    /// Inode data
    pub data: InodeData,
    /// Dirty flag
    pub dirty: bool,
}

impl Inode {
    /// Create a new inode
    pub fn new(ino: u64, data: InodeData) -> Self {
        Inode {
            ino,
            data,
            dirty: false,
        }
    }

    /// Mark inode as dirty
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}
