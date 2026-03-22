//! ELF loader for user programs
//!
//! Parses and loads ELF64 executables and shared objects into user address space.
//! Supports ET_EXEC (static executables), ET_DYN (PIE executables and .so files),
//! PT_INTERP (dynamic linker path), PT_DYNAMIC, and PT_PHDR segments.
//!
//! — BlackLatch: the parser that decides whether your binary lives or dies.
//! One misaligned p_offset and it's segfault city.

#![no_std]

use os_core::VirtAddr;
use proc_traits::MemoryFlags;

/// ELF magic number
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class: 64-bit
const ELFCLASS64: u8 = 2;

/// ELF data: little endian
const ELFDATA2LSB: u8 = 1;

/// ELF type: executable
const ET_EXEC: u16 = 2;

/// ELF type: shared object / PIE executable
/// — BlackLatch: ET_DYN covers both .so files and position-independent executables.
/// GCC -pie produces ET_DYN with an entry point. The kernel must accept both types.
const ET_DYN: u16 = 3;

/// Program header type: loadable segment
const PT_LOAD: u32 = 1;

/// Program header type: dynamic linking information
/// — WireSaint: contains the .dynamic section with DT_NEEDED, DT_SYMTAB, etc.
const PT_DYNAMIC: u32 = 2;

/// Program header type: interpreter path
/// — BlackLatch: contains the null-terminated path to the dynamic linker
/// (e.g., "/lib/ld-oxide.so.1"). If present, kernel loads the interpreter
/// instead of jumping straight to the executable's entry point.
const PT_INTERP: u32 = 3;

/// Program header type: program header table location in memory
/// — WireSaint: tells the dynamic linker where to find the phdr table in the
/// loaded image. AT_PHDR in the aux vector points here.
const PT_PHDR: u32 = 6;

/// Program header type: Thread-Local Storage template
const PT_TLS: u32 = 7;

/// Program header flags
const PF_X: u32 = 1; // Execute
const PF_W: u32 = 2; // Write
const PF_R: u32 = 4; // Read

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

/// Thread-Local Storage (TLS) template information
#[derive(Debug, Clone, Copy)]
pub struct TlsTemplate {
    /// Offset in file where TLS initialization image is located
    pub file_offset: usize,
    /// Size of the TLS initialization image in file
    pub file_size: usize,
    /// Total size of TLS block in memory (includes BSS)
    pub mem_size: usize,
    /// Required alignment for TLS block
    pub align: usize,
}

/// PT_INTERP segment information — path to the dynamic linker
/// — BlackLatch: the 256-byte buffer is generous. Linux paths max at 4096 but
/// interpreter paths are always short ("/lib/ld-linux-x86-64.so.2" is 29 bytes).
/// If you need more than 256 chars for your interpreter path, rethink your life.
#[derive(Debug, Clone)]
pub struct InterpInfo {
    /// Interpreter path bytes (null-terminated, up to 256 bytes)
    pub path: [u8; 256],
    /// Length of the path (excluding null terminator)
    pub len: usize,
}

impl InterpInfo {
    /// Get the interpreter path as a byte slice
    pub fn as_bytes(&self) -> &[u8] {
        &self.path[..self.len]
    }
}

/// PT_DYNAMIC segment info — offset and size of the .dynamic section
/// — WireSaint: the dynamic linker needs to find this to walk DT_NEEDED entries.
#[derive(Debug, Clone, Copy)]
pub struct DynamicInfo {
    /// Virtual address of the .dynamic section
    pub vaddr: u64,
    /// File offset of the .dynamic section
    pub file_offset: usize,
    /// Size in file
    pub file_size: usize,
}

/// PT_PHDR segment info — program header table location in memory
/// — WireSaint: AT_PHDR in the aux vector points to the phdr table as loaded
/// in the process's address space, not the file offset.
#[derive(Debug, Clone, Copy)]
pub struct PhdrInfo {
    /// Virtual address where the phdr table is mapped
    pub vaddr: u64,
    /// Size of each program header entry
    pub entry_size: u16,
    /// Number of program headers
    pub count: u16,
}

/// Parsed ELF executable information
/// — BlackLatch: now handles ET_DYN (PIE/shared objects) alongside ET_EXEC.
/// Dynamic linking metadata (PT_INTERP, PT_DYNAMIC, PT_PHDR) extracted during parse.
#[derive(Debug)]
pub struct ElfExecutable<'a> {
    /// Raw ELF data
    data: &'a [u8],
    /// Entry point address
    pub entry: VirtAddr,
    /// ELF type (ET_EXEC=2 or ET_DYN=3)
    pub elf_type: u16,
    /// Program headers (PT_LOAD segments)
    segments: [Option<LoadSegment>; 16],
    /// Number of segments
    segment_count: usize,
    /// Thread-Local Storage template (if present)
    tls_template: Option<TlsTemplate>,
    /// PT_INTERP: dynamic linker path (if present)
    /// — BlackLatch: None means statically linked. Some means "go find ld-oxide.so"
    interp: Option<InterpInfo>,
    /// PT_DYNAMIC: .dynamic section info (if present)
    dynamic: Option<DynamicInfo>,
    /// PT_PHDR: program header table in memory (if present)
    phdr: Option<PhdrInfo>,
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
    /// Not an executable or shared object
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
    /// PT_INTERP path too long or malformed
    InterpPathTooLong,
}

impl<'a> ElfExecutable<'a> {
    /// Parse an ELF executable from raw bytes.
    ///
    /// `elf_machine` — expected e_machine value (from ExecConfig::ELF_MACHINE_EXEC)
    /// `user_addr_limit` — upper boundary for user addresses (from ExecConfig::USER_ADDR_LIMIT)
    ///
    /// — BlackLatch: caller passes arch constants so this parser works on any arch.
    pub fn parse_with_arch(
        data: &'a [u8],
        elf_machine: u16,
        user_addr_limit: u64,
    ) -> Result<Self, ElfError> {
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

        // — BlackLatch: Accept both ET_EXEC and ET_DYN. PIE executables are ET_DYN
        // with an entry point. Shared libraries are ET_DYN with DT_SONAME.
        // Static executables are ET_EXEC. All three are valid load targets.
        if header.e_type != ET_EXEC && header.e_type != ET_DYN {
            return Err(ElfError::NotExecutable);
        }

        // Check architecture using caller-provided machine type
        if header.e_machine != elf_machine {
            return Err(ElfError::WrongArch);
        }

        let entry = VirtAddr::new(header.e_entry);

        // — BlackLatch: For ET_EXEC, entry point must be in user space.
        // For ET_DYN (PIE), entry is relative to load base — validated later.
        if header.e_type == ET_EXEC && entry.as_u64() >= user_addr_limit {
            return Err(ElfError::InvalidSegmentAddress);
        }

        // Parse program headers
        let ph_offset = header.e_phoff as usize;
        let ph_size = header.e_phentsize as usize;
        let ph_count = header.e_phnum as usize;

        let mut segments: [Option<LoadSegment>; 16] = [None; 16];
        let mut segment_count = 0;
        let mut tls_template: Option<TlsTemplate> = None;
        let mut interp: Option<InterpInfo> = None;
        let mut dynamic: Option<DynamicInfo> = None;
        let mut phdr: Option<PhdrInfo> = None;

        for i in 0..ph_count {
            let ph_start = ph_offset + i * ph_size;
            if ph_start + ph_size > data.len() {
                return Err(ElfError::SegmentOutOfBounds);
            }

            let ph = unsafe { &*(data.as_ptr().add(ph_start) as *const Elf64ProgramHeader) };

            match ph.p_type {
                PT_LOAD if ph.p_memsz > 0 => {
                    if segment_count >= 16 {
                        return Err(ElfError::TooManySegments);
                    }

                    // — BlackLatch: For ET_EXEC, validate segment addresses.
                    // For ET_DYN, vaddrs are relative to load base (can be 0).
                    if header.e_type == ET_EXEC && ph.p_vaddr >= user_addr_limit {
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

                PT_TLS => {
                    // — GraveShift: TLS template. Will be used by exec to set up
                    // the initial TLS block with architecture-specific layout.
                    tls_template = Some(TlsTemplate {
                        file_offset: ph.p_offset as usize,
                        file_size: ph.p_filesz as usize,
                        mem_size: ph.p_memsz as usize,
                        align: ph.p_align as usize,
                    });
                }

                PT_INTERP => {
                    // — BlackLatch: Extract the interpreter path. This is a
                    // null-terminated string embedded in the ELF file.
                    let offset = ph.p_offset as usize;
                    let size = ph.p_filesz as usize;
                    if offset + size > data.len() {
                        return Err(ElfError::SegmentOutOfBounds);
                    }
                    // Strip trailing null if present
                    let path_data = &data[offset..offset + size];
                    let path_len = path_data.iter().position(|&b| b == 0).unwrap_or(size);
                    if path_len >= 256 {
                        return Err(ElfError::InterpPathTooLong);
                    }
                    let mut info = InterpInfo {
                        path: [0u8; 256],
                        len: path_len,
                    };
                    info.path[..path_len].copy_from_slice(&path_data[..path_len]);
                    interp = Some(info);
                }

                PT_DYNAMIC => {
                    // — WireSaint: Record location of .dynamic section for the
                    // dynamic linker to find DT_NEEDED, DT_STRTAB, etc.
                    dynamic = Some(DynamicInfo {
                        vaddr: ph.p_vaddr,
                        file_offset: ph.p_offset as usize,
                        file_size: ph.p_filesz as usize,
                    });
                }

                PT_PHDR => {
                    // — WireSaint: Record where the program header table lives
                    // in the loaded image. Kernel passes this as AT_PHDR.
                    phdr = Some(PhdrInfo {
                        vaddr: ph.p_vaddr,
                        entry_size: header.e_phentsize,
                        count: header.e_phnum,
                    });
                }

                _ => {
                    // — BlackLatch: ignore GNU_STACK, NOTE, GNU_RELRO, etc.
                    // We don't need them for loading.
                }
            }
        }

        if segment_count == 0 {
            return Err(ElfError::NoSegments);
        }

        Ok(Self {
            data,
            entry,
            elf_type: header.e_type,
            segments,
            segment_count,
            tls_template,
            interp,
            dynamic,
            phdr,
        })
    }

    /// Parse an ELF executable using hardcoded x86_64 constants.
    /// — BlackLatch: backward-compatible entry point for callers that haven't
    /// been updated to pass arch parameters yet.
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> {
        Self::parse_with_arch(data, 0x3E, 0x0000_8000_0000_0000)
    }

    /// Get the entry point address
    pub fn entry_point(&self) -> VirtAddr {
        self.entry
    }

    /// Get the ELF type (ET_EXEC or ET_DYN)
    pub fn elf_type(&self) -> u16 {
        self.elf_type
    }

    /// Check if this is a position-independent executable or shared object
    pub fn is_pie(&self) -> bool {
        self.elf_type == ET_DYN
    }

    /// Get PT_INTERP info if present (dynamic linker path)
    pub fn interp(&self) -> Option<&InterpInfo> {
        self.interp.as_ref()
    }

    /// Get PT_DYNAMIC info if present
    pub fn dynamic(&self) -> Option<&DynamicInfo> {
        self.dynamic.as_ref()
    }

    /// Get PT_PHDR info if present
    pub fn phdr(&self) -> Option<&PhdrInfo> {
        self.phdr.as_ref()
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

    /// Get the TLS template if present
    pub fn tls_template(&self) -> Option<&TlsTemplate> {
        self.tls_template.as_ref()
    }

    /// Get the TLS initialization data
    pub fn tls_data(&self) -> &[u8] {
        if let Some(tls) = &self.tls_template {
            if tls.file_size > 0 {
                return &self.data[tls.file_offset..][..tls.file_size];
            }
        }
        &[]
    }

    /// Get the raw ELF data
    pub fn raw_data(&self) -> &[u8] {
        self.data
    }

    /// Get the ELF header
    pub fn header(&self) -> &Elf64Header {
        unsafe { &*(self.data.as_ptr() as *const Elf64Header) }
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

    /// Get the minimum virtual address across all LOAD segments
    /// — BlackLatch: needed for PIE/ET_DYN to compute load base offset.
    /// For ET_EXEC this is the actual load address. For ET_DYN it's the
    /// relative base (often 0) that gets relocated to the actual load address.
    pub fn min_vaddr(&self) -> u64 {
        let mut min = u64::MAX;
        for segment in self.segments() {
            let aligned = segment.vaddr.as_u64() & !0xFFF;
            if aligned < min {
                min = aligned;
            }
        }
        if min == u64::MAX { 0 } else { min }
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

// ============================================================================
// Auxiliary Vector Types
// — WireSaint: the aux vector is how the kernel communicates ELF metadata to
// the dynamic linker. It sits on the stack between envp's NULL terminator
// and the random bytes. Each entry is a (type, value) pair.
// ============================================================================

/// Auxiliary vector entry types (from elf.h AT_* constants)
/// — WireSaint: Linux-compatible numbering. The dynamic linker expects these
/// exact values — don't renumber them or ld-oxide.so will read garbage.
#[repr(u64)]
#[derive(Debug, Clone, Copy)]
pub enum AuxType {
    /// End of aux vector
    AtNull = 0,
    /// Program headers location in memory
    AtPhdr = 3,
    /// Size of one program header entry
    AtPhent = 4,
    /// Number of program headers
    AtPhnum = 5,
    /// System page size
    AtPagesz = 6,
    /// Interpreter base address (where ld-oxide.so was loaded)
    AtBase = 7,
    /// Entry point of the main executable (not the interpreter)
    AtEntry = 9,
    /// Address of 16 random bytes (for stack canary / ASLR seed)
    AtRandom = 25,
    /// Filename of the executed program
    AtExecfn = 31,
}

/// Auxiliary vector entry — (type, value) pair written to user stack
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AuxEntry {
    pub a_type: u64,
    pub a_val: u64,
}

impl AuxEntry {
    pub const fn new(typ: AuxType, val: u64) -> Self {
        Self {
            a_type: typ as u64,
            a_val: val,
        }
    }

    pub const fn null() -> Self {
        Self {
            a_type: 0,
            a_val: 0,
        }
    }
}
