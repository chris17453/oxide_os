//! ELF64 parsing for the userspace dynamic linker
//!
//! — WireSaint: a stripped-down ELF parser that reads via raw memory access
//! (the program headers are already mapped by the kernel). No file I/O needed
//! for the main executable's headers — only for loading DT_NEEDED .so files.

/// ELF64 program header (same layout as kernel's Elf64ProgramHeader)
/// — WireSaint: repr(C) is load-bearing. The kernel maps these structs
/// directly from the ELF file. Get the layout wrong and every field
/// after the first is garbage.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Phdr {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

/// ELF64 dynamic entry (from .dynamic section)
/// — WireSaint: each entry is a tag-value pair. Tags identify what the value
/// means: DT_NEEDED = index into strtab for a library name, DT_STRTAB = address
/// of the string table, DT_SYMTAB = address of the symbol table, etc.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Dyn {
    pub d_tag: i64,
    pub d_val: u64,
}

/// ELF64 symbol table entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Sym {
    pub st_name: u32,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: u16,
    pub st_value: u64,
    pub st_size: u64,
}

/// ELF64 relocation entry with addend (Rela)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Rela {
    pub r_offset: u64,
    pub r_info: u64,
    pub r_addend: i64,
}

impl Elf64Rela {
    /// Extract relocation type from r_info
    pub fn r_type(&self) -> u32 {
        (self.r_info & 0xFFFFFFFF) as u32
    }

    /// Extract symbol index from r_info
    pub fn r_sym(&self) -> u32 {
        (self.r_info >> 32) as u32
    }
}

/// Program header type constants
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_PHDR: u32 = 6;
pub const PT_TLS: u32 = 7;

/// Dynamic tag constants
/// — WireSaint: the critical subset for basic dynamic linking.
/// Full list is ~50 tags, but we only need these for initial bootstrapping.
pub const DT_NULL: i64 = 0;
pub const DT_NEEDED: i64 = 1;
pub const DT_PLTRELSZ: i64 = 2;
pub const DT_PLTGOT: i64 = 3;
pub const DT_HASH: i64 = 4;
pub const DT_STRTAB: i64 = 5;
pub const DT_SYMTAB: i64 = 6;
pub const DT_RELA: i64 = 7;
pub const DT_RELASZ: i64 = 8;
pub const DT_RELAENT: i64 = 9;
pub const DT_STRSZ: i64 = 10;
pub const DT_SYMENT: i64 = 11;
pub const DT_INIT: i64 = 12;
pub const DT_FINI: i64 = 13;
pub const DT_SONAME: i64 = 14;
pub const DT_JMPREL: i64 = 23;
pub const DT_INIT_ARRAY: i64 = 25;
pub const DT_FINI_ARRAY: i64 = 26;
pub const DT_INIT_ARRAYSZ: i64 = 27;
pub const DT_FINI_ARRAYSZ: i64 = 28;
pub const DT_GNU_HASH: i64 = 0x6ffffef5u32 as i64;

/// Parsed PT_DYNAMIC segment info
/// — WireSaint: all the pointers the linker needs to find symbols, strings,
/// and relocations. These are virtual addresses in the loaded image.
#[derive(Debug, Clone)]
pub struct DynamicInfo {
    pub strtab: u64,
    pub strsz: u64,
    pub symtab: u64,
    pub syment: u64,
    pub rela: u64,
    pub relasz: u64,
    pub relaent: u64,
    pub jmprel: u64,
    pub pltrelsz: u64,
    pub pltgot: u64,
    pub init: u64,
    pub fini: u64,
    pub init_array: u64,
    pub init_arraysz: u64,
    pub fini_array: u64,
    pub fini_arraysz: u64,
    pub hash: u64,
    pub gnu_hash: u64,
    /// DT_NEEDED library name offsets (into strtab)
    pub needed: [u64; 32],
    pub needed_count: usize,
}

impl DynamicInfo {
    pub fn new() -> Self {
        Self {
            strtab: 0, strsz: 0, symtab: 0, syment: 24,
            rela: 0, relasz: 0, relaent: 24,
            jmprel: 0, pltrelsz: 0, pltgot: 0,
            init: 0, fini: 0,
            init_array: 0, init_arraysz: 0,
            fini_array: 0, fini_arraysz: 0,
            hash: 0, gnu_hash: 0,
            needed: [0; 32], needed_count: 0,
        }
    }

    /// Relocate all address fields by adding base_offset.
    /// — WireSaint: shared libraries store raw vaddrs in .dynamic entries.
    /// After loading at a different base, all pointer fields need adjustment.
    /// Size fields (strsz, relasz, relaent, syment, pltrelsz, *_arraysz) are
    /// NOT adjusted — they're byte counts, not addresses.
    pub fn relocate(&mut self, base_offset: u64) {
        if self.strtab != 0 { self.strtab += base_offset; }
        if self.symtab != 0 { self.symtab += base_offset; }
        if self.rela != 0 { self.rela += base_offset; }
        if self.jmprel != 0 { self.jmprel += base_offset; }
        if self.pltgot != 0 { self.pltgot += base_offset; }
        if self.init != 0 { self.init += base_offset; }
        if self.fini != 0 { self.fini += base_offset; }
        if self.init_array != 0 { self.init_array += base_offset; }
        if self.fini_array != 0 { self.fini_array += base_offset; }
        if self.hash != 0 { self.hash += base_offset; }
        if self.gnu_hash != 0 { self.gnu_hash += base_offset; }
        // — WireSaint: DT_NEEDED offsets are string table indices, not addresses.
        // They don't get relocated.
    }
}

/// Find PT_DYNAMIC segment from program headers
/// — WireSaint: walks the phdr array (already mapped in memory by the kernel)
/// looking for PT_DYNAMIC. Returns the virtual address of the .dynamic section.
pub fn find_dynamic(phdr: *const Elf64Phdr, phnum: usize) -> Option<u64> {
    for i in 0..phnum {
        let ph = unsafe { &*phdr.add(i) };
        if ph.p_type == PT_DYNAMIC {
            return Some(ph.p_vaddr);
        }
    }
    None
}

/// Parse the .dynamic section into a DynamicInfo struct
/// — WireSaint: walks the Elf64Dyn array at the given address until DT_NULL.
/// Extracts all the pointers needed for symbol resolution and relocation.
pub unsafe fn parse_dynamic(dyn_addr: u64) -> DynamicInfo {
    let mut info = DynamicInfo::new();
    let mut ptr = dyn_addr as *const Elf64Dyn;

    loop {
        let entry = unsafe { &*ptr };
        match entry.d_tag {
            DT_NULL => break,
            DT_NEEDED => {
                if info.needed_count < 32 {
                    info.needed[info.needed_count] = entry.d_val;
                    info.needed_count += 1;
                }
            }
            DT_STRTAB => info.strtab = entry.d_val,
            DT_STRSZ => info.strsz = entry.d_val,
            DT_SYMTAB => info.symtab = entry.d_val,
            DT_SYMENT => info.syment = entry.d_val,
            DT_RELA => info.rela = entry.d_val,
            DT_RELASZ => info.relasz = entry.d_val,
            DT_RELAENT => info.relaent = entry.d_val,
            DT_JMPREL => info.jmprel = entry.d_val,
            DT_PLTRELSZ => info.pltrelsz = entry.d_val,
            DT_PLTGOT => info.pltgot = entry.d_val,
            DT_INIT => info.init = entry.d_val,
            DT_FINI => info.fini = entry.d_val,
            DT_INIT_ARRAY => info.init_array = entry.d_val,
            DT_INIT_ARRAYSZ => info.init_arraysz = entry.d_val,
            DT_FINI_ARRAY => info.fini_array = entry.d_val,
            DT_FINI_ARRAYSZ => info.fini_arraysz = entry.d_val,
            DT_HASH => info.hash = entry.d_val,
            _ if entry.d_tag == DT_GNU_HASH => info.gnu_hash = entry.d_val,
            _ => {} // — WireSaint: ignore unknown tags
        }
        ptr = unsafe { ptr.add(1) };
    }

    info
}

/// Get a C string from the string table at a given offset
/// — WireSaint: returns a byte slice up to (but not including) the null terminator.
/// Panics if the string table is not mapped or the offset is out of bounds.
pub unsafe fn strtab_get(strtab: u64, offset: u64) -> &'static [u8] {
    let start = (strtab + offset) as *const u8;
    let mut len = 0usize;
    while unsafe { *start.add(len) } != 0 {
        len += 1;
        if len > 4096 {
            // — WireSaint: safety valve — no library name should be 4KB long
            break;
        }
    }
    unsafe { core::slice::from_raw_parts(start, len) }
}
