//! ext4 file operations

use block::BlockDevice;

use crate::error::{Ext4Error, Ext4Result};
use crate::extent::map_block;
use crate::group_desc::read_block;
use crate::inode::Ext4Inode;
use crate::superblock::Ext4Superblock;

/// Read data from a file
pub fn read_file(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    inode: &Ext4Inode,
    offset: u64,
    buf: &mut [u8],
) -> Ext4Result<usize> {
    let file_size = inode.size();
    let block_size = sb.block_size();

    // Check offset
    if offset >= file_size {
        return Ok(0);
    }

    // Clamp read length to file size
    let max_read = (file_size - offset) as usize;
    let read_len = buf.len().min(max_read);

    if read_len == 0 {
        return Ok(0);
    }

    let mut bytes_read = 0;
    let mut current_offset = offset;

    // Allocate block buffer
    let mut block_buf = alloc::vec![0u8; block_size as usize];

    while bytes_read < read_len {
        let logical_block = current_offset / block_size;
        let offset_in_block = (current_offset % block_size) as usize;

        // How much can we read from this block?
        let bytes_remaining_in_block = block_size as usize - offset_in_block;
        let bytes_to_read = (read_len - bytes_read).min(bytes_remaining_in_block);

        // Map logical block to physical
        match map_block(device, sb, inode, logical_block)? {
            Some(phys_block) => {
                // Read the block
                read_block(device, sb, phys_block, &mut block_buf)?;

                // Copy data to output buffer
                buf[bytes_read..bytes_read + bytes_to_read]
                    .copy_from_slice(&block_buf[offset_in_block..offset_in_block + bytes_to_read]);
            }
            None => {
                // Sparse file - fill with zeros
                buf[bytes_read..bytes_read + bytes_to_read].fill(0);
            }
        }

        bytes_read += bytes_to_read;
        current_offset += bytes_to_read as u64;
    }

    Ok(bytes_read)
}

/// Read symbolic link target (stored inline if small enough)
pub fn read_symlink(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    inode: &Ext4Inode,
) -> Ext4Result<alloc::string::String> {
    if !inode.is_symlink() {
        return Err(Ext4Error::InvalidInode);
    }

    let size = inode.size() as usize;

    // Fast symlinks store the target directly in i_block
    // (up to 60 bytes, which is the size of i_block array)
    if size <= 60 && inode.i_blocks_lo == 0 {
        // Target is stored in i_block
        let bytes = unsafe {
            core::slice::from_raw_parts(inode.i_block.as_ptr() as *const u8, size)
        };
        return Ok(alloc::string::String::from_utf8_lossy(bytes).into_owned());
    }

    // Slow symlink - read from data blocks
    let mut buf = alloc::vec![0u8; size];
    read_file(device, sb, inode, 0, &mut buf)?;

    Ok(alloc::string::String::from_utf8_lossy(&buf).into_owned())
}

// ============================================================================
// WRITE SUPPORT
// ============================================================================

use crate::bitmap::{alloc_block, free_block};
use crate::extent::{insert_extent, try_extend_extent};
use crate::group_desc::{write_block, BlockGroupTable};

/// Write data to a file
///
/// This function handles block allocation as needed to extend the file.
pub fn write_file(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    inode: &mut Ext4Inode,
    offset: u64,
    buf: &[u8],
) -> Ext4Result<usize> {
    if inode.is_dir() {
        return Err(Ext4Error::IsDirectory);
    }

    let block_size = sb.block_size();

    if buf.is_empty() {
        return Ok(0);
    }

    let mut bytes_written = 0usize;
    let mut current_offset = offset;

    // Allocate block buffer
    let mut block_buf = alloc::vec![0u8; block_size as usize];

    // Determine group for allocation (prefer group 0 for now)
    let preferred_group = Some(0u32);

    while bytes_written < buf.len() {
        let logical_block = current_offset / block_size;
        let offset_in_block = (current_offset % block_size) as usize;

        // How much can we write to this block?
        let bytes_remaining_in_block = block_size as usize - offset_in_block;
        let bytes_to_write = (buf.len() - bytes_written).min(bytes_remaining_in_block);

        // Check if block exists
        let phys_block = match map_block(device, sb, inode, logical_block)? {
            Some(phys) => {
                // Block exists - read it first (partial write)
                if offset_in_block != 0 || bytes_to_write != block_size as usize {
                    read_block(device, sb, phys, &mut block_buf)?;
                }
                phys
            }
            None => {
                // Need to allocate a new block
                let new_block = alloc_block(device, sb, group_table, preferred_group)?
                    .ok_or(Ext4Error::NoSpace)?;

                // Try to extend existing extent or insert new one
                if !try_extend_extent(inode, logical_block as u32, new_block)? {
                    insert_extent(inode, logical_block as u32, new_block, 1)?;
                }

                // Zero the buffer for new block
                block_buf.fill(0);

                // Update block count
                let blocks_512 = inode.blocks() + (block_size / 512);
                inode.set_blocks(blocks_512);

                new_block
            }
        };

        // Write data to buffer
        block_buf[offset_in_block..offset_in_block + bytes_to_write]
            .copy_from_slice(&buf[bytes_written..bytes_written + bytes_to_write]);

        // Write block to disk
        write_block(device, sb, phys_block, &block_buf)?;

        bytes_written += bytes_to_write;
        current_offset += bytes_to_write as u64;
    }

    // Update file size if we wrote past the end
    let new_end = offset + bytes_written as u64;
    if new_end > inode.size() {
        inode.set_size(new_end);
    }

    Ok(bytes_written)
}

/// Truncate a file to a specific size
///
/// If the new size is smaller, free the unused blocks.
/// If the new size is larger, the file becomes sparse (no new allocation).
pub fn truncate_file(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    inode: &mut Ext4Inode,
    new_size: u64,
) -> Ext4Result<()> {
    let old_size = inode.size();
    let block_size = sb.block_size();

    if new_size == old_size {
        return Ok(());
    }

    if new_size > old_size {
        // Extending - just update size (file becomes sparse)
        inode.set_size(new_size);
        return Ok(());
    }

    // Shrinking - need to free blocks
    let old_blocks = (old_size + block_size - 1) / block_size;
    let new_blocks = (new_size + block_size - 1) / block_size;

    // Free blocks from new_blocks to old_blocks
    for logical_block in new_blocks..old_blocks {
        if let Some(phys) = map_block(device, sb, inode, logical_block)? {
            free_block(device, sb, group_table, phys)?;
        }
    }

    // Update size
    inode.set_size(new_size);

    // Update block count
    let blocks_512 = new_blocks * (block_size / 512);
    inode.set_blocks(blocks_512);

    // Note: We should also update the extent tree to remove freed extents,
    // but that's complex. For now, the blocks are freed but the extent
    // entries remain (pointing to freed blocks). A proper implementation
    // would clean up the extent tree.

    Ok(())
}

/// Write a symbolic link target
///
/// Uses fast symlink (inline in i_block) if target fits, otherwise allocates a block.
pub fn write_symlink(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    inode: &mut Ext4Inode,
    target: &str,
) -> Ext4Result<()> {
    use crate::inode::file_type::S_IFLNK;

    // Verify this is a symlink
    if inode.file_type() != S_IFLNK {
        return Err(Ext4Error::InvalidInode);
    }

    let target_bytes = target.as_bytes();

    if target_bytes.len() <= 60 {
        // Fast symlink - store directly in i_block
        let i_block_bytes = unsafe {
            core::slice::from_raw_parts_mut(inode.i_block.as_mut_ptr() as *mut u8, 60)
        };
        i_block_bytes[..target_bytes.len()].copy_from_slice(target_bytes);

        // Clear i_flags (no extents for fast symlinks)
        inode.i_flags &= !crate::inode::flags::EXTENTS;
        inode.i_blocks_lo = 0;
        inode.set_size(target_bytes.len() as u64);
    } else {
        // Slow symlink - need to allocate blocks
        // Initialize extent header
        crate::inode::init_extent_header(inode);

        // Write the target as file data
        write_file(device, sb, group_table, inode, 0, target_bytes)?;
    }

    Ok(())
}

/// Zero out a region of a file (for sparse file support)
pub fn zero_range(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    inode: &Ext4Inode,
    offset: u64,
    len: u64,
) -> Ext4Result<()> {
    let block_size = sb.block_size();
    let end = offset + len;
    let mut current = offset;

    let mut block_buf = alloc::vec![0u8; block_size as usize];

    while current < end {
        let logical_block = current / block_size;
        let offset_in_block = (current % block_size) as usize;
        let remaining_in_block = block_size as usize - offset_in_block;
        let to_zero = (remaining_in_block as u64).min(end - current) as usize;

        // Only write if block exists (sparse blocks are already "zero")
        if let Some(phys) = map_block(device, sb, inode, logical_block)? {
            // Read block
            read_block(device, sb, phys, &mut block_buf)?;

            // Zero the range
            block_buf[offset_in_block..offset_in_block + to_zero].fill(0);

            // Write back
            write_block(device, sb, phys, &block_buf)?;
        }

        current += to_zero as u64;
    }

    Ok(())
}
