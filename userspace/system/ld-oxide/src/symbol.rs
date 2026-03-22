//! Symbol resolution for ld-oxide
//!
//! — ThreadRogue: the matchmaker between "I need printf" and "printf lives at 0x7f..."
//! Uses both ELF hash and GNU hash for lookup. Global symbol scope is searched
//! breadth-first across all loaded objects (main exe first, then .so files in
//! DT_NEEDED order).

use crate::elf::Elf64Sym;

/// ELF hash function (DT_HASH)
/// — ThreadRogue: the classic Sys V hash. O(n/buckets) lookup.
/// GNU hash is faster but some old .so files only have DT_HASH.
pub fn elf_hash(name: &[u8]) -> u32 {
    let mut h: u32 = 0;
    for &byte in name {
        h = (h << 4).wrapping_add(byte as u32);
        let g = h & 0xF000_0000;
        if g != 0 {
            h ^= g >> 24;
        }
        h &= !g;
    }
    h
}

/// GNU hash function (DT_GNU_HASH)
/// — ThreadRogue: bloom filter + bucket chain. Much faster than ELF hash for
/// miss-heavy workloads (which is most of dynamic linking — you look up a symbol
/// in 5 libraries, it's only defined in 1).
pub fn gnu_hash(name: &[u8]) -> u32 {
    let mut h: u32 = 5381;
    for &byte in name {
        h = h.wrapping_mul(33).wrapping_add(byte as u32);
    }
    h
}

/// Look up a symbol by name in a symbol table using ELF hash.
///
/// — ThreadRogue: `hash_table` points to the DT_HASH structure:
///   [nbucket: u32] [nchain: u32] [bucket[nbucket]: u32...] [chain[nchain]: u32...]
/// `symtab` points to the .dynsym table. `strtab` points to .dynstr.
///
/// Returns the symbol's st_value (already adjusted by caller to absolute address)
/// or None if not found.
pub unsafe fn lookup_elf_hash(
    name: &[u8],
    hash_table: u64,
    symtab: u64,
    strtab: u64,
    syment: u64,
) -> Option<&'static Elf64Sym> {
    let hash_ptr = hash_table as *const u32;
    let nbucket = unsafe { *hash_ptr } as usize;
    let _nchain = unsafe { *hash_ptr.add(1) } as usize;
    let buckets = unsafe { hash_ptr.add(2) };
    let chains = unsafe { buckets.add(nbucket) };

    let h = elf_hash(name);
    let mut idx = unsafe { *buckets.add((h as usize) % nbucket) } as usize;

    while idx != 0 {
        let sym = unsafe { &*((symtab + idx as u64 * syment) as *const Elf64Sym) };
        let sym_name_offset = sym.st_name as u64;
        let sym_name = unsafe { crate::elf::strtab_get(strtab, sym_name_offset) };

        if sym_name == name && sym.st_shndx != 0 {
            return Some(sym);
        }

        idx = unsafe { *chains.add(idx) } as usize;
    }

    None
}

/// Look up a symbol by name using GNU hash (DT_GNU_HASH).
///
/// — ThreadRogue: GNU hash structure:
///   [nbuckets: u32] [symoffset: u32] [bloom_size: u32] [bloom_shift: u32]
///   [bloom[bloom_size]: u64...]
///   [buckets[nbuckets]: u32...]
///   [chain values...]
///
/// The bloom filter provides fast rejection, buckets index into the chain,
/// and the chain uses the hash's low bit to terminate walks.
pub unsafe fn lookup_gnu_hash(
    name: &[u8],
    gnu_hash_table: u64,
    symtab: u64,
    strtab: u64,
    syment: u64,
) -> Option<&'static Elf64Sym> {
    let base = gnu_hash_table as *const u32;
    let nbuckets = unsafe { *base } as usize;
    let symoffset = unsafe { *base.add(1) } as usize;
    let bloom_size = unsafe { *base.add(2) } as usize;
    let bloom_shift = unsafe { *base.add(3) } as usize;

    if nbuckets == 0 || bloom_size == 0 { return None; }

    let h = gnu_hash(name);

    // — ThreadRogue: bloom filter check (64-bit words on x86_64)
    let bloom = (base as *const u64).add(2); // skip 4 u32 header = 2 u64
    let bloom_word = unsafe { *bloom.add((h as usize / 64) % bloom_size) };
    let mask = (1u64 << (h % 64)) | (1u64 << ((h >> bloom_shift) % 64));
    if bloom_word & mask != mask {
        return None; // — ThreadRogue: definite miss
    }

    // — ThreadRogue: find bucket
    let buckets = (bloom.add(bloom_size)) as *const u32;
    let bucket_idx = (h as usize) % nbuckets;
    let mut sym_idx = unsafe { *buckets.add(bucket_idx) } as usize;
    if sym_idx == 0 { return None; }

    // — ThreadRogue: walk the chain
    let chains = unsafe { buckets.add(nbuckets) };
    loop {
        let chain_val = unsafe { *chains.add(sym_idx - symoffset) };
        // — ThreadRogue: compare hash values (top 31 bits)
        if (chain_val | 1) == (h | 1) {
            // Hash match — verify name
            let sym = unsafe { &*((symtab + sym_idx as u64 * syment) as *const Elf64Sym) };
            let sym_name = unsafe { crate::elf::strtab_get(strtab, sym.st_name as u64) };
            if sym_name == name && sym.st_shndx != 0 {
                return Some(sym);
            }
        }
        // — ThreadRogue: bit 0 set means end of chain
        if chain_val & 1 != 0 { break; }
        sym_idx += 1;
    }

    None
}

/// Compare two byte slices for equality
/// — ThreadRogue: can't use slice::eq in no_std without pulling in more deps
fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

/// Look up a symbol by index in a symbol table.
///
/// — ThreadRogue: direct index lookup (no hashing). Used when we already know
/// the symbol index from a relocation entry's r_sym field.
pub unsafe fn get_sym_by_index(
    symtab: u64,
    syment: u64,
    index: u32,
) -> &'static Elf64Sym {
    unsafe { &*((symtab + index as u64 * syment) as *const Elf64Sym) }
}

/// Get the name of a symbol from the string table.
pub unsafe fn get_sym_name(strtab: u64, sym: &Elf64Sym) -> &'static [u8] {
    unsafe { crate::elf::strtab_get(strtab, sym.st_name as u64) }
}
