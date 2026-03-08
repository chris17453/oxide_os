//! OXIDE UEFI Boot Manager
//!
//! A cyberpunk-themed pre-boot environment with graphical kernel selection,
//! boot option editing, and a UEFI diagnostic console. This is not your
//! grandmother's bootloader — it has opinions about typography.
//!
//! Boot flow:
//!   UEFI firmware → BOOTX64.EFI → scan kernels → graphical boot menu →
//!   user selects/edits options → load selected kernel → pass boot options →
//!   jump to kernel
//!
//! — NeonRoot: the gatekeeper before the kernel awakens

#![no_std]
#![no_main]
#![allow(unused)]

use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr;

use boot_proto::{
    BOOT_INFO_MAGIC, BootInfo, FramebufferInfo, KERNEL_VIRT_BASE, MAX_MEMORY_REGIONS, MemoryRegion,
    MemoryType, PHYS_MAP_BASE, PixelFormat,
};

mod config;
mod console;
mod discovery;
mod editor;
mod efi;
mod elf;
mod font;
mod input;
mod menu;
mod paging;

use efi::{
    EfiBltPixel, EfiBltOperation, EfiGraphicsOutputProtocol, EfiInputKey,
    EfiHandle, EfiStatus, EfiSystemTable, FmtBuf,
    EFI_SUCCESS,
};

/// Page size
const PAGE_SIZE: u64 = 4096;

/// Kernel stack size (256KB) — WireSaint: enough headroom for deep call chains + debug
const KERNEL_STACK_SIZE: usize = 256 * 1024;

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(handle: EfiHandle, st: *mut EfiSystemTable) -> EfiStatus {
    // Initialize our custom EFI bindings — SableWire: plugging in the umbilical cord
    unsafe { efi::init(handle, st) };

    // — PatchBay: announce ourselves with NT-style build number
    log_fmt(format_args!(
        "[BOOT] OXIDE Boot Manager v{} (Build {})",
        env!("OXIDE_VERSION_STRING"),
        env!("OXIDE_BUILD_NUMBER")
    ));

    // ── Phase 1: Discovery ──
    // — NeonRoot: scanning the ESP for signs of intelligent kernel life

    // Try config-driven discovery first, fall back to auto-scan
    let mut boot_config = match discovery::load_config_file() {
        Some(mut cfg) => {
            log("[BOOT] Loaded boot.cfg");
            discovery::validate_config_entries(&mut cfg);
            cfg
        }
        None => {
            log("[BOOT] No boot.cfg found, auto-scanning for kernels...");
            discovery::auto_scan_kernels()
        }
    };

    // Get screen dimensions for menu layout
    let (screen_width, screen_height) = get_screen_dimensions();

    // ── Phase 2: Menu / Selection ──
    // — NeonRoot: the moment of choice — which kernel lives, which kernel sleeps

    let mut boot_options_buf = [0u8; 256];
    let mut boot_options_len: usize = 0;

    let selected_index = if boot_config.entry_count == 0 {
        // No kernels found — show error and wait
        show_error_screen(screen_width, screen_height);
        halt();
    } else if boot_config.entry_count == 1 && boot_config.timeout_secs == 0 {
        // Single kernel, instant boot — no menu needed
        // — NeonRoot: one kernel to rule them all, one kernel to bind them
        let opts = boot_config.entries[0].options_str().as_bytes();
        let len = opts.len().min(255);
        boot_options_buf[..len].copy_from_slice(&opts[..len]);
        boot_options_len = len;
        0usize
    } else {
        // Show graphical boot menu
        let (idx, opts_buf, opts_len) = run_boot_menu(&mut boot_config, screen_width, screen_height);
        boot_options_buf[..opts_len].copy_from_slice(&opts_buf[..opts_len]);
        boot_options_len = opts_len;
        idx
    };

    // ── Phase 3: Load and Boot ──
    // — NeonRoot: the point of no return — next stop: kernel_main

    let entry = &boot_config.entries[selected_index];
    {
        let mut buf = FmtBuf::<128>::new();
        write!(buf, "[BOOT] Booting: {}", entry.label_str()).ok();
        log(buf.as_str());
    }
    if boot_options_len > 0 {
        let opts_str = core::str::from_utf8(&boot_options_buf[..boot_options_len]).unwrap_or("");
        let mut buf = FmtBuf::<256>::new();
        write!(buf, "[BOOT] Options: {}", opts_str).ok();
        log(buf.as_str());
    }

    // Clear screen before kernel load
    efi::clear_screen();

    // Load kernel from the selected entry's path using page-backed allocation
    // — SableWire: kernel ELF can be 33MB+ in debug mode — scratch arena won't cut it
    let kernel_data = match load_kernel_from_entry(entry) {
        Ok(data) => data,
        Err(e) => {
            let mut buf = FmtBuf::<128>::new();
            write!(buf, "[ERROR] Failed to load kernel: {}", e).ok();
            log(buf.as_str());
            halt();
        }
    };

    // Parse ELF
    let elf_info = match elf::parse_elf(kernel_data) {
        Ok(info) => info,
        Err(e) => {
            let mut buf = FmtBuf::<128>::new();
            write!(buf, "[ERROR] Failed to parse ELF: {}", e).ok();
            log(buf.as_str());
            halt();
        }
    };

    // Allocate memory for kernel
    let kernel_pages = (elf_info.load_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let kernel_phys =
        efi::allocate_pages(kernel_pages as usize).expect("Failed to allocate memory for kernel");

    // Load kernel segments into final location
    elf::load_segments(kernel_data, &elf_info, kernel_phys);
    // Note: kernel_data pages (LOADER_DATA) remain allocated but are dead weight
    // after segments are copied. The kernel can reclaim them via the memory map.

    // Load initramfs (from entry's initramfs path, or default)
    let (initramfs_phys, initramfs_size) = load_initramfs_from_entry(entry);

    // Initialize graphics
    let fb_info = get_framebuffer_info();

    // Enumerate video modes
    let video_modes = enumerate_video_modes();

    // Set up page tables
    let pml4_phys = paging::setup_page_tables(kernel_phys, elf_info.load_size);

    // Allocate boot info pages (BootInfo is ~5KB due to arrays + cmdline)
    let boot_info_phys = efi::allocate_pages(2).expect("Failed to allocate boot info pages");

    // Allocate kernel stack (256KB) — GraveShift: UEFI stack is tiny & unmapped post-jump
    let stack_pages = KERNEL_STACK_SIZE / PAGE_SIZE as usize;
    let stack_phys = efi::allocate_pages(stack_pages).expect("Failed to allocate kernel stack");
    let stack_top_virt = PHYS_MAP_BASE + stack_phys + KERNEL_STACK_SIZE as u64;

    // Get memory map and exit boot services simultaneously
    // — SableWire: the spec-required exit_boot_services dance — get map, exit, retry if stale
    let mut mmap_buf = [0u8; 16384]; // 16KB should be enough for any memory map
    let mut map_key: usize = 0;
    let mut desc_size: usize = 0;
    let mut desc_count: usize = 0;

    // Parse memory map into our format BEFORE exit_boot_services
    // We need to read the map first, then exit
    let mut memory_regions = [MemoryRegion::empty(); MAX_MEMORY_REGIONS];
    let mut region_count: usize = 0;

    {
        // Get memory map for our use (before exit)
        let bs = efi::boot_services().expect("Boot services not available");
        let mut map_size = mmap_buf.len();
        let mut mk: usize = 0;
        let mut ds: usize = 0;
        let mut dv: u32 = 0;

        let status = unsafe {
            (bs.get_memory_map)(
                &mut map_size,
                mmap_buf.as_mut_ptr() as *mut efi::EfiMemoryDescriptor,
                &mut mk,
                &mut ds,
                &mut dv,
            )
        };

        if !efi::efi_error(status) && ds > 0 {
            let count = map_size / ds;
            for i in 0..count {
                if region_count >= MAX_MEMORY_REGIONS {
                    break;
                }
                let offset = i * ds;
                let desc = unsafe {
                    &*(mmap_buf.as_ptr().add(offset) as *const efi::EfiMemoryDescriptor)
                };

                let ty = match desc.memory_type {
                    efi::boot_services::EFI_CONVENTIONAL_MEMORY => MemoryType::Usable,
                    efi::boot_services::EFI_BOOT_SERVICES_CODE
                    | efi::boot_services::EFI_BOOT_SERVICES_DATA => MemoryType::BootServices,
                    efi::boot_services::EFI_ACPI_RECLAIM_MEMORY => MemoryType::AcpiReclaimable,
                    efi::boot_services::EFI_ACPI_MEMORY_NVS => MemoryType::AcpiNvs,
                    efi::boot_services::EFI_LOADER_CODE
                    | efi::boot_services::EFI_LOADER_DATA => MemoryType::Bootloader,
                    _ => MemoryType::Reserved,
                };

                memory_regions[region_count] = MemoryRegion::new(
                    desc.physical_start,
                    desc.number_of_pages * PAGE_SIZE,
                    ty,
                );
                region_count += 1;
            }
        }
    }

    // Verify stack allocation is in memory map — ColdCipher
    let stack_end = stack_phys + KERNEL_STACK_SIZE as u64;
    let mut stack_covered = false;
    for i in 0..region_count {
        let region = &memory_regions[i];
        if region.ty == MemoryType::Bootloader {
            if region.start <= stack_phys && region.start + region.len >= stack_end {
                stack_covered = true;
                break;
            }
        }
    }

    if !stack_covered && region_count < MAX_MEMORY_REGIONS {
        log("[BOOT-FIX] Stack allocation NOT in memory map - manually adding it");
        memory_regions[region_count] = MemoryRegion::new(
            stack_phys,
            KERNEL_STACK_SIZE as u64,
            MemoryType::Bootloader,
        );
        region_count += 1;
    }

    // Extract RSDP physical address from UEFI configuration tables
    // — SableWire: tapping the firmware's ACPI root before we burn the bridge
    let rsdp_phys = find_rsdp_in_config_tables();

    // Create boot info with command line from selected options
    let mut boot_info = create_boot_info(
        kernel_phys,
        elf_info.load_size,
        pml4_phys,
        &memory_regions[..region_count],
        fb_info,
        video_modes,
        initramfs_phys,
        initramfs_size,
        rsdp_phys,
    );

    // Copy boot options into BootInfo cmdline
    // — BlackLatch: the last message from the boot manager before the bridge burns
    let cmdline_len = boot_options_len.min(255);
    boot_info.cmdline[..cmdline_len].copy_from_slice(&boot_options_buf[..cmdline_len]);
    boot_info.cmdline_len = cmdline_len as u32;

    // Write boot info to the pre-allocated pages
    unsafe {
        ptr::write(boot_info_phys as *mut BootInfo, boot_info);
    }

    // Calculate addresses for kernel jump
    let kernel_entry_virt = KERNEL_VIRT_BASE + (elf_info.entry - elf_info.load_base);
    let boot_info_virt = PHYS_MAP_BASE + boot_info_phys;

    // Show final boot message
    log("");
    log("Launching OXIDE OS...");
    log("");

    // Exit boot services — the point of no return!
    // — SableWire: burning the bridge — after this, UEFI is dead to us
    let exited = unsafe {
        efi::exit_boot_services(
            &mut mmap_buf,
            &mut map_key,
            &mut desc_size,
            &mut desc_count,
        )
    };

    if !exited {
        // Can't even print — boot services are gone. Just halt.
        loop { unsafe { core::arch::asm!("hlt") }; }
    }

    // Switch to our page tables and jump to kernel
    unsafe {
        core::arch::asm!(
            "mov cr3, rax",
            "mov rsp, rdx",
            "mov rdi, rsi",
            "jmp rcx",
            in("rax") pml4_phys,
            in("rsi") boot_info_virt,
            in("rcx") kernel_entry_virt,
            in("rdx") stack_top_virt,
            options(noreturn)
        );
    }
}

// ══════════════════════════════════════════════════════════════════
// Boot Menu Orchestration
// — NeonRoot: the conductor of the pre-boot symphony
// ══════════════════════════════════════════════════════════════════

/// Run the graphical boot menu and return (selected_index, options_buf, options_len)
fn run_boot_menu(
    config: &mut config::BootConfig,
    width: usize,
    height: usize,
) -> (usize, [u8; 256], usize) {
    loop {
        // Create menu state
        let mut state = menu::MenuState::new(config, width, height);

        // Render the full menu
        with_gop(|gop| {
            menu::render_full_menu(gop, config, &state);
        });

        // Run the input event loop
        match input::run_menu_loop(config, &mut state) {
            input::MenuResult::Boot(idx) => {
                let mut opts = [0u8; 256];
                let len = get_entry_options(config, idx, &mut opts);
                return (idx, opts, len);
            }
            input::MenuResult::Console => {
                // Run diagnostic console
                match console::run_console(config) {
                    console::ConsoleResult::ReturnToMenu => {
                        // Loop back to render menu again
                        continue;
                    }
                    console::ConsoleResult::Boot(idx) => {
                        let mut opts = [0u8; 256];
                        let len = get_entry_options(config, idx, &mut opts);
                        return (idx, opts, len);
                    }
                    console::ConsoleResult::ManualBoot {
                        path,
                        path_len,
                        options,
                        options_len,
                    } => {
                        // Create a temporary entry for manual boot
                        let mut entry = config::BootEntry::empty();
                        entry.path[..path_len].copy_from_slice(&path[..path_len]);
                        entry.path_len = path_len;
                        entry.label[..7].copy_from_slice(b"Manual ");
                        entry.label_len = 7;
                        entry.options[..options_len]
                            .copy_from_slice(&options[..options_len]);
                        entry.options_len = options_len;
                        // Set default initramfs
                        let ifr = b"\\EFI\\OXIDE\\initramfs.cpio";
                        entry.initramfs_path[..ifr.len()].copy_from_slice(ifr);
                        entry.initramfs_path_len = ifr.len();
                        entry.valid = true;

                        // Add to config temporarily
                        if config.entry_count < config::MAX_ENTRIES {
                            let idx = config.entry_count;
                            config.entries[idx] = entry;
                            config.entry_count += 1;
                            let mut opts = [0u8; 256];
                            let len = get_entry_options(config, idx, &mut opts);
                            return (idx, opts, len);
                        }
                        // Config full — just boot first entry
                        let mut opts = [0u8; 256];
                        let len = get_entry_options(config, 0, &mut opts);
                        return (0, opts, len);
                    }
                }
            }
            input::MenuResult::Error => {
                // Shouldn't happen, but fall through to first entry
                let mut opts = [0u8; 256];
                let len = get_entry_options(config, 0, &mut opts);
                return (0, opts, len);
            }
        }
    }
}

/// Show error screen when no kernels are found
fn show_error_screen(width: usize, height: usize) {
    with_gop(|gop| {
        menu::render_error_screen(gop, width, height, "No bootable kernels found on ESP");
    });

    // Wait for key — C opens console, anything else halts
    loop {
        if let Some(key) = efi::read_key() {
            // — NeonRoot: VirtIO keyboard sets scan_code for all keys, check unicode_char directly
            if key.unicode_char != 0 {
                let c = key.unicode_char;
                if c == b'c' as u16 || c == b'C' as u16 {
                    let mut empty_config = config::BootConfig::empty();
                    let result = console::run_console(&mut empty_config);
                    match result {
                        console::ConsoleResult::ReturnToMenu => {
                            // Re-show error
                            with_gop(|gop| {
                                menu::render_error_screen(
                                    gop,
                                    width,
                                    height,
                                    "No bootable kernels found on ESP",
                                );
                            });
                            continue;
                        }
                        _ => return, // Console handled it
                    }
                }
                return;
            } else {
                return;
            }
        }
        efi::stall(50_000);
    }
}

/// Get the boot options bytes for an entry into a buffer. Returns length.
fn get_entry_options(config: &config::BootConfig, idx: usize, out: &mut [u8; 256]) -> usize {
    if idx < config.entry_count {
        let entry = &config.entries[idx];
        let len = entry.options_len.min(255);
        out[..len].copy_from_slice(&entry.options[..len]);
        len
    } else {
        0
    }
}

// ══════════════════════════════════════════════════════════════════
// Kernel & Initramfs Loading
// — SableWire: the file I/O layer that bridges firmware and kernel
// ══════════════════════════════════════════════════════════════════

/// Load kernel ELF from a boot entry's path into scratch arena
fn load_kernel_from_entry(entry: &config::BootEntry) -> Result<&'static [u8], &'static str> {
    if entry.path_len == 0 {
        return Err("Empty kernel path");
    }

    // — SableWire: use page-backed allocation for kernel images (can be 33MB+ in debug mode)
    // The 2MB scratch arena is for config files and small data only.
    match discovery::load_large_file_from_esp(&entry.path, entry.path_len) {
        Some(data) if !data.is_empty() => Ok(data),
        Some(_) => Err("Kernel file is empty"),
        None => {
            // Fallback: try the hardcoded default path
            // — SableWire: if the config lied, try the obvious place
            load_kernel_file_default()
        }
    }
}

/// Load kernel from the default hardcoded path (fallback)
fn load_kernel_file_default() -> Result<&'static [u8], &'static str> {
    let default_path = b"\\EFI\\OXIDE\\kernel.elf";
    discovery::load_large_file_from_esp(default_path, default_path.len())
        .ok_or("Kernel file not found")
}

/// Load initramfs from a boot entry's initramfs path, or the default path
/// — SableWire: uses page-backed loading because initramfs can exceed 2MB scratch arena
fn load_initramfs_from_entry(entry: &config::BootEntry) -> (u64, u64) {
    // Try entry-specific initramfs path first
    if entry.initramfs_path_len > 0 {
        if let Some(data) =
            discovery::load_large_file_from_esp(&entry.initramfs_path, entry.initramfs_path_len)
        {
            if !data.is_empty() {
                // — SableWire: data is already in allocated pages from load_large_file_from_esp,
                // return the physical address directly (no double-copy needed)
                let phys_addr = data.as_ptr() as u64;
                return (phys_addr, data.len() as u64);
            }
        }
    }

    // Fallback to default path
    let default_path = b"\\EFI\\OXIDE\\initramfs.cpio";
    match discovery::load_large_file_from_esp(default_path, default_path.len()) {
        Some(data) if !data.is_empty() => {
            let phys_addr = data.as_ptr() as u64;
            (phys_addr, data.len() as u64)
        }
        _ => (0, 0), // Non-fatal — SableWire: kernel can boot without initramfs
    }
}


// ══════════════════════════════════════════════════════════════════
// UEFI Utilities
// — SableWire: the plumbing behind the curtain
// ══════════════════════════════════════════════════════════════════

/// Get framebuffer info from GOP
fn get_framebuffer_info() -> Option<FramebufferInfo> {
    let gop_handle = efi::locate_handle_for_protocol(&efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID)?;
    let gop: *mut EfiGraphicsOutputProtocol = efi::handle_protocol(
        gop_handle,
        &efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
    )?;

    unsafe {
        let mode = &*(*gop).mode;
        let info = &*mode.info;

        let format = match info.pixel_format {
            efi::gop::EfiGraphicsPixelFormat::PixelRedGreenBlueReserved8BitPerColor => PixelFormat::Rgb,
            efi::gop::EfiGraphicsPixelFormat::PixelBlueGreenRedReserved8BitPerColor => PixelFormat::Bgr,
            _ => PixelFormat::Unknown,
        };

        // — GlassSignal: VirtIO-GPU-backed GOP sets frame_buffer_base = 0 because
        // it uses DMA resources, not a linear framebuffer. Reporting base=0 to the
        // kernel makes it write pixels to physical address 0 (wrong) and tricks the
        // VirtIO-GPU driver into thinking GOP is active (skipping real init).
        // Return None so the kernel falls through to VirtIO-GPU takeover.
        if mode.frame_buffer_base == 0 || mode.frame_buffer_size == 0 {
            return None;
        }

        Some(FramebufferInfo {
            base: mode.frame_buffer_base,
            size: mode.frame_buffer_size as u64,
            width: info.horizontal_resolution,
            height: info.vertical_resolution,
            stride: info.pixels_per_scan_line,
            bpp: 32,
            format,
        })
    }
}

/// Enumerate all available video modes from GOP
fn enumerate_video_modes() -> Option<boot_proto::VideoModeList> {
    use boot_proto::{MAX_VIDEO_MODES, VideoMode, VideoModeList};

    let gop_handle = efi::locate_handle_for_protocol(&efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID)?;
    let gop: *mut EfiGraphicsOutputProtocol = efi::handle_protocol(
        gop_handle,
        &efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
    )?;

    let mut mode_list = VideoModeList::empty();

    unsafe {
        let mode_ptr = (*gop).mode;
        let current_info = &*(*mode_ptr).info;
        let current_w = current_info.horizontal_resolution;
        let current_h = current_info.vertical_resolution;
        let current_stride = current_info.pixels_per_scan_line;
        let max_mode = (*mode_ptr).max_mode;

        for mode_num in 0..max_mode {
            if mode_list.count as usize >= MAX_VIDEO_MODES {
                break;
            }

            let mut info_size: usize = 0;
            let mut info_ptr: *const efi::gop::EfiGraphicsOutputModeInformation = ptr::null();
            let status = ((*gop).query_mode)(gop, mode_num, &mut info_size, &mut info_ptr);
            if efi::efi_error(status) || info_ptr.is_null() {
                continue;
            }

            let mode_info = &*info_ptr;
            let format = match mode_info.pixel_format {
                efi::gop::EfiGraphicsPixelFormat::PixelRedGreenBlueReserved8BitPerColor => PixelFormat::Rgb,
                efi::gop::EfiGraphicsPixelFormat::PixelBlueGreenRedReserved8BitPerColor => PixelFormat::Bgr,
                _ => PixelFormat::Unknown,
            };

            let bpp = 32u32;
            let width = mode_info.horizontal_resolution;
            let height = mode_info.vertical_resolution;
            let stride = mode_info.pixels_per_scan_line;
            let framebuffer_size = (stride as u64) * (height as u64) * ((bpp / 8) as u64);

            mode_list.modes[mode_list.count as usize] = VideoMode {
                mode_number: mode_num,
                width,
                height,
                bpp,
                format,
                stride,
                framebuffer_size,
            };

            if width == current_w && height == current_h && stride == current_stride {
                mode_list.current_mode = mode_list.count;
            }

            mode_list.count += 1;
        }
    }

    if mode_list.count > 0 {
        Some(mode_list)
    } else {
        None
    }
}

/// Find the ACPI RSDP physical address from UEFI configuration tables.
/// — SableWire: scanning the firmware config table for the ACPI anchor
pub fn find_rsdp_in_config_tables() -> u64 {
    let count = efi::config_table_count();

    // Prefer ACPI 2.0 (XSDP)
    for i in 0..count {
        if let Some(entry) = efi::config_table_entry(i) {
            if entry.vendor_guid == efi::ACPI_20_TABLE_GUID {
                return entry.vendor_table as u64;
            }
        }
    }

    // Fallback to ACPI 1.0 (RSDP)
    for i in 0..count {
        if let Some(entry) = efi::config_table_entry(i) {
            if entry.vendor_guid == efi::ACPI_TABLE_GUID {
                return entry.vendor_table as u64;
            }
        }
    }

    0
}

/// Get screen dimensions from GOP
fn get_screen_dimensions() -> (usize, usize) {
    let gop_handle = match efi::locate_handle_for_protocol(&efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID) {
        Some(h) => h,
        None => return (1024, 768), // — NeonRoot: sensible fallback
    };

    let gop: *mut EfiGraphicsOutputProtocol = match efi::handle_protocol(
        gop_handle,
        &efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
    ) {
        Some(g) => g,
        None => return (1024, 768),
    };

    unsafe {
        let mode = &*(*gop).mode;
        let info = &*mode.info;
        (info.horizontal_resolution as usize, info.vertical_resolution as usize)
    }
}

/// Helper to get GOP and run a closure with it
pub(crate) fn with_gop(f: impl FnOnce(*mut EfiGraphicsOutputProtocol)) {
    let gop_handle = match efi::locate_handle_for_protocol(&efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID) {
        Some(h) => h,
        None => return,
    };

    let gop: *mut EfiGraphicsOutputProtocol = match efi::handle_protocol(
        gop_handle,
        &efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
    ) {
        Some(g) => g,
        None => return,
    };

    f(gop);
}

/// Create boot info structure
fn create_boot_info(
    kernel_phys: u64,
    kernel_size: u64,
    pml4_phys: u64,
    memory_regions: &[MemoryRegion],
    framebuffer: Option<FramebufferInfo>,
    video_modes: Option<boot_proto::VideoModeList>,
    initramfs_phys: u64,
    initramfs_size: u64,
    rsdp_phys: u64,
) -> BootInfo {
    let mut info = BootInfo::empty();
    info.magic = BOOT_INFO_MAGIC;
    info.kernel_phys_base = kernel_phys;
    info.kernel_virt_base = KERNEL_VIRT_BASE;
    info.kernel_size = kernel_size;
    info.pml4_phys = pml4_phys;
    info.phys_map_base = PHYS_MAP_BASE;
    info.framebuffer = framebuffer;
    info.video_modes = video_modes;
    info.initramfs_phys = initramfs_phys;
    info.initramfs_size = initramfs_size;
    info.rsdp_physical_address = rsdp_phys;

    let count = memory_regions.len().min(MAX_MEMORY_REGIONS);
    info.memory_region_count = count as u64;
    for (i, region) in memory_regions.iter().take(count).enumerate() {
        info.memory_regions[i] = *region;
    }

    info
}

// ══════════════════════════════════════════════════════════════════
// OXIDE Logo Drawing (preserved from original)
// — NeonVale: the iconic OXIDE wordmark, pixel by pixel
// ══════════════════════════════════════════════════════════════════

/// Draw the OXIDE logo graphically
/// Made pub(crate) so menu.rs can call it for the boot menu header
pub(crate) fn draw_oxide_logo(gop: *mut EfiGraphicsOutputProtocol, width: usize, height: usize) {
    let logo_width = 500;
    let logo_height = 150;
    let start_x = if width >= logo_width {
        (width - logo_width) / 2
    } else {
        0
    };
    // — NeonVale: Logo goes at the TOP. Period. Small margin so it doesn't
    // kiss the bezel, then everything else flows below it.
    let start_y = 20;

    let oxide_orange = EfiBltPixel::new(255, 140, 0);
    let accent_cyan = EfiBltPixel::new(0, 255, 255);
    let bg_dark = EfiBltPixel::new(10, 10, 15);

    // Draw clean background for logo area
    font::fill_rect(gop, start_x, start_y, logo_width, logo_height, bg_dark);

    // Draw modern, clean OXIDE text
    let letter_y = start_y + 40;
    let letter_spacing = 85;
    draw_letter_o_modern(gop, start_x + 20, letter_y, oxide_orange, accent_cyan);
    draw_letter_x_modern(
        gop,
        start_x + 20 + letter_spacing,
        letter_y,
        oxide_orange,
        accent_cyan,
    );
    draw_letter_i_modern(
        gop,
        start_x + 20 + letter_spacing * 2,
        letter_y,
        oxide_orange,
        accent_cyan,
    );
    draw_letter_d_modern(
        gop,
        start_x + 20 + letter_spacing * 3,
        letter_y,
        oxide_orange,
        accent_cyan,
    );
    draw_letter_e_modern(
        gop,
        start_x + 20 + letter_spacing * 4,
        letter_y,
        oxide_orange,
        accent_cyan,
    );

    // Accent line underneath
    let line_y = start_y + logo_height - 20;
    for x in start_x + 20..start_x + logo_width - 20 {
        blt_fill(gop, accent_cyan, x, line_y, 1, 2);
    }
}

/// Helper to call GOP Blt with VideoFill
/// — NeonVale: the atomic pixel operation — one color, one rectangle
#[inline]
pub(crate) fn blt_fill(
    gop: *mut EfiGraphicsOutputProtocol,
    color: EfiBltPixel,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
) {
    unsafe {
        ((*gop).blt)(
            gop,
            &color,
            EfiBltOperation::BltVideoFill,
            0, 0,
            x, y,
            w, h,
            0,
        );
    }
}

/// Modern letter drawing functions with clean, bold design
/// — NeonVale: each glyph is a statement, not an apology

fn draw_letter_o_modern(
    gop: *mut EfiGraphicsOutputProtocol,
    x: usize,
    y: usize,
    primary: EfiBltPixel,
    _accent: EfiBltPixel,
) {
    let width = 60;
    let height = 70;
    let thickness = 8;

    for dy in 0..height {
        for dx in 0..width {
            let draw = if dy < thickness || dy >= height - thickness {
                dx >= 10 && dx < width - 10
            } else {
                dx < thickness || dx >= width - thickness
            };
            if draw {
                blt_fill(gop, primary, x + dx, y + dy, 1, 1);
            }
        }
    }
}

fn draw_letter_x_modern(
    gop: *mut EfiGraphicsOutputProtocol,
    x: usize,
    y: usize,
    primary: EfiBltPixel,
    _accent: EfiBltPixel,
) {
    let width = 60;
    let height = 70;
    let thickness = 8;

    for dy in 0..height {
        for dx in 0..width {
            let ratio = dy as f32 / height as f32;
            let diag1_x = (ratio * width as f32) as usize;
            let diag2_x = width - (ratio * width as f32) as usize;

            let draw = (dx >= diag1_x.saturating_sub(thickness / 2)
                && dx < diag1_x + thickness / 2)
                || (dx >= diag2_x.saturating_sub(thickness / 2) && dx < diag2_x + thickness / 2);

            if draw {
                blt_fill(gop, primary, x + dx, y + dy, 1, 1);
            }
        }
    }
}

fn draw_letter_i_modern(
    gop: *mut EfiGraphicsOutputProtocol,
    x: usize,
    y: usize,
    primary: EfiBltPixel,
    _accent: EfiBltPixel,
) {
    let width = 30;
    let height = 70;
    let thickness = 8;
    let bar_width = 12;

    for dy in 0..height {
        for dx in 0..width {
            let draw = if dy < thickness || dy >= height - thickness {
                true
            } else {
                dx >= (width - bar_width) / 2 && dx < (width + bar_width) / 2
            };
            if draw {
                blt_fill(gop, primary, x + dx, y + dy, 1, 1);
            }
        }
    }
}

fn draw_letter_d_modern(
    gop: *mut EfiGraphicsOutputProtocol,
    x: usize,
    y: usize,
    primary: EfiBltPixel,
    _accent: EfiBltPixel,
) {
    let width = 60;
    let height = 70;
    let thickness = 8;

    for dy in 0..height {
        for dx in 0..width {
            let draw = if dx < thickness {
                true
            } else if dy < thickness || dy >= height - thickness {
                dx >= thickness && dx < width - 10
            } else {
                dx >= width - thickness
            };
            if draw {
                blt_fill(gop, primary, x + dx, y + dy, 1, 1);
            }
        }
    }
}

fn draw_letter_e_modern(
    gop: *mut EfiGraphicsOutputProtocol,
    x: usize,
    y: usize,
    primary: EfiBltPixel,
    _accent: EfiBltPixel,
) {
    let width = 55;
    let height = 70;
    let thickness = 8;

    for dy in 0..height {
        for dx in 0..width {
            let draw = if dx < thickness {
                true
            } else {
                dy < thickness
                    || dy >= height - thickness
                    || (dy >= height / 2 - thickness / 2 && dy < height / 2 + thickness / 2)
            };
            if draw {
                blt_fill(gop, primary, x + dx, y + dy, 1, 1);
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════
// Logging & Panic
// ══════════════════════════════════════════════════════════════════

/// Log a message to UEFI console
pub(crate) fn log(msg: &str) {
    efi::print_ascii(msg);
    efi::print_ucs2(&[b'\r' as u16, b'\n' as u16, 0]);
}

/// Log a formatted message
pub fn log_fmt(args: core::fmt::Arguments) {
    let mut buf = FmtBuf::<512>::new();
    let _ = buf.write_fmt(args);
    efi::print_ascii(buf.as_str());
    efi::print_ucs2(&[b'\r' as u16, b'\n' as u16, 0]);
}

/// Halt the CPU
fn halt() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt") };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log("");
    log("BOOTLOADER PANIC!");
    let mut buf = FmtBuf::<512>::new();
    let _ = write!(buf, "{}", info);
    log(buf.as_str());
    halt()
}
