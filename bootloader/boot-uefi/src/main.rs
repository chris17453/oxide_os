//! OXIDE UEFI Bootloader
//!
//! Loads the OXIDE kernel and transfers control to it.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use alloc::format;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr;

use boot_proto::{
    BootInfo, FramebufferInfo, MemoryRegion, MemoryType, PixelFormat,
    BOOT_INFO_MAGIC, MAX_MEMORY_REGIONS, KERNEL_VIRT_BASE, PHYS_MAP_BASE,
};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, BltOp, BltPixel};
use uefi::proto::console::text::{Key, ScanCode};
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{AllocateType, MemoryType as UefiMemoryType};
use uefi::mem::memory_map::MemoryMap;
use uefi::{Char16};

mod elf;
mod paging;

/// Kernel file path on the EFI partition
const KERNEL_PATH: &str = "\\EFI\\OXIDE\\kernel.elf";

/// Initramfs file path on the EFI partition
const INITRAMFS_PATH: &str = "\\EFI\\OXIDE\\initramfs.cpio";

/// Page size
const PAGE_SIZE: u64 = 4096;

/// Boot screen timeout in seconds
const BOOT_TIMEOUT_SECONDS: u8 = 15;

/// Boot display modes
#[derive(Clone, Copy, PartialEq)]
enum DisplayMode {
    Graphical,
    Ascii,
    Text,  // Traditional text-only
}

/// Boot configuration
struct BootConfig {
    display_mode: DisplayMode,
    timeout_seconds: u8,
}

#[entry]
fn main() -> Status {
    // Initialize UEFI services
    uefi::helpers::init().expect("Failed to initialize UEFI helpers");

    // Default boot configuration
    let mut config = BootConfig {
        display_mode: DisplayMode::Graphical,
        timeout_seconds: BOOT_TIMEOUT_SECONDS,
    };

    // Show interactive boot screen and handle user input
    config = show_boot_screen(config);
    
    // Clear screen for boot process
    clear_screen();
    
    // Display logo based on selected mode
    match config.display_mode {
        DisplayMode::Graphical => {
            if !display_graphical_logo() {
                // Fallback to ASCII if graphics not available
                display_ascii_logo();
            }
        }
        DisplayMode::Ascii => display_ascii_logo(),
        DisplayMode::Text => {
            // Skip logo, go straight to text boot
            log("OXIDE UEFI Bootloader - Text Mode");
            log("=====================================");
        }
    }
    
    // Initialize progress tracking
    let total_steps = 12;
    let mut current_step = 0;

    // Step 1: Load kernel
    update_progress(&mut current_step, total_steps, "Loading kernel file...");
    let kernel_data = match load_kernel_file() {
        Ok(data) => data,
        Err(e) => {
            log_fmt(format_args!("[ERROR] Failed to load kernel: {}", e));
            halt();
        }
    };

    // Step 2: Parse ELF
    update_progress(&mut current_step, total_steps, "Parsing ELF headers...");
    let elf_info = match elf::parse_elf(&kernel_data) {
        Ok(info) => info,
        Err(e) => {
            log_fmt(format_args!("[ERROR] Failed to parse ELF: {}", e));
            halt();
        }
    };

    // Step 3: Allocate memory for kernel
    update_progress(&mut current_step, total_steps, "Allocating kernel memory...");
    let kernel_pages = (elf_info.load_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let kernel_phys = allocate_pages(kernel_pages as usize)
        .expect("Failed to allocate memory for kernel");

    // Step 4: Load kernel segments
    update_progress(&mut current_step, total_steps, "Loading kernel segments...");
    elf::load_segments(&kernel_data, &elf_info, kernel_phys);

    // Step 5: Load initramfs
    update_progress(&mut current_step, total_steps, "Loading initramfs...");
    let (initramfs_phys, initramfs_size) = match load_initramfs() {
        Ok((phys, size)) => (phys, size),
        Err(_) => (0, 0) // Non-fatal
    };

    // Step 6: Initialize graphics
    update_progress(&mut current_step, total_steps, "Initializing graphics...");
    let fb_info = get_framebuffer_info();

    // Step 7: Enumerate video modes
    update_progress(&mut current_step, total_steps, "Enumerating video modes...");
    let video_modes = enumerate_video_modes();

    // Step 8: Set up page tables
    update_progress(&mut current_step, total_steps, "Setting up page tables...");
    let pml4_phys = paging::setup_page_tables(kernel_phys, elf_info.load_size);

    // Step 9: Get memory map
    update_progress(&mut current_step, total_steps, "Getting memory map...");
    let memory_regions = get_memory_map();

    // Step 10: Create boot info
    update_progress(&mut current_step, total_steps, "Creating boot information...");
    let boot_info = create_boot_info(
        kernel_phys,
        elf_info.load_size,
        pml4_phys,
        &memory_regions,
        fb_info,
        video_modes,
        initramfs_phys,
        initramfs_size,
    );

    // Step 11: Allocate boot info page
    update_progress(&mut current_step, total_steps, "Finalizing boot setup...");
    let boot_info_phys = allocate_pages(1).expect("Failed to allocate boot info page");
    unsafe {
        ptr::write(boot_info_phys as *mut BootInfo, boot_info);
    }

    // Calculate addresses for kernel jump
    let kernel_entry_virt = KERNEL_VIRT_BASE + (elf_info.entry - elf_info.load_base);
    let boot_info_virt = PHYS_MAP_BASE + boot_info_phys;

    // Step 12: Transfer control to kernel
    update_progress(&mut current_step, total_steps, "Transferring control to kernel...");
    
    // Show final boot message
    log("");
    log("🚀 Boot complete! Launching OXIDE OS...");
    log("");

    // Exit boot services - after this, no more UEFI calls!
    let st = uefi::table::system_table_boot().expect("Boot services not available");
    unsafe {
        st.exit_boot_services(uefi::table::boot::MemoryType::LOADER_DATA);
    }

    // Switch to our page tables and jump to kernel
    // Use inline assembly to ensure correct register setup
    // Use explicit registers to prevent the compiler from reusing registers
    unsafe {
        core::arch::asm!(
            // Load new CR3 (switch page tables)
            "mov cr3, rax",
            // Jump to kernel with boot_info in rdi (System V ABI)
            "mov rdi, rsi",
            "jmp rcx",
            in("rax") pml4_phys,
            in("rsi") boot_info_virt,
            in("rcx") kernel_entry_virt,
            options(noreturn)
        );
    }
}

/// Clear the screen
fn clear_screen() {
    if let Some(mut st) = uefi::table::system_table_boot() {
        let _ = st.stdout().clear();
    }
}

/// Show interactive boot screen with user options
fn show_boot_screen(mut config: BootConfig) -> BootConfig {
    // Initialize timer for accurate timeout tracking
    init_timer();
    clear_screen();
    
    // Display boot options screen
    log("");
    log("        ██████╗ ██╗  ██╗██╗██████╗ ███████╗");
    log("       ██╔═══██╗╚██╗██╔╝██║██╔══██╗██╔════╝");
    log("       ██║   ██║ ╚███╔╝ ██║██║  ██║█████╗  ");
    log("       ██║   ██║ ██╔██╗ ██║██║  ██║██╔══╝  ");
    log("       ╚██████╔╝██╔╝ ██╗██║██████╔╝███████╗");
    log("        ╚═════╝ ╚═╝  ╚═╝╚═╝╚═════╝ ╚══════╝");
    log("");
    log("            Operating System - Version 0.1.0");
    log("");
    log("═══════════════════════════════════════════════════");
    // Use format! to include dynamic timeout
    let timeout_msg = format!("  Boot Options - Press key within {} seconds:", config.timeout_seconds);
    log(&timeout_msg);
    log("═══════════════════════════════════════════════════");
    log("");
    
    match config.display_mode {
        DisplayMode::Graphical => log("  Current Mode: [GRAPHICAL] ASCII  TEXT"),
        DisplayMode::Ascii => log("  Current Mode:  GRAPHICAL [ASCII]  TEXT"),
        DisplayMode::Text => log("  Current Mode:  GRAPHICAL  ASCII [TEXT]"),
    }
    log("");
    log("  [TAB]   - Cycle display modes");
    log("  [ESC]   - Skip to text mode");
    log("  [ENTER] - Continue with current mode");
    log("");
    
    // Wait for user input with timeout
    let start_time = get_time_ms();
    let timeout_ms = config.timeout_seconds as u64 * 1000;
    
    loop {
        // Check for timeout (use saturating_sub to prevent overflow)
        if get_time_ms().saturating_sub(start_time) > timeout_ms {
            break;
        }
        
        // Check for key input
        if let Some(key) = check_key_press() {
            match key {
                Key::Special(ScanCode::ESCAPE) => {
                    config.display_mode = DisplayMode::Text;
                    break;
                }
                Key::Printable(c) if c == Char16::try_from('\t').unwrap_or(Char16::try_from(' ').unwrap()) => {
                    config.display_mode = match config.display_mode {
                        DisplayMode::Graphical => DisplayMode::Ascii,
                        DisplayMode::Ascii => DisplayMode::Text,
                        DisplayMode::Text => DisplayMode::Graphical,
                    };
                    // Refresh display
                    return show_boot_screen(config);
                }
                Key::Printable(c) if c == Char16::try_from('\r').unwrap_or(Char16::try_from(' ').unwrap()) || c == Char16::try_from('\n').unwrap_or(Char16::try_from(' ').unwrap()) => {
                    break;
                }
                _ => {}
            }
        }
        
        // Small delay to prevent busy waiting
        spin_wait_ms(50);
    }
    
    config
}

/// Display graphical logo (if graphics available)
fn display_graphical_logo() -> bool {
    // Try to get graphics output protocol
    let st = match uefi::table::system_table_boot() {
        Some(st) => st,
        None => return false,
    };
    let bs = st.boot_services();
    
    // Get graphics output protocol
    let gop_handle = match bs.get_handle_for_protocol::<GraphicsOutput>() {
        Ok(handle) => handle,
        Err(_) => return false,
    };
    
    let mut gop = match bs.open_protocol_exclusive::<GraphicsOutput>(gop_handle) {
        Ok(gop) => gop,
        Err(_) => return false,
    };
    
    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    
    // Draw a simple graphical logo
    draw_oxide_logo(&mut *gop, width, height);
    
    // Display text information
    log("");
    log("            Operating System - Version 0.1.0");
    log("              UEFI Bootloader Starting...");
    log("");
    
    true
}

/// Draw the OXIDE logo graphically
fn draw_oxide_logo(gop: &mut GraphicsOutput, width: usize, height: usize) {
    let logo_width = 400;
    let logo_height = 200;
    let start_x = (width - logo_width) / 2;
    let start_y = (height - logo_height) / 2;
    
    // Create a simple geometric logo
    let oxide_color = BltPixel::new(0, 150, 255); // Blue
    let bg_color = BltPixel::new(20, 20, 20);     // Dark background
    
    // Draw background rectangle
    for y in start_y..start_y + logo_height {
        for x in start_x..start_x + logo_width {
            let _ = gop.blt(BltOp::VideoFill {
                color: bg_color,
                dest: (x, y),
                dims: (1, 1),
            });
        }
    }
    
    // Draw OXIDE text pattern (simplified geometric representation)
    // This is a basic implementation - in a real system you'd use proper font rendering
    draw_letter_o(gop, start_x + 50, start_y + 60, oxide_color);
    draw_letter_x(gop, start_x + 120, start_y + 60, oxide_color);
    draw_letter_i(gop, start_x + 190, start_y + 60, oxide_color);
    draw_letter_d(gop, start_x + 230, start_y + 60, oxide_color);
    draw_letter_e(gop, start_x + 300, start_y + 60, oxide_color);
}

/// Draw letter shapes (simplified geometric versions)
fn draw_letter_o(gop: &mut GraphicsOutput, x: usize, y: usize, color: BltPixel) {
    // Draw a circle-like shape for 'O'
    for dy in 0..60 {
        for dx in 0..40 {
            if (dx == 0 || dx == 39 || dy == 0 || dy == 59) ||
               (dx > 5 && dx < 35 && (dy < 8 || dy > 52)) {
                let _ = gop.blt(BltOp::VideoFill {
                    color,
                    dest: (x + dx, y + dy),
                    dims: (1, 1),
                });
            }
        }
    }
}

fn draw_letter_x(gop: &mut GraphicsOutput, x: usize, y: usize, color: BltPixel) {
    // Draw an X shape
    for dy in 0..60 {
        for dx in 0..40 {
            if dx == dy * 40 / 60 || dx == 40 - dy * 40 / 60 {
                let _ = gop.blt(BltOp::VideoFill {
                    color,
                    dest: (x + dx, y + dy),
                    dims: (2, 2),
                });
            }
        }
    }
}

fn draw_letter_i(gop: &mut GraphicsOutput, x: usize, y: usize, color: BltPixel) {
    // Draw an I shape
    for dy in 0..60 {
        for dx in 0..20 {
            if dx > 6 && dx < 14 {
                let _ = gop.blt(BltOp::VideoFill {
                    color,
                    dest: (x + dx, y + dy),
                    dims: (1, 1),
                });
            }
        }
    }
}

fn draw_letter_d(gop: &mut GraphicsOutput, x: usize, y: usize, color: BltPixel) {
    // Draw a D shape
    for dy in 0..60 {
        for dx in 0..40 {
            if dx == 0 || (dx > 20 && ((dy < 8 && dx < 35) || (dy > 52 && dx < 35) || 
                                      (dy >= 8 && dy <= 52 && dx == 35))) {
                let _ = gop.blt(BltOp::VideoFill {
                    color,
                    dest: (x + dx, y + dy),
                    dims: (1, 1),
                });
            }
        }
    }
}

fn draw_letter_e(gop: &mut GraphicsOutput, x: usize, y: usize, color: BltPixel) {
    // Draw an E shape
    for dy in 0..60 {
        for dx in 0..35 {
            if dx == 0 || dy == 0 || dy == 30 || dy == 59 {
                let _ = gop.blt(BltOp::VideoFill {
                    color,
                    dest: (x + dx, y + dy),
                    dims: (1, 1),
                });
            }
        }
    }
}

/// Display ASCII logo
fn display_ascii_logo() {
    log("");
    log("        ██████╗ ██╗  ██╗██╗██████╗ ███████╗");
    log("       ██╔═══██╗╚██╗██╔╝██║██╔══██╗██╔════╝");
    log("       ██║   ██║ ╚███╔╝ ██║██║  ██║█████╗  ");
    log("       ██║   ██║ ██╔██╗ ██║██║  ██║██╔══╝  ");
    log("       ╚██████╔╝██╔╝ ██╗██║██████╔╝███████╗");
    log("        ╚═════╝ ╚═╝  ╚═╝╚═╝╚═════╝ ╚══════╝");
    log("");
    log("            Operating System - Version 0.1.0");
    log("              UEFI Bootloader Starting...");
    log("");
}

/// Display a progress bar
fn display_progress(current: usize, total: usize, message: &str) {
    let progress_width = 40;
    let filled = (current * progress_width) / total;
    let empty = progress_width - filled;
    
    let mut progress_bar = alloc::string::String::new();
    progress_bar.push('[');
    for _ in 0..filled {
        progress_bar.push('█');
    }
    for _ in 0..empty {
        progress_bar.push('░');
    }
    progress_bar.push(']');
    
    let percentage = (current * 100) / total;
    
    // Clear previous lines and display new progress
    if let Some(mut st) = uefi::table::system_table_boot() {
        // Move cursor up and clear lines
        let _ = st.stdout().write_str("\x1b[2K\r"); // Clear current line
        let _ = st.stdout().write_str("\x1b[1A\x1b[2K\r"); // Move up and clear previous line
    }
    
    log_fmt(format_args!("{} {}% {}", progress_bar, percentage, message));
    log("");
}

/// Update progress and display current operation
fn update_progress(step: &mut usize, total: usize, message: &str) {
    *step += 1;
    display_progress(*step, total, message);
}

/// Check for key press without blocking
fn check_key_press() -> Option<Key> {
    let mut st = uefi::table::system_table_boot()?;
    let stdin = st.stdin();
    
    // Check if a key is available
    match stdin.read_key() {
        Ok(Some(key)) => Some(key),
        _ => None,
    }
}

/// Simple elapsed time tracker
static mut ELAPSED_MS: u64 = 0;

fn init_timer() {
    unsafe {
        ELAPSED_MS = 0;
    }
}

fn get_time_ms() -> u64 {
    unsafe { ELAPSED_MS }
}

/// Spin wait for specified milliseconds
/// Uses a calibrated busy loop since UEFI stall may not work reliably in QEMU
fn spin_wait_ms(ms: u64) {
    // Try UEFI stall first
    if let Some(st) = uefi::table::system_table_boot() {
        st.boot_services().stall(ms as usize * 1000);
    }

    // Also do a CPU busy-wait as backup (roughly calibrated for ~1ms per 100000 iterations)
    // This ensures we actually wait even if stall doesn't work
    for _ in 0..(ms * 50000) {
        unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
    }

    unsafe { ELAPSED_MS += ms; }
}

/// Load the kernel file from the EFI system partition
fn load_kernel_file() -> Result<Vec<u8>, &'static str> {
    let st = uefi::table::system_table_boot().ok_or("No boot services")?;
    let bs = st.boot_services();

    // Get the filesystem protocol
    let fs_handle = bs
        .get_handle_for_protocol::<SimpleFileSystem>()
        .map_err(|_| "No filesystem")?;

    let mut fs = bs
        .open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
        .map_err(|_| "Failed to open filesystem")?;

    // Open the root directory
    let mut root = fs.open_volume().map_err(|_| "Failed to open volume")?;

    // Open the kernel file
    let kernel_handle = root
        .open(
            cstr16!("\\EFI\\OXIDE\\kernel.elf"),
            FileMode::Read,
            FileAttribute::empty(),
        )
        .map_err(|_| "Kernel file not found")?;

    let mut kernel_file = match kernel_handle.into_type().map_err(|_| "Invalid file type")? {
        FileType::Regular(f) => f,
        FileType::Dir(_) => return Err("Kernel path is a directory"),
    };

    // Get file size
    let mut info_buf = [0u8; 256];
    let info = kernel_file
        .get_info::<FileInfo>(&mut info_buf)
        .map_err(|_| "Failed to get file info")?;
    let file_size = info.file_size() as usize;

    // Read the file
    let mut data = alloc::vec![0u8; file_size];
    kernel_file
        .read(&mut data)
        .map_err(|_| "Failed to read kernel file")?;

    Ok(data)
}

/// Load the initramfs file from the EFI system partition
/// Returns (physical address, size) of the loaded initramfs
fn load_initramfs() -> Result<(u64, u64), &'static str> {
    let st = uefi::table::system_table_boot().ok_or("No boot services")?;
    let bs = st.boot_services();

    // Get the filesystem protocol
    let fs_handle = bs
        .get_handle_for_protocol::<SimpleFileSystem>()
        .map_err(|_| "No filesystem")?;

    let mut fs = bs
        .open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
        .map_err(|_| "Failed to open filesystem")?;

    // Open the root directory
    let mut root = fs.open_volume().map_err(|_| "Failed to open volume")?;

    // Open the initramfs file
    let initramfs_handle = root
        .open(
            cstr16!("\\EFI\\OXIDE\\initramfs.cpio"),
            FileMode::Read,
            FileAttribute::empty(),
        )
        .map_err(|_| "Initramfs file not found")?;

    let mut initramfs_file = match initramfs_handle.into_type().map_err(|_| "Invalid file type")? {
        FileType::Regular(f) => f,
        FileType::Dir(_) => return Err("Initramfs path is a directory"),
    };

    // Get file size
    let mut info_buf = [0u8; 256];
    let info = initramfs_file
        .get_info::<FileInfo>(&mut info_buf)
        .map_err(|_| "Failed to get file info")?;
    let file_size = info.file_size() as u64;

    if file_size == 0 {
        return Err("Initramfs is empty");
    }

    // Allocate memory for the initramfs
    let pages = (file_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let phys_addr = allocate_pages(pages as usize).ok_or("Failed to allocate initramfs memory")?;

    // Read directly into the allocated memory
    let buffer = unsafe { core::slice::from_raw_parts_mut(phys_addr as *mut u8, file_size as usize) };
    initramfs_file
        .read(buffer)
        .map_err(|_| "Failed to read initramfs file")?;

    Ok((phys_addr, file_size))
}

/// Allocate pages of memory
fn allocate_pages(count: usize) -> Option<u64> {
    let st = uefi::table::system_table_boot()?;
    let bs = st.boot_services();

    bs.allocate_pages(
        AllocateType::AnyPages,
        UefiMemoryType::LOADER_DATA,
        count,
    )
    .ok()
}

/// Get the memory map from UEFI
fn get_memory_map() -> Vec<MemoryRegion> {
    let st = uefi::table::system_table_boot().expect("Boot services not available");
    let bs = st.boot_services();

    let mmap = bs.memory_map(UefiMemoryType::LOADER_DATA).expect("Failed to get memory map");

    let mut regions = Vec::new();

    for desc in mmap.entries() {
        let ty = match desc.ty {
            UefiMemoryType::CONVENTIONAL => MemoryType::Usable,
            UefiMemoryType::BOOT_SERVICES_CODE | UefiMemoryType::BOOT_SERVICES_DATA => {
                MemoryType::BootServices
            }
            UefiMemoryType::ACPI_RECLAIM => MemoryType::AcpiReclaimable,
            UefiMemoryType::ACPI_NON_VOLATILE => MemoryType::AcpiNvs,
            UefiMemoryType::LOADER_CODE | UefiMemoryType::LOADER_DATA => MemoryType::Bootloader,
            _ => MemoryType::Reserved,
        };

        regions.push(MemoryRegion::new(
            desc.phys_start,
            desc.page_count * PAGE_SIZE,
            ty,
        ));
    }

    regions
}

/// Get framebuffer info from GOP
fn get_framebuffer_info() -> Option<FramebufferInfo> {
    let st = uefi::table::system_table_boot()?;
    let bs = st.boot_services();

    let gop_handle = bs.get_handle_for_protocol::<GraphicsOutput>().ok()?;
    let mut gop = bs.open_protocol_exclusive::<GraphicsOutput>(gop_handle).ok()?;

    let mode = gop.current_mode_info();
    let mut fb = gop.frame_buffer();

    let format = match mode.pixel_format() {
        uefi::proto::console::gop::PixelFormat::Rgb => PixelFormat::Rgb,
        uefi::proto::console::gop::PixelFormat::Bgr => PixelFormat::Bgr,
        _ => PixelFormat::Unknown,
    };

    Some(FramebufferInfo {
        base: fb.as_mut_ptr() as u64,
        size: fb.size() as u64,
        width: mode.resolution().0 as u32,
        height: mode.resolution().1 as u32,
        stride: mode.stride() as u32,
        bpp: 32,
        format,
    })
}

/// Enumerate all available video modes from GOP
fn enumerate_video_modes() -> Option<boot_proto::VideoModeList> {
    use boot_proto::{VideoMode, VideoModeList, MAX_VIDEO_MODES};

    let st = uefi::table::system_table_boot()?;
    let bs = st.boot_services();

    let gop_handle = bs.get_handle_for_protocol::<GraphicsOutput>().ok()?;
    let gop = bs.open_protocol_exclusive::<GraphicsOutput>(gop_handle).ok()?;

    let mut mode_list = VideoModeList::empty();

    // Get current mode info to find which mode is active
    let current_info = gop.current_mode_info();
    let current_res = current_info.resolution();

    // Iterate through all modes using the modes() iterator
    for (mode_num, mode) in gop.modes().enumerate() {
        if mode_list.count as usize >= MAX_VIDEO_MODES {
            break;
        }

        let mode_info = mode.info();

        let format = match mode_info.pixel_format() {
            uefi::proto::console::gop::PixelFormat::Rgb => PixelFormat::Rgb,
            uefi::proto::console::gop::PixelFormat::Bgr => PixelFormat::Bgr,
            _ => PixelFormat::Unknown,
        };

        // Calculate BPP based on format
        let bpp = match mode_info.pixel_format() {
            uefi::proto::console::gop::PixelFormat::Rgb |
            uefi::proto::console::gop::PixelFormat::Bgr => 32,
            _ => 32, // Default assumption
        };

        let (width, height) = mode_info.resolution();
        let stride = mode_info.stride() as u32;
        let framebuffer_size = (stride as u64) * (height as u64) * ((bpp / 8) as u64);

        mode_list.modes[mode_list.count as usize] = VideoMode {
            mode_number: mode_num as u32,
            width: width as u32,
            height: height as u32,
            bpp,
            format,
            stride,
            framebuffer_size,
        };

        // Track which mode is current (compare by resolution as a heuristic)
        if mode_info.resolution() == current_res && mode_info.stride() == current_info.stride() {
            mode_list.current_mode = mode_list.count;
        }

        mode_list.count += 1;
    }

    if mode_list.count > 0 {
        Some(mode_list)
    } else {
        None
    }
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

    let count = memory_regions.len().min(MAX_MEMORY_REGIONS);
    info.memory_region_count = count as u64;
    for (i, region) in memory_regions.iter().take(count).enumerate() {
        info.memory_regions[i] = *region;
    }

    info
}

/// Log a message to UEFI console
fn log(msg: &str) {
    if let Some(mut st) = uefi::table::system_table_boot() {
        let _ = st.stdout().write_str(msg);
        let _ = st.stdout().write_str("\r\n");
    }
}

/// Log a formatted message
fn log_fmt(args: core::fmt::Arguments) {
    if let Some(mut st) = uefi::table::system_table_boot() {
        let _ = st.stdout().write_fmt(args);
        let _ = st.stdout().write_str("\r\n");
    }
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
    if let Some(mut st) = uefi::table::system_table_boot() {
        let _ = st.stdout().write_fmt(format_args!("{}\r\n", info));
    }
    halt()
}
