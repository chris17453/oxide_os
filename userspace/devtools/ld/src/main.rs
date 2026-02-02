//! OXIDE Linker (ld)
//!
//! A minimal ELF64 linker for OXIDE OS.
//! Links relocatable object files into executables.

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

/// Maximum input files
const MAX_FILES: usize = 64;

/// Maximum sections per file
const MAX_SECTIONS: usize = 32;

/// Maximum symbols
const MAX_SYMBOLS: usize = 1024;

/// Maximum relocations
const MAX_RELOCS: usize = 2048;

/// Maximum output size (1MB)
const MAX_OUTPUT: usize = 1024 * 1024;

/// Input file buffer (64KB per file)
const FILE_BUF_SIZE: usize = 65536;

/// Default base address for executables
const DEFAULT_BASE: u64 = 0x400000;

/// Page size
const PAGE_SIZE: u64 = 0x1000;

// ELF constants
const ELFMAG: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ET_REL: u16 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

const SHT_PROGBITS: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;
const SHT_NOBITS: u32 = 8;

const SHF_WRITE: u64 = 1;
const SHF_ALLOC: u64 = 2;
const SHF_EXECINSTR: u64 = 4;

const STB_GLOBAL: u8 = 1;
const SHN_UNDEF: u16 = 0;

const R_X86_64_64: u32 = 1;
const R_X86_64_PC32: u32 = 2;
const R_X86_64_PLT32: u32 = 4;
const R_X86_64_32: u32 = 10;
const R_X86_64_32S: u32 = 11;

/// ELF64 header
#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Ehdr {
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
#[derive(Clone, Copy)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

/// ELF64 section header
#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Shdr {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
}

/// ELF64 symbol
#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Sym {
    st_name: u32,
    st_info: u8,
    st_other: u8,
    st_shndx: u16,
    st_value: u64,
    st_size: u64,
}

/// ELF64 relocation with addend
#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Rela {
    r_offset: u64,
    r_info: u64,
    r_addend: i64,
}

/// Linker symbol
#[derive(Clone, Copy)]
struct LinkerSymbol {
    name: [u8; 64],
    value: u64,
    size: u64,
    section: u8, // 0=undef, 1=text, 2=data, 3=bss
    binding: u8,
    defined: bool,
    file_idx: usize,
}

impl LinkerSymbol {
    const fn new() -> Self {
        LinkerSymbol {
            name: [0u8; 64],
            value: 0,
            size: 0,
            section: 0,
            binding: 0,
            defined: false,
            file_idx: 0,
        }
    }
}

/// Linker relocation
#[derive(Clone, Copy)]
struct LinkerReloc {
    offset: u64,    // Offset in output section
    sym_idx: usize, // Symbol index
    rtype: u32,
    addend: i64,
    section: u8, // 1=text, 2=data
}

impl LinkerReloc {
    const fn new() -> Self {
        LinkerReloc {
            offset: 0,
            sym_idx: 0,
            rtype: 0,
            addend: 0,
            section: 0,
        }
    }
}

/// Input file info
struct InputFile {
    data: [u8; FILE_BUF_SIZE],
    size: usize,
    text_offset: u64, // Offset into output text section
    data_offset: u64, // Offset into output data section
    bss_offset: u64,  // Offset into output bss
    first_sym: usize, // First symbol index in global table
}

impl InputFile {
    const fn new() -> Self {
        InputFile {
            data: [0u8; FILE_BUF_SIZE],
            size: 0,
            text_offset: 0,
            data_offset: 0,
            bss_offset: 0,
            first_sym: 0,
        }
    }
}

/// Linker state
struct Linker {
    /// Input files
    files: [InputFile; MAX_FILES],
    num_files: usize,

    /// Output sections
    text: [u8; MAX_OUTPUT],
    text_len: usize,
    data: [u8; MAX_OUTPUT],
    data_len: usize,
    bss_len: usize,

    /// Symbol table
    symbols: [LinkerSymbol; MAX_SYMBOLS],
    num_symbols: usize,

    /// Relocations
    relocs: [LinkerReloc; MAX_RELOCS],
    num_relocs: usize,

    /// Base address
    base_addr: u64,

    /// Entry point symbol
    entry_name: [u8; 64],

    /// Error flag
    had_error: bool,
}

impl Linker {
    fn new() -> Self {
        const EMPTY_FILE: InputFile = InputFile::new();
        const EMPTY_SYM: LinkerSymbol = LinkerSymbol::new();
        const EMPTY_RELOC: LinkerReloc = LinkerReloc::new();

        Linker {
            files: [EMPTY_FILE; MAX_FILES],
            num_files: 0,
            text: [0u8; MAX_OUTPUT],
            text_len: 0,
            data: [0u8; MAX_OUTPUT],
            data_len: 0,
            bss_len: 0,
            symbols: [EMPTY_SYM; MAX_SYMBOLS],
            num_symbols: 0,
            relocs: [EMPTY_RELOC; MAX_RELOCS],
            num_relocs: 0,
            base_addr: DEFAULT_BASE,
            entry_name: [0u8; 64],
            had_error: false,
        }
    }

    /// Report error
    fn error(&mut self, msg: &str) {
        eprints("ld: ");
        eprintlns(msg);
        self.had_error = true;
    }

    /// Read an input file
    fn read_file(&mut self, path: &str) -> bool {
        if self.num_files >= MAX_FILES {
            self.error("too many input files");
            return false;
        }

        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprints("ld: cannot open ");
            eprintlns(path);
            self.had_error = true;
            return false;
        }

        let file = &mut self.files[self.num_files];
        let n = syscall::sys_read(fd, &mut file.data);
        close(fd);

        if n < 0 {
            self.error("read error");
            return false;
        }

        file.size = n as usize;
        self.num_files += 1;

        true
    }

    /// Process all input files
    fn process_files(&mut self) {
        for i in 0..self.num_files {
            self.process_object(i);
            if self.had_error {
                return;
            }
        }
    }

    /// Process a single object file
    fn process_object(&mut self, file_idx: usize) {
        let file_size = self.files[file_idx].size;
        if file_size < 64 {
            self.error("invalid object file");
            return;
        }

        // Use raw pointer to avoid borrow checker issues
        let data_ptr = self.files[file_idx].data.as_ptr();
        let data = unsafe { core::slice::from_raw_parts(data_ptr, file_size) };

        // Verify ELF magic
        if data[0..4] != ELFMAG {
            self.error("not an ELF file");
            return;
        }

        // Verify 64-bit
        if data[4] != ELFCLASS64 {
            self.error("not a 64-bit ELF");
            return;
        }

        // Read ELF header
        let ehdr = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };

        // Verify relocatable
        if ehdr.e_type != ET_REL {
            self.error("not a relocatable file");
            return;
        }

        // Verify x86_64
        if ehdr.e_machine != EM_X86_64 {
            self.error("not an x86_64 file");
            return;
        }

        // Read section headers
        let shoff = ehdr.e_shoff as usize;
        let shnum = ehdr.e_shnum as usize;
        let shentsize = ehdr.e_shentsize as usize;

        if shoff + shnum * shentsize > file_size {
            self.error("invalid section headers");
            return;
        }

        // Find string table section
        let shstrndx = ehdr.e_shstrndx as usize;
        let shstrtab_shdr = unsafe {
            &*((data.as_ptr() as usize + shoff + shstrndx * shentsize) as *const Elf64Shdr)
        };
        let shstrtab = &data[shstrtab_shdr.sh_offset as usize..];

        // Find symtab and strtab
        let mut symtab_shdr: Option<&Elf64Shdr> = None;
        let mut strtab_offset: usize = 0;

        for i in 0..shnum {
            let shdr =
                unsafe { &*((data.as_ptr() as usize + shoff + i * shentsize) as *const Elf64Shdr) };

            if shdr.sh_type == SHT_SYMTAB {
                symtab_shdr = Some(shdr);
            } else if shdr.sh_type == SHT_STRTAB && i != shstrndx {
                strtab_offset = shdr.sh_offset as usize;
            }
        }

        // Record file's section offsets
        self.files[file_idx].text_offset = self.text_len as u64;
        self.files[file_idx].data_offset = self.data_len as u64;
        self.files[file_idx].bss_offset = self.bss_len as u64;
        self.files[file_idx].first_sym = self.num_symbols;

        // Process sections - copy text and data
        for i in 0..shnum {
            let shdr =
                unsafe { &*((data.as_ptr() as usize + shoff + i * shentsize) as *const Elf64Shdr) };

            // Get section name
            let name_off = shdr.sh_name as usize;
            let name = get_cstring(&shstrtab[name_off..]);

            if shdr.sh_type == SHT_PROGBITS && (shdr.sh_flags & SHF_ALLOC) != 0 {
                let src =
                    &data[shdr.sh_offset as usize..shdr.sh_offset as usize + shdr.sh_size as usize];

                if (shdr.sh_flags & SHF_EXECINSTR) != 0 || bytes_eq_len(name, b".text") {
                    // Text section
                    self.text[self.text_len..self.text_len + src.len()].copy_from_slice(src);
                    self.text_len += src.len();
                } else {
                    // Data section
                    self.data[self.data_len..self.data_len + src.len()].copy_from_slice(src);
                    self.data_len += src.len();
                }
            } else if shdr.sh_type == SHT_NOBITS && (shdr.sh_flags & SHF_ALLOC) != 0 {
                // BSS section
                self.bss_len += shdr.sh_size as usize;
            }
        }

        // Process symbols
        if let Some(symtab) = symtab_shdr {
            let sym_count = (symtab.sh_size / symtab.sh_entsize) as usize;
            let strtab = &data[strtab_offset..];

            for j in 1..sym_count {
                // Skip NULL symbol
                let sym = unsafe {
                    &*((data.as_ptr() as usize + symtab.sh_offset as usize + j * 24)
                        as *const Elf64Sym)
                };

                let name = get_cstring(&strtab[sym.st_name as usize..]);
                if name.is_empty() {
                    continue;
                }

                // Determine section
                let section = if sym.st_shndx == SHN_UNDEF {
                    0
                } else {
                    // Look up section to determine if text/data/bss
                    let sec_shdr = unsafe {
                        &*((data.as_ptr() as usize + shoff + sym.st_shndx as usize * shentsize)
                            as *const Elf64Shdr)
                    };
                    if (sec_shdr.sh_flags & SHF_EXECINSTR) != 0 {
                        1 // text
                    } else if sec_shdr.sh_type == SHT_NOBITS {
                        3 // bss
                    } else {
                        2 // data
                    }
                };

                let binding = (sym.st_info >> 4) & 0x0F;
                let defined = sym.st_shndx != SHN_UNDEF;

                // Calculate value (add file's section offset)
                let value = if defined {
                    match section {
                        1 => sym.st_value + self.files[file_idx].text_offset,
                        2 => sym.st_value + self.files[file_idx].data_offset,
                        3 => sym.st_value + self.files[file_idx].bss_offset,
                        _ => sym.st_value,
                    }
                } else {
                    0
                };

                self.add_symbol(
                    name,
                    value,
                    sym.st_size,
                    section,
                    binding,
                    defined,
                    file_idx,
                );
            }
        }

        // Process relocations
        for i in 0..shnum {
            let shdr =
                unsafe { &*((data.as_ptr() as usize + shoff + i * shentsize) as *const Elf64Shdr) };

            if shdr.sh_type == SHT_RELA {
                // Get the section this applies to
                let target_shdr = unsafe {
                    &*((data.as_ptr() as usize + shoff + shdr.sh_info as usize * shentsize)
                        as *const Elf64Shdr)
                };

                let target_section = if (target_shdr.sh_flags & SHF_EXECINSTR) != 0 {
                    1
                } else {
                    2
                };

                let rela_count = (shdr.sh_size / shdr.sh_entsize) as usize;

                for j in 0..rela_count {
                    let rela = unsafe {
                        &*((data.as_ptr() as usize + shdr.sh_offset as usize + j * 24)
                            as *const Elf64Rela)
                    };

                    let sym_idx = (rela.r_info >> 32) as usize;
                    let rtype = (rela.r_info & 0xFFFFFFFF) as u32;

                    // Map local symbol index to global
                    let global_sym_idx = self.files[file_idx].first_sym + sym_idx - 1;

                    // Calculate offset in output section
                    let offset = if target_section == 1 {
                        rela.r_offset + self.files[file_idx].text_offset
                    } else {
                        rela.r_offset + self.files[file_idx].data_offset
                    };

                    self.add_reloc(offset, global_sym_idx, rtype, rela.r_addend, target_section);
                }
            }
        }
    }

    /// Add a symbol
    fn add_symbol(
        &mut self,
        name: &[u8],
        value: u64,
        size: u64,
        section: u8,
        binding: u8,
        defined: bool,
        file_idx: usize,
    ) {
        // Check if symbol already exists
        for i in 0..self.num_symbols {
            if bytes_eq_len(&self.symbols[i].name, name) {
                if defined && binding == STB_GLOBAL {
                    if self.symbols[i].defined && self.symbols[i].binding == STB_GLOBAL {
                        eprints("ld: duplicate symbol: ");
                        prints(bytes_to_str(name));
                        printlns("");
                        self.had_error = true;
                        return;
                    }
                    self.symbols[i].value = value;
                    self.symbols[i].size = size;
                    self.symbols[i].section = section;
                    self.symbols[i].binding = binding;
                    self.symbols[i].defined = true;
                    self.symbols[i].file_idx = file_idx;
                }
                return;
            }
        }

        // Add new symbol
        if self.num_symbols >= MAX_SYMBOLS {
            self.error("too many symbols");
            return;
        }

        let idx = self.num_symbols;
        copy_bytes(&mut self.symbols[idx].name, name);
        self.symbols[idx].value = value;
        self.symbols[idx].size = size;
        self.symbols[idx].section = section;
        self.symbols[idx].binding = binding;
        self.symbols[idx].defined = defined;
        self.symbols[idx].file_idx = file_idx;
        self.num_symbols += 1;
    }

    /// Add a relocation
    fn add_reloc(&mut self, offset: u64, sym_idx: usize, rtype: u32, addend: i64, section: u8) {
        if self.num_relocs >= MAX_RELOCS {
            self.error("too many relocations");
            return;
        }

        let idx = self.num_relocs;
        self.relocs[idx].offset = offset;
        self.relocs[idx].sym_idx = sym_idx;
        self.relocs[idx].rtype = rtype;
        self.relocs[idx].addend = addend;
        self.relocs[idx].section = section;
        self.num_relocs += 1;
    }

    /// Resolve symbols and check for undefined references
    fn resolve_symbols(&mut self) {
        for i in 0..self.num_symbols {
            if !self.symbols[i].defined && self.symbols[i].binding == STB_GLOBAL {
                eprints("ld: undefined reference to `");
                prints(bytes_to_str(&self.symbols[i].name));
                printlns("`");
                self.had_error = true;
            }
        }
    }

    /// Apply relocations
    fn apply_relocs(&mut self) {
        // Calculate final section addresses
        let text_addr = self.base_addr + 0x1000; // After ELF header
        let data_addr = text_addr + (self.text_len as u64 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let bss_addr = data_addr + self.data_len as u64;

        for i in 0..self.num_relocs {
            let reloc = &self.relocs[i];
            let sym_idx = reloc.sym_idx;

            if sym_idx >= self.num_symbols {
                continue;
            }

            let sym = &self.symbols[sym_idx];

            // Calculate symbol address
            let sym_addr = match sym.section {
                1 => text_addr + sym.value,
                2 => data_addr + sym.value,
                3 => bss_addr + sym.value,
                _ => sym.value,
            };

            // Calculate relocation address
            let reloc_addr = if reloc.section == 1 {
                text_addr + reloc.offset
            } else {
                data_addr + reloc.offset
            };

            // Get pointer to patch location
            let patch_ptr = if reloc.section == 1 {
                &mut self.text[reloc.offset as usize..]
            } else {
                &mut self.data[reloc.offset as usize..]
            };

            // Apply relocation
            match reloc.rtype {
                R_X86_64_64 => {
                    // Absolute 64-bit
                    let value = (sym_addr as i64 + reloc.addend) as u64;
                    patch_ptr[0] = value as u8;
                    patch_ptr[1] = (value >> 8) as u8;
                    patch_ptr[2] = (value >> 16) as u8;
                    patch_ptr[3] = (value >> 24) as u8;
                    patch_ptr[4] = (value >> 32) as u8;
                    patch_ptr[5] = (value >> 40) as u8;
                    patch_ptr[6] = (value >> 48) as u8;
                    patch_ptr[7] = (value >> 56) as u8;
                }
                R_X86_64_PC32 | R_X86_64_PLT32 => {
                    // PC-relative 32-bit
                    let value = (sym_addr as i64 - reloc_addr as i64 + reloc.addend) as i32;
                    patch_ptr[0] = value as u8;
                    patch_ptr[1] = (value >> 8) as u8;
                    patch_ptr[2] = (value >> 16) as u8;
                    patch_ptr[3] = (value >> 24) as u8;
                }
                R_X86_64_32 => {
                    // Absolute 32-bit unsigned
                    let value = (sym_addr as i64 + reloc.addend) as u32;
                    patch_ptr[0] = value as u8;
                    patch_ptr[1] = (value >> 8) as u8;
                    patch_ptr[2] = (value >> 16) as u8;
                    patch_ptr[3] = (value >> 24) as u8;
                }
                R_X86_64_32S => {
                    // Absolute 32-bit signed
                    let value = (sym_addr as i64 + reloc.addend) as i32;
                    patch_ptr[0] = value as u8;
                    patch_ptr[1] = (value >> 8) as u8;
                    patch_ptr[2] = (value >> 16) as u8;
                    patch_ptr[3] = (value >> 24) as u8;
                }
                _ => {
                    eprints("ld: unsupported relocation type ");
                    print_u64(reloc.rtype as u64);
                    printlns("");
                }
            }
        }
    }

    /// Find entry point address
    fn find_entry(&self) -> u64 {
        let text_addr = self.base_addr + 0x1000;
        let data_addr = text_addr + (self.text_len as u64 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let bss_addr = data_addr + self.data_len as u64;

        let entry_name = if self.entry_name[0] != 0 {
            &self.entry_name[..]
        } else {
            b"_start" as &[u8]
        };

        for i in 0..self.num_symbols {
            if bytes_eq_len(&self.symbols[i].name, entry_name) && self.symbols[i].defined {
                return match self.symbols[i].section {
                    1 => text_addr + self.symbols[i].value,
                    2 => data_addr + self.symbols[i].value,
                    3 => bss_addr + self.symbols[i].value,
                    _ => text_addr,
                };
            }
        }

        // Default to start of text
        text_addr
    }

    /// Write output executable
    fn write_output(&self, path: &str) -> bool {
        let fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o755);
        if fd < 0 {
            return false;
        }

        // Calculate addresses
        let text_addr = self.base_addr + 0x1000;
        let data_addr = (text_addr + self.text_len as u64 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        let entry = self.find_entry();

        // ELF header
        let ehdr_size = 64;
        let phdr_size = 56;
        let num_phdrs = if self.data_len > 0 { 2 } else { 1 };

        let mut ehdr = [0u8; 64];
        ehdr[0..4].copy_from_slice(&ELFMAG);
        ehdr[4] = ELFCLASS64;
        ehdr[5] = 1; // Little endian
        ehdr[6] = 1; // EV_CURRENT
        write_u16(&mut ehdr[16..], ET_EXEC);
        write_u16(&mut ehdr[18..], EM_X86_64);
        write_u32(&mut ehdr[20..], 1); // e_version
        write_u64(&mut ehdr[24..], entry); // e_entry
        write_u64(&mut ehdr[32..], ehdr_size as u64); // e_phoff
        write_u16(&mut ehdr[52..], ehdr_size as u16); // e_ehsize
        write_u16(&mut ehdr[54..], phdr_size as u16); // e_phentsize
        write_u16(&mut ehdr[56..], num_phdrs as u16); // e_phnum

        syscall::sys_write(fd, &ehdr);

        // Program headers
        // Text segment
        let text_file_off = (ehdr_size + num_phdrs * phdr_size) as u64;
        let mut text_phdr = [0u8; 56];
        write_u32(&mut text_phdr[0..], PT_LOAD);
        write_u32(&mut text_phdr[4..], PF_R | PF_X);
        write_u64(&mut text_phdr[8..], text_file_off); // p_offset
        write_u64(&mut text_phdr[16..], text_addr); // p_vaddr
        write_u64(&mut text_phdr[24..], text_addr); // p_paddr
        write_u64(&mut text_phdr[32..], self.text_len as u64); // p_filesz
        write_u64(&mut text_phdr[40..], self.text_len as u64); // p_memsz
        write_u64(&mut text_phdr[48..], PAGE_SIZE); // p_align

        syscall::sys_write(fd, &text_phdr);

        // Data segment (if any)
        if self.data_len > 0 {
            let data_file_off = text_file_off + self.text_len as u64;
            let data_memsz = self.data_len + self.bss_len;

            let mut data_phdr = [0u8; 56];
            write_u32(&mut data_phdr[0..], PT_LOAD);
            write_u32(&mut data_phdr[4..], PF_R | PF_W);
            write_u64(&mut data_phdr[8..], data_file_off);
            write_u64(&mut data_phdr[16..], data_addr);
            write_u64(&mut data_phdr[24..], data_addr);
            write_u64(&mut data_phdr[32..], self.data_len as u64);
            write_u64(&mut data_phdr[40..], data_memsz as u64);
            write_u64(&mut data_phdr[48..], PAGE_SIZE);

            syscall::sys_write(fd, &data_phdr);
        }

        // Write text section
        syscall::sys_write(fd, &self.text[..self.text_len]);

        // Write data section
        if self.data_len > 0 {
            syscall::sys_write(fd, &self.data[..self.data_len]);
        }

        close(fd);
        true
    }
}

/// Copy bytes
fn copy_bytes(dst: &mut [u8], src: &[u8]) {
    let len = src
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(src.len())
        .min(dst.len() - 1);
    dst[..len].copy_from_slice(&src[..len]);
    dst[len] = 0;
}

/// Compare byte slices
fn bytes_eq_len(a: &[u8], b: &[u8]) -> bool {
    let a_len = a.iter().position(|&c| c == 0).unwrap_or(a.len());
    let b_len = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    if a_len != b_len {
        return false;
    }
    for i in 0..a_len {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

/// Get C string from buffer
fn get_cstring(buf: &[u8]) -> &[u8] {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    &buf[..end]
}

/// Convert byte slice to str
fn bytes_to_str(s: &[u8]) -> &str {
    let len = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    unsafe { core::str::from_utf8_unchecked(&s[..len]) }
}

/// Write u16 little-endian
fn write_u16(buf: &mut [u8], v: u16) {
    buf[0] = v as u8;
    buf[1] = (v >> 8) as u8;
}

/// Write u32 little-endian
fn write_u32(buf: &mut [u8], v: u32) {
    buf[0] = v as u8;
    buf[1] = (v >> 8) as u8;
    buf[2] = (v >> 16) as u8;
    buf[3] = (v >> 24) as u8;
}

/// Write u64 little-endian
fn write_u64(buf: &mut [u8], v: u64) {
    write_u32(&mut buf[0..], v as u32);
    write_u32(&mut buf[4..], (v >> 32) as u32);
}

// Global linker instance - too large for stack
// Using UnsafeCell for interior mutability
use core::cell::UnsafeCell;

struct LinkerCell(UnsafeCell<core::mem::MaybeUninit<Linker>>);
unsafe impl Sync for LinkerCell {}

static LINKER: LinkerCell = LinkerCell(UnsafeCell::new(core::mem::MaybeUninit::uninit()));
static mut LINKER_INITIALIZED: bool = false;

/// Get linker instance
fn linker() -> &'static mut Linker {
    unsafe {
        let ptr = (*LINKER.0.get()).as_mut_ptr();
        if !LINKER_INITIALIZED {
            ptr.write(Linker::new());
            LINKER_INITIALIZED = true;
        }
        &mut *ptr
    }
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: ld [-o output] [-e entry] file...");
        return 1;
    }

    // Get linker instance
    let ld = linker();

    // Default entry point
    copy_bytes(&mut ld.entry_name, b"_start");

    // Parse arguments
    let mut output_file = "a.out";
    let mut i = 1;

    while i < argc as usize {
        let arg = get_arg(argv, i);

        if bytes_eq_len(arg, b"-o") {
            i += 1;
            if i < argc as usize {
                output_file = bytes_to_str(get_arg(argv, i));
            }
        } else if bytes_eq_len(arg, b"-e") {
            i += 1;
            if i < argc as usize {
                copy_bytes(&mut ld.entry_name, get_arg(argv, i));
            }
        } else if arg[0] != b'-' {
            if !ld.read_file(bytes_to_str(arg)) {
                return 1;
            }
        }
        i += 1;
    }

    if ld.num_files == 0 {
        eprintlns("ld: no input files");
        return 1;
    }

    // Process files
    ld.process_files();
    if ld.had_error {
        return 1;
    }

    // Resolve symbols
    ld.resolve_symbols();
    if ld.had_error {
        return 1;
    }

    // Apply relocations
    ld.apply_relocs();

    // Write output
    if !ld.write_output(output_file) {
        eprints("ld: cannot create ");
        eprintlns(output_file);
        return 1;
    }

    0
}

/// Get argument at index
fn get_arg(argv: *const *const u8, idx: usize) -> &'static [u8] {
    unsafe {
        let ptr = *argv.add(idx);
        if ptr.is_null() {
            return b"";
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::slice::from_raw_parts(ptr, len)
    }
}
