//! Unit tests for ext4 filesystem
//!
//! These tests use a RamDisk to test filesystem operations in isolation.
//! They can be run from within the kernel test framework.

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use block::BlockDevice;
    use block::device::RamDisk;

    use crate::bitmap::{BlockBitmap, InodeBitmap};
    use crate::extent::{EXT4_EXT_MAGIC, Extent, ExtentHeader};
    use crate::inode::{Ext4Inode, file_type, flags as inode_flags};
    use crate::superblock::{EXT4_MAGIC, Ext4Superblock};

    /// Create a minimal ext4 superblock for testing
    fn create_test_superblock() -> Ext4Superblock {
        let mut sb = unsafe { core::mem::zeroed::<Ext4Superblock>() };
        sb.s_magic = EXT4_MAGIC;
        sb.s_log_block_size = 0; // 1024 byte blocks
        sb.s_blocks_per_group = 8192;
        sb.s_inodes_per_group = 1024;
        sb.s_inodes_count = 1024;
        sb.s_blocks_count_lo = 8192;
        sb.s_free_blocks_count_lo = 8000;
        sb.s_free_inodes_count = 1000;
        sb.s_first_data_block = 1;
        sb.s_inode_size = 256;
        sb.s_rev_level = 1;
        sb.s_feature_incompat = crate::superblock::incompat::EXTENTS;
        sb
    }

    /// Test superblock validation
    #[test]
    fn test_superblock_validation() {
        let sb = create_test_superblock();
        assert_eq!(sb.s_magic, EXT4_MAGIC);
        assert_eq!(sb.block_size(), 1024);
        assert!(sb.validate().is_ok());
    }

    /// Test inode creation
    #[test]
    fn test_inode_creation() {
        let inode = crate::inode::new_inode(file_type::S_IFREG | 0o644, 1000, 1000);
        assert!(inode.is_file());
        assert!(!inode.is_dir());
        assert!(inode.uses_extents());
        assert_eq!(inode.size(), 0);
        assert_eq!(inode.permissions(), 0o644);
    }

    /// Test directory inode creation
    #[test]
    fn test_dir_inode_creation() {
        let inode = crate::inode::new_inode(file_type::S_IFDIR | 0o755, 0, 0);
        assert!(inode.is_dir());
        assert!(!inode.is_file());
        assert_eq!(inode.permissions(), 0o755);
    }

    /// Test inode size operations
    #[test]
    fn test_inode_size() {
        let mut inode = crate::inode::new_inode(file_type::S_IFREG | 0o644, 0, 0);

        // Test small size
        inode.set_size(1024);
        assert_eq!(inode.size(), 1024);

        // Test large size (> 4GB)
        let large_size: u64 = 5 * 1024 * 1024 * 1024; // 5GB
        inode.set_size(large_size);
        assert_eq!(inode.size(), large_size);
    }

    /// Test extent creation
    #[test]
    fn test_extent_creation() {
        let extent = Extent::new(0, 1000, 10);
        assert_eq!(extent.ee_block, 0);
        assert_eq!(extent.start(), 1000);
        assert_eq!(extent.len(), 10);
        assert!(!extent.is_uninitialized());
    }

    /// Test extent mapping
    #[test]
    fn test_extent_mapping() {
        let extent = Extent::new(100, 5000, 50);

        // Block within extent
        assert!(extent.contains(100));
        assert!(extent.contains(149));
        assert_eq!(extent.map(100), Some(5000));
        assert_eq!(extent.map(110), Some(5010));
        assert_eq!(extent.map(149), Some(5049));

        // Block outside extent
        assert!(!extent.contains(99));
        assert!(!extent.contains(150));
        assert_eq!(extent.map(99), None);
        assert_eq!(extent.map(150), None);
    }

    /// Test extent header initialization
    #[test]
    fn test_extent_header_init() {
        let mut inode = crate::inode::new_inode(file_type::S_IFREG | 0o644, 0, 0);
        crate::inode::init_extent_header(&mut inode);

        let header = crate::extent::parse_header(&inode.i_block).unwrap();
        assert_eq!(header.eh_magic, EXT4_EXT_MAGIC);
        assert_eq!(header.eh_entries, 0);
        assert_eq!(header.eh_max, 4);
        assert_eq!(header.eh_depth, 0);
    }

    /// Test extent insertion
    #[test]
    fn test_extent_insertion() {
        let mut inode = crate::inode::new_inode(file_type::S_IFREG | 0o644, 0, 0);
        crate::inode::init_extent_header(&mut inode);

        // Insert first extent
        let result = crate::extent::insert_extent(&mut inode, 0, 1000, 10);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 10);

        // Verify the extent was inserted
        let (header, extents) = crate::extent::get_extents_from_inode(&inode.i_block).unwrap();
        assert_eq!(header.eh_entries, 1);
        assert_eq!(extents[0].ee_block, 0);
        assert_eq!(extents[0].start(), 1000);
        assert_eq!(extents[0].len(), 10);
    }

    /// Test multiple extent insertions
    #[test]
    fn test_multiple_extent_insertions() {
        let mut inode = crate::inode::new_inode(file_type::S_IFREG | 0o644, 0, 0);
        crate::inode::init_extent_header(&mut inode);

        // Insert three extents
        assert!(crate::extent::insert_extent(&mut inode, 0, 1000, 10).is_ok());
        assert!(crate::extent::insert_extent(&mut inode, 20, 2000, 5).is_ok());
        assert!(crate::extent::insert_extent(&mut inode, 10, 1500, 5).is_ok()); // Insert in middle

        let (header, extents) = crate::extent::get_extents_from_inode(&inode.i_block).unwrap();
        assert_eq!(header.eh_entries, 3);

        // Verify sorted order
        assert_eq!(extents[0].ee_block, 0);
        assert_eq!(extents[1].ee_block, 10);
        assert_eq!(extents[2].ee_block, 20);
    }

    /// Test extent extension
    #[test]
    fn test_extent_extension() {
        let mut inode = crate::inode::new_inode(file_type::S_IFREG | 0o644, 0, 0);
        crate::inode::init_extent_header(&mut inode);

        // Insert initial extent
        crate::extent::insert_extent(&mut inode, 0, 1000, 10).unwrap();

        // Try to extend with contiguous block
        let extended = crate::extent::try_extend_extent(&mut inode, 10, 1010).unwrap();
        assert!(extended);

        // Verify extent was extended
        let (_, extents) = crate::extent::get_extents_from_inode(&inode.i_block).unwrap();
        assert_eq!(extents[0].len(), 11);
    }

    /// Test inode link count
    #[test]
    fn test_inode_links() {
        let mut inode = crate::inode::new_inode(file_type::S_IFREG | 0o644, 0, 0);
        assert_eq!(inode.i_links_count, 1);

        inode.inc_links();
        assert_eq!(inode.i_links_count, 2);

        inode.dec_links();
        assert_eq!(inode.i_links_count, 1);

        inode.dec_links();
        assert_eq!(inode.i_links_count, 0);
    }

    /// Test inode block count
    #[test]
    fn test_inode_blocks() {
        let mut inode = crate::inode::new_inode(file_type::S_IFREG | 0o644, 0, 0);
        assert_eq!(inode.blocks(), 0);

        // Set blocks (in 512-byte units)
        inode.set_blocks(16); // 8KB
        assert_eq!(inode.blocks(), 16);

        // Test large block count
        let large_blocks: u64 = 1024 * 1024 * 1024; // Many blocks
        inode.set_blocks(large_blocks);
        assert_eq!(inode.blocks(), large_blocks);
    }

    /// Test directory entry size calculation
    #[test]
    fn test_dir_entry_size() {
        // Minimum entry: 8 bytes header + name, aligned to 4 bytes
        // Name "." = 1 byte -> 8 + 1 = 9 -> aligned to 12
        // Name ".." = 2 bytes -> 8 + 2 = 10 -> aligned to 12
        // Name "test" = 4 bytes -> 8 + 4 = 12 -> aligned to 12
        // Name "hello" = 5 bytes -> 8 + 5 = 13 -> aligned to 16

        fn entry_size(name_len: usize) -> usize {
            let size = 8 + name_len;
            (size + 3) & !3
        }

        assert_eq!(entry_size(1), 12); // "."
        assert_eq!(entry_size(2), 12); // ".."
        assert_eq!(entry_size(4), 12); // "test"
        assert_eq!(entry_size(5), 16); // "hello"
        assert_eq!(entry_size(255), 264); // max name
    }

    /// Test file type conversions
    #[test]
    fn test_file_type_conversions() {
        assert_eq!(
            crate::dir::mode_to_file_type(file_type::S_IFREG),
            crate::dir::file_type::REG_FILE
        );
        assert_eq!(
            crate::dir::mode_to_file_type(file_type::S_IFDIR),
            crate::dir::file_type::DIR
        );
        assert_eq!(
            crate::dir::mode_to_file_type(file_type::S_IFLNK),
            crate::dir::file_type::SYMLINK
        );
        assert_eq!(
            crate::dir::mode_to_file_type(file_type::S_IFCHR),
            crate::dir::file_type::CHRDEV
        );
        assert_eq!(
            crate::dir::mode_to_file_type(file_type::S_IFBLK),
            crate::dir::file_type::BLKDEV
        );
        assert_eq!(
            crate::dir::mode_to_file_type(file_type::S_IFIFO),
            crate::dir::file_type::FIFO
        );
        assert_eq!(
            crate::dir::mode_to_file_type(file_type::S_IFSOCK),
            crate::dir::file_type::SOCK
        );
    }

    /// Test symlink types
    #[test]
    fn test_symlink_inode() {
        let inode = crate::inode::new_inode(file_type::S_IFLNK | 0o777, 0, 0);
        assert!(inode.is_symlink());
        assert!(!inode.is_file());
        assert!(!inode.is_dir());
    }

    /// Test device inodes
    #[test]
    fn test_device_inodes() {
        let chr = crate::inode::new_inode(file_type::S_IFCHR | 0o666, 0, 0);
        assert!(chr.is_char_device());

        let blk = crate::inode::new_inode(file_type::S_IFBLK | 0o660, 0, 0);
        assert!(blk.is_block_device());

        let fifo = crate::inode::new_inode(file_type::S_IFIFO | 0o644, 0, 0);
        assert!(fifo.is_fifo());

        let sock = crate::inode::new_inode(file_type::S_IFSOCK | 0o755, 0, 0);
        assert!(sock.is_socket());
    }
}

/// Run all ext4 tests (for kernel test framework)
pub fn run_tests() {
    #[cfg(test)]
    {
        // Tests would be run by cargo test
    }

    // For kernel testing, we can add manual test functions here
    test_extent_basic();
    test_inode_basic();
}

/// Basic extent test for kernel environment
fn test_extent_basic() {
    use crate::extent::Extent;
    use crate::inode::{file_type, init_extent_header, new_inode};

    let mut inode = new_inode(file_type::S_IFREG | 0o644, 0, 0);
    init_extent_header(&mut inode);

    // Insert and verify extent
    let result = crate::extent::insert_extent(&mut inode, 0, 1000, 10);
    assert!(result.is_ok(), "Failed to insert extent");

    let (header, extents) =
        crate::extent::get_extents_from_inode(&inode.i_block).expect("Failed to get extents");
    assert_eq!(header.eh_entries, 1, "Wrong extent count");
    assert_eq!(extents[0].start(), 1000, "Wrong physical block");
}

/// Basic inode test for kernel environment
fn test_inode_basic() {
    use crate::inode::{file_type, new_inode};

    let inode = new_inode(file_type::S_IFREG | 0o644, 1000, 1000);
    assert!(inode.is_file(), "Should be a file");
    assert!(inode.uses_extents(), "Should use extents");

    let dir_inode = new_inode(file_type::S_IFDIR | 0o755, 0, 0);
    assert!(dir_inode.is_dir(), "Should be a directory");
}
