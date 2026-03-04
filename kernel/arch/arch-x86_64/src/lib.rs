//! OXIDE x86_64 Architecture Implementation
//!
//! Provides x86_64-specific implementations of architecture traits.

#![no_std]
#![allow(unused)]

extern crate ps2;

use arch_traits::{
    Arch, AtomicOps, CacheOps, ControlRegisters, DmaOps, Endianness, ExceptionHandler,
    InterruptContext as ArchInterruptContext, PortIo, SyscallInterface, SystemRegisters,
    TlbControl,
};
use os_core::{PhysAddr, VirtAddr};

pub mod ap_boot;
pub mod apic;
pub mod context;
pub mod exceptions;
pub mod gdt;
pub mod idt;
pub mod serial;
pub mod syscall;
pub mod usermode;

/// Return the current hardware CPU identifier (APIC ID on x86_64)
pub fn cpu_id() -> Option<u32> {
    Some(apic::id() as u32)
}

/// x86_64 architecture implementation
pub struct X86_64;

impl Arch for X86_64 {
    fn name() -> &'static str {
        "x86_64"
    }

    fn page_size() -> usize {
        4096
    }

    fn kernel_base() -> VirtAddr {
        VirtAddr::new(0xFFFF_FFFF_8000_0000)
    }

    fn halt() -> ! {
        loop {
            unsafe {
                core::arch::asm!("hlt");
            }
        }
    }

    fn disable_interrupts() {
        unsafe {
            core::arch::asm!("cli", options(nomem, nostack));
        }
    }

    fn enable_interrupts() {
        unsafe {
            core::arch::asm!("sti", options(nomem, nostack));
        }
    }

    fn interrupts_enabled() -> bool {
        let flags: u64;
        unsafe {
            core::arch::asm!(
                "pushfq",
                "pop {}",
                out(reg) flags,
                options(nomem)
            );
        }
        // IF flag is bit 9
        (flags & (1 << 9)) != 0
    }
}

impl TlbControl for X86_64 {
    #[inline]
    fn flush(addr: VirtAddr) {
        unsafe {
            core::arch::asm!("invlpg [{}]", in(reg) addr.as_u64(), options(nostack, preserves_flags));
        }
    }

    #[inline]
    fn flush_all() {
        unsafe {
            let cr3: u64;
            core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
            core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack));
        }
    }

    #[inline]
    fn read_root() -> PhysAddr {
        let cr3: u64;
        unsafe {
            core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
        }
        PhysAddr::new(cr3 & 0x000F_FFFF_FFFF_F000)
    }

    #[inline]
    unsafe fn write_root(root: PhysAddr) {
        unsafe {
            core::arch::asm!("mov cr3, {}", in(reg) root.as_u64(), options(nostack));
        }
    }
}

impl PortIo for X86_64 {
    #[inline]
    unsafe fn inb(port: u16) -> u8 {
        unsafe { inb(port) }
    }

    #[inline]
    unsafe fn outb(port: u16, value: u8) {
        unsafe { outb(port, value) }
    }

    #[inline]
    unsafe fn inw(port: u16) -> u16 {
        unsafe { inw(port) }
    }

    #[inline]
    unsafe fn outw(port: u16, value: u16) {
        unsafe { outw(port, value) }
    }

    #[inline]
    unsafe fn inl(port: u16) -> u32 {
        unsafe { inl(port) }
    }

    #[inline]
    unsafe fn outl(port: u16, value: u32) {
        unsafe { outl(port, value) }
    }
}

// ============================================================================
// Control Registers Implementation
// — GraveShift
// ============================================================================

impl ControlRegisters for X86_64 {
    type PageTableRoot = PhysAddr;

    #[inline]
    fn read_page_table_root() -> Self::PageTableRoot {
        let cr3: u64;
        unsafe {
            core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
        }
        PhysAddr::new(cr3 & 0x000F_FFFF_FFFF_F000)
    }

    #[inline]
    unsafe fn write_page_table_root(root: Self::PageTableRoot) {
        unsafe {
            core::arch::asm!("mov cr3, {}", in(reg) root.as_u64(), options(nostack));
        }
    }

    #[inline]
    fn read_instruction_pointer() -> u64 {
        // On x86_64, RIP can only be read relative to current position
        // Use a dummy call to get approximate RIP
        let rip: u64;
        unsafe {
            core::arch::asm!(
                "lea {}, [rip]",
                out(reg) rip,
                options(nomem, nostack, preserves_flags)
            );
        }
        rip
    }

    #[inline]
    fn read_stack_pointer() -> u64 {
        let rsp: u64;
        unsafe {
            core::arch::asm!(
                "mov {}, rsp",
                out(reg) rsp,
                options(nomem, nostack, preserves_flags)
            );
        }
        rsp
    }
}

// ============================================================================
// System Registers (MSR) Implementation
// — GraveShift
// ============================================================================

impl SystemRegisters for X86_64 {
    #[inline]
    unsafe fn read_sys_reg(id: u32) -> u64 {
        let low: u32;
        let high: u32;
        unsafe {
            core::arch::asm!(
                "rdmsr",
                in("ecx") id,
                out("eax") low,
                out("edx") high,
                options(nomem, nostack, preserves_flags)
            );
        }
        ((high as u64) << 32) | (low as u64)
    }

    #[inline]
    unsafe fn write_sys_reg(id: u32, value: u64) {
        let low = value as u32;
        let high = (value >> 32) as u32;
        unsafe {
            core::arch::asm!(
                "wrmsr",
                in("ecx") id,
                in("eax") low,
                in("edx") high,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

// ============================================================================
// Endianness Implementation (x86_64 is little-endian)
// — NeonRoot
// ============================================================================

impl Endianness for X86_64 {
    #[inline]
    fn is_big_endian() -> bool {
        false
    }

    #[inline]
    fn is_little_endian() -> bool {
        true
    }

    // TO little-endian (no-op on x86_64)
    #[inline]
    fn to_le16(val: u16) -> u16 {
        val
    }

    #[inline]
    fn to_le32(val: u32) -> u32 {
        val
    }

    #[inline]
    fn to_le64(val: u64) -> u64 {
        val
    }

    // FROM little-endian (no-op on x86_64)
    #[inline]
    fn from_le16(val: u16) -> u16 {
        val
    }

    #[inline]
    fn from_le32(val: u32) -> u32 {
        val
    }

    #[inline]
    fn from_le64(val: u64) -> u64 {
        val
    }

    // TO big-endian (swap on x86_64)
    #[inline]
    fn to_be16(val: u16) -> u16 {
        val.swap_bytes()
    }

    #[inline]
    fn to_be32(val: u32) -> u32 {
        val.swap_bytes()
    }

    #[inline]
    fn to_be64(val: u64) -> u64 {
        val.swap_bytes()
    }

    // FROM big-endian (swap on x86_64)
    #[inline]
    fn from_be16(val: u16) -> u16 {
        val.swap_bytes()
    }

    #[inline]
    fn from_be32(val: u32) -> u32 {
        val.swap_bytes()
    }

    #[inline]
    fn from_be64(val: u64) -> u64 {
        val.swap_bytes()
    }
}

// ============================================================================
// Cache Operations (x86_64 has hardware cache coherency)
// — WireSaint
// ============================================================================

impl CacheOps for X86_64 {
    #[inline]
    unsafe fn flush_cache() {
        // WBINVD - Write back and invalidate all caches
        unsafe {
            core::arch::asm!("wbinvd", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn flush_cache_range(_start: VirtAddr, _len: usize) {
        // On x86_64, cache is coherent with memory
        // CLFLUSH could be used for specific lines, but generally not needed
        // For compatibility with non-coherent architectures, we provide no-op
    }

    #[inline]
    unsafe fn invalidate_cache_range(_start: VirtAddr, _len: usize) {
        // x86_64 cache is coherent - no manual invalidation needed
    }

    #[inline]
    unsafe fn invalidate_icache() {
        // x86_64 has coherent instruction cache
        // Self-modifying code requires serializing instruction
        // Use MFENCE to serialize execution
        unsafe {
            core::arch::asm!("mfence", options(nomem, nostack, preserves_flags));
        }
    }

    #[inline]
    fn is_cache_coherent() -> bool {
        true
    }
}

// ============================================================================
// DMA Operations (x86_64 has coherent DMA)
// — WireSaint
// ============================================================================

impl DmaOps for X86_64 {
    #[inline]
    fn is_dma_coherent() -> bool {
        true
    }

    #[inline]
    unsafe fn dma_sync_for_device(_addr: PhysAddr, _len: usize) {
        // x86_64 DMA is coherent - no sync needed
    }

    #[inline]
    unsafe fn dma_sync_for_cpu(_addr: PhysAddr, _len: usize) {
        // x86_64 DMA is coherent - no sync needed
    }

    #[inline]
    unsafe fn dma_map(addr: VirtAddr, _len: usize) -> PhysAddr {
        // Simple identity mapping for now
        // In reality, we'd need to translate via page tables
        PhysAddr::new(addr.as_u64())
    }

    #[inline]
    unsafe fn dma_unmap(_addr: PhysAddr, _len: usize) {
        // x86_64 coherent DMA - nothing to unmap
    }
}

// ============================================================================
// Atomic Operations (x86_64 lock prefix)
// — RustViper
// ============================================================================

impl AtomicOps for X86_64 {
    #[inline]
    unsafe fn atomic_compare_exchange_64(ptr: *mut u64, old: u64, new: u64) -> u64 {
        let prev: u64;
        unsafe {
            core::arch::asm!(
                "lock cmpxchg qword ptr [{ptr}], {new}",
                ptr = in(reg) ptr,
                new = in(reg) new,
                inout("rax") old => prev,
                options(nostack)
            );
        }
        prev
    }

    #[inline]
    unsafe fn memory_barrier() {
        // MFENCE - Full memory barrier
        unsafe {
            core::arch::asm!("mfence", options(nomem, nostack, preserves_flags));
        }
    }

    #[inline]
    unsafe fn read_barrier() {
        // LFENCE - Load fence
        unsafe {
            core::arch::asm!("lfence", options(nomem, nostack, preserves_flags));
        }
    }

    #[inline]
    unsafe fn write_barrier() {
        // SFENCE - Store fence
        unsafe {
            core::arch::asm!("sfence", options(nomem, nostack, preserves_flags));
        }
    }
}

// ============================================================================
// Exception Handling Implementation
// — BlackLatch
// ============================================================================

impl ExceptionHandler for X86_64 {
    type ExceptionFrame = exceptions::InterruptFrame;
    type ExceptionVector = u8;

    #[inline]
    unsafe fn register_exception(vector: Self::ExceptionVector, handler: usize) {
        // Register handler in IDT
        // This delegates to the IDT module to set up the handler
        unsafe {
            idt::set_handler(vector, handler as u64);
        }
    }

    #[inline]
    unsafe fn init_exceptions() {
        // Initialize IDT and exception handlers
        // This is already done in init(), but we provide the trait method
        unsafe {
            idt::init();
        }
    }

    fn exception_context_from_frame(frame: &Self::ExceptionFrame) -> ArchInterruptContext {
        // Convert x86_64 interrupt frame to architecture-agnostic context
        // Fill in what we can from the frame
        let mut general_purpose = [0u64; 32];
        // x86_64 has only 16 GP registers, leave rest as 0

        ArchInterruptContext {
            general_purpose,
            instruction_pointer: frame.rip,
            stack_pointer: frame.rsp,
            flags: frame.rflags,
            arch_specific: [
                frame.cs, // Code segment
                frame.ss, // Stack segment
                0, 0, 0, 0, 0, 0,
            ],
        }
    }
}

// ============================================================================
// Syscall Interface Implementation
// — ThreadRogue
// ============================================================================

impl SyscallInterface for X86_64 {
    type SyscallFrame = syscall::SyscallUserContext;

    #[inline]
    unsafe fn init_syscall_mechanism() {
        // Initialize syscall/sysret MSRs
        unsafe {
            syscall::init();
        }
    }

    #[inline]
    fn syscall_entry_point() -> usize {
        // Return address of syscall entry function
        // This is set up in syscall::init() via LSTAR MSR
        syscall::syscall_entry as *const () as usize
    }

    fn syscall_number(frame: &Self::SyscallFrame) -> usize {
        // Syscall number is in RAX
        frame.rax as usize
    }

    fn syscall_args(frame: &Self::SyscallFrame) -> [usize; 6] {
        // x86_64 syscall ABI: RDI, RSI, RDX, R10, R8, R9
        [
            frame.rdi as usize,
            frame.rsi as usize,
            frame.rdx as usize,
            frame.r10 as usize,
            frame.r8 as usize,
            frame.r9 as usize,
        ]
    }

    fn set_syscall_return(frame: &mut Self::SyscallFrame, value: usize) {
        // Return value goes in RAX
        frame.rax = value as u64;
    }
}

/// Write a byte to an I/O port
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Read a byte from an I/O port
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

/// Write a word to an I/O port
#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Read a word from an I/O port
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        core::arch::asm!(
            "in ax, dx",
            in("dx") port,
            out("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

/// Write a dword to an I/O port
#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Read a dword from an I/O port
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {
        core::arch::asm!(
            "in eax, dx",
            in("dx") port,
            out("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

/// Print to console (for use in arch crate)
/// — PatchBay: Renamed from serial_print but now routes to os_log → console
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        struct OsLogWriter;
        impl Write for OsLogWriter {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                unsafe {
                    os_log::write_str_raw(s);
                }
                Ok(())
            }
        }
        let mut w = OsLogWriter;
        let _ = write!(w, $($arg)*);
    }};
}

/// Print to console with newline (for use in arch crate)
/// — PatchBay: Renamed from serial_println but now routes to os_log → console
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => {{
        $crate::serial_print!($($arg)*);
        $crate::serial_print!("\n");
    }};
}

// ============================================================================
// Per-CPU double-fault IST stacks
//
// — WireSaint: Each CPU needs its own double-fault stack in its own TSS.
// If two CPUs share an IST stack and both double-fault simultaneously,
// they corrupt each other's fault frame. That would be spectacular.
// Spectacular in the "QEMU silent exit" way. Not the fireworks way.
// ============================================================================

/// Page size for IST stack sizing
const DF_STACK_PAGES: usize = 5; // 20KB — double faults rarely recurse deep
const DF_STACK_SIZE: usize = 4096 * DF_STACK_PAGES;

/// Per-CPU double-fault IST stacks.
///
/// Static fixed-size arrays are ideal here — no heap needed, no allocation
/// failure possible. The BSP uses slot 0; each AP uses its logical cpu_id slot.
///
/// — WireSaint: This is 256 * 20KB = 5MB of static BSS. Fine for a kernel.
/// The alternative — heap allocating these — requires the heap to be up before
/// GDT init, which creates a circular dependency. Static wins.
static mut DF_STACKS: [[u8; DF_STACK_SIZE]; MAX_CPUS] = [[0; DF_STACK_SIZE]; MAX_CPUS];

// ============================================================================
// SMAP / SMEP — Supervisor Mode Access/Execution Prevention
//
// — ColdCipher: CR4.SMAP (bit 21) and CR4.SMEP (bit 20) are per-CPU control
// bits. Every CPU — BSP and every AP — must set them independently. Forgetting
// a single AP leaves it running naked: kernel code can freely touch user pages
// and execute user mappings. That's not a kernel. That's a suggestion.
//
// CPUID leaf 7, subleaf 0 (EAX=7, ECX=0):
//   EBX bit  7 → SMEP supported (CR4 bit 20)
//   EBX bit 20 → SMAP supported (CR4 bit 21)
//
// Precondition: P1.3 already baked AC into SFMASK (0x44700). Every syscall
// entry clears AC automatically, so SMAP is enforced from syscall entry until
// the dispatcher explicitly calls STAC. The CR4 bits just arm the hardware
// enforcement. Without them, STAC/CLAC are liturgy without a god — they do
// nothing. With them, an AC=0 kernel touch of user memory is a hard #PF.
// ============================================================================

/// CR4 bit 20 — Supervisor Mode Execution Prevention
const CR4_SMEP: u64 = 1 << 20;
/// CR4 bit 21 — Supervisor Mode Access Prevention
const CR4_SMAP: u64 = 1 << 21;

/// Enable SMEP and SMAP in CR4 on the calling CPU, if the CPU supports them.
///
/// — ColdCipher: Call this on every CPU during its init path. CR4 is per-CPU
/// hardware; setting it on the BSP does exactly nothing for the APs. Every
/// CPU that skips this call is a CPU that will happily dereference user pointers
/// in kernel mode without raising a finger. We do not allow that.
///
/// # Safety
/// Must be called with interrupts disabled or from single-threaded init context.
/// The CPUID check gates the CR4 write — if the CPU lacks the feature, we skip
/// the bit. This is safe even in heterogeneous virtual environments.
pub unsafe fn enable_smap_smep() {
    // — ColdCipher: CPUID leaf 7 / subleaf 0 is the structured extended feature
    // leaf. EBX returns the feature bits. We need both SMEP (bit 7) and SMAP
    // (bit 20). RBX is callee-saved in the SysV ABI but CPUID clobbers it, so
    // we push/pop around it to keep the compiler's register allocator sane.
    let ebx: u32;
    unsafe {
        core::arch::asm!(
            "push rbx",
            "mov eax, 7",
            "xor ecx, ecx",
            "cpuid",
            "mov {0:e}, ebx",
            "pop rbx",
            out(reg) ebx,
            out("eax") _,
            out("ecx") _,
            out("edx") _,
        );
    }

    let smep_supported = (ebx & (1 << 7)) != 0;
    let smap_supported = (ebx & (1 << 20)) != 0;

    if !smep_supported && !smap_supported {
        // — ColdCipher: Nothing to do. Probably running on something ancient or
        // a stripped-down QEMU config without these features. Not ideal, but we
        // don't GP-fault trying to set bits the CPU won't accept.
        return;
    }

    let mut cr4: u64;
    unsafe {
        core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack));
    }

    // — ColdCipher: SMEP first. Execution prevention before access prevention.
    // Belt AND suspenders. We don't trust user mappings to be inert.
    if smep_supported {
        cr4 |= CR4_SMEP;
    }
    if smap_supported {
        cr4 |= CR4_SMAP;
    }

    unsafe {
        // — ColdCipher: Writing CR4 is serializing on x86. No need for MFENCE.
        // From this instruction forward, the CPU enforces the new policy.
        // Any kernel code that was relying on silent user-page access just broke.
        // Good. It deserved to break.
        core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nostack));
    }
}

/// Initialize x86_64 architecture components for the BSP (CPU 0).
///
/// This sets up:
/// - Per-CPU GDT with TSS for CPU 0
/// - Double-fault IST stack for CPU 0
/// - IDT with exception handlers (shared, loaded by each CPU)
/// - Local APIC
/// - SMEP + SMAP in CR4 (P3.1: hardware enforcement of user-space isolation)
///
/// # Safety
/// Must only be called once, on the BSP (CPU 0), during kernel initialization.
pub unsafe fn init() {
    use core::ptr::addr_of_mut;

    unsafe {
        // Initialize per-CPU GDT + TSS for the BSP (cpu 0).
        // This calls gdt::init() which does register_cpu(0, 0) + init_cpu(0).
        gdt::init();
        serial_println!("[x86_64] GDT initialized (BSP cpu 0)");

        // Set up per-CPU IST stack for double faults (IST slot 0 = hardware IST1).
        // — WireSaint: DF_STACKS[0] is the BSP's double-fault stack.
        // Stack grows down, so top = base + size.
        let stack_top = addr_of_mut!(DF_STACKS[0]) as u64 + DF_STACK_SIZE as u64;
        gdt::set_ist(0, stack_top); // ist[0] = IST1 — used by double fault handler

        // Initialize IDT (shared structure; each CPU loads it via LIDT in idt::init())
        idt::init();
        serial_println!("[x86_64] IDT initialized");

        // — ColdCipher: P3.1 — Arm SMEP and SMAP on the BSP. SFMASK already
        // clears AC on every syscall entry (P1.3). Now we tell the hardware to
        // actually enforce it. CR4 bits 20 and 21, set and forgotten. Beautiful.
        enable_smap_smep();

        // Initialize APIC
        apic::init();
    }
}

/// Initialize x86_64 per-CPU components for an Application Processor.
///
/// Call this from the AP's init callback after gdt::init_cpu has already run.
/// Sets up this CPU's double-fault IST stack in its own TSS slot, and arms
/// SMEP + SMAP in this CPU's CR4.
///
/// # Safety
/// Must be called on the AP itself (not cross-CPU). cpu_id must be < MAX_CPUS.
/// Must be called after gdt::init_cpu(cpu_id).
///
/// — WireSaint: Keeps IST population DRY by mirroring what BSP init() does.
/// Each AP calls this once. Don't skip it or the double-fault handler gets a
/// zero IST address on that CPU and triple-faults on the first double fault.
/// — ColdCipher: CR4 is per-CPU. If you only set SMAP/SMEP on the BSP and
/// forget the APs, every AP-scheduled task runs without hardware memory isolation.
/// That's not a bug. That's a policy decision. A very bad one.
pub unsafe fn init_ap(cpu_id: usize) {
    use core::ptr::addr_of_mut;

    if cpu_id >= MAX_CPUS {
        return;
    }

    unsafe {
        // — WireSaint: Each AP slot in DF_STACKS is pre-zeroed (BSS).
        // Stack grows down: top of stack = base address + size.
        let stack_top = addr_of_mut!(DF_STACKS[cpu_id]) as u64 + DF_STACK_SIZE as u64;
        gdt::set_ist(0, stack_top); // ist[0] = IST1 for this CPU's double fault

        // — ColdCipher: P3.1 — Each AP needs its own CR4 write. The BSP's CR4
        // setting does not propagate to APs — they each boot with whatever the
        // trampoline left in CR4 (PAE only). SMEP and SMAP must be re-armed here.
        enable_smap_smep();
    }
}

/// Start the system timer for preemptive scheduling
pub fn start_timer(frequency_hz: u32) {
    apic::start_timer(frequency_hz);
}

/// Unmask keyboard and mouse IRQs in the IOAPIC
///
/// — BlackLatch: Call after `sti` so pending PS2 IRQs don't fire into
/// a half-initialized interrupt path.
pub fn unmask_io_irqs() {
    apic::unmask_io_irqs();
}

/// Get the current CPU's APIC ID.
///
/// — NeonRoot: Reads the LAPIC ID register directly. Safe from any context.
pub fn apic_id() -> u8 {
    apic::id()
}

/// Get current timer tick count
pub fn timer_ticks() -> u64 {
    exceptions::ticks()
}

/// Set the scheduler callback for preemptive context switching
///
/// The callback is called on each timer interrupt with the current RSP
/// and should return the RSP to restore from.
///
/// # Safety
/// The callback must be valid and handle context switching correctly.
pub unsafe fn set_scheduler_callback(callback: exceptions::SchedulerCallback) {
    unsafe {
        exceptions::set_scheduler_callback(callback);
    }
}

/// Register a terminal tick callback (called at ~30 FPS from timer interrupt)
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_terminal_tick_callback(callback: fn()) {
    unsafe {
        exceptions::set_terminal_tick_callback(callback);
    }
}

/// Initialize the PS/2 keyboard controller (i8042)
///
/// Must be called before keyboard input will work. UEFI firmware may
/// leave the PS/2 controller disabled after ExitBootServices.
pub fn init_ps2_keyboard() {
    exceptions::init_ps2_keyboard();
}

/// Get a scancode from the keyboard buffer
pub fn get_scancode() -> Option<u8> {
    exceptions::get_scancode()
}

/// Poll i8042 directly for a scancode (fallback when IRQ1 doesn't fire)
///
/// # Safety
/// Must only be called from interrupt context (e.g., timer ISR).
pub unsafe fn poll_keyboard() -> Option<u8> {
    unsafe { exceptions::poll_keyboard() }
}

/// Read a byte from the serial port (COM1) if available
pub fn serial_read() -> Option<u8> {
    serial::read_byte()
}

/// Read a byte from the serial port without locking (for interrupt handlers)
///
/// # Safety
/// Must only be called from interrupt context.
pub unsafe fn serial_read_unsafe() -> Option<u8> {
    unsafe { serial::read_byte_unsafe() }
}

/// Register a keyboard interrupt callback (called on keyboard IRQ)
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_keyboard_callback(callback: fn()) {
    unsafe {
        exceptions::set_keyboard_callback(callback);
    }
}

/// Register a mouse interrupt callback (called on mouse IRQ 12)
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_mouse_callback(callback: fn()) {
    unsafe {
        exceptions::set_mouse_callback(callback);
    }
}

/// Register a TLB shootdown IPI callback (called on IPI from other CPUs)
///
/// # Safety
/// The callback must be valid and thread-safe.
pub unsafe fn set_tlb_shootdown_callback(callback: fn()) {
    unsafe {
        exceptions::set_tlb_shootdown_callback(callback);
    }
}

/// Re-export syscall user context type and getter
pub use syscall::{SyscallUserContext, get_user_context};

/// Re-export usermode transition functions and types
pub use usermode::{
    UserContext, enter_usermode, enter_usermode_with_context, jump_to_usermode, return_to_usermode,
};

/// Read the Time Stamp Counter
#[inline]
pub fn read_tsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}

/// Cached TSC frequency — BSP calibrates via PIT, APs reuse the value.
/// The PIT is shared hardware (ports 0x42/0x43/0x61); concurrent access from
/// multiple APs corrupts the calibration and produces garbage frequency measurements.
/// — SableWire
static CACHED_TSC_FREQUENCY: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Calibrate TSC frequency using PIT reference
///
/// Returns TSC frequency in Hz. First caller (BSP) does real PIT calibration;
/// subsequent callers (APs) reuse cached result to avoid PIT data race.
/// — SableWire
pub fn calibrate_tsc() -> u64 {
    // SableWire: Fast path — return cached result if BSP already calibrated
    let cached = CACHED_TSC_FREQUENCY.load(core::sync::atomic::Ordering::Acquire);
    if cached != 0 {
        return cached;
    }

    const PIT_FREQUENCY: u32 = 1193182;
    const CALIBRATION_MS: u32 = 10;

    // Set up PIT channel 2 for calibration
    let pit_count = (PIT_FREQUENCY / 1000) * CALIBRATION_MS;

    unsafe {
        // Set PIT to one-shot mode, channel 2
        outb(0x61, (inb(0x61) & 0xFD) | 0x01); // Gate high, speaker off
        outb(0x43, 0xB0); // Channel 2, lobyte/hibyte, mode 0, binary

        // Set PIT count
        outb(0x42, (pit_count & 0xFF) as u8);
        outb(0x42, ((pit_count >> 8) & 0xFF) as u8);
    }

    // Read TSC at start
    let tsc_start = read_tsc();

    unsafe {
        // Reset PIT gate to start counting
        let val = inb(0x61) & 0xFE;
        outb(0x61, val);
        outb(0x61, val | 0x01);

        // Wait for PIT output to go high (count reached zero)
        while inb(0x61) & 0x20 == 0 {
            core::hint::spin_loop();
        }
    }

    // Read TSC at end
    let tsc_end = read_tsc();
    let tsc_elapsed = tsc_end - tsc_start;

    // Calculate frequency: (ticks / milliseconds) * 1000 = ticks per second (Hz)
    let frequency = (tsc_elapsed / CALIBRATION_MS as u64) * 1000;

    // SableWire: Cache for APs — they must not touch the PIT
    CACHED_TSC_FREQUENCY.store(frequency, core::sync::atomic::Ordering::Release);

    frequency
}

/// Get TSC frequency in Hz
///
/// Returns calibrated frequency. Must call calibrate_tsc() first (typically from APIC init).
/// — SableWire
pub fn tsc_frequency() -> u64 {
    let freq = CACHED_TSC_FREQUENCY.load(core::sync::atomic::Ordering::Acquire);
    if freq == 0 {
        // GraveShift: Fallback for early boot before calibration
        // This should only happen if tsc_frequency() called before APIC init
        2_500_000_000
    } else {
        freq
    }
}

/// Delay for a given number of milliseconds using TSC
pub fn delay_ms(ms: u64) {
    let ticks_per_ms = tsc_frequency() / 1000;
    let end = read_tsc() + (ms * ticks_per_ms);
    while read_tsc() < end {
        core::hint::spin_loop();
    }
}

/// Delay for a given number of microseconds using TSC
pub fn delay_us(us: u64) {
    let ticks_per_us = tsc_frequency() / 1_000_000;
    let end = read_tsc() + (us * ticks_per_us);
    while read_tsc() < end {
        core::hint::spin_loop();
    }
}

// ============================================================================
// Kernel Preemption Control — Linux-style preempt_count
//
// — GraveShift: The old KERNEL_PREEMPT_OK boolean was a per-CPU "please preempt
// me" flag that had to be manually toggled by every blocking syscall (~56 sites).
// Set it around VFS/block ops that hold spinlocks? Congrats, the scheduler
// preempts the lock holder and the next task deadlocks on the same lock. Don't
// set it? Task stalls until the emergency timeout fires. Pick your poison.
//
// Linux's answer: every spin_lock() increments preempt_count, every spin_unlock()
// decrements it. Scheduler only preempts when count == 0. No manual annotations.
// No foot-guns. The lock TELLS you when it's safe. Revolutionary, I know.
// ============================================================================

use core::sync::atomic::{AtomicI32, Ordering};

const MAX_CPUS: usize = 256;

/// — GraveShift: Per-CPU preemption nesting counter. Zero means preemptable.
/// Each spinlock acquire increments, each unlock decrements. Lock-free atomics
/// because this gets hammered from both thread and ISR context. The timer ISR
/// reads it to decide "can I context-switch this CPU?" — if non-zero, something
/// holds a lock and preempting would be suicide-by-deadlock.
static PREEMPT_COUNT: [AtomicI32; MAX_CPUS] = [const { AtomicI32::new(0) }; MAX_CPUS];

/// Get this CPU's preempt counter. ISR-safe, lock-free.
#[inline]
fn preempt_counter() -> &'static AtomicI32 {
    let apic_id = crate::apic::id();
    let cpu_id = gdt::cpu_id_from_apic(apic_id);
    let idx = core::cmp::min(cpu_id, MAX_CPUS - 1);
    &PREEMPT_COUNT[idx]
}

/// Disable preemption (increment counter). Called by spinlock acquire.
/// Lock-free, ISR-safe, nesting-safe.
/// — GraveShift: Every lock() bumps this. Every unlock() drops it. When the
/// timer ISR fires and sees count > 0, it knows we're holding a lock and backs
/// off. No more "preempted while holding heap lock → next task deadlocks."
#[inline]
pub fn preempt_disable() {
    preempt_counter().fetch_add(1, Ordering::Relaxed);
}

/// Enable preemption (decrement counter). Called by spinlock release.
/// Lock-free, ISR-safe, nesting-safe.
/// — GraveShift: When this hits zero, the CPU is fair game for preemption again.
#[inline]
pub fn preempt_enable() {
    preempt_counter().fetch_sub(1, Ordering::Relaxed);
}

/// Check if current CPU is preemptable (counter == 0).
/// — GraveShift: The scheduler's one question: "can I yank this task?" If any
/// lock is held (count > 0), the answer is no. Period.
#[inline]
pub fn preemptable() -> bool {
    preempt_counter().load(Ordering::Relaxed) == 0
}

/// Get current preempt_count value (for save/restore across context switches).
#[inline]
pub fn get_preempt_count() -> i32 {
    preempt_counter().load(Ordering::Relaxed)
}

/// Set preempt_count (for restore after context switch to incoming task).
/// — GraveShift: The incoming task was preempted mid-lock. Its saved count
/// tells us exactly how deep it was. Restore it so the lock depth matches
/// reality when the task resumes.
#[inline]
pub fn set_preempt_count(val: i32) {
    preempt_counter().store(val, Ordering::Relaxed);
}

// ============================================================================
// Backward-compat aliases — kpo (kernel_preempt_ok) API
//
// — GraveShift: The old boolean API is used by ~56 call sites across syscalls.
// These aliases translate the old semantics into the new counter model:
//   allow_kernel_preempt()   → set count to 0 (fully preemptable)
//   disallow_kernel_preempt() → set count to 1 (one "virtual lock" held)
//   is_kernel_preempt_allowed() → check count == 0
//   clear_kernel_preempt()   → set count to 0
//
// These are LEGACY. New code should use KernelMutex (which calls
// preempt_disable/enable automatically) instead of manual kpo toggling.
// ============================================================================

/// Legacy: allow kernel preemption (sets counter to 0).
/// Call this before HLT in yielding syscalls like nanosleep.
pub fn allow_kernel_preempt() {
    preempt_counter().store(0, Ordering::Relaxed);
}

/// Legacy: disallow kernel preemption (sets counter to 1).
pub fn disallow_kernel_preempt() {
    // — GraveShift: Only bump if we're currently preemptable. Old code toggled
    // a boolean, so calling disallow twice was idempotent. We preserve that
    // behavior — don't stack counts from legacy callers who aren't paired.
    let current = preempt_counter().load(Ordering::Relaxed);
    if current <= 0 {
        preempt_counter().store(1, Ordering::Relaxed);
    }
}

/// Legacy: check if kernel preemption is currently allowed.
pub fn is_kernel_preempt_allowed() -> bool {
    preemptable()
}

/// Legacy: clear kernel preemption flag (called by scheduler after preempting).
pub fn clear_kernel_preempt() {
    preempt_counter().store(0, Ordering::Relaxed);
}
