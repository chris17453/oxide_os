//! x86_64 Exec Configuration, TLS Layout, Process Context Ops, and ELF Relocations
//!
//! — BlackLatch: All the x86_64-specific magic that exec.rs used to hardcode inline.
//! Canonical 48-bit addresses, Variant II TLS, rflags=0x202, cs=0x23/ss=0x1B — it's
//! all here in one file instead of scattered across the kernel like shrapnel.

use arch_traits::{
    ElfRelocation, ExecConfig, ProcessContextOps, TlsLayout, TlsVariant,
};

/// x86_64 address space layout constants
/// — ColdCipher: canonical 48-bit virtual addresses. Bit 47 sign-extended means
/// user space lives in 0x0000_0000_0000_0000..0x0000_7FFF_FFFF_FFFF and kernel
/// in 0xFFFF_8000_0000_0000..0xFFFF_FFFF_FFFF_FFFF. The 0x8000_0000_0000 limit
/// is the boundary between the two halves.
impl ExecConfig for super::X86_64 {
    /// Stack sits just below the 128TB user limit. Room for ASLR to slide down.
    const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;

    /// mmap region starts 512GB below the stack — plenty of room for both
    const MMAP_BASE_DEFAULT: u64 = 0x0000_7000_0000_0000;

    /// TLS lives below mmap so they can't collide. One page below mmap base.
    const TLS_BASE: u64 = 0x0000_6FFF_F000_0000;

    /// Anything at or above this is kernel territory
    const USER_ADDR_LIMIT: u64 = 0x0000_8000_0000_0000;

    /// EM_X86_64
    const ELF_MACHINE_EXEC: u16 = 0x3E;
}

/// x86_64 TLS Variant II layout
/// — GraveShift: TP (thread pointer, stored in FS register) points to the END
/// of the TLS block, right at the TCB. Compiler generates negative offsets from
/// FS to reach TLS data: `%fs:-8` for the first TLS variable. The TCB's first
/// qword is a self-pointer (%fs:0 == TP) — glibc/musl depend on this for
/// __tls_get_addr fast paths.
impl TlsLayout for super::X86_64 {
    const VARIANT: TlsVariant = TlsVariant::VariantII;

    /// 64 bytes: self-pointer (8) + DTV pointer (8) + padding/future use (48)
    /// — GraveShift: musl only needs 8 bytes for the self-pointer, but glibc
    /// expects a full TCB with DTV, canary, and more. 64 bytes covers both
    /// without wasting a whole page.
    const TCB_SIZE: usize = 64;

    #[inline]
    fn thread_pointer(alloc_base: u64, mem_size: usize) -> u64 {
        // — GraveShift: Variant II — TP = alloc_base + TLS data size
        // Data lives at [alloc_base .. alloc_base + mem_size)
        // TCB lives at [alloc_base + mem_size .. alloc_base + mem_size + TCB_SIZE)
        // FS register = alloc_base + mem_size = start of TCB
        alloc_base + mem_size as u64
    }

    #[inline]
    fn tls_data_offset() -> usize {
        // — GraveShift: Variant II — TLS init data goes at the START of the allocation
        // (before TCB). Compiler accesses via negative offsets from TP.
        0
    }

    #[inline]
    fn tcb_self_pointer_offset() -> usize {
        // — GraveShift: %fs:0 must read back the TP value itself. Offset 0 from TCB.
        0
    }
}

/// x86_64 process context type and operations for exec/fork
/// — SableWire: rflags=0x202 means IF (interrupt flag) set + reserved bit 1.
/// cs=0x23 is user code (ring 3, GDT index 4). ss=0x1B is user data (ring 3,
/// GDT index 3). Get these wrong and you either triple-fault or run user code
/// in ring 0. Neither is a career-enhancing move.
impl ProcessContextOps for super::X86_64 {
    type Context = super::X86_64ProcessContext;

    #[inline]
    fn new_user_context(entry: u64, sp: u64, tls_base: u64) -> Self::Context {
        let mut ctx = Self::Context::default();
        ctx.rip = entry;
        ctx.rsp = sp;
        ctx.rflags = 0x202; // IF set + reserved bit 1
        ctx.cs = 0x23; // User code segment (ring 3)
        ctx.ss = 0x1B; // User data segment (ring 3)
        ctx.fs_base = tls_base;
        // — SableWire: SysV ABI says all GP regs are undefined at program start.
        // Zero them for security (no kernel data leaks) and reproducibility.
        // RDI/RSI/RDX specifically zeroed — program reads argc/argv from stack,
        // NOT from registers. Only libc _start wrappers use registers for args.
        ctx.rdi = 0;
        ctx.rsi = 0;
        ctx.rdx = 0;
        ctx
    }

    #[inline]
    fn get_ip(ctx: &Self::Context) -> u64 {
        ctx.rip
    }

    #[inline]
    fn set_ip(ctx: &mut Self::Context, ip: u64) {
        ctx.rip = ip;
    }

    #[inline]
    fn get_sp(ctx: &Self::Context) -> u64 {
        ctx.rsp
    }

    #[inline]
    fn set_sp(ctx: &mut Self::Context, sp: u64) {
        ctx.rsp = sp;
    }

    #[inline]
    fn set_tls_base(ctx: &mut Self::Context, tls_base: u64) {
        ctx.fs_base = tls_base;
    }
}

/// x86_64 process context — mirrors ProcessContext from proc crate
/// — SableWire: this is the arch-side definition. The proc crate's ProcessContext
/// type aliases to this via the ProcessContextOps trait.
#[derive(Debug, Clone, Default)]
pub struct X86_64ProcessContext {
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub cs: u64,
    pub ss: u64,
    pub fs_base: u64,
    pub gs_base: u64,
}

/// x86_64 ELF relocation implementation
/// — WireSaint: every relocation formula from the x86_64 ABI supplement, section 4.4.
/// S=symbol, A=addend, P=place, B=base, GOT=GOT base. The match arms are the same
/// as dl/reloc.rs — this is the canonical implementation, dl/reloc.rs dispatches here.
impl ElfRelocation for super::X86_64 {
    fn apply_relocation(
        base: usize,
        offset: u64,
        r_type_raw: u32,
        sym_value: usize,
        addend: i64,
        got: usize,
    ) -> Result<(), &'static str> {
        let target = base + offset as usize;

        match r_type_raw {
            // R_X86_64_NONE
            0 => {}

            // R_X86_64_64: S + A
            1 => {
                let value = sym_value.wrapping_add(addend as usize);
                unsafe { *(target as *mut u64) = value as u64; }
            }

            // R_X86_64_PC32: S + A - P
            2 => {
                let value = (sym_value as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(target as i64);
                unsafe { *(target as *mut i32) = value as i32; }
            }

            // R_X86_64_PLT32: L + A - P
            4 => {
                let value = (sym_value as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(target as i64);
                unsafe { *(target as *mut i32) = value as i32; }
            }

            // R_X86_64_COPY
            5 => {
                return Err("R_X86_64_COPY not supported in apply_relocation");
            }

            // R_X86_64_GLOB_DAT (6) / R_X86_64_JUMP_SLOT (7): S
            6 | 7 => {
                unsafe { *(target as *mut u64) = sym_value as u64; }
            }

            // R_X86_64_RELATIVE: B + A
            8 => {
                let value = (base as i64).wrapping_add(addend) as u64;
                unsafe { *(target as *mut u64) = value; }
            }

            // R_X86_64_GOTPCREL (9) / GOTPCRELX (41) / REX_GOTPCRELX (42)
            9 | 41 | 42 => {
                let value = (got as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(target as i64);
                unsafe { *(target as *mut i32) = value as i32; }
            }

            // R_X86_64_32: S + A (zero-extended)
            10 => {
                let value = sym_value.wrapping_add(addend as usize) as u32;
                unsafe { *(target as *mut u32) = value; }
            }

            // R_X86_64_32S: S + A (sign-extended)
            11 => {
                let value = (sym_value as i64).wrapping_add(addend) as i32;
                unsafe { *(target as *mut i32) = value; }
            }

            // R_X86_64_IRELATIVE: indirect function resolver — (*)(B + A)()
            37 => {
                let func_addr = (base as i64).wrapping_add(addend) as usize;
                let resolver: extern "C" fn() -> usize =
                    unsafe { core::mem::transmute(func_addr) };
                let resolved = resolver();
                unsafe { *(target as *mut u64) = resolved as u64; }
            }

            _ => {
                return Err("Unsupported x86_64 relocation type");
            }
        }

        Ok(())
    }

    #[inline]
    fn is_none_reloc(r_type: u32) -> bool {
        r_type == 0 // R_X86_64_NONE
    }

    #[inline]
    fn is_relative_reloc(r_type: u32) -> bool {
        r_type == 8 // R_X86_64_RELATIVE
    }

    #[inline]
    fn is_got_reloc(r_type: u32) -> bool {
        matches!(r_type, 3 | 6 | 9 | 41 | 42) // GOT32, GLOB_DAT, GOTPCREL, GOTPCRELX, REX_GOTPCRELX
    }
}
