//! Relocation handling

#![allow(non_camel_case_types)]
#![allow(unsafe_op_in_unsafe_fn)]

/// x86_64 relocation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RelocationType {
    /// No relocation
    R_X86_64_NONE = 0,
    /// Direct 64-bit
    R_X86_64_64 = 1,
    /// PC-relative 32-bit
    R_X86_64_PC32 = 2,
    /// GOT entry 32-bit
    R_X86_64_GOT32 = 3,
    /// PLT entry 32-bit
    R_X86_64_PLT32 = 4,
    /// Copy symbol at runtime
    R_X86_64_COPY = 5,
    /// Create GOT entry
    R_X86_64_GLOB_DAT = 6,
    /// Create PLT entry
    R_X86_64_JUMP_SLOT = 7,
    /// Adjust by program base
    R_X86_64_RELATIVE = 8,
    /// GOT offset for PC-relative
    R_X86_64_GOTPCREL = 9,
    /// Direct 32-bit zero-extended
    R_X86_64_32 = 10,
    /// Direct 32-bit sign-extended
    R_X86_64_32S = 11,
    /// Direct 16-bit zero-extended
    R_X86_64_16 = 12,
    /// PC-relative 16-bit
    R_X86_64_PC16 = 13,
    /// Direct 8-bit
    R_X86_64_8 = 14,
    /// PC-relative 8-bit
    R_X86_64_PC8 = 15,
    /// Thread-local descriptor
    R_X86_64_DTPMOD64 = 16,
    /// Offset in TLS block
    R_X86_64_DTPOFF64 = 17,
    /// Offset in initial TLS block
    R_X86_64_TPOFF64 = 18,
    /// PC-relative TLS descriptor call
    R_X86_64_TLSGD = 19,
    /// PC-relative TLS descriptor call
    R_X86_64_TLSLD = 20,
    /// Offset in TLS block
    R_X86_64_DTPOFF32 = 21,
    /// Offset in initial TLS block
    R_X86_64_GOTTPOFF = 22,
    /// Offset in initial TLS block
    R_X86_64_TPOFF32 = 23,
    /// PC-relative 64-bit
    R_X86_64_PC64 = 24,
    /// GOT offset 64-bit
    R_X86_64_GOTOFF64 = 25,
    /// PC-relative offset to GOT
    R_X86_64_GOTPC32 = 26,
    /// Size of symbol
    R_X86_64_SIZE32 = 32,
    /// Size of symbol (64-bit)
    R_X86_64_SIZE64 = 33,
    /// GOT entry for TLS descriptor
    R_X86_64_GOTPC32_TLSDESC = 34,
    /// TLS descriptor call
    R_X86_64_TLSDESC_CALL = 35,
    /// TLS descriptor
    R_X86_64_TLSDESC = 36,
    /// Adjust indirect call
    R_X86_64_IRELATIVE = 37,
    /// Relative to program base, 64-bit
    R_X86_64_RELATIVE64 = 38,
    /// PC-relative 32-bit with GOT base
    R_X86_64_GOTPCRELX = 41,
    /// Relaxable PC-relative 32-bit with GOT base
    R_X86_64_REX_GOTPCRELX = 42,
}

impl RelocationType {
    /// Convert from raw relocation type
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::R_X86_64_NONE),
            1 => Some(Self::R_X86_64_64),
            2 => Some(Self::R_X86_64_PC32),
            3 => Some(Self::R_X86_64_GOT32),
            4 => Some(Self::R_X86_64_PLT32),
            5 => Some(Self::R_X86_64_COPY),
            6 => Some(Self::R_X86_64_GLOB_DAT),
            7 => Some(Self::R_X86_64_JUMP_SLOT),
            8 => Some(Self::R_X86_64_RELATIVE),
            9 => Some(Self::R_X86_64_GOTPCREL),
            10 => Some(Self::R_X86_64_32),
            11 => Some(Self::R_X86_64_32S),
            37 => Some(Self::R_X86_64_IRELATIVE),
            41 => Some(Self::R_X86_64_GOTPCRELX),
            42 => Some(Self::R_X86_64_REX_GOTPCRELX),
            _ => None,
        }
    }
}

/// Relocation entry
#[derive(Debug, Clone)]
pub struct Relocation {
    /// Offset to apply relocation
    pub offset: u64,
    /// Relocation type
    pub r_type: RelocationType,
    /// Symbol index
    pub sym_idx: u32,
    /// Addend
    pub addend: i64,
}

impl Relocation {
    /// Parse Rela entry from bytes
    pub fn parse_rela(data: &[u8]) -> Option<Self> {
        if data.len() < 24 {
            return None;
        }

        let offset = u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]);
        let info = u64::from_le_bytes([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]);
        let addend = i64::from_le_bytes([data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23]]);

        let r_type = RelocationType::from_raw((info & 0xFFFFFFFF) as u32)?;
        let sym_idx = (info >> 32) as u32;

        Some(Relocation {
            offset,
            r_type,
            sym_idx,
            addend,
        })
    }

    /// Parse Rel entry from bytes (no addend)
    pub fn parse_rel(data: &[u8]) -> Option<Self> {
        if data.len() < 16 {
            return None;
        }

        let offset = u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]);
        let info = u64::from_le_bytes([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]);

        let r_type = RelocationType::from_raw((info & 0xFFFFFFFF) as u32)?;
        let sym_idx = (info >> 32) as u32;

        Some(Relocation {
            offset,
            r_type,
            sym_idx,
            addend: 0,
        })
    }
}

/// Apply a relocation
///
/// - `base`: Base load address of the object
/// - `reloc`: The relocation to apply
/// - `sym_value`: Resolved symbol value (or 0 if no symbol)
/// - `got`: Address of the GOT
///
/// Returns Ok(()) on success, Err(msg) on failure
pub fn apply_relocation(
    base: usize,
    reloc: &Relocation,
    sym_value: usize,
    got: usize,
) -> Result<(), &'static str> {
    let target = base + reloc.offset as usize;

    // S = symbol value
    // A = addend
    // P = place (target address)
    // B = base address
    // G = GOT entry address
    // GOT = GOT base address

    match reloc.r_type {
        RelocationType::R_X86_64_NONE => {
            // No operation
        }

        RelocationType::R_X86_64_64 => {
            // S + A
            let value = sym_value.wrapping_add(reloc.addend as usize);
            unsafe { *(target as *mut u64) = value as u64; }
        }

        RelocationType::R_X86_64_PC32 => {
            // S + A - P
            let value = (sym_value as i64)
                .wrapping_add(reloc.addend)
                .wrapping_sub(target as i64);
            unsafe { *(target as *mut i32) = value as i32; }
        }

        RelocationType::R_X86_64_PLT32 => {
            // L + A - P (where L is PLT entry, usually same as S for lazy binding)
            let value = (sym_value as i64)
                .wrapping_add(reloc.addend)
                .wrapping_sub(target as i64);
            unsafe { *(target as *mut i32) = value as i32; }
        }

        RelocationType::R_X86_64_COPY => {
            // Copy symbol contents - this is handled by the loader, not here
            return Err("R_X86_64_COPY not supported in apply_relocation");
        }

        RelocationType::R_X86_64_GLOB_DAT | RelocationType::R_X86_64_JUMP_SLOT => {
            // S (direct symbol address)
            unsafe { *(target as *mut u64) = sym_value as u64; }
        }

        RelocationType::R_X86_64_RELATIVE => {
            // B + A
            let value = (base as i64).wrapping_add(reloc.addend) as u64;
            unsafe { *(target as *mut u64) = value; }
        }

        RelocationType::R_X86_64_GOTPCREL |
        RelocationType::R_X86_64_GOTPCRELX |
        RelocationType::R_X86_64_REX_GOTPCRELX => {
            // G + GOT + A - P
            // Simplified: just compute PC-relative to GOT entry containing S
            let value = (got as i64)
                .wrapping_add(reloc.addend)
                .wrapping_sub(target as i64);
            unsafe { *(target as *mut i32) = value as i32; }
        }

        RelocationType::R_X86_64_32 => {
            // S + A (truncated to 32 bits, zero-extended)
            let value = sym_value.wrapping_add(reloc.addend as usize) as u32;
            unsafe { *(target as *mut u32) = value; }
        }

        RelocationType::R_X86_64_32S => {
            // S + A (truncated to 32 bits, sign-extended)
            let value = (sym_value as i64).wrapping_add(reloc.addend) as i32;
            unsafe { *(target as *mut i32) = value; }
        }

        RelocationType::R_X86_64_IRELATIVE => {
            // Call indirect function: value = (*)(B + A)()
            // The result of calling the function at B + A becomes the relocation value
            let func_addr = (base as i64).wrapping_add(reloc.addend) as usize;
            let resolver: extern "C" fn() -> usize = unsafe { core::mem::transmute(func_addr) };
            let resolved = resolver();
            unsafe { *(target as *mut u64) = resolved as u64; }
        }

        _ => {
            return Err("Unsupported relocation type");
        }
    }

    Ok(())
}

/// Relocation iterator for RELA sections
pub struct RelaIterator<'a> {
    data: &'a [u8],
    offset: usize,
    entry_size: usize,
}

impl<'a> RelaIterator<'a> {
    /// Create new RELA iterator
    pub fn new(data: &'a [u8], entry_size: usize) -> Self {
        RelaIterator {
            data,
            offset: 0,
            entry_size: if entry_size == 0 { 24 } else { entry_size },
        }
    }
}

impl<'a> Iterator for RelaIterator<'a> {
    type Item = Relocation;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset + self.entry_size > self.data.len() {
            return None;
        }

        let reloc = Relocation::parse_rela(&self.data[self.offset..])?;
        self.offset += self.entry_size;
        Some(reloc)
    }
}
