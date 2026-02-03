//! WAD file loader for Doom
//!
//! Handles loading and parsing of Doom WAD files.
//! -- WireSaint: Storage systems + filesystems integration

use libc::{open, read, close, lseek, O_RDONLY, SEEK_SET};

/// WAD file header
#[repr(C, packed)]
struct WadHeader {
    magic: [u8; 4],     // "IWAD" or "PWAD"
    num_lumps: u32,
    dir_offset: u32,
}

/// WAD directory entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct WadLump {
    pub offset: u32,
    pub size: u32,
    pub name: [u8; 8],
}

/// WAD file data structure
pub struct WadFile {
    fd: i32,
    lumps: [WadLump; 4096],  // Fixed size array for no_std
    num_lumps: usize,
}

impl WadFile {
    /// Load a WAD file from the filesystem
    /// -- WireSaint: File I/O through VFS, clean and direct
    pub fn load(path: &str) -> Option<Self> {
        let fd = open(path, O_RDONLY, 0);
        if fd < 0 {
            return None;
        }

        // Read WAD header
        let mut header: WadHeader = unsafe { core::mem::zeroed() };
        let header_slice = unsafe {
            core::slice::from_raw_parts_mut(
                &mut header as *mut _ as *mut u8,
                core::mem::size_of::<WadHeader>(),
            )
        };
        if read(fd, header_slice) < 0 {
            close(fd);
            return None;
        }

        // Verify magic
        if &header.magic != b"IWAD" && &header.magic != b"PWAD" {
            close(fd);
            return None;
        }

        // Read directory
        let num_lumps = header.num_lumps.min(4096) as usize;
        let mut lumps = [WadLump {
            offset: 0,
            size: 0,
            name: [0; 8],
        }; 4096];

        lseek(fd, header.dir_offset as i64, SEEK_SET);
        for i in 0..num_lumps {
            let lump_slice = unsafe {
                core::slice::from_raw_parts_mut(
                    &mut lumps[i] as *mut _ as *mut u8,
                    core::mem::size_of::<WadLump>(),
                )
            };
            if read(fd, lump_slice) < 0 {
                close(fd);
                return None;
            }
        }

        Some(WadFile {
            fd,
            lumps,
            num_lumps,
        })
    }

    /// Create a minimal built-in WAD placeholder when external assets are missing
    /// -- WireSaint: Emergency cache - keep the run alive even without disk
    pub fn built_in() -> Self {
        WadFile {
            fd: -1,
            lumps: [WadLump {
                offset: 0,
                size: 0,
                name: [0; 8],
            }; 4096],
            num_lumps: 0,
        }
    }

    /// Find a lump by name
    pub fn find_lump(&self, name: &[u8; 8]) -> Option<&WadLump> {
        for i in 0..self.num_lumps {
            if &self.lumps[i].name == name {
                return Some(&self.lumps[i]);
            }
        }
        None
    }

    /// Read lump data into a buffer
    /// -- WireSaint: Efficient block reads, no nonsense
    pub fn read_lump(&self, lump: &WadLump, buffer: &mut [u8]) -> bool {
        if lump.size as usize > buffer.len() {
            return false;
        }

        lseek(self.fd, lump.offset as i64, SEEK_SET);
        read(self.fd, &mut buffer[..lump.size as usize]) == lump.size as isize
    }

    /// Get number of lumps
    pub fn num_lumps(&self) -> usize {
        self.num_lumps
    }
}

impl Drop for WadFile {
    fn drop(&mut self) {
        if self.fd >= 0 {
            close(self.fd);
        }
    }
}
