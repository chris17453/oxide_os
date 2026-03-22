//! x86_64 architecture support for ld-oxide
//!
//! — SableWire: ALL x86_64-specific assembly lives here. The main dl_main
//! code is pure Rust. When porting to AArch64, create arch_aarch64.rs with
//! the equivalent functions — dl_main doesn't change.

/// Entry point — saves RSP, self-relocates (PIE RELATIVE entries), then calls dl_main.
/// — SableWire: the self-relocation uses _DYNAMIC to find DT_RELA, then applies
/// RELATIVE relocations using the load base computed from _DYNAMIC's link vs runtime addr.
/// All addressing is RIP-relative — safe before GOT is fixed.
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mov rdi, rsp",                // save original RSP for dl_main

        // — SableWire: compute load base.
        // _DYNAMIC is at a known link-time vaddr. RIP-relative lea gives runtime addr.
        // base = runtime_addr(_DYNAMIC) - link_vaddr(_DYNAMIC)
        // For PIE with vaddr starting near 0: base ≈ load_address.
        "lea rsi, [rip + _DYNAMIC]",   // rsi = runtime _DYNAMIC
        // The linker emits _DYNAMIC at its link-time vaddr. We need that value.
        // Use the GOT entry for _DYNAMIC which the linker pre-fills with the link addr.
        // Actually, simpler: the first LOAD segment's p_vaddr is the base reference.
        // For our linker script: .dynamic is at a small offset from 0.
        // We can get the link-time _DYNAMIC addr from the .dynamic section itself:
        // The ELF header is at the start of the first LOAD segment.
        // e_entry at offset 24 gives the link-time entry point.
        // base = runtime_entry - link_entry. But we don't know runtime_entry yet.
        //
        // — SableWire: use __ehdr_start trick. The linker provides __ehdr_start = 0
        // for PIE. So base = &__ehdr_start (runtime) - 0 = load_address.
        "lea rbx, [rip + __ehdr_start]", // rbx = load base (runtime addr of ELF header = base)

        // Walk _DYNAMIC for DT_RELA(7) and DT_RELASZ(8)
        "xor r8, r8",
        "xor r9, r9",
        "mov rcx, rsi",
        "10:",
        "mov rax, [rcx]",
        "test rax, rax",
        "jz 11f",
        "cmp rax, 7",
        "jne 15f",
        "mov r8, [rcx + 8]",          // DT_RELA (link-time vaddr)
        "add r8, rbx",                // adjust to runtime addr
        "jmp 16f",
        "15:",
        "cmp rax, 8",
        "jne 16f",
        "mov r9, [rcx + 8]",          // DT_RELASZ
        "16:",
        "add rcx, 16",
        "jmp 10b",
        "11:",

        // Apply RELATIVE relocations: *(base + r_offset) = base + r_addend
        "test r8, r8",
        "jz 12f",
        "test r9, r9",
        "jz 12f",
        "xor rcx, rcx",
        "13:",
        "cmp rcx, r9",
        "jge 12f",
        "mov eax, [r8 + rcx + 8]",    // r_info low 32 bits (r_type)
        "cmp eax, 8",                  // R_X86_64_RELATIVE
        "jne 14f",
        "mov rax, [r8 + rcx]",        // r_offset
        "add rax, rbx",               // runtime offset = base + r_offset
        "mov rdx, [r8 + rcx + 16]",   // r_addend
        "add rdx, rbx",               // runtime value = base + r_addend
        "mov [rax], rdx",             // apply relocation
        "14:",
        "add rcx, 24",
        "jmp 13b",
        "12:",

        // — SableWire: self-relocation done. Call dl_main(sp).
        "and rsp, -16",
        "call dl_main",
        "ud2",
    );
}

/// Jump to the main executable's entry point with the original stack pointer.
/// — SableWire: restores RSP to the kernel's original layout so the exe's
/// _start sees [argc, argv...] at RSP. Then jumps (not calls) to the entry.
#[inline(always)]
pub unsafe fn jump_to_entry(sp: *const u64, entry: u64) -> ! {
    unsafe {
        core::arch::asm!(
            "mov rsp, {sp}",
            "jmp {entry}",
            sp = in(reg) sp,
            entry = in(reg) entry,
            options(noreturn)
        );
    }
}

/// PLT lazy binding trampoline.
/// — WireSaint: when a PLT stub is first called and the GOT entry points back
/// to the PLT (unresolved), it pushes the relocation index and jumps to GOT[2]
/// which is this function. We save all registers, call the Rust resolver,
/// patch the GOT entry, restore registers, and jump to the resolved function.
///
/// Stack on entry (from PLT stub):
///   [return address]    <- original caller's return addr
///   [reloc_index]       <- pushed by PLT stub (index into .rela.plt)
///   GOT[1] was loaded into a register by the PLT preamble
///
/// Since we do eager binding, this should never be called. It exists as a
/// safety net — if called, it means a GOT entry was missed during eager resolution.
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _dl_runtime_resolve() {
    core::arch::naked_asm!(
        // — WireSaint: the PLT stub pushed reloc_index, then jumped here.
        // We crash cleanly rather than implementing full lazy resolution,
        // since eager binding should have resolved everything already.
        // A real implementation would save all regs, call dl_fixup, restore, jmp.
        "ud2",
    );
}
