//! EFFLUXFS File operations

use alloc::vec::Vec;

use crate::inode::InodeData;
use crate::superblock::Superblock;
use crate::{EffluxfsError, EffluxfsResult, BLOCK_SIZE};
use block::BlockDevice;

/// Read file data
pub fn read_file(
    device: &dyn BlockDevice,
    sb: &Superblock,
    inode: &InodeData,
    offset: u64,
    buf: &mut [u8],
) -> EffluxfsResult<usize> {
    if offset >= inode.size {
        return Ok(0);
    }

    let block_size = sb.block_size as u64;
    let mut bytes_read = 0usize;
    let mut current_offset = offset;
    let max_read = buf.len().min((inode.size - offset) as usize);

    while bytes_read < max_read {
        let block_num = current_offset / block_size;
        let block_offset = (current_offset % block_size) as usize;

        // Get physical block
        let phys_block = get_block(device, sb, inode, block_num)?;
        if phys_block == 0 {
            // Sparse file - return zeros
            let to_read = (block_size as usize - block_offset).min(max_read - bytes_read);
            buf[bytes_read..bytes_read + to_read].fill(0);
            bytes_read += to_read;
            current_offset += to_read as u64;
            continue;
        }

        // Read block
        let mut block_buf = alloc::vec![0u8; block_size as usize];
        device.read(phys_block, &mut block_buf)?;

        let to_read = (block_size as usize - block_offset).min(max_read - bytes_read);
        buf[bytes_read..bytes_read + to_read]
            .copy_from_slice(&block_buf[block_offset..block_offset + to_read]);

        bytes_read += to_read;
        current_offset += to_read as u64;
    }

    Ok(bytes_read)
}

/// Write file data
pub fn write_file(
    device: &dyn BlockDevice,
    sb: &Superblock,
    inode: &mut InodeData,
    offset: u64,
    buf: &[u8],
    alloc_block: impl Fn() -> EffluxfsResult<u64>,
) -> EffluxfsResult<usize> {
    let block_size = sb.block_size as u64;
    let mut bytes_written = 0usize;
    let mut current_offset = offset;

    while bytes_written < buf.len() {
        let block_num = current_offset / block_size;
        let block_offset = (current_offset % block_size) as usize;

        // Get or allocate physical block
        let phys_block = get_or_alloc_block(device, sb, inode, block_num, &alloc_block)?;

        // Read-modify-write if partial block
        let mut block_buf = alloc::vec![0u8; block_size as usize];
        if block_offset != 0 || buf.len() - bytes_written < block_size as usize {
            device.read(phys_block, &mut block_buf)?;
        }

        let to_write = (block_size as usize - block_offset).min(buf.len() - bytes_written);
        block_buf[block_offset..block_offset + to_write]
            .copy_from_slice(&buf[bytes_written..bytes_written + to_write]);

        device.write(phys_block, &block_buf)?;

        bytes_written += to_write;
        current_offset += to_write as u64;
    }

    // Update size if we extended the file
    if offset + bytes_written as u64 > inode.size {
        inode.size = offset + bytes_written as u64;
    }

    Ok(bytes_written)
}

/// Get physical block number for a logical block
fn get_block(
    device: &dyn BlockDevice,
    sb: &Superblock,
    inode: &InodeData,
    block_num: u64,
) -> EffluxfsResult<u64> {
    let block_size = sb.block_size as u64;
    let ptrs_per_block = block_size / 8; // 64-bit pointers

    // Direct blocks (0-11)
    if block_num < 12 {
        return Ok(inode.direct[block_num as usize]);
    }

    let block_num = block_num - 12;

    // Single indirect (12 to 12 + ptrs_per_block - 1)
    if block_num < ptrs_per_block {
        if inode.indirect == 0 {
            return Ok(0);
        }
        return read_indirect_ptr(device, inode.indirect, block_num as usize, block_size);
    }

    let block_num = block_num - ptrs_per_block;

    // Double indirect
    if block_num < ptrs_per_block * ptrs_per_block {
        if inode.double_indirect == 0 {
            return Ok(0);
        }
        let idx1 = block_num / ptrs_per_block;
        let idx2 = block_num % ptrs_per_block;

        let indirect_block = read_indirect_ptr(device, inode.double_indirect, idx1 as usize, block_size)?;
        if indirect_block == 0 {
            return Ok(0);
        }
        return read_indirect_ptr(device, indirect_block, idx2 as usize, block_size);
    }

    let block_num = block_num - ptrs_per_block * ptrs_per_block;

    // Triple indirect
    if block_num < ptrs_per_block * ptrs_per_block * ptrs_per_block {
        if inode.triple_indirect == 0 {
            return Ok(0);
        }
        let idx1 = block_num / (ptrs_per_block * ptrs_per_block);
        let idx2 = (block_num / ptrs_per_block) % ptrs_per_block;
        let idx3 = block_num % ptrs_per_block;

        let d_indirect = read_indirect_ptr(device, inode.triple_indirect, idx1 as usize, block_size)?;
        if d_indirect == 0 {
            return Ok(0);
        }
        let s_indirect = read_indirect_ptr(device, d_indirect, idx2 as usize, block_size)?;
        if s_indirect == 0 {
            return Ok(0);
        }
        return read_indirect_ptr(device, s_indirect, idx3 as usize, block_size);
    }

    Err(EffluxfsError::InvalidArgument)
}

/// Get or allocate a physical block
fn get_or_alloc_block(
    device: &dyn BlockDevice,
    sb: &Superblock,
    inode: &mut InodeData,
    block_num: u64,
    alloc_block: &impl Fn() -> EffluxfsResult<u64>,
) -> EffluxfsResult<u64> {
    let existing = get_block(device, sb, inode, block_num)?;
    if existing != 0 {
        return Ok(existing);
    }

    // Allocate new block
    let new_block = alloc_block()?;
    set_block(device, sb, inode, block_num, new_block, alloc_block)?;
    inode.blocks += 1;

    Ok(new_block)
}

/// Set physical block number for a logical block
fn set_block(
    device: &dyn BlockDevice,
    sb: &Superblock,
    inode: &mut InodeData,
    block_num: u64,
    phys_block: u64,
    alloc_block: &impl Fn() -> EffluxfsResult<u64>,
) -> EffluxfsResult<()> {
    let block_size = sb.block_size as u64;
    let ptrs_per_block = block_size / 8;

    // Direct blocks
    if block_num < 12 {
        inode.direct[block_num as usize] = phys_block;
        return Ok(());
    }

    let block_num = block_num - 12;

    // Single indirect
    if block_num < ptrs_per_block {
        if inode.indirect == 0 {
            inode.indirect = alloc_block()?;
            inode.blocks += 1;
            // Zero out the new indirect block
            let zeros = alloc::vec![0u8; block_size as usize];
            device.write(inode.indirect, &zeros)?;
        }
        return write_indirect_ptr(device, inode.indirect, block_num as usize, phys_block, block_size);
    }

    // Double and triple indirect would follow similar patterns
    // Simplified for now

    Err(EffluxfsError::NoSpace)
}

/// Read a pointer from an indirect block
fn read_indirect_ptr(
    device: &dyn BlockDevice,
    indirect_block: u64,
    index: usize,
    block_size: u64,
) -> EffluxfsResult<u64> {
    let mut buf = alloc::vec![0u8; block_size as usize];
    device.read(indirect_block, &mut buf)?;

    let offset = index * 8;
    Ok(u64::from_le_bytes([
        buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3],
        buf[offset + 4], buf[offset + 5], buf[offset + 6], buf[offset + 7],
    ]))
}

/// Write a pointer to an indirect block
fn write_indirect_ptr(
    device: &dyn BlockDevice,
    indirect_block: u64,
    index: usize,
    value: u64,
    block_size: u64,
) -> EffluxfsResult<()> {
    let mut buf = alloc::vec![0u8; block_size as usize];
    device.read(indirect_block, &mut buf)?;

    let offset = index * 8;
    buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());

    device.write(indirect_block, &buf)?;
    Ok(())
}

/// Truncate a file
pub fn truncate_file(
    device: &dyn BlockDevice,
    sb: &Superblock,
    inode: &mut InodeData,
    new_size: u64,
    free_block: impl Fn(u64) -> EffluxfsResult<()>,
) -> EffluxfsResult<()> {
    if new_size >= inode.size {
        // Extending - just update size
        inode.size = new_size;
        return Ok(());
    }

    let block_size = sb.block_size as u64;
    let old_blocks = (inode.size + block_size - 1) / block_size;
    let new_blocks = (new_size + block_size - 1) / block_size;

    // Free blocks from new_blocks to old_blocks
    for block_num in new_blocks..old_blocks {
        let phys = get_block(device, sb, inode, block_num)?;
        if phys != 0 {
            free_block(phys)?;
            inode.blocks -= 1;
        }
    }

    inode.size = new_size;
    Ok(())
}
