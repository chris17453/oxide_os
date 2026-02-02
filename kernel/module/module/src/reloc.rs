//! ELF relocation processing
//!
//! Handles architecture-specific relocations for module loading.

use crate::symbol::lookup_symbol;
use crate::{ModuleError, ModuleResult};

/// x86_64 relocation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RelocX86_64 {
    /// No relocation
    None = 0,
    /// 64-bit absolute: S + A
    R64 = 1,
    /// 32-bit PC-relative: S + A - P
    PC32 = 2,
    /// 32-bit GOT offset: G + A
    GOT32 = 3,
    /// 32-bit PLT offset: L + A - P
    PLT32 = 4,
    /// Copy symbol at runtime
    Copy = 5,
    /// Create GOT entry
    GlobDat = 6,
    /// Create PLT entry
    JumpSlot = 7,
    /// Adjust by program base
    Relative = 8,
    /// 32-bit GOT PC-relative: G + GOT + A - P
    GOTPCREL = 9,
    /// Direct 32-bit zero-extended: S + A
    R32 = 10,
    /// Direct 32-bit sign-extended: S + A
    R32S = 11,
    /// Direct 16-bit zero-extended
    R16 = 12,
    /// 16-bit PC-relative
    PC16 = 13,
    /// Direct 8-bit sign-extended
    R8 = 14,
    /// 8-bit PC-relative
    PC8 = 15,
    /// PC-relative 64-bit
    PC64 = 24,
    /// 32-bit offset to GOT
    GOTOFF64 = 25,
    /// PC-relative offset to GOT
    GOTPC32 = 26,
    /// 32-bit signed PC relative offset to GOT
    GOTPCRELX = 41,
    /// Relaxable GOTPCRELX
    RexGOTPCRELX = 42,
}

impl RelocX86_64 {
    pub fn from_u32(val: u32) -> Option<Self> {
        match val {
            0 => Some(RelocX86_64::None),
            1 => Some(RelocX86_64::R64),
            2 => Some(RelocX86_64::PC32),
            3 => Some(RelocX86_64::GOT32),
            4 => Some(RelocX86_64::PLT32),
            5 => Some(RelocX86_64::Copy),
            6 => Some(RelocX86_64::GlobDat),
            7 => Some(RelocX86_64::JumpSlot),
            8 => Some(RelocX86_64::Relative),
            9 => Some(RelocX86_64::GOTPCREL),
            10 => Some(RelocX86_64::R32),
            11 => Some(RelocX86_64::R32S),
            12 => Some(RelocX86_64::R16),
            13 => Some(RelocX86_64::PC16),
            14 => Some(RelocX86_64::R8),
            15 => Some(RelocX86_64::PC8),
            24 => Some(RelocX86_64::PC64),
            25 => Some(RelocX86_64::GOTOFF64),
            26 => Some(RelocX86_64::GOTPC32),
            41 => Some(RelocX86_64::GOTPCRELX),
            42 => Some(RelocX86_64::RexGOTPCRELX),
            _ => None,
        }
    }
}

/// Relocation entry (RELA format)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Rela64 {
    /// Offset in section where relocation applies
    pub offset: u64,
    /// Relocation type and symbol index
    pub info: u64,
    /// Constant addend
    pub addend: i64,
}

impl Rela64 {
    /// Get the relocation type
    pub fn r_type(&self) -> u32 {
        (self.info & 0xffffffff) as u32
    }

    /// Get the symbol index
    pub fn r_sym(&self) -> u32 {
        (self.info >> 32) as u32
    }
}

/// Apply x86_64 relocations to a loaded module
///
/// # Arguments
/// * `base` - Base address where module is loaded
/// * `rela` - Relocation entries
/// * `symtab` - Symbol table entries
/// * `strtab` - String table for symbol names
///
/// # Safety
/// Caller must ensure base address and relocation targets are valid.
pub unsafe fn apply_relocations_x86_64(
    base: usize,
    rela: &[Rela64],
    symtab: &[Sym64],
    strtab: &[u8],
) -> ModuleResult<()> {
    for rel in rela {
        let r_type = RelocX86_64::from_u32(rel.r_type()).ok_or(ModuleError::UnknownRelocation)?;

        if r_type == RelocX86_64::None {
            continue;
        }

        let sym_idx = rel.r_sym() as usize;
        let sym = &symtab[sym_idx];

        // Get symbol value
        let sym_value = if sym.st_shndx == 0 {
            // Undefined symbol, look up in kernel/modules
            let name = get_string(strtab, sym.st_name as usize);
            lookup_symbol(name).ok_or(ModuleError::SymbolNotFound)?
        } else {
            // Defined in this module
            base + sym.st_value as usize
        };

        let place = base + rel.offset as usize;
        let addend = rel.addend;

        // Apply relocation based on type
        // S = symbol value, A = addend, P = place
        match r_type {
            RelocX86_64::None => {}

            RelocX86_64::R64 => {
                // S + A
                let val = (sym_value as i64 + addend) as u64;
                *(place as *mut u64) = val;
            }

            RelocX86_64::PC32 | RelocX86_64::PLT32 => {
                // S + A - P (32-bit PC-relative)
                let val = (sym_value as i64 + addend - place as i64) as i32;
                *(place as *mut i32) = val;
            }

            RelocX86_64::R32 => {
                // S + A (32-bit)
                let val = (sym_value as i64 + addend) as u32;
                *(place as *mut u32) = val;
            }

            RelocX86_64::R32S => {
                // S + A (32-bit signed)
                let val = (sym_value as i64 + addend) as i32;
                *(place as *mut i32) = val;
            }

            RelocX86_64::PC64 => {
                // S + A - P (64-bit PC-relative)
                let val = (sym_value as i64 + addend - place as i64) as i64;
                *(place as *mut i64) = val;
            }

            RelocX86_64::GOTPCREL | RelocX86_64::GOTPCRELX | RelocX86_64::RexGOTPCRELX => {
                // For modules, we handle GOT-relative as direct symbol access
                // S + A - P (simplified, no actual GOT)
                let val = (sym_value as i64 + addend - place as i64) as i32;
                *(place as *mut i32) = val;
            }

            _ => return Err(ModuleError::UnknownRelocation),
        }
    }

    Ok(())
}

/// ELF64 symbol entry
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Sym64 {
    /// Symbol name (index into string table)
    pub st_name: u32,
    /// Type and binding
    pub st_info: u8,
    /// Reserved
    pub st_other: u8,
    /// Section index
    pub st_shndx: u16,
    /// Symbol value
    pub st_value: u64,
    /// Symbol size
    pub st_size: u64,
}

impl Sym64 {
    /// Get symbol binding (local, global, weak)
    pub fn binding(&self) -> u8 {
        self.st_info >> 4
    }

    /// Get symbol type (notype, object, func, etc)
    pub fn stype(&self) -> u8 {
        self.st_info & 0xf
    }
}

/// Symbol binding values
pub const STB_LOCAL: u8 = 0;
pub const STB_GLOBAL: u8 = 1;
pub const STB_WEAK: u8 = 2;

/// Symbol type values
pub const STT_NOTYPE: u8 = 0;
pub const STT_OBJECT: u8 = 1;
pub const STT_FUNC: u8 = 2;
pub const STT_SECTION: u8 = 3;
pub const STT_FILE: u8 = 4;

/// Get a null-terminated string from a string table
fn get_string(strtab: &[u8], offset: usize) -> &str {
    let start = offset;
    let mut end = offset;
    while end < strtab.len() && strtab[end] != 0 {
        end += 1;
    }
    core::str::from_utf8(&strtab[start..end]).unwrap_or("")
}
