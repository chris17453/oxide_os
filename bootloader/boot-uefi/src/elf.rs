//! ELF file parser for kernel loading

use core::mem;
use core::ptr;

/// ELF magic number
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class: 64-bit
const ELFCLASS64: u8 = 2;

/// ELF data: little endian
const ELFDATA2LSB: u8 = 1;

/// Program header type: loadable segment
const PT_LOAD: u32 = 1;

/// ELF64 file header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

/// ELF64 program header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

/// Information extracted from ELF file
#[derive(Debug)]
pub struct ElfInfo {
    /// Entry point virtual address
    pub entry: u64,
    /// Lowest virtual address to load
    pub load_base: u64,
    /// Total size needed in memory
    pub load_size: u64,
    /// Program headers
    pub segments: alloc::vec::Vec<Segment>,
}

/// A loadable segment
#[derive(Debug, Clone)]
pub struct Segment {
    /// Offset in file
    pub file_offset: u64,
    /// Size in file
    pub file_size: u64,
    /// Virtual address
    pub vaddr: u64,
    /// Size in memory
    pub mem_size: u64,
}

/// Parse an ELF file and extract loading information
pub fn parse_elf(data: &[u8]) -> Result<ElfInfo, &'static str> {
    if data.len() < mem::size_of::<Elf64Header>() {
        return Err("File too small for ELF header");
    }

    // Parse header
    let header = unsafe { &*(data.as_ptr() as *const Elf64Header) };

    // Validate magic
    if header.e_ident[0..4] != ELF_MAGIC {
        return Err("Invalid ELF magic");
    }

    // Check 64-bit
    if header.e_ident[4] != ELFCLASS64 {
        return Err("Not a 64-bit ELF");
    }

    // Check little endian
    if header.e_ident[5] != ELFDATA2LSB {
        return Err("Not little-endian ELF");
    }

    // Check x86_64 machine type
    if header.e_machine != 0x3E {
        return Err("Not x86_64 ELF");
    }

    let entry = header.e_entry;
    let ph_offset = header.e_phoff as usize;
    let ph_size = header.e_phentsize as usize;
    let ph_count = header.e_phnum as usize;

    // Find all loadable segments
    let mut segments = alloc::vec::Vec::new();
    let mut load_base = u64::MAX;
    let mut load_end = 0u64;

    for i in 0..ph_count {
        let ph_start = ph_offset + i * ph_size;
        if ph_start + ph_size > data.len() {
            return Err("Program header out of bounds");
        }

        let ph = unsafe { &*(data.as_ptr().add(ph_start) as *const Elf64ProgramHeader) };

        if ph.p_type == PT_LOAD {
            segments.push(Segment {
                file_offset: ph.p_offset,
                file_size: ph.p_filesz,
                vaddr: ph.p_vaddr,
                mem_size: ph.p_memsz,
            });

            if ph.p_vaddr < load_base {
                load_base = ph.p_vaddr;
            }

            let seg_end = ph.p_vaddr + ph.p_memsz;
            if seg_end > load_end {
                load_end = seg_end;
            }
        }
    }

    if segments.is_empty() {
        return Err("No loadable segments");
    }

    let load_size = load_end - load_base;

    Ok(ElfInfo {
        entry,
        load_base,
        load_size,
        segments,
    })
}

/// Load ELF segments into memory
///
/// The kernel is loaded at `phys_base`, and the segments are copied
/// according to their virtual addresses relative to the ELF load base.
pub fn load_segments(data: &[u8], info: &ElfInfo, phys_base: u64) {
    for segment in &info.segments {
        // Calculate destination in physical memory
        let offset = segment.vaddr - info.load_base;
        let dest = (phys_base + offset) as *mut u8;

        // Copy file data
        if segment.file_size > 0 {
            let src = &data[segment.file_offset as usize..][..segment.file_size as usize];
            unsafe {
                ptr::copy_nonoverlapping(src.as_ptr(), dest, segment.file_size as usize);
            }
        }

        // Zero the rest (BSS)
        if segment.mem_size > segment.file_size {
            let bss_start = unsafe { dest.add(segment.file_size as usize) };
            let bss_size = segment.mem_size - segment.file_size;
            unsafe {
                ptr::write_bytes(bss_start, 0, bss_size as usize);
            }
        }
    }
}
