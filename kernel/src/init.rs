//! Kernel initialization for the OXIDE kernel.
//!
//! Contains the kernel entry point and boot sequence.

extern crate alloc;

// — GraveShift: force-link driver crates so their #[used] #[link_section = ".pci_drivers"]
// statics survive linker GC. Without these, crates with no direct symbol references
// get stripped entirely, taking their driver registrations with them into the void.
extern crate virtio_gpu;
extern crate virtio_net;
extern crate virtio_snd;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Write;
use core::ptr::addr_of_mut;

use crate::arch;
use arch_traits::Arch;
use block::{BlockDevice, BlockDeviceInfo, BlockError, BlockResult};
use boot_proto::{BootInfo, MemoryType as BootMemoryType};
use devfs::DevFs;
use elf::ElfExecutable;
use mm_manager::{self, MemoryManager};
use mm_paging::{phys_to_virt, read_cr3};
use mm_traits::FrameAllocator as _;
use net::NetworkDevice;
use os_core::VirtAddr;
use proc::{ProcessMeta, alloc_pid};
use proc_traits::MemoryFlags;
use procfs::ProcFs;
use pty::{PtsDir, PtyManager};
use syscall::SyscallContext;
use tmpfs::TmpDir;
use vfs::{File, FileFlags, MountFlags, VnodeOps, mount::GLOBAL_VFS};

use crate::console;
use crate::fault;
use crate::globals::{HEAP_ALLOCATOR, HEAP_SIZE, HEAP_STORAGE, KERNEL_PML4, MEMORY_MANAGER};
use crate::memory;
use crate::mount::{kernel_mount, kernel_pivot_root, kernel_umount};
use crate::process::get_current_task_fs_base;
use crate::process::{kernel_clone, kernel_exec, kernel_fork, kernel_wait, user_exit};
use crate::scheduler;
use crate::smp_init;

/// Serial writer adapter for os_log normal (locking) path
///
/// — GraveShift: os_log::println!() goes to COM1 serial. Period.
/// The old code routed it to terminal::write() which dumped kernel log
/// messages onto the user's screen — debug noise mixed with login prompts.
/// Terminal output has its own paths (BootWriter for boot, VtTtyDriver for echo).
/// See docs/agents/isr-output-serial-only.md for the full horror story.
struct OsLogSerialWriter;

impl os_log::SerialWriter for OsLogSerialWriter {
    fn write_byte(&mut self, byte: u8) {
        arch::serial::write_byte(byte);
    }
}

/// Static writer for os_log (needs to live for 'static lifetime)
static mut OS_LOG_WRITER: OsLogSerialWriter = OsLogSerialWriter;

/// — PatchBay: Console-only boot writer. Serial is DEAD. All output goes to
/// framebuffer/terminal (stderr). Early boot shows on screen, not serial port.
struct BootWriter {
    console_enabled: bool,
}

impl core::fmt::Write for BootWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        // NO MORE SERIAL. Only write to console/terminal.
        if self.console_enabled {
            console::console_write(s.as_bytes());
        } else if fb::is_initialized() {
            // Before terminal is ready, write to basic framebuffer
            for byte in s.bytes() {
                fb::putchar(byte as char);
            }
        }
        Ok(())
    }
}

/// Callback for terminal query responses (DSR, DA, etc.)
/// Injects response bytes into VT TTY input so apps receive them
fn terminal_response_callback(data: &[u8]) {
    for &byte in data {
        vt::push_input_global(byte);
    }
}

/// Callback for VT switching (Alt+F1 through Alt+F6)
/// Switches to the requested virtual terminal
fn vt_switch_callback(vt_num: usize) {
    if let Some(vt_mgr) = vt::get_manager() {
        vt_mgr.switch_to(vt_num);
    }
}

/// Callback for VT switch notification to terminal emulator
/// — WireSaint: Called from ISR context (keyboard IRQ → Alt+F1-F6 → vt::switch_to).
/// MUST use try_flush() — blocking flush() deadlocks if sys_write holds TERMINAL lock
/// on the same CPU. If try_lock fails, the next tick() or write() will render anyway.
fn terminal_vt_switch_callback(_vt_num: usize) {
    if terminal::is_initialized() {
        terminal::try_flush();
    }
}

/// Kernel entry point
///
/// Called by the bootloader after setting up page tables and jumping to higher half.
pub fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Initialize serial port first for early debugging
    arch::serial_init();

    // Enable SMAP (Supervisor Mode Access Prevention) if supported
    // SMAP allows STAC/CLAC instructions to work properly
    // Note: qemu64 CPU doesn't support SMAP, so STAC/CLAC will cause INVALID OPCODE
    unsafe {
        // Check if SMAP is supported via CPUID (EAX=7, ECX=0): SMAP = EBX bit 20
        let ebx_out: u32;
        core::arch::asm!(
            "push rbx",           // Save RBX (callee-saved)
            "mov eax, 7",
            "xor ecx, ecx",
            "cpuid",
            "mov {0:e}, ebx",      // Move EBX to output without using EBX as constraint
            "pop rbx",            // Restore RBX
            out(reg) ebx_out,
            out("eax") _,
            out("ecx") _,
            out("edx") _,
        );

        let smap_supported = (ebx_out & (1 << 20)) != 0;
        if smap_supported {
            // — PatchBay: NO SERIAL. Early messages suppressed until framebuffer ready.
            // SMAP detection happens but message waits for proper output.
            // TODO: Fix SMAP - there's a complex timing issue where AC gets cleared between
            // syscalls. The STAC/CLAC coverage is correct, but something else is clearing AC.
            // For now, disable SMAP to get the system working.
            // let mut cr4: u64;
            // core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack));
            // cr4 |= 1 << 21; // Set SMAP bit (bit 21)
            // core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nostack));
        }
    }

    // Register os_log writers — normal (locking) + ISR-safe (lock-free)
    // SAFETY: OS_LOG_WRITER is static and serial::init() has been called.
    // The unsafe writer fns do raw port I/O without any locks.
    unsafe {
        os_log::register_writer(&mut *addr_of_mut!(OS_LOG_WRITER));
        // — PatchBay: NO MORE SERIAL. Everything goes to console (stderr) now.
        os_log::register_unsafe_writer(console::write_byte_unsafe, console::write_str_unsafe);
    }

    let mut writer = BootWriter {
        console_enabled: false,
    };

    // Print boot banner
    let _ = writeln!(writer);
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "  OXIDE Operating System");
    let _ = writeln!(writer, "  Version 0.1.0");
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer);

    let _ = writeln!(writer, "[INFO] Kernel started on x86_64");
    let _ = writeln!(writer, "[INFO] Serial output initialized");
    let _ = writeln!(writer);
    let _ = writeln!(writer, "[CONFIG] System Configuration:");
    let _ = writeln!(writer, "[CONFIG]   OS Type:      OXIDE Operating System");
    let _ = writeln!(writer, "[CONFIG]   Version:      0.1.0");
    let _ = writeln!(writer, "[CONFIG]   Architecture: x86_64");
    let _ = writeln!(writer, "[CONFIG]   Target:       x86_64-unknown-none");

    #[cfg(debug_assertions)]
    let _ = writeln!(writer, "[CONFIG]   Build:        debug (unoptimized)");
    #[cfg(not(debug_assertions))]
    let _ = writeln!(writer, "[CONFIG]   Build:        release (optimized)");

    let _ = write!(writer, "[CONFIG]   Features:     ");
    let mut first = true;

    #[cfg(feature = "debug-syscall")]
    {
        if !first {
            let _ = write!(writer, ", ");
        }
        let _ = write!(writer, "debug-syscall");
        first = false;
    }
    #[cfg(feature = "debug-fork")]
    {
        if !first {
            let _ = write!(writer, ", ");
        }
        let _ = write!(writer, "debug-fork");
        first = false;
    }
    #[cfg(feature = "debug-cow")]
    {
        if !first {
            let _ = write!(writer, ", ");
        }
        let _ = write!(writer, "debug-cow");
        first = false;
    }
    #[cfg(feature = "debug-proc")]
    {
        if !first {
            let _ = write!(writer, ", ");
        }
        let _ = write!(writer, "debug-proc");
        first = false;
    }
    #[cfg(feature = "debug-sched")]
    {
        if !first {
            let _ = write!(writer, ", ");
        }
        let _ = write!(writer, "debug-sched");
        first = false;
    }
    #[cfg(feature = "debug-mouse")]
    {
        if !first {
            let _ = write!(writer, ", ");
        }
        let _ = write!(writer, "debug-mouse");
        first = false;
    }
    #[cfg(feature = "debug-input")]
    {
        if !first {
            let _ = write!(writer, ", ");
        }
        let _ = write!(writer, "debug-input");
        first = false;
    }
    #[cfg(feature = "debug-lock")]
    {
        if !first {
            let _ = write!(writer, ", ");
        }
        let _ = write!(writer, "debug-lock");
        first = false;
    }
    #[cfg(feature = "debug-console")]
    {
        if !first {
            let _ = write!(writer, ", ");
        }
        let _ = write!(writer, "debug-console");
        first = false;
    }

    if first {
        let _ = write!(writer, "none");
    }
    let _ = writeln!(writer);

    let _ = writeln!(writer, "[CONFIG]   Compiler:     rustc (edition 2024)");
    let _ = writeln!(writer);

    // Validate boot info
    if !boot_info.is_valid() {
        let _ = writeln!(writer, "[ERROR] Invalid boot info magic!");
        arch::X86_64::halt();
    }
    let _ = writeln!(writer, "[INFO] Boot info validated");

    // Print boot info
    let _ = writeln!(
        writer,
        "[INFO] Kernel physical base: {:#x}",
        boot_info.kernel_phys_base
    );
    let _ = writeln!(
        writer,
        "[INFO] Kernel virtual base: {:#x}",
        boot_info.kernel_virt_base
    );
    let _ = writeln!(
        writer,
        "[INFO] Kernel size: {} bytes",
        boot_info.kernel_size
    );
    let _ = writeln!(
        writer,
        "[INFO] Physical map base: {:#x}",
        boot_info.phys_map_base
    );
    let _ = writeln!(writer, "[INFO] PML4 physical: {:#x}", boot_info.pml4_phys);

    // Print memory regions
    let _ = writeln!(
        writer,
        "[INFO] Memory regions: {}",
        boot_info.memory_region_count
    );
    let mut total_usable = 0u64;
    for region in boot_info.memory_regions() {
        // — ColdCipher: After ExitBootServices(), BOOT_SERVICES memory is fully
        // reclaimable — Linux does this too (efi_memmap_usable). Previous exclusion
        // starved the buddy allocator with 256M RAM, causing getty to OOM-hang.
        if matches!(
            region.ty,
            BootMemoryType::Usable | BootMemoryType::BootServices
        ) {
            total_usable += region.len;
        }
    }
    let _ = writeln!(
        writer,
        "[INFO] Total usable memory: {} MB",
        total_usable / (1024 * 1024)
    );

    // Initialize heap with static storage for now
    let _ = writeln!(writer, "[INFO] Initializing heap allocator...");
    unsafe {
        let heap_start = addr_of_mut!(HEAP_STORAGE) as usize;
        HEAP_ALLOCATOR.init(heap_start, HEAP_SIZE);
    }
    let _ = writeln!(writer, "[INFO] Heap initialized: {} KB", HEAP_SIZE / 1024);

    // Initialize memory manager (buddy allocator - no 4GB cap!)
    let _ = writeln!(
        writer,
        "[INFO] Initializing memory manager (buddy allocator)..."
    );

    // The buddy allocator writes FreeBlock headers into free pages during init.
    // We must exclude ALL bootloader-allocated structures from usable memory:
    // 1. Low memory (first 1MB) - BIOS/UEFI data
    // 2. Kernel (kernel_phys_base to kernel_phys_base + kernel_size)
    // 3. Initramfs (if loaded)
    // 4. Page tables (PML4 and all levels - bootloader allocates ~64KB typically)

    const LOW_MEM_LIMIT: u64 = 0x10_0000; // First 1MB reserved

    let kernel_start = boot_info.kernel_phys_base;
    let kernel_end = kernel_start + boot_info.kernel_size;

    // [WORKAROUND] Bootloader doesn't mark ALL its allocations correctly — ColdCipher
    // Corrupted block at 0x1c060000 is marked USABLE but contains page tables.
    // The BOOTLOADER region starts at 0x1c063000 (12KB after corruption).
    // Protect 2MB before bootloader regions to catch this bug.
    const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;
    let current_rsp: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) current_rsp);
    }
    let rsp_phys = if current_rsp >= PHYS_MAP_BASE {
        current_rsp - PHYS_MAP_BASE
    } else {
        current_rsp
    };

    // Find lowest BOOTLOADER region and protect 2MB before it
    let mut bootloader_min = u64::MAX;
    for region in boot_info.memory_regions() {
        if matches!(region.ty, BootMemoryType::Bootloader) {
            bootloader_min = bootloader_min.min(region.start);
        }
    }

    let uefi_guard_start = if bootloader_min != u64::MAX {
        bootloader_min.saturating_sub(0x200000) // 2MB before bootloader
    } else {
        rsp_phys.saturating_sub(0x200000) // Fallback to RSP - 2MB
    };
    let uefi_guard_end = bootloader_min;

    let _ = writeln!(writer, "[INFO] Protected regions:");
    let _ = writeln!(writer, "[INFO]   Low memory: 0x0 - {:#x}", LOW_MEM_LIMIT);
    let _ = writeln!(
        writer,
        "[INFO]   Kernel: {:#x} - {:#x}",
        kernel_start, kernel_end
    );

    // Helper to check if an address range overlaps with protected regions
    let is_protected = |addr: u64| -> bool {
        // Low memory
        if addr < LOW_MEM_LIMIT {
            return true;
        }
        // Kernel
        if addr >= kernel_start && addr < kernel_end {
            return true;
        }
        // UEFI guard (2MB before bootloader to catch allocation bugs)
        if uefi_guard_end != u64::MAX && addr >= uefi_guard_start && addr < uefi_guard_end {
            return true;
        }
        false
    };

    // Build memory regions, splitting around protected areas
    let mut regions: Vec<(os_core::PhysAddr, u64, bool)> = Vec::new();

    for boot_region in boot_info.memory_regions() {
        // — ColdCipher: BootServices memory is safe after ExitBootServices().
        // Firmware only retains RuntimeServices regions. Excluding BootServices
        // starved the system at 256M, killing getty before it could print.
        let base_usable = matches!(
            boot_region.ty,
            BootMemoryType::Usable | BootMemoryType::BootServices
        );

        if !base_usable {
            // Non-usable regions pass through as-is
            regions.push((
                os_core::PhysAddr::new(boot_region.start),
                boot_region.len,
                false,
            ));
            continue;
        }

        // For usable regions, split around protected areas
        let mut current = boot_region.start;
        let region_end = boot_region.start + boot_region.len;

        while current < region_end {
            // Find start of next usable segment (skip protected)
            while current < region_end && is_protected(current) {
                current += 0x1000; // Skip page by page
            }

            if current >= region_end {
                break;
            }

            // Find end of this usable segment
            let segment_start = current;
            while current < region_end && !is_protected(current) {
                current += 0x1000;
            }

            let segment_len = current - segment_start;
            if segment_len > 0 {
                regions.push((os_core::PhysAddr::new(segment_start), segment_len, true));
            }
        }
    }

    let _ = writeln!(
        writer,
        "[INFO] Processed {} memory regions for buddy allocator",
        regions.len()
    );

    // Initialize the global memory manager
    // SAFETY: This is called once during boot with valid memory regions
    unsafe {
        // Need to get a mutable reference to initialize
        let mm_ptr = &MEMORY_MANAGER as *const MemoryManager as *mut MemoryManager;
        (*mm_ptr).init(&regions);
        mm_manager::init_global(&MEMORY_MANAGER);
    }

    let _ = writeln!(
        writer,
        "[INFO] Memory manager initialized (buddy allocator)"
    );

    let total_bytes = MEMORY_MANAGER.total_bytes();
    let free_bytes = MEMORY_MANAGER.free_bytes();
    // — GraveShift: dump memory info to serial for 256M investigation
    unsafe {
        os_log::write_str_raw("[MEM] total=0x");
        arch::serial::write_u64_hex_unsafe(total_bytes as u64);
        os_log::write_str_raw(" free=0x");
        arch::serial::write_u64_hex_unsafe(free_bytes as u64);
        os_log::write_str_raw("\n");
    }
    let _ = writeln!(
        writer,
        "[INFO] Total memory: {} MB ({} bytes)",
        total_bytes / (1024 * 1024),
        total_bytes
    );
    let _ = writeln!(
        writer,
        "[INFO] Free memory: {} MB ({} bytes)",
        free_bytes / (1024 * 1024),
        free_bytes
    );

    // Initialize framebuffer if available
    if let Some(ref fb_info) = boot_info.framebuffer {
        let _ = writeln!(writer, "[INFO] Initializing framebuffer...");
        // — GraveShift: dump FB address to serial — hunting the 256M framebuffer ghost
        unsafe {
            os_log::write_str_raw("[FB] base=0x");
            arch::serial::write_u64_hex_unsafe(fb_info.base);
            os_log::write_str_raw(" ");
            arch::serial::write_u64_hex_unsafe(fb_info.width as u64);
            os_log::write_str_raw("x");
            arch::serial::write_u64_hex_unsafe(fb_info.height as u64);
            os_log::write_str_raw(" phys_map=0x");
            arch::serial::write_u64_hex_unsafe(boot_info.phys_map_base);
            os_log::write_str_raw("\n");
        }
        let _ = writeln!(
            writer,
            "[INFO] Framebuffer: {}x{} @ {:#x}",
            fb_info.width, fb_info.height, fb_info.base
        );
        let _ = writeln!(
            writer,
            "[INFO] Stride: {} pixels, BPP: {}",
            fb_info.stride, fb_info.bpp
        );

        // Initialize with video modes if available
        fb::init_from_boot(
            fb_info,
            boot_info.phys_map_base,
            boot_info.video_modes.as_ref(),
        );
        let _ = writeln!(writer, "[INFO] Framebuffer initialized");

        // Log video mode count
        let mode_count = fb::get_mode_count();
        let _ = writeln!(writer, "[INFO] Video modes available: {}", mode_count);

        // Initialize terminal emulator with framebuffer
        if let Some(framebuffer) = fb::framebuffer() {
            terminal::init(framebuffer);

            // Register callback for terminal query responses (DSR, DA, etc.)
            unsafe {
                terminal::set_response_callback(terminal_response_callback);
            }

            let _ = writeln!(writer, "[INFO] Terminal emulator initialized");
        }

        // Clear the screen with a dark background
        terminal::clear();

        // — GraveShift: The void ends here. Boot messages now paint to the
        // framebuffer so the user sees something besides the abyss.
        writer.console_enabled = true;
        // — GraveShift: os_log console writer intentionally disabled.
        // BootWriter handles init.rs boot messages → terminal. os_log registration
        // would route ALL kernel println! (debug macros, error logs) to the display,
        // polluting userspace output. Debug stays on serial where it belongs.

        let _ = writeln!(writer, "[INFO] Terminal ready");
    } else {
        let _ = writeln!(writer, "[INFO] No framebuffer available, serial-only mode");
    }

    // Initialize architecture components (GDT, IDT, APIC)
    let _ = writeln!(writer, "[INFO] Initializing x86_64 architecture...");
    unsafe {
        arch::init();
    }

    // Initialize SMP subsystem for Bootstrap Processor
    let _ = writeln!(writer, "[INFO] Initializing SMP subsystem...");
    let bsp_apic_id = arch::apic::id();
    unsafe {
        smp::cpu::init_bsp(bsp_apic_id as u32);
        // Note: init_bsp() already calls set_cpu_online(0)
    }
    let _ = writeln!(
        writer,
        "[SMP] Bootstrap Processor initialized (CPU 0, APIC ID {})",
        bsp_apic_id
    );

    // Enumerate CPUs from ACPI MADT — SableWire: silicon census from firmware tables
    let _ = writeln!(writer, "[SMP] Enumerating CPUs from ACPI MADT...");
    let mut ap_count = 0u32;
    if boot_info.rsdp_physical_address != 0 {
        let rsdp_virt = (boot_info.phys_map_base + boot_info.rsdp_physical_address) as *const u8;
        // Safety: RSDP address provided by UEFI firmware, mapped via phys_map_base
        if let Some(rsdp) = unsafe { acpi::Rsdp::parse(rsdp_virt) } {
            let _ = writeln!(
                writer,
                "[ACPI] RSDP v{} — RSDT 0x{:08x}, XSDT 0x{:016x}",
                rsdp.revision, rsdp.rsdt_address, rsdp.xsdt_address
            );

            // Find the MADT in RSDT/XSDT
            // Safety: ACPI tables are mapped through phys_map_base
            if let Some(madt_phys) = unsafe {
                acpi::find_table(
                    boot_info.phys_map_base,
                    rsdp.xsdt_address,
                    rsdp.rsdt_address,
                    &acpi::madt::MADT_SIGNATURE,
                )
            } {
                let _ = writeln!(writer, "[ACPI] MADT found at 0x{:016x}", madt_phys);

                let mut madt_entries = [acpi::MadtEntry::LocalApic(acpi::MadtLocalApic {
                    entry_type: 0,
                    length: 0,
                    acpi_processor_uid: 0,
                    apic_id: 0,
                    flags: 0,
                }); acpi::madt::MAX_MADT_CPUS];

                // Safety: MADT is mapped through phys_map_base
                let num_cpus = unsafe {
                    acpi::parse_madt(boot_info.phys_map_base, madt_phys, &mut madt_entries)
                };
                let _ = writeln!(writer, "[ACPI] MADT reports {} usable CPU(s)", num_cpus);

                // Register each AP (skip BSP by matching APIC ID)
                let mut cpu_idx = 1u32;
                for i in 0..num_cpus {
                    let apic_id = match madt_entries[i] {
                        acpi::MadtEntry::LocalApic(ref lapic) => lapic.apic_id as u32,
                        acpi::MadtEntry::LocalX2Apic(ref x2) => x2.x2apic_id,
                    };

                    // BSP already registered — skip it
                    if apic_id == bsp_apic_id as u32 {
                        continue;
                    }

                    if (cpu_idx as usize) < smp::MAX_CPUS {
                        // — GraveShift: wiring each silicon core into the grid
                        unsafe { smp::cpu::register_cpu(cpu_idx, apic_id, false) };
                        let _ = writeln!(
                            writer,
                            "[SMP] CPU {} registered (APIC ID {}) — offline, pending boot",
                            cpu_idx, apic_id
                        );
                        cpu_idx += 1;
                        ap_count += 1;
                    }
                }
            } else {
                let _ = writeln!(writer, "[ACPI] No MADT found — single CPU mode");
            }
        } else {
            let _ = writeln!(writer, "[ACPI] RSDP parse failed — single CPU mode");
        }
    } else {
        let _ = writeln!(writer, "[ACPI] No RSDP from bootloader — single CPU mode");
    }

    let _ = writeln!(
        writer,
        "[SMP] CPUs detected: {}, CPUs online: {}",
        smp::cpu::cpu_count(),
        smp::cpu::cpus_online()
    );

    if smp::cpu::cpus_online() > 1 {
        let _ = writeln!(writer, "[SMP] Multi-CPU mode: TLB shootdown will use IPIs");
    } else {
        let _ = writeln!(
            writer,
            "[SMP] Single-CPU mode: TLB shootdown uses local flush only"
        );
    }

    // Register TLB shootdown IPI callback
    unsafe {
        arch::set_tlb_shootdown_callback(smp::tlb::handle_tlb_shootdown);
    }

    // Boot Application Processors via INIT-SIPI-SIPI — GraveShift: igniting each core
    if smp::cpu::cpu_count() > 1 {
        let total_aps = smp::cpu::cpu_count() - 1;
        let _ = writeln!(
            writer,
            "[SMP] Booting {} Application Processor(s)...",
            total_aps
        );

        // Get CR3 for APs to use (current page table)
        let cr3 = <arch::X86_64 as arch_traits::TlbControl>::read_root();

        // Register AP initialization callback
        unsafe {
            arch::ap_boot::register_ap_init_callback(smp_init::ap_init_callback);
        }

        for cpu_id in 1..smp::cpu::cpu_count() {
            // Allocate per-AP kernel stack (16KB)
            let ap_stack_phys = mm_manager::mm()
                .alloc_contiguous(4)
                .expect("Failed to allocate AP stack");
            let ap_stack_virt = mm_paging::phys_to_virt(ap_stack_phys).as_u64() + (4 * 4096);

            // Set up trampoline code at 0x8000 (rewritten per AP)
            unsafe {
                arch::ap_boot::setup_trampoline(
                    cr3,
                    ap_stack_virt,
                    arch::ap_boot::ap_entry_rust as *const () as u64,
                );
            }

            let _ = writeln!(writer, "[SMP] Sending INIT-SIPI-SIPI to CPU {}...", cpu_id);
            match smp::cpu::boot_ap(cpu_id, arch::ap_boot::TRAMPOLINE_PAGE) {
                Ok(()) => {
                    let _ = writeln!(writer, "[SMP] CPU {} is now online!", cpu_id);
                }
                Err(e) => {
                    let _ = writeln!(writer, "[SMP] Failed to boot CPU {}: {}", cpu_id, e);
                }
            }
        }

        let _ = writeln!(
            writer,
            "[SMP] AP trampoline at 0x{:x}",
            arch::ap_boot::TRAMPOLINE_PHYS
        );
        let _ = writeln!(
            writer,
            "[SMP] CPUs online: {}/{}",
            smp::cpu::cpus_online(),
            smp::cpu::cpu_count()
        );
    }

    // Register page fault callback for COW handling
    unsafe {
        arch::exceptions::set_page_fault_callback(fault::page_fault_handler);
    }

    // Register terminal tick callback for ~30 FPS rendering
    if terminal::is_initialized() {
        unsafe {
            arch::set_terminal_tick_callback(console::terminal_tick);
        }
        let _ = writeln!(writer, "[INFO] Terminal tick callback registered (~30 FPS)");
    }

    // Initialize PS/2 keyboard controller (i8042)
    // UEFI firmware may leave PS/2 disabled after ExitBootServices
    arch::init_ps2_keyboard();
    let _ = writeln!(writer, "[INFO] PS/2 keyboard initialized");

    // Initialize PS/2 mouse and keyboard drivers (registers with input subsystem)
    debug_mouse!("[mouse] Initializing PS/2 drivers...");
    ps2::init();

    // CRITICAL FIX: Re-enable interrupts immediately after PS/2 init
    // PS/2 init calls cli() but doesn't call sti(). Without this, all subsequent
    // writeln! calls hang because terminal rendering needs interrupts.
    arch::X86_64::enable_interrupts();

    let _ = writeln!(writer, "[INFO] PS/2 drivers initialized (keyboard + mouse)");

    // Connect keyboard IRQ 1 to PS/2 keyboard driver
    debug_input!("[INPUT] Registering IRQ 1 callback for PS/2 keyboard");
    unsafe {
        arch::set_keyboard_callback(ps2::handle_keyboard_irq);
    }
    let _ = writeln!(writer, "[INFO] PS/2 keyboard IRQ callback registered");

    // Connect keyboard input to console via shared kbd module
    // — GraveShift: both PS/2 and VirtIO now use input::kbd for key→console conversion.
    // Register callbacks once here instead of per-driver.
    // Safety: Called during single-threaded initialization
    unsafe {
        input::kbd::set_console_callback(devfs::console_input_callback);
    }
    let _ = writeln!(writer, "[INFO] Keyboard console callback registered (shared kbd module)");

    // Connect Alt+Fn keys to VT switching via shared kbd module
    // Safety: Called during single-threaded initialization
    unsafe {
        input::kbd::set_vt_switch_callback(vt_switch_callback);
    }
    let _ = writeln!(writer, "[INFO] VT switch callback registered (shared kbd module)");

    // Connect mouse IRQ 12 to PS/2 mouse driver
    debug_mouse!("[mouse] Registering IRQ 12 callback for PS/2 mouse");
    unsafe {
        arch::set_mouse_callback(ps2::handle_mouse_irq);
    }
    let _ = writeln!(writer, "[INFO] PS/2 mouse IRQ callback registered");

    // Initialize graphical mouse cursor on framebuffer
    if fb::is_initialized() {
        debug_mouse!("[mouse] Initializing graphical cursor on framebuffer");
        fb::mouse_init();
        let _ = writeln!(writer, "[INFO] Mouse cursor initialized");
    } else {
        debug_mouse!("[mouse] No framebuffer — skipping graphical cursor init");
    }

    // — InputShade: PCI enumeration must happen before VirtIO input probe.
    // Previously this lived in network init (Phase 3), but VirtIO keyboard/tablet
    // are PCI devices discovered here. enumerate() is idempotent — it early-returns
    // if already called, so the network phase re-call is harmless.
    pci::enumerate();

    // ========================================
    // Dynamic Driver Loading System (NEW)
    // ========================================
    // — GraveShift: collect drivers from linker sections and auto-probe
    let _ = writeln!(writer, "[DRIVER] Initializing driver registry from linker sections...");
    driver_core::init_driver_registry();

    let _ = writeln!(writer, "[DRIVER] Probing all PCI devices with registered drivers...");

    let _ = driver_core::probe_all_devices();

    let _ = writeln!(writer, "[DRIVER] Probing ISA devices...");
    let _ = driver_core::probe_isa_devices();

    // Log registered drivers
    let pci_drivers = driver_core::list_pci_drivers();
    let _ = writeln!(writer, "[DRIVER] Registered PCI drivers: {}", pci_drivers.len());
    for driver_name in pci_drivers.iter() {
        let _ = writeln!(writer, "[DRIVER]   - {}", driver_name);
    }

    // — InputShade: VirtIO input devices are probed by driver_core above.
    // No more double-probing with probe_all_pci() — the PciDriver::probe()
    // handles device init and input subsystem registration.

    // — GlassSignal: the UEFI GOP framebuffer is memory-mapped and works without
    // explicit GPU flushes. VirtIO-GPU's SET_SCANOUT to a new resource doesn't take
    // effect in QEMU — the display continues showing UEFI's GOP memory.
    // So we keep the UEFI GOP fb and don't switch the terminal to a different buffer.

    // Set up input subsystem wake callback for blocking reads on /dev/input/eventN
    // — GraveShift: This callback fires from keyboard/mouse ISR context.
    // MUST use try_wake_up (non-blocking) — sched::wake_up would deadlock
    // if the interrupted code holds the RQ spin lock.
    fn isr_safe_wake(pid: u32) {
        sched::try_wake_up(pid);
    }
    unsafe {
        input::set_wake_callback(isr_safe_wake);
    }
    let _ = writeln!(writer, "[INFO] Input wake callback registered");

    // Initialize and register preemptive scheduler (BSP)

    scheduler::init();

    let _ = writeln!(writer, "[INFO] Scheduler initialized");

    // NeonRoot: Register the hardware CPU ID callback BEFORE timers or APs can
    // call this_cpu(). Without this, the scheduler uses a single global atomic
    // for CPU ID — all 4 CPUs stomp on it, every scheduler tick uses the wrong
    // run queue, instant SMP crash.
    sched::register_cpu_id_fn(|| {
        let apic_id = arch::apic_id() as u32;
        smp::cpu::cpu_id_from_apic(apic_id)
    });
    let _ = writeln!(writer, "[INFO] SMP CPU ID callback registered");

    unsafe {
        arch::set_scheduler_callback(scheduler::scheduler_tick);
    }
    let _ = writeln!(writer, "[INFO] Preemptive scheduler registered");

    // Register wall-clock provider for subsystems (ext4 timestamps, etc.)
    // — WireSaint: bridging the tick counter to filesystem time
    os_core::register_wall_clock(syscall::time::wall_clock_secs);
    let _ = writeln!(writer, "[INFO] Wall-clock time bridge registered");

    // Register credentials provider for subsystems (ext4 ownership, etc.)
    // — EmberLock: bridging process identity to filesystem ownership
    os_core::register_creds_provider(current_process_uid_gid);
    let _ = writeln!(writer, "[INFO] Credentials bridge registered");

    // NeonRoot: Enable interrupts BEFORE starting the timer. If the timer
    // starts first, a pending tick fires the instant sti executes. The
    // scheduler_tick handler can crash if it runs before the BSP has a
    // current task. Starting the timer after sti gives a clean first tick.
    let _ = writeln!(writer, "[INFO] Enabling interrupts...");

    arch::X86_64::enable_interrupts();
    let _ = writeln!(writer, "[INFO] Interrupts enabled");

    // BlackLatch: Unmask IOAPIC keyboard/mouse now that sti is live and
    // all handlers are registered. Any stale PS2 data will fire here safely.
    arch::unmask_io_irqs();

    // NeonRoot: Timer and AP release are DEFERRED until right before entering
    // usermode. The entire boot sequence runs as a single kernel thread with
    // no scheduling. Starting the timer here would fire scheduler_tick and
    // terminal_tick every 10ms during VFS/network/block init, causing deadlocks
    // when those callbacks take locks that boot code already holds (e.g. COM1
    // serial lock during writeln!). Timer starts at the very end of init.

    let _ = writeln!(writer);

    // Test heap allocation
    let _ = writeln!(writer, "[INFO] Testing heap allocation...");
    let boxed_value = Box::new(42u32);
    let _ = writeln!(writer, "[INFO] Box::new(42) = {}", *boxed_value);

    let _ = writeln!(writer);
    let _ = writeln!(writer, "OXIDE kernel initialized successfully!");
    let _ = writeln!(writer);

    // ========================================
    // Phase 3: User Mode Test
    // ========================================

    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "  Phase 3: User Mode Test");
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer);

    // Initialize syscall mechanism
    let _ = writeln!(writer, "[USER] Initializing syscall mechanism...");
    unsafe {
        arch::syscall::init();
    }

    // Set up syscall handlers
    let _ = writeln!(writer, "[USER] Setting up syscall handlers...");
    let syscall_ctx = SyscallContext {
        console_write: Some(console::console_write),
        exit: Some(user_exit),
        fork: Some(kernel_fork),
        clone: Some(kernel_clone),
        exec: Some(kernel_exec),
        wait: Some(kernel_wait),
        mount: Some(kernel_mount),
        umount: Some(kernel_umount),
        pivot_root: Some(kernel_pivot_root),
        serial_write: Some(console::serial_write_bytes),
        get_current_fs_base: Some(get_current_task_fs_base),
        allow_kernel_preempt: Some(arch::allow_kernel_preempt),
        disallow_kernel_preempt: Some(arch::disallow_kernel_preempt),
    };
    unsafe {
        syscall::init(syscall_ctx);
    }

    // Register the syscall dispatch function
    unsafe {
        arch::syscall::set_syscall_handler(syscall_dispatch);
    }

    // -- GraveShift: Signals delivered on syscall return, not just timer ticks
    unsafe {
        arch::syscall::set_signal_check_function(crate::scheduler::check_signals_on_syscall_return);
    }

    // -- BlackLatch: User faults (SIGSEGV, SIGFPE) kill the process, not the kernel
    unsafe {
        arch::exceptions::set_user_fault_kill_callback(crate::scheduler::kill_faulting_process);
    }

    // ========================================
    // VFS Initialization
    // ========================================
    let _ = writeln!(writer, "[VFS] Initializing virtual filesystem...");

    // Mount tmpfs as root filesystem
    let root_fs = TmpDir::new_root();
    if let Err(e) = GLOBAL_VFS.mount(root_fs.clone(), "/", MountFlags::empty(), "tmpfs") {
        let _ = writeln!(writer, "[VFS] Failed to mount root: {:?}", e);
        arch::X86_64::halt();
    }
    let _ = writeln!(writer, "[VFS] Mounted tmpfs at /");

    // Create /dev directory
    if let Err(e) = root_fs.mkdir("dev", vfs::Mode::DEFAULT_DIR) {
        let _ = writeln!(writer, "[VFS] Failed to create /dev: {:?}", e);
        arch::X86_64::halt();
    }

    // Mount devfs at /dev
    let dev_fs = DevFs::new();

    // — GraveShift: VT manager init. Creates TTY devices + lock-free input rings.
    // Must happen before devfs registration (VtDevice needs the Arc<VtManager>).
    let vt_manager = vt::init();
    let _ = writeln!(writer, "[INFO] VT manager initialized ({} virtual terminals)", vt::NUM_VTS);

    // ⚡ GraveShift: Propagate real terminal dimensions to all VT TTYs.
    // Without this, TIOCGWINSZ returns the 24x80 default and every TUI
    // app (top, vim, less) thinks it's on a VT100 from 1978. ⚡
    if let Some((cols, rows)) = terminal::dimensions() {
        let (xpixel, ypixel) = fb::framebuffer()
            .map(|fb| (fb.width() as u16, fb.height() as u16))
            .unwrap_or((0, 0));
        vt::set_global_winsize(rows as u16, cols as u16, xpixel, ypixel);
        let _ = writeln!(
            writer,
            "[VFS] VT winsize set to {}x{} ({}x{} px)",
            cols, rows, xpixel, ypixel
        );
    }

    // Register /dev/tty1 through /dev/tty6 in devfs
    // Wire /dev/console to tty1 (the primary VT)
    for i in 0..vt::NUM_VTS {
        let vt_device = vt::VtDevice::new(i, vt_manager.clone(), 1000 + i as u64);
        if i == 0 {
            // /dev/console delegates to /dev/tty1 (the active VT)
            devfs::set_console_backend(vt_device.clone());
        }
        let device_name = alloc::format!("tty{}", i + 1);
        dev_fs.register(&device_name, vt_device);
    }

    if let Err(e) = GLOBAL_VFS.mount(dev_fs, "/dev", MountFlags::empty(), "devfs") {
        let _ = writeln!(writer, "[VFS] Failed to mount devfs: {:?}", e);
        arch::X86_64::halt();
    }
    let _ = writeln!(
        writer,
        "[VFS] Mounted devfs at /dev with {} VT devices",
        vt::NUM_VTS
    );

    // Set up legacy console write function for devfs (fallback for early boot)
    unsafe {
        devfs::devices::set_console_write(console::console_write_bytes);
    }

    // Set up framebuffer info callback for /dev/fb0
    unsafe {
        devfs::devices::set_fb_info_callback(memory::get_fb_device_info);
        devfs::devices::set_fb_mode_count_callback(memory::get_fb_mode_count);
        devfs::devices::set_fb_mode_info_callback(memory::get_fb_mode_info);
        devfs::devices::set_fb_mode_set_callback(memory::set_fb_mode);
    }

    // Set up random number generator callback for /dev/urandom and /dev/random
    unsafe {
        devfs::set_random_fill_callback(crypto::random::fill_bytes);
    }

    // Set up /dev/kmsg callbacks for PID, uptime, and process name
    unsafe {
        devfs::set_pid_callback(kmsg_get_pid);
        devfs::set_uptime_callback(kmsg_get_uptime_ms);
        devfs::set_proc_name_callback(kmsg_get_proc_name);
    }

    // Set up signal callbacks for Ctrl+C handling and SIGWINCH
    unsafe {
        devfs::set_signal_fg_callback(signal_foreground_pgrp); // Console TTY (legacy)
        pty::set_signal_pgrp_callback(signal_pgrp_callback); // PTY devices
        vt::set_signal_pgrp_callback(signal_pgrp_callback); // VT devices
        tty::set_signal_pgrp_callback(signal_pgrp_callback); // 🔥 TTY SIGWINCH support 🔥
        vt::set_console_write_callback(console::console_write); // VT output
        vt::set_yield_callback(vt_yield); // VT blocking yield
        vt::set_vt_switch_callback(terminal_vt_switch_callback); // 🔥 VT switch redraw 🔥
        vt::set_signal_pending_callback(is_signal_pending); // — GraveShift: EINTR for blocked reads
    }

    // Create /proc directory
    if let Err(e) = root_fs.mkdir("proc", vfs::Mode::DEFAULT_DIR) {
        let _ = writeln!(writer, "[VFS] Failed to create /proc: {:?}", e);
        arch::X86_64::halt();
    }

    // Set memory stats callback for procfs
    unsafe {
        procfs::set_memory_stats_callback(memory::get_memory_stats);
    }

    // Mount procfs at /proc
    let proc_fs = ProcFs::new();
    if let Err(e) = GLOBAL_VFS.mount(proc_fs, "/proc", MountFlags::empty(), "procfs") {
        let _ = writeln!(writer, "[VFS] Failed to mount procfs: {:?}", e);
        arch::X86_64::halt();
    }
    let _ = writeln!(writer, "[VFS] Mounted procfs at /proc");

    // Initialize PTY subsystem
    let pty_manager = Arc::new(PtyManager::new());

    // Create /dev/pts directory
    if let Err(e) = root_fs.mkdir("pts", vfs::Mode::DEFAULT_DIR) {
        // Ignore error if directory already exists somehow
        let _ = writeln!(writer, "[VFS] Note: /dev/pts mkdir: {:?}", e);
    }

    // Get the devfs to register PTY devices
    if let Ok(_devfs_vnode) = GLOBAL_VFS.lookup("/dev") {
        // We need to downcast and use DevFs's register method
        // For now, the PTY devices are accessible through the PtsDir
        let _ = writeln!(writer, "[VFS] PTY manager initialized");
    }

    // Mount pts filesystem at /dev/pts (using tmpfs for the mount point)
    let pts_dir = PtsDir::new(pty_manager.clone(), 100);
    if let Err(e) = GLOBAL_VFS.mount(pts_dir, "/dev/pts", MountFlags::empty(), "devpts") {
        let _ = writeln!(writer, "[VFS] Failed to mount devpts: {:?}", e);
        // Non-fatal - PTY support just won't work
    } else {
        let _ = writeln!(writer, "[VFS] Mounted devpts at /dev/pts");
    }

    let _ = writeln!(writer, "[VFS] VFS initialized");

    // ========================================
    // Network Initialization
    // ========================================
    let _ = writeln!(writer, "[NET] Initializing network stack...");

    // — NeonRoot: driver_core already probed and registered VirtIO net via PciDriver::probe().
    // We just need to grab it from the net subsystem and wire up the higher-level stack.
    let net_devices = net::devices();
    let net_initialized = if let Some(device) = net_devices.first() {
        let mac = device.mac_address();
        let _ = writeln!(writer, "[NET] VirtIO network device found via driver_core");
        let _ = writeln!(
            writer,
            "[NET] MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac.0[0], mac.0[1], mac.0[2], mac.0[3], mac.0[4], mac.0[5]
        );

        // Create network interface and initialize TCP/IP stack
        let interface = Arc::new(net::NetworkInterface::new(device.clone()));
        net::interface::add_interface(interface.clone());

        tcpip::init(interface.clone());
        let _ = writeln!(writer, "[NET] TCP/IP stack initialized");

        // Try to acquire IP address via DHCP (with short timeout)
        let _ = writeln!(writer, "[NET] Attempting DHCP (timeout: 2s)...");
        let dhcp_result = tcpip::acquire_lease(interface.clone());
        match dhcp_result {
            Ok(lease) => {
                let _ = writeln!(writer, "[NET] DHCP lease acquired: {}", lease.ip_addr);
                let _ = writeln!(writer, "[NET]   Netmask: {}", lease.subnet_mask);
                if let Some(gw) = lease.gateway {
                    let _ = writeln!(writer, "[NET]   Gateway: {}", gw);
                }
                for dns in &lease.dns_servers {
                    let _ = writeln!(writer, "[NET]   DNS: {}", dns);
                }
                let _ = writeln!(writer, "[NET]   Lease time: {} seconds", lease.lease_time);
            }
            Err(e) => {
                let _ = writeln!(writer, "[NET] DHCP failed: {:?}, networkd will retry", e);
            }
        }

        true
    } else {
        let _ = writeln!(writer, "[NET] No network device registered by driver_core");
        false
    };

    // If no VirtIO, initialize loopback device at minimum
    if !net_initialized {
        let _ = writeln!(writer, "[NET] Initializing loopback device only");
        let loopback = Arc::new(net::LoopbackDevice::new());
        net::register_device(loopback.clone());

        let lo_interface = Arc::new(net::NetworkInterface::new(loopback));
        lo_interface
            .set_ipv4_addr(
                net::Ipv4Addr::new(127, 0, 0, 1),
                net::Ipv4Addr::new(255, 0, 0, 0),
            )
            .ok();
        net::interface::add_interface(lo_interface);
    }

    let _ = writeln!(writer, "[NET] Network initialization complete");

    // — GlassSignal: VirtIO GPU and VirtIO SND are probed by driver_core above.
    // GPU's PciDriver::probe() checks fb::is_initialized() before stealing the display.
    // SND's PciDriver::probe() calls init_from_pci() directly. No legacy code needed.

    // ========================================
    // Intel HDA Audio Probe
    // ========================================
    // — EchoFrame: real hardware audio, the silicon sings through HDA
    let hda_devices = pci::find_intel_hda();
    if let Some(pci_dev) = hda_devices.first() {
        let _ = writeln!(
            writer,
            "[SND] Intel HDA found at {:02x}:{:02x}.{} (PCI {:04x}:{:04x})",
            pci_dev.address.bus,
            pci_dev.address.device,
            pci_dev.address.function,
            pci_dev.vendor_id,
            pci_dev.device_id
        );
        match intel_hda::init_from_pci(pci_dev) {
            Ok(()) => {
                let _ = writeln!(writer, "[SND] Intel HDA audio initialized");
            }
            Err(e) => {
                let _ = writeln!(writer, "[SND] Intel HDA init failed: {}", e);
            }
        }
    }

    // ========================================
    // Block Device Initialization
    // ========================================
    let _ = writeln!(writer, "[BLK] Initializing block devices...");

    // Probe for VirtIO block devices at standard MMIO addresses
    let mut virtio_blk_devices = unsafe { virtio_blk::probe_all() };
    let mmio_count = virtio_blk_devices.len();

    // Probe for VirtIO block devices on PCI bus
    let pci_blk_devices = virtio_blk::probe_all_pci();
    let pci_count = pci_blk_devices.len();
    virtio_blk_devices.extend(pci_blk_devices);

    let _ = writeln!(
        writer,
        "[BLK] Found {} VirtIO block devices ({} MMIO, {} PCI)",
        virtio_blk_devices.len(),
        mmio_count,
        pci_count
    );

    // Track if we found a root ext4 partition to mount
    let mut ext4_root_partition: Option<Arc<dyn BlockDevice>> = None;

    // Register each device and check for partitions/filesystems
    for (idx, device) in virtio_blk_devices.into_iter().enumerate() {
        let info = device.info();
        let _ = writeln!(
            writer,
            "[BLK] virtio{}: {} blocks of {} bytes ({})",
            idx,
            info.block_count,
            info.block_size,
            if info.read_only { "RO" } else { "RW" }
        );

        // Wrap device in Arc for partition sharing
        let device_arc: Arc<dyn BlockDevice> = Arc::new(device);

        // Check for GPT partition table
        if gpt::has_gpt(&*device_arc) {
            let _ = writeln!(writer, "[BLK] virtio{}: GPT partition table detected", idx);

            match gpt::Gpt::parse(&*device_arc) {
                Ok(gpt_table) => {
                    let _ = writeln!(
                        writer,
                        "[BLK] virtio{}: {} partitions found",
                        idx,
                        gpt_table.entries.len()
                    );

                    // Get partitions as Partition objects
                    let partitions = gpt_table.partitions(device_arc.clone());

                    for (part_idx, partition) in partitions.into_iter().enumerate() {
                        let part_num = part_idx + 1;
                        let part_name = alloc::format!("virtio{}p{}", idx, part_num);
                        let label = gpt_table.entries[part_idx].name_string();
                        let entry = &gpt_table.entries[part_idx];

                        // Get partition type
                        let type_str = if entry.is_linux_fs() {
                            "Linux filesystem"
                        } else if entry.is_efi_system() {
                            "EFI System"
                        } else if entry.is_fs() {
                            "OXIDE filesystem"
                        } else {
                            "Unknown"
                        };

                        if label.is_empty() {
                            let _ = writeln!(
                                writer,
                                "[BLK]   {}: LBA {}-{} ({} blocks) - {}",
                                part_name,
                                entry.first_lba,
                                entry.last_lba,
                                entry.size_blocks(),
                                type_str
                            );
                        } else {
                            let _ = writeln!(
                                writer,
                                "[BLK]   {}: LBA {}-{} ({} blocks) - {}, label=\"{}\"",
                                part_name,
                                entry.first_lba,
                                entry.last_lba,
                                entry.size_blocks(),
                                type_str,
                                label
                            );
                        }

                        // Wrap partition in Arc for filesystem detection
                        let partition_arc: Arc<dyn BlockDevice> = Arc::new(partition);

                        // Check if partition contains ext4 filesystem
                        if ext4::is_ext4(&*partition_arc) {
                            let _ =
                                writeln!(writer, "[BLK]   {}: ext4 filesystem detected", part_name);

                            if let Ok(ext4_info) = ext4::get_info(&*partition_arc) {
                                let _ = writeln!(
                                    writer,
                                    "[BLK]   {}: {} total blocks, {} free blocks",
                                    part_name, ext4_info.blocks_total, ext4_info.blocks_free
                                );
                            }

                            // If this is a Linux filesystem partition with ext4, consider it for root mount
                            if entry.is_linux_fs() && ext4_root_partition.is_none() {
                                ext4_root_partition = Some(partition_arc.clone());
                                let _ = writeln!(
                                    writer,
                                    "[BLK]   {}: Selected as root filesystem candidate",
                                    part_name
                                );
                            }
                        }

                        // Register partition as a block device
                        // Need to create a new Partition since we moved the original
                        let part_for_reg = block::Partition::new(
                            device_arc.clone(),
                            entry.first_lba,
                            entry.size_blocks(),
                            part_num as u8,
                            Box::leak(part_name.clone().into_boxed_str()),
                        );
                        block::register_device(part_name, Box::new(part_for_reg));
                    }
                }
                Err(e) => {
                    let _ = writeln!(writer, "[BLK] virtio{}: GPT parse error: {:?}", idx, e);
                }
            }
        } else {
            // No GPT - check if whole disk is ext4 (raw filesystem)
            if ext4::is_ext4(&*device_arc) {
                let _ = writeln!(
                    writer,
                    "[BLK] virtio{}: ext4 filesystem detected (no partition table)",
                    idx
                );

                if let Ok(ext4_info) = ext4::get_info(&*device_arc) {
                    let _ = writeln!(
                        writer,
                        "[BLK] virtio{}: {} total blocks, {} free blocks",
                        idx, ext4_info.blocks_total, ext4_info.blocks_free
                    );
                }

                // Use as root if no other candidate
                if ext4_root_partition.is_none() {
                    ext4_root_partition = Some(device_arc.clone());
                    let _ = writeln!(
                        writer,
                        "[BLK] virtio{}: Selected as root filesystem candidate",
                        idx
                    );
                }
            }
        }

        // Register the whole device
        let dev_name = alloc::format!("virtio{}", idx);
        // We need to clone the Arc and create a wrapper since we need a Box
        block::register_device(dev_name, Box::new(BlockDeviceWrapper(device_arc)));
    }

    let _ = writeln!(writer, "[BLK] Block device initialization complete");

    // Try to use ext4 as root filesystem if it has /sbin/init
    // TEMPORARY: Force initramfs-only boot to debug ext4 mount hang
    let mut ext4_as_root = false;
    if false && ext4_root_partition.is_some() {
        let ext4_device = ext4_root_partition.as_ref().unwrap();
        let _ = writeln!(
            writer,
            "[EXT4] Checking ext4 partition for root filesystem..."
        );
        let _ = writeln!(writer, "[EXT4] About to call ext4::mount()...");

        match ext4::mount(ext4_device.clone(), false) {
            Ok(ext4_root) => {
                // Check if ext4 has /sbin/init
                let has_init = ext4_root.lookup("sbin/init").is_ok();

                if has_init {
                    let _ = writeln!(writer, "[EXT4] Found /sbin/init on ext4 partition");
                    let _ = writeln!(writer, "[EXT4] Using ext4 as root filesystem");

                    // Mount ext4 as root
                    if let Err(e) =
                        GLOBAL_VFS.mount(ext4_root.clone(), "/", MountFlags::empty(), "ext4")
                    {
                        let _ = writeln!(writer, "[EXT4] Failed to mount ext4 as root: {:?}", e);
                    } else {
                        ext4_as_root = true;
                        let _ = writeln!(writer, "[EXT4] Mounted ext4 as root filesystem at /");

                        // List root directory contents
                        if let Ok(root_vnode) = GLOBAL_VFS.lookup("/") {
                            let _ = writeln!(writer, "[EXT4] Root directory contents:");
                            let mut offset = 0u64;
                            let mut count = 0;
                            while let Ok(Some(entry)) = root_vnode.readdir(offset) {
                                if count < 10 {
                                    let _ = writeln!(writer, "[EXT4]   {}", entry.name);
                                }
                                count += 1;
                                offset += 1;
                            }
                            if count > 10 {
                                let _ = writeln!(
                                    writer,
                                    "[EXT4]   ... and {} more entries",
                                    count - 10
                                );
                            }
                        }
                    }
                } else {
                    let _ = writeln!(writer, "[EXT4] No /sbin/init found on ext4 partition");
                }
            }
            Err(e) => {
                let _ = writeln!(writer, "[EXT4] Failed to mount ext4 filesystem: {:?}", e);
            }
        }
    }

    // If ext4 wasn't used as root, fall back to initramfs
    if !ext4_as_root {
        // Load and mount the initramfs (loaded from disk by bootloader)
        let initramfs_data = match boot_info.initramfs() {
            Some(data) => {
                let _ = writeln!(
                    writer,
                    "[INITRAMFS] Initramfs at phys {:#x}, {} bytes",
                    boot_info.initramfs_phys, boot_info.initramfs_size
                );
                data
            }
            None => {
                let _ = writeln!(
                    writer,
                    "[INITRAMFS] ERROR: No initramfs loaded by bootloader!"
                );
                arch::X86_64::halt();
            }
        };

        let _ = writeln!(
            writer,
            "[INITRAMFS] Loading initramfs ({} bytes)...",
            initramfs_data.len()
        );

        let initramfs_root = match initramfs::load(initramfs_data) {
            Ok(root) => root,
            Err(e) => {
                let _ = writeln!(writer, "[INITRAMFS] Failed to load initramfs: {:?}", e);
                arch::X86_64::halt();
            }
        };



        // Mount initramfs as root filesystem
        if let Err(e) = GLOBAL_VFS.mount(
            initramfs_root.clone(),
            "/",
            MountFlags::empty(),
            "initramfs",
        ) {
            let _ = writeln!(writer, "[INITRAMFS] Failed to mount initramfs: {:?}", e);
            arch::X86_64::halt();
        }
        let _ = writeln!(writer, "[INITRAMFS] Mounted as root filesystem at /");

        // Mount tmpfs on writable directories (initramfs is read-only)
        let writable_dirs = ["/run", "/tmp", "/var/log", "/var/lib", "/var/run"];
        for dir in &writable_dirs {
            let tmpfs = TmpDir::new_root();
            if let Err(e) = GLOBAL_VFS.mount(tmpfs, dir, MountFlags::empty(), "tmpfs") {
                let _ = writeln!(
                    writer,
                    "[VFS] Note: Could not mount tmpfs at {}: {:?}",
                    dir, e
                );
            } else {
                let _ = writeln!(writer, "[VFS] Mounted tmpfs at {}", dir);
            }
        }

        // If ext4 was found but didn't have init, mount it at /mnt/root
        if let Some(ext4_device) = ext4_root_partition.clone() {
            if let Ok(ext4_root) = ext4::mount(ext4_device, false) {
                // Create /mnt directory if it doesn't exist
                if GLOBAL_VFS.lookup("/mnt").is_err() {
                    if let Ok(root) = GLOBAL_VFS.lookup("/") {
                        let _ = root.mkdir("mnt", vfs::Mode::DEFAULT_DIR);
                    }
                }

                // Create /mnt/root directory for ext4 mount
                if let Ok(mnt) = GLOBAL_VFS.lookup("/mnt") {
                    let _ = mnt.mkdir("root", vfs::Mode::DEFAULT_DIR);
                }

                if let Err(e) =
                    GLOBAL_VFS.mount(ext4_root, "/mnt/root", MountFlags::empty(), "ext4")
                {
                    let _ = writeln!(writer, "[EXT4] Failed to mount ext4 at /mnt/root: {:?}", e);
                } else {
                    let _ = writeln!(writer, "[EXT4] Mounted ext4 filesystem at /mnt/root");
                }
            }
        }

    }

    // Load /sbin/init
    let init_path = "/sbin/init";
    let _ = writeln!(writer, "[USER] Loading {}...", init_path);
    let init_vnode = match GLOBAL_VFS.lookup(init_path) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(writer, "[USER] Failed to find {}: {:?}", init_path, e);
            arch::X86_64::halt();
        }
    };

    // Read init binary
    let init_size = init_vnode.size() as usize;
    let mut init_data = alloc::vec![0u8; init_size];
    match init_vnode.read(0, &mut init_data) {
        Ok(n) if n == init_size => {}
        Ok(n) => {
            let _ = writeln!(writer, "[USER] Short read: {} of {} bytes", n, init_size);
            arch::X86_64::halt();
        }
        Err(e) => {
            let _ = writeln!(writer, "[USER] Failed to read init: {:?}", e);
            arch::X86_64::halt();
        }
    }
    let _ = writeln!(writer, "[USER] Read {} bytes from init", init_size);

    // Parse the ELF
    let elf = match ElfExecutable::parse(&init_data) {
        Ok(e) => e,
        Err(err) => {
            let _ = writeln!(writer, "[USER] Failed to parse init ELF: {:?}", err);
            arch::X86_64::halt();
        }
    };

    let _ = writeln!(
        writer,
        "[USER] ELF entry point: {:#x}",
        elf.entry_point().as_u64()
    );

    // Check for TLS segment
    let has_tls = elf.tls_template().is_some();
    if has_tls {
        let _ = writeln!(
            writer,
            "[USER] Init has TLS segment, will set up thread-local storage"
        );
    }

    // Create user address space
    let _ = writeln!(writer, "[USER] Creating user address space...");
    let kernel_pml4 = read_cr3();
    let _ = writeln!(writer, "[USER] Kernel PML4: {:#x}", kernel_pml4.as_u64());

    // Store kernel PML4 for fork/exec
    unsafe {
        KERNEL_PML4 = kernel_pml4.as_u64();
    }

    let mut user_space =
        match unsafe { proc::UserAddressSpace::new_with_kernel(mm_manager::mm(), kernel_pml4) } {
            Some(s) => s,
            None => {
                let _ = writeln!(writer, "[USER] Failed to create user address space!");
                arch::X86_64::halt();
            }
        };
    let user_pml4 = user_space.pml4_phys();
    let _ = writeln!(
        writer,
        "[USER] User PML4: {:#x} (raw u64: {})",
        user_pml4.as_u64(),
        user_pml4.as_u64()
    );

    // Load ELF segments into user address space
    // Handle overlapping segments by checking if pages are already mapped
    for seg in elf.segments() {
        let (page_base, page_size) = elf::ElfLoader::segment_pages(seg);
        let num_pages = page_size / 4096;
        let _page_offset = elf::ElfLoader::segment_page_offset(seg);

        // Allocate pages that aren't already mapped, or update flags if already mapped
        for i in 0..num_pages {
            let page_addr = VirtAddr::new(page_base.as_u64() + (i as u64 * 4096));

            // Check if page is already mapped
            let existing = user_space.translate(page_addr);

            if existing.is_none() {
                // Allocate single page
                if let Err(e) = user_space.allocate_pages(page_addr, 1, seg.flags, mm_manager::mm())
                {
                    let _ = writeln!(
                        writer,
                        "[USER] Failed to allocate page at {:#x}: {:?}",
                        page_addr.as_u64(),
                        e
                    );
                    arch::X86_64::halt();
                }
            } else {
                // Page already mapped - upgrade permissions if this segment needs more
                // (e.g., .data segment may need write permission on a page that .text only needed read)
                if seg.flags.writable() {
                    user_space.update_user_page_flags(page_addr, MemoryFlags::WRITE);
                }
            }
        }

        // Copy segment data page by page (physical pages may not be contiguous!)
        let seg_data = elf.segment_data(seg);
        let seg_vaddr_start = seg.vaddr.as_u64();
        let mut data_offset = 0usize;
        let mut mem_offset = 0usize;

        while data_offset < seg_data.len() || mem_offset < seg.mem_size {
            // Calculate which virtual page we're on
            let current_vaddr = seg_vaddr_start + mem_offset as u64;
            let page_vaddr = VirtAddr::new(current_vaddr & !0xFFF);
            let in_page_offset = (current_vaddr & 0xFFF) as usize;
            let bytes_remaining_in_page = 4096 - in_page_offset;

            // Get physical address for this page
            let phys = match user_space.translate(page_vaddr) {
                Some(p) => p,
                None => {
                    let _ = writeln!(
                        writer,
                        "[USER] translate({:#x}) failed!",
                        page_vaddr.as_u64()
                    );
                    arch::X86_64::halt();
                }
            };

            let dest_virt = phys_to_virt(phys);
            let dest = unsafe { dest_virt.as_mut_ptr::<u8>().add(in_page_offset) };

            // Copy file data for this page
            let file_bytes_remaining = seg_data.len().saturating_sub(data_offset);
            let copy_len = bytes_remaining_in_page.min(file_bytes_remaining);

            if copy_len > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        seg_data.as_ptr().add(data_offset),
                        dest,
                        copy_len,
                    );
                }
                data_offset += copy_len;
            }

            // Zero BSS portion of this page (if any)
            let bss_start_in_seg = seg.file_size;
            let bss_end_in_seg = seg.mem_size;

            if mem_offset + bytes_remaining_in_page > bss_start_in_seg
                && mem_offset < bss_end_in_seg
            {
                // There's some BSS in this page
                let bss_start_in_page = if mem_offset < bss_start_in_seg {
                    (bss_start_in_seg - mem_offset).min(bytes_remaining_in_page)
                } else {
                    0
                };
                let bss_end_in_page = (bss_end_in_seg - mem_offset).min(bytes_remaining_in_page);
                let bss_len = bss_end_in_page - bss_start_in_page;

                if bss_len > 0 {
                    unsafe {
                        core::ptr::write_bytes(dest.add(bss_start_in_page), 0, bss_len);
                    }
                }
            }

            mem_offset += bytes_remaining_in_page;
        }

    }

    // Set up TLS (Thread-Local Storage) if needed
    let tls_base = if let Some(tls_template) = elf.tls_template() {
        let _ = writeln!(writer, "[USER] Setting up TLS...");
        let tls_size = tls_template.mem_size;
        let tcb_size = 64; // Thread Control Block size
        let total_size = tls_size + tcb_size;
        let pages_needed = (total_size + 4095) / 4096;
        let tls_vaddr = VirtAddr::new(0x0000_7000_0000_0000); // TLS region

        // Allocate TLS pages
        if let Err(e) = user_space.allocate_pages(
            tls_vaddr,
            pages_needed,
            MemoryFlags::READ
                .union(MemoryFlags::WRITE)
                .union(MemoryFlags::USER),
            mm_manager::mm(),
        ) {
            let _ = writeln!(writer, "[USER] Failed to allocate TLS: {:?}", e);
            arch::X86_64::halt();
        }

        // Copy TLS initialization data
        let tls_data = elf.tls_data();
        if !tls_data.is_empty() {
            let _ = writeln!(
                writer,
                "[USER] Copying TLS initialization data ({} bytes)...",
                tls_data.len()
            );
            // Write TLS data page by page
            let mut offset = 0usize;
            while offset < tls_data.len() {
                let current_vaddr = tls_vaddr.as_u64() + offset as u64;
                let page_vaddr = VirtAddr::new(current_vaddr & !0xFFF);
                let in_page_offset = (current_vaddr & 0xFFF) as usize;
                let bytes_remaining = tls_data.len() - offset;
                let bytes_in_page = core::cmp::min(4096 - in_page_offset, bytes_remaining);

                // Get physical address for this page
                let phys = user_space
                    .translate(page_vaddr)
                    .expect("TLS page not mapped");
                let dest_virt = phys_to_virt(phys);
                unsafe {
                    let dest = dest_virt.as_mut_ptr::<u8>().add(in_page_offset);
                    core::ptr::copy_nonoverlapping(
                        tls_data.as_ptr().add(offset),
                        dest,
                        bytes_in_page,
                    );
                }
                offset += bytes_in_page;
            }
        }

        // TCB is at the end of the TLS block
        // FS register will point here
        let tcb_addr = tls_vaddr.as_u64() + tls_size as u64;

        // Write self-pointer to TCB (required by x86-64 TLS ABI)
        let tcb_page_vaddr = VirtAddr::new(tcb_addr & !0xFFF);
        let tcb_page_offset = (tcb_addr & 0xFFF) as usize;
        let tcb_phys = user_space
            .translate(tcb_page_vaddr)
            .expect("TCB page not mapped");
        let tcb_dest_virt = phys_to_virt(tcb_phys);
        unsafe {
            let tcb_dest = tcb_dest_virt.as_mut_ptr::<u8>().add(tcb_page_offset) as *mut u64;
            *tcb_dest = tcb_addr; // Self-pointer
        }

        let _ = writeln!(
            writer,
            "[USER] TLS initialized: base={:#x}, size={}, TCB={:#x}",
            tls_vaddr.as_u64(),
            tls_size,
            tcb_addr
        );

        Some(tcb_addr)
    } else {
        None
    };

    // Allocate user stack (larger to accommodate typical program needs)
    let _ = writeln!(writer, "[USER] Allocating user stack...");
    let user_stack_pages = 64; // 256 KB stack
    // Stack grows down, so allocate below the top address
    let user_stack_base = VirtAddr::new(0x7FFF_F000_0000 - (user_stack_pages * 4096) as u64);
    let stack_flags = MemoryFlags::READ
        .union(MemoryFlags::WRITE)
        .union(MemoryFlags::USER);

    if let Err(e) = user_space.allocate_pages(
        user_stack_base,
        user_stack_pages,
        stack_flags,
        mm_manager::mm(),
    ) {
        let _ = writeln!(writer, "[USER] Failed to allocate user stack: {:?}", e);
        arch::X86_64::halt();
    }

    let user_stack_end = VirtAddr::new(user_stack_base.as_u64() + (user_stack_pages * 4096) as u64);
    let _ = writeln!(
        writer,
        "[USER] User stack: {:#x} - {:#x}",
        user_stack_base.as_u64(),
        user_stack_end.as_u64()
    );

    // Set up argc/argv for init on the stack
    // Stack layout (growing down from top):
    //   [string]     - "/bin/init\0"
    //   [padding]
    //   [NULL]       - envp terminator
    //   [NULL]       - argv terminator
    //   [argv[0]]    - pointer to "/bin/init"
    //   [argc]       <- RSP points here
    let init_path_bytes = b"/bin/init\0";
    let mut stack_ptr = user_stack_end.as_u64();

    // Write string "/bin/init\0" near top of stack
    stack_ptr -= init_path_bytes.len() as u64;
    stack_ptr &= !0xF; // 16-byte align
    let string_addr = stack_ptr;

    // Write string to stack
    {
        let page_vaddr = VirtAddr::new(string_addr & !0xFFF);
        let page_offset = (string_addr & 0xFFF) as usize;
        let phys = user_space
            .translate(page_vaddr)
            .expect("Stack page not mapped");
        let dest_virt = phys_to_virt(phys);
        unsafe {
            let dest = dest_virt.as_mut_ptr::<u8>().add(page_offset);
            core::ptr::copy_nonoverlapping(init_path_bytes.as_ptr(), dest, init_path_bytes.len());
        }
    }

    // Space for: argc, argv[0], argv[1]=NULL, envp[0]=NULL (4 * 8 = 32 bytes)
    stack_ptr -= 32;
    stack_ptr &= !0xF;
    let final_rsp = stack_ptr;

    // Helper to write a u64 to the user stack
    let write_u64 = |addr: u64, value: u64| {
        let page_vaddr = VirtAddr::new(addr & !0xFFF);
        let page_offset = (addr & 0xFFF) as usize;
        let phys = user_space
            .translate(page_vaddr)
            .expect("Stack page not mapped");
        let dest_virt = phys_to_virt(phys);
        unsafe {
            let dest = dest_virt.as_mut_ptr::<u8>().add(page_offset) as *mut u64;
            *dest = value;
        }
    };

    // Write argc = 1
    write_u64(stack_ptr, 1);
    stack_ptr += 8;
    // Write argv[0] = pointer to string
    write_u64(stack_ptr, string_addr);
    stack_ptr += 8;
    // Write argv[1] = NULL (terminator)
    write_u64(stack_ptr, 0);
    stack_ptr += 8;
    // Write envp[0] = NULL (terminator)
    write_u64(stack_ptr, 0);

    let user_stack_top = VirtAddr::new(final_rsp);
    let _ = writeln!(
        writer,
        "[USER] Init stack set up: argc=1 argv={:#x} rsp={:#x}",
        string_addr, final_rsp
    );

    // Allocate kernel stack for syscalls and interrupts
    let _ = writeln!(writer, "[USER] Allocating kernel stack...");
    // Allocate 128KB kernel stack - fork+COW uses ~67KB during deep recursion
    const KERNEL_STACK_SIZE: usize = 128 * 1024;
    let kernel_stack_pages = KERNEL_STACK_SIZE / 4096;
    let kernel_stack_phys = mm_manager::mm()
        .alloc_contiguous(kernel_stack_pages)
        .expect("Failed to allocate kernel stack");
    // Convert physical to virtual for the kernel to use
    let kernel_stack_virt = phys_to_virt(kernel_stack_phys);
    let kernel_stack_top = kernel_stack_virt.as_u64() + KERNEL_STACK_SIZE as u64;

    // Set kernel stack for:
    // 1. Syscalls (stored in GS base for syscall handler)
    // 2. Interrupts (TSS.RSP0 for privilege level changes)
    // — GraveShift: BSP = cpu 0. Each CPU needs its own per-CPU syscall data.
    unsafe {
        arch::syscall::init_kernel_stack(0, kernel_stack_top);
    }
    arch::gdt::set_kernel_stack(kernel_stack_top); // TSS.RSP0 for interrupts

    // Allocate a stack for double fault handling (IST1)
    let df_stack: Box<[u8; 8192]> = Box::new([0u8; 8192]);
    let df_stack_ptr = Box::into_raw(df_stack);
    let df_stack_top = unsafe { (df_stack_ptr as *const u8).add(8192) as u64 };
    arch::gdt::set_ist(0, df_stack_top); // IST1 = ist[0]

    // Get the entry point before switching
    let entry_point = elf.entry_point().as_u64();
    let _ = writeln!(writer, "[USER] Kernel stack top: {:#x}", kernel_stack_top);

    // Create and register init process (PID 1)
    let _ = writeln!(writer, "[USER] Registering init process...");
    let init_pid = alloc_pid(); // Should be 1
    let _ = writeln!(writer, "[USER] Init PID: {}", init_pid);

    // Create ProcessMeta for init
    let mut init_meta = ProcessMeta::new(
        init_pid, // tgid
        user_space,
    );

    // Set up standard file descriptors (stdin, stdout, stderr) for init
    let _ = writeln!(writer, "[USER] Setting up stdin/stdout/stderr...");
    match GLOBAL_VFS.lookup("/dev/console") {
        Ok(console_vnode) => {
            // stdin (read-only)
            let stdin = Arc::new(File::new(console_vnode.clone(), FileFlags::O_RDONLY));
            if let Err(e) = init_meta.fd_table.insert(0, stdin) {
                let _ = writeln!(writer, "[USER] Failed to set up stdin: {:?}", e);
            }

            // stdout (write-only)
            let stdout = Arc::new(File::new(console_vnode.clone(), FileFlags::O_WRONLY));
            if let Err(e) = init_meta.fd_table.insert(1, stdout) {
                let _ = writeln!(writer, "[USER] Failed to set up stdout: {:?}", e);
            }

            // stderr (write-only)
            let stderr = Arc::new(File::new(console_vnode, FileFlags::O_WRONLY));
            if let Err(e) = init_meta.fd_table.insert(2, stderr) {
                let _ = writeln!(writer, "[USER] Failed to set up stderr: {:?}", e);
            }

            let _ = writeln!(writer, "[USER] Standard fds set up (0,1,2 -> /dev/console)");
        }
        Err(e) => {
            let _ = writeln!(writer, "[USER] Failed to open /dev/console: {:?}", e);
        }
    }

    // Set cmdline for init so ps shows it correctly
    init_meta.cmdline = alloc::vec![alloc::string::String::from("/init")];

    // Wrap in Arc<Mutex<>> for Task
    let init_meta_arc = Arc::new(spin::Mutex::new(init_meta));

    // Create a Task for init with the ProcessMeta
    let _ = writeln!(writer, "[USER] Adding init to scheduler...");
    let init_task = sched::Task::new_with_meta(
        init_pid,
        0, // ppid
        kernel_stack_phys,
        KERNEL_STACK_SIZE,
        init_meta_arc.lock().address_space.pml4_phys(),
        elf.entry_point().as_u64(),
        user_stack_top.as_u64(),
        init_meta_arc.clone(),
    );

    // Add to scheduler
    sched::add_task(init_task);
    let _ = writeln!(writer, "[USER] Calling sched::switch_to...");
    sched::switch_to(init_pid); // Mark init as the currently running task

    let _ = writeln!(writer, "[USER] Init process registered");

    // Get the PML4 from the init meta
    let user_pml4_phys = init_meta_arc.lock().address_space.pml4_phys();

    let _ = writeln!(writer);
    let _ = writeln!(writer, "[USER] Entering user mode at {:#x}...", entry_point);
    let _ = writeln!(writer, "[USER] PML4 phys: {:#x}", user_pml4_phys.as_u64());
    let _ = writeln!(writer, "[USER] User RSP: {:#x}", user_stack_top.as_u64());
    let _ = writeln!(writer, "[USER] Kernel stack: {:#x}", kernel_stack_top);
    if let Some(fs_base) = tls_base {
        let _ = writeln!(writer, "[USER] TLS FS base: {:#x}", fs_base);
    }
    let _ = writeln!(writer);

    // NeonRoot: NOW start the BSP timer and release APs. All boot init is done,
    // the init process is in the scheduler, and we're about to enter usermode.
    // Starting the timer earlier causes deadlocks — timer callbacks take locks
    // that boot code holds (serial, VFS, etc.).
    //
    // CRITICAL: No writeln! after start_timer — the timer fires every 10ms and
    // terminal_tick/scheduler_tick can run. Lock-free writes only via os_log.
    // — PatchBay: NO MORE SERIAL. os_log routes to console now.

    unsafe {
        os_log::write_str_raw("[INFO] Starting APIC timer at 100Hz...\n");
    }
    arch::start_timer(100);
    smp_init::signal_ap_ready();
    unsafe {
        os_log::write_str_raw("[SMP] AP timer gate released — all CPUs active\n");
    }

    // NeonRoot: Straight into usermode — no more locks, no more delays.
    // enter_usermode does cli immediately, so the timer won't fire during
    // the page table switch or iretq setup.
    unsafe {
        arch::usermode::enter_usermode(
            kernel_stack_top,
            user_pml4_phys.as_u64(),
            entry_point,
            user_stack_top.as_u64(),
            tls_base.unwrap_or(0),
        );
    }
}

/// Send signal to all processes in a process group
///
/// — GraveShift: Called from BOTH ISR context (push_input Ctrl+C fast path) and
/// normal kernel context (VT drain path). Uses try_lock everywhere because ISR
/// context can't spin on contended locks — that's a one-way ticket to deadlock city.
/// After queuing the signal, try_wake_up() kicks TASK_INTERRUPTIBLE processes back
/// to TASK_RUNNING so they don't sleep through their own death sentence. Linux does
/// this in complete_signal() → signal_wake_up() → wake_up_state(). Without it,
/// Ctrl+C queues SIGINT but the process snoozes through nanosleep blissfully unaware.
fn kill_pgrp(pgid: u32, sig: i32) {
    use signal::SigInfo;

    let info = SigInfo::kill(sig, 0, 0);
    let all_pids = sched::all_pids();

    unsafe {
        os_log::write_str_raw("[KILL-PGRP] pgid=");
        arch_x86_64::serial::write_u64_hex_unsafe(pgid as u64);
        os_log::write_str_raw(" sig=");
        arch_x86_64::serial::write_u64_hex_unsafe(sig as u64);
        os_log::write_str_raw("\n");
    }

    for pid in all_pids {
        if let Some(meta) = sched::get_task_meta(pid) {
            // — GraveShift: try_lock because we may be in ISR context.
            // If contended, skip this PID — signal delivery will catch it on next drain.
            if let Some(mut guard) = meta.try_lock() {
                if guard.pgid == pgid {
                    guard.send_signal(sig, Some(info.clone()));
                    drop(guard);
                    unsafe {
                        os_log::write_str_raw("[KILL-PGRP] sent sig to pid=");
                        arch_x86_64::serial::write_u64_hex_unsafe(pid as u64);
                        os_log::write_str_raw("\n");
                    }
                    // — GraveShift: Wake the process if it's sleeping.
                    let woke = sched::try_wake_up(pid);
                    unsafe {
                        os_log::write_str_raw("[KILL-PGRP] wake=");
                        os_log::write_str_raw(if woke { "OK" } else { "FAIL" });
                        os_log::write_str_raw("\n");
                    }
                }
            }
        }
    }
}

/// Yield callback for VT blocking reads.
///
/// Enables kernel preemption and halts until the next interrupt, allowing
/// the scheduler to context-switch to other processes. Without this, a
/// process blocked in a VtManager::read() spinloop would monopolize the
/// CPU because the timer interrupt refuses to preempt non-preemptible
/// kernel code.
fn vt_yield() {
    // — SableWire: yield the CPU but PRESERVE the caller's kernel_preempt_ok state.
    // CRITICAL: Disable kernel preemption BEFORE scheduler operations. yield_current()
    // and set_need_resched() acquire the RQ lock via with_rq. If kernel_preempt_ok is
    // true when the timer fires during that lock hold, the ISR won't take the early
    // return and will try pick_next_task → with_rq → deadlock on the same lock.
    // Disabling preemption here makes the ISR bail (in_kernel && !kernel_preempt_ok).
    // Re-enable only for the HLT wait, where no locks are held.
    let was_preempt_ok = arch::is_kernel_preempt_allowed();
    if was_preempt_ok {
        arch::disallow_kernel_preempt();
    }
    sched::yield_current();
    sched::set_need_resched();
    // — SableWire: Re-enable preemption for HLT. No locks held here — safe for the
    // timer ISR to context-switch us out while we sleep.
    arch::allow_kernel_preempt();
    unsafe {
        core::arch::asm!("sti", "hlt", options(nomem, nostack));
    }
    // Restore the caller's preemption state — don't clobber it
    if !was_preempt_ok {
        arch::disallow_kernel_preempt();
    }
}

/// Kmsg callback: get current PID
fn kmsg_get_pid() -> u32 {
    sched::current_pid_lockfree().unwrap_or(0)
}

/// Kmsg callback: get uptime in milliseconds
fn kmsg_get_uptime_ms() -> u64 {
    arch::timer_ticks() * 10 // 100 Hz timer, each tick = 10ms
}

/// Kmsg callback: get process name for a PID
fn kmsg_get_proc_name(pid: u32, buf: &mut [u8]) -> usize {
    if let Some(meta) = sched::get_task_meta(pid) {
        let meta = meta.lock();
        if let Some(cmd) = meta.cmdline.first() {
            // Extract just the filename from the path
            let name = if let Some(slash_pos) = cmd.rfind('/') {
                &cmd[slash_pos + 1..]
            } else {
                cmd.as_str()
            };
            let len = name.len().min(buf.len());
            buf[..len].copy_from_slice(&name.as_bytes()[..len]);
            return len;
        }
    }
    0
}

/// Get current process UID/GID for the os_core credentials bridge
/// — EmberLock: identity extraction for subsystem consumption
fn current_process_uid_gid() -> (u32, u32) {
    sched::with_current_meta(|meta| (meta.credentials.uid, meta.credentials.gid)).unwrap_or((0, 0))
}

/// Check if the current process has pending signals that should interrupt a blocking read.
///
/// — GraveShift: Filters out SIG_IGN signals so the shell (which ignores SIGINT) doesn't
/// get spuriously interrupted from read(). Only returns true for signals that would
/// actually DO something — terminate, stop, or invoke a user handler.
fn is_signal_pending() -> bool {
    let pid = match sched::current_pid() {
        Some(pid) if pid > 1 => pid,
        _ => return false,
    };
    if let Some(meta_arc) = sched::get_task_meta(pid) {
        if let Some(meta) = meta_arc.try_lock() {
            return signal::delivery::should_interrupt_for_signal(
                &meta.pending_signals.set(),
                &meta.signal_mask,
                &meta.sigactions,
            );
        }
    }
    false
}

/// Signal process group callback for PTYs
fn signal_pgrp_callback(pgid: i32, sig: i32) {
    if pgid > 0 {
        kill_pgrp(pgid as u32, sig);
    }
}

/// Send signal to the TTY foreground process group
///
/// This is called when Ctrl+C or Ctrl+\ is pressed.
/// Sends the signal to all processes in the TTY's foreground process group.
fn signal_foreground_pgrp(sig: i32) {
    // Get the foreground process group from /dev/console TTY
    // For now, use a simple approach: get the TTY from VFS
    if let Ok(console) = vfs::mount::GLOBAL_VFS.lookup("/dev/console") {
        // Try to get the TTY and its foreground pgid via ioctl
        use tty::TIOCGPGRP;
        let mut pgid: i32 = 0;
        let pgid_ptr = &mut pgid as *mut i32 as u64;

        if console.ioctl(TIOCGPGRP, pgid_ptr).is_ok() && pgid > 0 {
            // Send signal to all processes in this process group
            kill_pgrp(pgid as u32, sig);
        }
    }
}

/// Syscall dispatch function
fn syscall_dispatch(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    // Handle sched_yield specially - it needs to context switch
    const SCHED_YIELD: u64 = 130;
    if number == SCHED_YIELD {
        return scheduler::kernel_yield();
    }

    syscall::dispatch(number, arg1, arg2, arg3, arg4, arg5, arg6)
}

/// Wrapper for Arc<dyn BlockDevice> to implement BlockDevice
///
/// This allows registering Arc-wrapped devices with the block device registry
/// which expects Box<dyn BlockDevice>.
pub struct BlockDeviceWrapper(pub Arc<dyn BlockDevice>);

impl BlockDevice for BlockDeviceWrapper {
    fn read(&self, start_block: u64, buf: &mut [u8]) -> BlockResult<usize> {
        self.0.read(start_block, buf)
    }

    fn write(&self, start_block: u64, buf: &[u8]) -> BlockResult<usize> {
        self.0.write(start_block, buf)
    }

    fn flush(&self) -> BlockResult<()> {
        self.0.flush()
    }

    fn block_size(&self) -> u32 {
        self.0.block_size()
    }

    fn block_count(&self) -> u64 {
        self.0.block_count()
    }

    fn info(&self) -> BlockDeviceInfo {
        self.0.info()
    }

    fn is_read_only(&self) -> bool {
        self.0.is_read_only()
    }
}
// Force rebuild 1769738726
