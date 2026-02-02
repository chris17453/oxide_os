//! ELF file parsing

#![allow(unused_imports)]

use alloc::vec::Vec;

/// ELF magic number
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class (32/64 bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ElfClass {
    Elf32 = 1,
    Elf64 = 2,
}

/// ELF data encoding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ElfData {
    LittleEndian = 1,
    BigEndian = 2,
}

/// ELF type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ElfType {
    None = 0,
    Rel = 1,  // Relocatable
    Exec = 2, // Executable
    Dyn = 3,  // Shared object
    Core = 4, // Core file
}

/// Program header type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ProgramType {
    Null = 0,
    Load = 1,
    Dynamic = 2,
    Interp = 3,
    Note = 4,
    Shlib = 5,
    Phdr = 6,
    Tls = 7,
    GnuEhFrame = 0x6474e550,
    GnuStack = 0x6474e551,
    GnuRelro = 0x6474e552,
}

/// Section header type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SectionType {
    Null = 0,
    Progbits = 1,
    Symtab = 2,
    Strtab = 3,
    Rela = 4,
    Hash = 5,
    Dynamic = 6,
    Note = 7,
    Nobits = 8,
    Rel = 9,
    Shlib = 10,
    Dynsym = 11,
    InitArray = 14,
    FiniArray = 15,
    PreinitArray = 16,
}

/// ELF header (64-bit)
#[derive(Debug, Clone)]
pub struct ElfHeader {
    /// Magic number
    pub magic: [u8; 4],
    /// File class
    pub class: ElfClass,
    /// Data encoding
    pub data: ElfData,
    /// ELF version
    pub version: u8,
    /// OS/ABI identification
    pub osabi: u8,
    /// ABI version
    pub abiversion: u8,
    /// Object file type
    pub elf_type: ElfType,
    /// Machine architecture
    pub machine: u16,
    /// Entry point address
    pub entry: u64,
    /// Program header table offset
    pub phoff: u64,
    /// Section header table offset
    pub shoff: u64,
    /// Processor-specific flags
    pub flags: u32,
    /// ELF header size
    pub ehsize: u16,
    /// Program header entry size
    pub phentsize: u16,
    /// Number of program headers
    pub phnum: u16,
    /// Section header entry size
    pub shentsize: u16,
    /// Number of section headers
    pub shnum: u16,
    /// Section name string table index
    pub shstrndx: u16,
}

impl ElfHeader {
    /// Parse ELF header from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 64 {
            return None;
        }

        // Check magic
        if data[0..4] != ELF_MAGIC {
            return None;
        }

        let class = match data[4] {
            1 => ElfClass::Elf32,
            2 => ElfClass::Elf64,
            _ => return None,
        };

        let encoding = match data[5] {
            1 => ElfData::LittleEndian,
            2 => ElfData::BigEndian,
            _ => return None,
        };

        // For now, only support 64-bit little-endian
        if class != ElfClass::Elf64 || encoding != ElfData::LittleEndian {
            return None;
        }

        let elf_type = match u16::from_le_bytes([data[16], data[17]]) {
            0 => ElfType::None,
            1 => ElfType::Rel,
            2 => ElfType::Exec,
            3 => ElfType::Dyn,
            4 => ElfType::Core,
            _ => return None,
        };

        Some(ElfHeader {
            magic: [data[0], data[1], data[2], data[3]],
            class,
            data: encoding,
            version: data[6],
            osabi: data[7],
            abiversion: data[8],
            elf_type,
            machine: u16::from_le_bytes([data[18], data[19]]),
            entry: u64::from_le_bytes([
                data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
            ]),
            phoff: u64::from_le_bytes([
                data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39],
            ]),
            shoff: u64::from_le_bytes([
                data[40], data[41], data[42], data[43], data[44], data[45], data[46], data[47],
            ]),
            flags: u32::from_le_bytes([data[48], data[49], data[50], data[51]]),
            ehsize: u16::from_le_bytes([data[52], data[53]]),
            phentsize: u16::from_le_bytes([data[54], data[55]]),
            phnum: u16::from_le_bytes([data[56], data[57]]),
            shentsize: u16::from_le_bytes([data[58], data[59]]),
            shnum: u16::from_le_bytes([data[60], data[61]]),
            shstrndx: u16::from_le_bytes([data[62], data[63]]),
        })
    }

    /// Check if this is a shared object
    pub fn is_shared_object(&self) -> bool {
        self.elf_type == ElfType::Dyn
    }
}

/// Program header (64-bit)
#[derive(Debug, Clone)]
pub struct ProgramHeader {
    /// Segment type
    pub p_type: u32,
    /// Segment flags
    pub p_flags: u32,
    /// Offset in file
    pub p_offset: u64,
    /// Virtual address
    pub p_vaddr: u64,
    /// Physical address
    pub p_paddr: u64,
    /// Size in file
    pub p_filesz: u64,
    /// Size in memory
    pub p_memsz: u64,
    /// Alignment
    pub p_align: u64,
}

impl ProgramHeader {
    /// Parse program header from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 56 {
            return None;
        }

        Some(ProgramHeader {
            p_type: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            p_flags: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            p_offset: u64::from_le_bytes([
                data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
            ]),
            p_vaddr: u64::from_le_bytes([
                data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
            ]),
            p_paddr: u64::from_le_bytes([
                data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
            ]),
            p_filesz: u64::from_le_bytes([
                data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39],
            ]),
            p_memsz: u64::from_le_bytes([
                data[40], data[41], data[42], data[43], data[44], data[45], data[46], data[47],
            ]),
            p_align: u64::from_le_bytes([
                data[48], data[49], data[50], data[51], data[52], data[53], data[54], data[55],
            ]),
        })
    }

    /// Check if this is a loadable segment
    pub fn is_loadable(&self) -> bool {
        self.p_type == ProgramType::Load as u32
    }

    /// Check if readable
    pub fn is_readable(&self) -> bool {
        self.p_flags & 0x4 != 0
    }

    /// Check if writable
    pub fn is_writable(&self) -> bool {
        self.p_flags & 0x2 != 0
    }

    /// Check if executable
    pub fn is_executable(&self) -> bool {
        self.p_flags & 0x1 != 0
    }
}

/// Section header (64-bit)
#[derive(Debug, Clone)]
pub struct SectionHeader {
    /// Section name (index into string table)
    pub sh_name: u32,
    /// Section type
    pub sh_type: u32,
    /// Section flags
    pub sh_flags: u64,
    /// Virtual address
    pub sh_addr: u64,
    /// Offset in file
    pub sh_offset: u64,
    /// Size
    pub sh_size: u64,
    /// Link to another section
    pub sh_link: u32,
    /// Additional info
    pub sh_info: u32,
    /// Alignment
    pub sh_addralign: u64,
    /// Entry size (for tables)
    pub sh_entsize: u64,
}

impl SectionHeader {
    /// Parse section header from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 64 {
            return None;
        }

        Some(SectionHeader {
            sh_name: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            sh_type: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            sh_flags: u64::from_le_bytes([
                data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
            ]),
            sh_addr: u64::from_le_bytes([
                data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
            ]),
            sh_offset: u64::from_le_bytes([
                data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
            ]),
            sh_size: u64::from_le_bytes([
                data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39],
            ]),
            sh_link: u32::from_le_bytes([data[40], data[41], data[42], data[43]]),
            sh_info: u32::from_le_bytes([data[44], data[45], data[46], data[47]]),
            sh_addralign: u64::from_le_bytes([
                data[48], data[49], data[50], data[51], data[52], data[53], data[54], data[55],
            ]),
            sh_entsize: u64::from_le_bytes([
                data[56], data[57], data[58], data[59], data[60], data[61], data[62], data[63],
            ]),
        })
    }
}

/// ELF symbol table entry (64-bit)
#[derive(Debug, Clone)]
pub struct Symbol {
    /// Name (index into string table)
    pub st_name: u32,
    /// Symbol info (type and binding)
    pub st_info: u8,
    /// Symbol visibility
    pub st_other: u8,
    /// Section index
    pub st_shndx: u16,
    /// Symbol value
    pub st_value: u64,
    /// Symbol size
    pub st_size: u64,
}

impl Symbol {
    /// Parse symbol from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 24 {
            return None;
        }

        Some(Symbol {
            st_name: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            st_info: data[4],
            st_other: data[5],
            st_shndx: u16::from_le_bytes([data[6], data[7]]),
            st_value: u64::from_le_bytes([
                data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
            ]),
            st_size: u64::from_le_bytes([
                data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
            ]),
        })
    }

    /// Get symbol type
    pub fn sym_type(&self) -> u8 {
        self.st_info & 0xf
    }

    /// Get symbol binding
    pub fn binding(&self) -> u8 {
        self.st_info >> 4
    }

    /// Is this a function?
    pub fn is_function(&self) -> bool {
        self.sym_type() == 2 // STT_FUNC
    }

    /// Is this an object (variable)?
    pub fn is_object(&self) -> bool {
        self.sym_type() == 1 // STT_OBJECT
    }

    /// Is this globally visible?
    pub fn is_global(&self) -> bool {
        self.binding() == 1 // STB_GLOBAL
    }

    /// Is this weak?
    pub fn is_weak(&self) -> bool {
        self.binding() == 2 // STB_WEAK
    }
}

/// Parsed ELF file
pub struct ElfFile {
    /// Header
    pub header: ElfHeader,
    /// Program headers
    pub program_headers: Vec<ProgramHeader>,
    /// Section headers
    pub section_headers: Vec<SectionHeader>,
}

impl ElfFile {
    /// Parse ELF file from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        let header = ElfHeader::parse(data)?;

        // Parse program headers
        let mut program_headers = Vec::new();
        let ph_start = header.phoff as usize;
        for i in 0..header.phnum as usize {
            let offset = ph_start + i * header.phentsize as usize;
            let ph = ProgramHeader::parse(&data[offset..])?;
            program_headers.push(ph);
        }

        // Parse section headers
        let mut section_headers = Vec::new();
        let sh_start = header.shoff as usize;
        for i in 0..header.shnum as usize {
            let offset = sh_start + i * header.shentsize as usize;
            let sh = SectionHeader::parse(&data[offset..])?;
            section_headers.push(sh);
        }

        Some(ElfFile {
            header,
            program_headers,
            section_headers,
        })
    }

    /// Calculate total size needed to load this ELF
    pub fn load_size(&self) -> usize {
        let mut min_addr = usize::MAX;
        let mut max_addr = 0usize;

        for ph in &self.program_headers {
            if ph.is_loadable() {
                let start = ph.p_vaddr as usize;
                let end = start + ph.p_memsz as usize;
                min_addr = min_addr.min(start);
                max_addr = max_addr.max(end);
            }
        }

        if min_addr == usize::MAX {
            0
        } else {
            max_addr - min_addr
        }
    }
}
