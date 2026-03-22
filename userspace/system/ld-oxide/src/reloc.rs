//! Relocation application for ld-oxide (userspace)
//!
//! — WireSaint: this is the userspace counterpart to kernel/libc-support/dl/src/reloc.rs.
//! Uses #[cfg(target_arch)] instead of traits since ld-oxide compiles for exactly one
//! target architecture at a time. No trait overhead, no vtable indirection.

use crate::elf::{Elf64Rela, Elf64Sym};

/// x86_64 relocation type constants
/// — WireSaint: from the x86_64 ABI supplement, Table 4.9
#[cfg(target_arch = "x86_64")]
pub mod r_type {
    pub const R_X86_64_NONE: u32 = 0;
    pub const R_X86_64_64: u32 = 1;
    pub const R_X86_64_PC32: u32 = 2;
    pub const R_X86_64_GLOB_DAT: u32 = 6;
    pub const R_X86_64_JUMP_SLOT: u32 = 7;
    pub const R_X86_64_RELATIVE: u32 = 8;
    pub const R_X86_64_DTPMOD64: u32 = 16;
    pub const R_X86_64_DTPOFF64: u32 = 17;
    pub const R_X86_64_TPOFF64: u32 = 18;
    pub const R_X86_64_IRELATIVE: u32 = 37;
}

/// Apply a single relocation entry.
///
/// — WireSaint: `base` is the load base (actual_load_addr - min_vaddr).
/// For PIE executables and .so files, all vaddrs are relative to this base.
/// `sym_value` is the resolved absolute address of the symbol (0 if unresolved).
///
/// # Safety
/// Writes directly to the target address. Caller must ensure the page is writable.
#[cfg(target_arch = "x86_64")]
pub unsafe fn apply_relocation(
    base: u64,
    rela: &Elf64Rela,
    sym_value: u64,
) {
    use r_type::*;

    let target = (base + rela.r_offset) as *mut u64;
    let rtype = rela.r_type();

    match rtype {
        R_X86_64_NONE => {}

        R_X86_64_64 => {
            // S + A
            let value = sym_value.wrapping_add(rela.r_addend as u64);
            unsafe { *target = value; }
        }

        R_X86_64_PC32 => {
            // S + A - P
            let p = target as u64;
            let value = (sym_value as i64)
                .wrapping_add(rela.r_addend)
                .wrapping_sub(p as i64);
            unsafe { *(target as *mut i32) = value as i32; }
        }

        R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
            // S
            unsafe { *target = sym_value; }
        }

        R_X86_64_RELATIVE => {
            // B + A
            let value = (base as i64).wrapping_add(rela.r_addend) as u64;
            unsafe { *target = value; }
        }

        R_X86_64_TPOFF64 => {
            // S + A (TLS offset from thread pointer)
            // — WireSaint: for static TLS (initial exec model). The value is
            // a negative offset from the thread pointer in Variant II.
            let value = sym_value.wrapping_add(rela.r_addend as u64);
            unsafe { *target = value; }
        }

        R_X86_64_IRELATIVE => {
            // — WireSaint: call the resolver function at B + A, use its return value
            let func_addr = (base as i64).wrapping_add(rela.r_addend) as u64;
            let resolver: extern "C" fn() -> u64 = unsafe { core::mem::transmute(func_addr) };
            let resolved = resolver();
            unsafe { *target = resolved; }
        }

        _ => {
            // — WireSaint: unknown relocation — skip it rather than crash.
            // The kernel-side dl already handles exotic types. We only need
            // the common ones for basic dynamic linking.
        }
    }
}

/// Set up GOT for PLT lazy binding.
/// — WireSaint: GOT[0] = .dynamic address, GOT[1] = cookie (lib base),
/// GOT[2] = dl_runtime_resolve trampoline address.
/// This enables lazy PLT resolution: first call to an unresolved PLT stub
/// triggers the resolver which patches the GOT entry in-place.
/// Pure Rust — no arch-specific code (just pointer writes).
pub unsafe fn setup_got_plt(pltgot: u64, cookie: u64, resolver: u64) {
    if pltgot == 0 { return; }
    let got = pltgot as *mut u64;
    unsafe { *got.add(1) = cookie; }
    unsafe { *got.add(2) = resolver; }
}

/// Apply all RELA relocations from a section.
///
/// — WireSaint: processes relocations in order. RELATIVE relocations (no symbol
/// lookup needed) are the most common in PIE/shared objects and are fast.
/// GLOB_DAT/JUMP_SLOT require symbol resolution via the callback.
///
/// # Safety
/// Caller must ensure `rela_addr` points to valid RELA entries and `count` is correct.
#[cfg(target_arch = "x86_64")]
pub unsafe fn apply_rela_section(
    base: u64,
    rela_addr: u64,
    count: usize,
    entry_size: usize,
    resolve_sym: impl Fn(u32) -> u64,
) {
    for i in 0..count {
        let rela_ptr = (rela_addr + (i * entry_size) as u64) as *const Elf64Rela;
        let rela = unsafe { &*rela_ptr };

        let sym_value = if rela.r_sym() != 0 {
            resolve_sym(rela.r_sym())
        } else {
            0
        };

        unsafe { apply_relocation(base, rela, sym_value); }
    }
}
