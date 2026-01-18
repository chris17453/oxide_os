//! ELF loader for user programs
//!
//! Parses and loads static ELF64 executables into user address space.

#![no_std]

use efflux_core::VirtAddr;
use efflux_proc_traits::MemoryFlags;

/// ELF magic number
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class: 64-bit
const ELFCLASS64: u8 = 2;

/// ELF data: little endian
const ELFDATA2LSB: u8 = 1;

/// ELF type: executable
const ET_EXEC: u16 = 2;

/// ELF machine: x86_64
const EM_X86_64: u16 = 0x3E;

/// Program header type: loadable segment
const PT_LOAD: u32 = 1;

/// Program header flags
const PF_X: u32 = 1;  // Execute
const PF_W: u32 = 2;  // Write
const PF_R: u32 = 4;  // Read

/// ELF64 file header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

/// ELF64 program header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

/// A loadable segment from an ELF file
#[derive(Debug, Clone, Copy)]
pub struct LoadSegment {
    /// Virtual address to load at
    pub vaddr: VirtAddr,
    /// Size in memory (may be larger than file_size for BSS)
    pub mem_size: usize,
    /// Offset in file
    pub file_offset: usize,
    /// Size in file
    pub file_size: usize,
    /// Memory protection flags
    pub flags: MemoryFlags,
}

/// Parsed ELF executable information
#[derive(Debug)]
pub struct ElfExecutable<'a> {
    /// Raw ELF data
    data: &'a [u8],
    /// Entry point address
    pub entry: VirtAddr,
    /// Program headers (PT_LOAD segments)
    segments: [Option<LoadSegment>; 16],
    /// Number of segments
    segment_count: usize,
}

/// ELF parsing error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    /// File too small
    TooSmall,
    /// Invalid magic number
    InvalidMagic,
    /// Not a 64-bit ELF
    Not64Bit,
    /// Not little-endian
    NotLittleEndian,
    /// Not an executable
    NotExecutable,
    /// Wrong architecture
    WrongArch,
    /// No loadable segments
    NoSegments,
    /// Too many segments
    TooManySegments,
    /// Segment out of bounds
    SegmentOutOfBounds,
    /// Invalid segment address (not in user space)
    InvalidSegmentAddress,
}

impl<'a> ElfExecutable<'a> {
    /// Parse an ELF executable from raw bytes
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> {
        // Check minimum size
        if data.len() < core::mem::size_of::<Elf64Header>() {
            return Err(ElfError::TooSmall);
        }

        // Parse header
        let header = unsafe { &*(data.as_ptr() as *const Elf64Header) };

        // Validate magic
        if header.e_ident[0..4] != ELF_MAGIC {
            return Err(ElfError::InvalidMagic);
        }

        // Check 64-bit
        if header.e_ident[4] != ELFCLASS64 {
            return Err(ElfError::Not64Bit);
        }

        // Check little endian
        if header.e_ident[5] != ELFDATA2LSB {
            return Err(ElfError::NotLittleEndian);
        }

        // Check executable type
        if header.e_type != ET_EXEC {
            return Err(ElfError::NotExecutable);
        }

        // Check architecture (x86_64)
        if header.e_machine != EM_X86_64 {
            return Err(ElfError::WrongArch);
        }

        let entry = VirtAddr::new(header.e_entry);

        // Validate entry point is in user space
        if entry.as_u64() >= 0x0000_8000_0000_0000 {
            return Err(ElfError::InvalidSegmentAddress);
        }

        // Parse program headers
        let ph_offset = header.e_phoff as usize;
        let ph_size = header.e_phentsize as usize;
        let ph_count = header.e_phnum as usize;

        let mut segments: [Option<LoadSegment>; 16] = [None; 16];
        let mut segment_count = 0;

        for i in 0..ph_count {
            let ph_start = ph_offset + i * ph_size;
            if ph_start + ph_size > data.len() {
                return Err(ElfError::SegmentOutOfBounds);
            }

            let ph = unsafe { &*(data.as_ptr().add(ph_start) as *const Elf64ProgramHeader) };

            if ph.p_type == PT_LOAD && ph.p_memsz > 0 {
                if segment_count >= 16 {
                    return Err(ElfError::TooManySegments);
                }

                // Validate segment is in user space
                if ph.p_vaddr >= 0x0000_8000_0000_0000 {
                    return Err(ElfError::InvalidSegmentAddress);
                }

                // Convert ELF flags to MemoryFlags
                let mut flags = MemoryFlags::USER;
                if ph.p_flags & PF_R != 0 {
                    flags = flags.union(MemoryFlags::READ);
                }
                if ph.p_flags & PF_W != 0 {
                    flags = flags.union(MemoryFlags::WRITE);
                }
                if ph.p_flags & PF_X != 0 {
                    flags = flags.union(MemoryFlags::EXECUTE);
                }

                segments[segment_count] = Some(LoadSegment {
                    vaddr: VirtAddr::new(ph.p_vaddr),
                    mem_size: ph.p_memsz as usize,
                    file_offset: ph.p_offset as usize,
                    file_size: ph.p_filesz as usize,
                    flags,
                });
                segment_count += 1;
            }
        }

        if segment_count == 0 {
            return Err(ElfError::NoSegments);
        }

        Ok(Self {
            data,
            entry,
            segments,
            segment_count,
        })
    }

    /// Get the entry point address
    pub fn entry_point(&self) -> VirtAddr {
        self.entry
    }

    /// Iterate over loadable segments
    pub fn segments(&self) -> impl Iterator<Item = &LoadSegment> {
        self.segments[..self.segment_count]
            .iter()
            .filter_map(|s| s.as_ref())
    }

    /// Get the data for a segment
    pub fn segment_data(&self, segment: &LoadSegment) -> &[u8] {
        if segment.file_size == 0 {
            return &[];
        }
        &self.data[segment.file_offset..][..segment.file_size]
    }

    /// Calculate total memory needed (aligned to page size)
    pub fn total_memory_size(&self) -> usize {
        let mut max_end = 0u64;
        let mut min_start = u64::MAX;

        for segment in self.segments() {
            let start = segment.vaddr.as_u64();
            let end = start + segment.mem_size as u64;

            if start < min_start {
                min_start = start;
            }
            if end > max_end {
                max_end = end;
            }
        }

        if min_start == u64::MAX {
            return 0;
        }

        // Align to page size
        let size = max_end - min_start;
        ((size + 4095) & !4095) as usize
    }
}

/// Load an ELF executable into an address space
///
/// This is a helper that coordinates ELF loading. The actual mapping
/// is done by the caller since it requires an allocator.
pub struct ElfLoader;

impl ElfLoader {
    /// Calculate the page-aligned base and size for a segment
    pub fn segment_pages(segment: &LoadSegment) -> (VirtAddr, usize) {
        let page_mask = !0xFFFu64;
        let vaddr_aligned = segment.vaddr.as_u64() & page_mask;
        let end = segment.vaddr.as_u64() + segment.mem_size as u64;
        let end_aligned = (end + 0xFFF) & page_mask;
        let size = (end_aligned - vaddr_aligned) as usize;

        (VirtAddr::new(vaddr_aligned), size)
    }

    /// Get the offset within the first page for segment data
    pub fn segment_page_offset(segment: &LoadSegment) -> usize {
        (segment.vaddr.as_u64() & 0xFFF) as usize
    }
}
