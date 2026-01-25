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
