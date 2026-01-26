//! ext4 extent tree handling

use block::BlockDevice;

use crate::error::{Ext4Error, Ext4Result};
use crate::group_desc::read_block;
use crate::inode::Ext4Inode;
use crate::superblock::Ext4Superblock;

/// Extent header magic
pub const EXT4_EXT_MAGIC: u16 = 0xF30A;

/// Maximum extent tree depth
pub const MAX_DEPTH: u16 = 5;

/// Extent header (12 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ExtentHeader {
    /// Magic number (0xF30A)
    pub eh_magic: u16,
    /// Number of valid entries
    pub eh_entries: u16,
    /// Capacity of entries
    pub eh_max: u16,
    /// Tree depth (0 = leaf)
    pub eh_depth: u16,
    /// Generation (unused)
    pub eh_generation: u32,
}

/// Extent index entry (12 bytes) - points to next level
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ExtentIndex {
    /// Logical block covered by this index
    pub ei_block: u32,
    /// Physical block of next level (low 32 bits)
    pub ei_leaf_lo: u32,
    /// Physical block (high 16 bits)
    pub ei_leaf_hi: u16,
    /// Unused
    pub ei_unused: u16,
}

impl ExtentIndex {
    /// Get the physical block number this index points to
    pub fn leaf_block(&self) -> u64 {
        self.ei_leaf_lo as u64 | ((self.ei_leaf_hi as u64) << 32)
    }
}

/// Extent leaf entry (12 bytes) - actual extent mapping
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Extent {
    /// First logical block covered
    pub ee_block: u32,
    /// Number of blocks covered
    pub ee_len: u16,
    /// Physical block (high 16 bits)
    pub ee_start_hi: u16,
    /// Physical block (low 32 bits)
    pub ee_start_lo: u32,
}

impl Extent {
    /// Get the starting physical block
    pub fn start(&self) -> u64 {
        self.ee_start_lo as u64 | ((self.ee_start_hi as u64) << 32)
    }

    /// Get length (handling uninitialized extents)
    pub fn len(&self) -> u32 {
        // High bit indicates uninitialized extent
        (self.ee_len & 0x7FFF) as u32
    }

    /// Check if extent is uninitialized (pre-allocated but not written)
    pub fn is_uninitialized(&self) -> bool {
        self.ee_len & 0x8000 != 0
    }

    /// Check if logical block is within this extent
    pub fn contains(&self, logical_block: u32) -> bool {
        logical_block >= self.ee_block && logical_block < self.ee_block + self.len()
    }

    /// Map a logical block to physical block
    pub fn map(&self, logical_block: u32) -> Option<u64> {
        if !self.contains(logical_block) {
            return None;
        }
        let offset = logical_block - self.ee_block;
        Some(self.start() + offset as u64)
    }
}

/// Parse extent header from inode i_block field
pub fn parse_header(i_block: &[u32; 15]) -> Ext4Result<ExtentHeader> {
    let bytes = unsafe { core::slice::from_raw_parts(i_block.as_ptr() as *const u8, 60) };

    let header: ExtentHeader =
        unsafe { core::ptr::read_unaligned(bytes.as_ptr() as *const ExtentHeader) };

    if header.eh_magic != EXT4_EXT_MAGIC {
        return Err(Ext4Error::InvalidExtent);
    }

    if header.eh_depth > MAX_DEPTH {
        return Err(Ext4Error::InvalidExtent);
    }

    Ok(header)
}

/// Get extents from inode i_block (depth 0 - leaf node in inode)
pub fn get_extents_from_inode(i_block: &[u32; 15]) -> Ext4Result<(&ExtentHeader, &[Extent])> {
    let bytes = unsafe { core::slice::from_raw_parts(i_block.as_ptr() as *const u8, 60) };

    let header: &ExtentHeader = unsafe { &*(bytes.as_ptr() as *const ExtentHeader) };

    if header.eh_magic != EXT4_EXT_MAGIC {
        return Err(Ext4Error::InvalidExtent);
    }

    // Extents start after the header (12 bytes)
    let extent_start = 12;
    let num_extents = header.eh_entries as usize;

    if num_extents > 4 {
        // Max extents that fit in i_block after header: (60-12)/12 = 4
        return Err(Ext4Error::InvalidExtent);
    }

    let extents = unsafe {
        core::slice::from_raw_parts(bytes[extent_start..].as_ptr() as *const Extent, num_extents)
    };

    Ok((header, extents))
}

/// Get extent indexes from inode i_block (depth > 0)
pub fn get_indexes_from_inode(i_block: &[u32; 15]) -> Ext4Result<(&ExtentHeader, &[ExtentIndex])> {
    let bytes = unsafe { core::slice::from_raw_parts(i_block.as_ptr() as *const u8, 60) };

    let header: &ExtentHeader = unsafe { &*(bytes.as_ptr() as *const ExtentHeader) };

    if header.eh_magic != EXT4_EXT_MAGIC {
        return Err(Ext4Error::InvalidExtent);
    }

    let index_start = 12;
    let num_indexes = header.eh_entries as usize;

    if num_indexes > 4 {
        return Err(Ext4Error::InvalidExtent);
    }

    let indexes = unsafe {
        core::slice::from_raw_parts(
            bytes[index_start..].as_ptr() as *const ExtentIndex,
            num_indexes,
        )
    };

    Ok((header, indexes))
}

/// Map a logical block to physical block using extent tree
pub fn map_block(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    inode: &Ext4Inode,
    logical_block: u64,
) -> Ext4Result<Option<u64>> {
    if !inode.uses_extents() {
        // Fall back to indirect blocks (not implemented yet)
        return Err(Ext4Error::UnsupportedFeature);
    }

    let logical_block = logical_block as u32;

    // Parse extent tree from inode
    let header = parse_header(&inode.i_block)?;

    if header.eh_depth == 0 {
        // Leaf node - extents are directly in inode
        let (_, extents) = get_extents_from_inode(&inode.i_block)?;

        for extent in extents {
            if let Some(phys) = extent.map(logical_block) {
                if extent.is_uninitialized() {
                    // Uninitialized extent - return zeros
                    return Ok(None);
                }
                return Ok(Some(phys));
            }
        }

        // Block not found in any extent (sparse file)
        return Ok(None);
    }

    // Tree has depth > 0, need to traverse index nodes
    traverse_extent_tree(device, sb, &inode.i_block, header.eh_depth, logical_block)
}

/// Traverse extent tree to find physical block
fn traverse_extent_tree(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    i_block: &[u32; 15],
    depth: u16,
    logical_block: u32,
) -> Ext4Result<Option<u64>> {
    // Get indexes from inode
    let (_, indexes) = get_indexes_from_inode(i_block)?;

    // Find the index entry that covers our logical block
    let mut target_index: Option<&ExtentIndex> = None;
    for idx in indexes {
        if logical_block >= idx.ei_block {
            target_index = Some(idx);
        } else {
            break;
        }
    }

    let index = target_index.ok_or(Ext4Error::InvalidExtent)?;
    let next_block = index.leaf_block();

    // Read the next level block
    let block_size = sb.block_size();
    let mut buf = alloc::vec![0u8; block_size as usize];
    read_block(device, sb, next_block, &mut buf)?;

    // Parse header
    let header: ExtentHeader =
        unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const ExtentHeader) };

    if header.eh_magic != EXT4_EXT_MAGIC {
        return Err(Ext4Error::InvalidExtent);
    }

    if header.eh_depth == 0 {
        // Leaf node - contains extents
        let num_extents = header.eh_entries as usize;
        let extents = unsafe {
            core::slice::from_raw_parts(buf[12..].as_ptr() as *const Extent, num_extents)
        };

        for extent in extents {
            if let Some(phys) = extent.map(logical_block) {
                if extent.is_uninitialized() {
                    return Ok(None);
                }
                return Ok(Some(phys));
            }
        }

        // Block not found (sparse)
        return Ok(None);
    }

    // Another index level - recurse
    let num_indexes = header.eh_entries as usize;
    let indexes = unsafe {
        core::slice::from_raw_parts(buf[12..].as_ptr() as *const ExtentIndex, num_indexes)
    };

    // Find the index entry
    let mut target_index: Option<&ExtentIndex> = None;
    for idx in indexes {
        if logical_block >= idx.ei_block {
            target_index = Some(idx);
        } else {
            break;
        }
    }

    let index = target_index.ok_or(Ext4Error::InvalidExtent)?;
    traverse_extent_tree_from_block(
        device,
        sb,
        index.leaf_block(),
        header.eh_depth - 1,
        logical_block,
    )
}

/// Continue traversing extent tree from a block
fn traverse_extent_tree_from_block(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    block: u64,
    depth: u16,
    logical_block: u32,
) -> Ext4Result<Option<u64>> {
    let block_size = sb.block_size();
    let mut buf = alloc::vec![0u8; block_size as usize];
    read_block(device, sb, block, &mut buf)?;

    let header: ExtentHeader =
        unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const ExtentHeader) };

    if header.eh_magic != EXT4_EXT_MAGIC {
        return Err(Ext4Error::InvalidExtent);
    }

    if depth == 0 || header.eh_depth == 0 {
        // Leaf node
        let num_extents = header.eh_entries as usize;
        let extents = unsafe {
            core::slice::from_raw_parts(buf[12..].as_ptr() as *const Extent, num_extents)
        };

        for extent in extents {
            if let Some(phys) = extent.map(logical_block) {
                if extent.is_uninitialized() {
                    return Ok(None);
                }
                return Ok(Some(phys));
            }
        }

        return Ok(None);
    }

    // Index node
    let num_indexes = header.eh_entries as usize;
    let indexes = unsafe {
        core::slice::from_raw_parts(buf[12..].as_ptr() as *const ExtentIndex, num_indexes)
    };

    let mut target_index: Option<&ExtentIndex> = None;
    for idx in indexes {
        if logical_block >= idx.ei_block {
            target_index = Some(idx);
        } else {
            break;
        }
    }

    let index = target_index.ok_or(Ext4Error::InvalidExtent)?;
    traverse_extent_tree_from_block(device, sb, index.leaf_block(), depth - 1, logical_block)
}

// ============================================================================
// WRITE SUPPORT
// ============================================================================

impl Extent {
    /// Create a new extent
    pub fn new(logical_block: u32, physical_block: u64, len: u16) -> Self {
        Extent {
            ee_block: logical_block,
            ee_len: len,
            ee_start_hi: (physical_block >> 32) as u16,
            ee_start_lo: physical_block as u32,
        }
    }
}

/// Insert an extent into an inode's extent tree (depth 0 only for now)
///
/// This handles the simple case where extents fit directly in the inode.
/// Returns the number of blocks that were successfully mapped.
pub fn insert_extent(
    inode: &mut crate::inode::Ext4Inode,
    logical_block: u32,
    physical_block: u64,
    len: u16,
) -> Ext4Result<u16> {
    if !inode.uses_extents() {
        return Err(Ext4Error::UnsupportedFeature);
    }

    let header = parse_header(&inode.i_block)?;

    // Currently only support depth 0 (extents in inode)
    if header.eh_depth != 0 {
        return Err(Ext4Error::UnsupportedFeature);
    }

    let max_extents = header.eh_max as usize;
    let current_entries = header.eh_entries as usize;

    // Check if we have room for another extent
    if current_entries >= max_extents {
        // Would need to grow the tree - not implemented yet
        return Err(Ext4Error::NoSpace);
    }

    // Create the new extent
    let new_extent = Extent::new(logical_block, physical_block, len);

    // Find insertion point (keep extents sorted by logical block)
    let mut insert_pos = current_entries;
    let i_block_bytes =
        unsafe { core::slice::from_raw_parts_mut(inode.i_block.as_mut_ptr() as *mut u8, 60) };

    // Read existing extents
    let extents_start = 12; // After header
    for i in 0..current_entries {
        let offset = extents_start + i * 12;
        let extent: Extent =
            unsafe { core::ptr::read_unaligned(i_block_bytes[offset..].as_ptr() as *const Extent) };

        if logical_block < extent.ee_block {
            insert_pos = i;
            break;
        }
    }

    // Shift extents after insertion point
    for i in (insert_pos..current_entries).rev() {
        let src_offset = extents_start + i * 12;
        let dst_offset = extents_start + (i + 1) * 12;

        let extent: Extent = unsafe {
            core::ptr::read_unaligned(i_block_bytes[src_offset..].as_ptr() as *const Extent)
        };

        let extent_bytes =
            unsafe { core::slice::from_raw_parts(&extent as *const Extent as *const u8, 12) };
        i_block_bytes[dst_offset..dst_offset + 12].copy_from_slice(extent_bytes);
    }

    // Write the new extent
    let offset = extents_start + insert_pos * 12;
    let extent_bytes =
        unsafe { core::slice::from_raw_parts(&new_extent as *const Extent as *const u8, 12) };
    i_block_bytes[offset..offset + 12].copy_from_slice(extent_bytes);

    // Update header entry count
    let new_header = ExtentHeader {
        eh_magic: EXT4_EXT_MAGIC,
        eh_entries: (current_entries + 1) as u16,
        eh_max: header.eh_max,
        eh_depth: 0,
        eh_generation: header.eh_generation,
    };

    let header_bytes =
        unsafe { core::slice::from_raw_parts(&new_header as *const ExtentHeader as *const u8, 12) };
    i_block_bytes[..12].copy_from_slice(header_bytes);

    Ok(len)
}

/// Try to extend an existing extent to cover additional blocks
///
/// Returns true if the extent was extended, false if a new extent is needed.
pub fn try_extend_extent(
    inode: &mut crate::inode::Ext4Inode,
    logical_block: u32,
    physical_block: u64,
) -> Ext4Result<bool> {
    if !inode.uses_extents() {
        return Err(Ext4Error::UnsupportedFeature);
    }

    let header = parse_header(&inode.i_block)?;

    if header.eh_depth != 0 {
        return Ok(false); // Can't extend in deeper trees without full implementation
    }

    let current_entries = header.eh_entries as usize;
    let i_block_bytes =
        unsafe { core::slice::from_raw_parts_mut(inode.i_block.as_mut_ptr() as *mut u8, 60) };

    // Check each extent to see if we can extend it
    let extents_start = 12;
    for i in 0..current_entries {
        let offset = extents_start + i * 12;
        let mut extent: Extent =
            unsafe { core::ptr::read_unaligned(i_block_bytes[offset..].as_ptr() as *const Extent) };

        // Check if this extent ends right before our logical block
        let extent_end_logical = extent.ee_block + extent.len();
        let extent_end_physical = extent.start() + extent.len() as u64;

        if extent_end_logical == logical_block && extent_end_physical == physical_block {
            // Can extend! Check length limit (max 32768 blocks, or 15 bits)
            if extent.ee_len < 0x7FFF {
                extent.ee_len += 1;

                // Write back
                let extent_bytes = unsafe {
                    core::slice::from_raw_parts(&extent as *const Extent as *const u8, 12)
                };
                i_block_bytes[offset..offset + 12].copy_from_slice(extent_bytes);
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Get the last logical block covered by extents in an inode
pub fn last_logical_block(inode: &crate::inode::Ext4Inode) -> Ext4Result<Option<u32>> {
    if !inode.uses_extents() {
        return Err(Ext4Error::UnsupportedFeature);
    }

    let header = parse_header(&inode.i_block)?;

    if header.eh_depth != 0 {
        // For deeper trees, we'd need to traverse to the rightmost leaf
        return Err(Ext4Error::UnsupportedFeature);
    }

    if header.eh_entries == 0 {
        return Ok(None);
    }

    let (_, extents) = get_extents_from_inode(&inode.i_block)?;

    let mut max_end = 0u32;
    for extent in extents {
        let end = extent.ee_block + extent.len();
        if end > max_end {
            max_end = end;
        }
    }

    if max_end == 0 {
        Ok(None)
    } else {
        Ok(Some(max_end - 1))
    }
}

/// Count total blocks covered by extents
pub fn count_extent_blocks(inode: &crate::inode::Ext4Inode) -> Ext4Result<u64> {
    if !inode.uses_extents() {
        return Err(Ext4Error::UnsupportedFeature);
    }

    let header = parse_header(&inode.i_block)?;

    if header.eh_depth != 0 {
        return Err(Ext4Error::UnsupportedFeature);
    }

    let (_, extents) = get_extents_from_inode(&inode.i_block)?;

    let mut total = 0u64;
    for extent in extents {
        total += extent.len() as u64;
    }

    Ok(total)
}
