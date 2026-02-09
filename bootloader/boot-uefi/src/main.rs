//! OXIDE UEFI Bootloader
//!
//! Loads the OXIDE kernel and transfers control to it.

#![no_std]
#![no_main]
#![allow(unused)]
#![allow(deprecated)] // uefi-rs API migration pending

extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr;

use boot_proto::{
    BOOT_INFO_MAGIC, BootInfo, FramebufferInfo, KERNEL_VIRT_BASE, MAX_MEMORY_REGIONS, MemoryRegion,
    MemoryType, PHYS_MAP_BASE, PixelFormat,
};
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::{BltOp, BltPixel, GraphicsOutput};
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{AllocateType, MemoryType as UefiMemoryType};

mod elf;
mod paging;

/// Kernel file path on the EFI partition
const KERNEL_PATH: &str = "\\EFI\\OXIDE\\kernel.elf";

/// Initramfs file path on the EFI partition
const INITRAMFS_PATH: &str = "\\EFI\\OXIDE\\initramfs.cpio";

/// Page size
const PAGE_SIZE: u64 = 4096;

/// Kernel stack size (256KB) — WireSaint: enough headroom for deep call chains + debug
const KERNEL_STACK_SIZE: usize = 256 * 1024;

#[entry]
fn main() -> Status {
    // Initialize UEFI services
    uefi::helpers::init().expect("Failed to initialize UEFI helpers");

    // Show graphical logo (non-blocking) and continue boot
    let _ = display_graphical_logo();

    // Clear screen for boot process
    clear_screen();

    // Load kernel
    let kernel_data = match load_kernel_file() {
        Ok(data) => data,
        Err(e) => {
            log_fmt(format_args!("[ERROR] Failed to load kernel: {}", e));
            halt();
        }
    };

    // Parse ELF
    let elf_info = match elf::parse_elf(&kernel_data) {
        Ok(info) => info,
        Err(e) => {
            log_fmt(format_args!("[ERROR] Failed to parse ELF: {}", e));
            halt();
        }
    };

    // Allocate memory for kernel
    let kernel_pages = (elf_info.load_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let kernel_phys =
        allocate_pages(kernel_pages as usize).expect("Failed to allocate memory for kernel");

    // Load kernel segments
    elf::load_segments(&kernel_data, &elf_info, kernel_phys);

    // Load initramfs
    let (initramfs_phys, initramfs_size) = match load_initramfs() {
        Ok((phys, size)) => (phys, size),
        Err(_) => (0, 0), // Non-fatal
    };

    // Initialize graphics
    let fb_info = get_framebuffer_info();

    // Enumerate video modes
    let video_modes = enumerate_video_modes();

    // Set up page tables
    let pml4_phys = paging::setup_page_tables(kernel_phys, elf_info.load_size);

    // Allocate boot info pages
    // BootInfo is larger than one page (~5KB due to memory_regions and video_modes arrays),
    // so we must allocate 2 pages to avoid overwriting adjacent page table memory
    let boot_info_phys = allocate_pages(2).expect("Failed to allocate boot info pages");

    // Allocate kernel stack (256KB) — GraveShift: UEFI stack is tiny & unmapped post-jump
    let stack_pages = KERNEL_STACK_SIZE / PAGE_SIZE as usize;
    let stack_phys = allocate_pages(stack_pages).expect("Failed to allocate kernel stack");
    let stack_top_virt = PHYS_MAP_BASE + stack_phys + KERNEL_STACK_SIZE as u64;

    // [DEBUG] Log stack allocation — ColdCipher: Verify memory map correctness
    use core::fmt::Write;
    let mut debug_msg = [0u8; 100];
    let mut cursor = 0;
    for &b in b"[BOOT-ALLOC] Stack: 0x" {
        debug_msg[cursor] = b;
        cursor += 1;
    }
    for i in (0..16).rev() {
        let nibble = ((stack_phys >> (i * 4)) & 0xF) as u8;
        debug_msg[cursor] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
        cursor += 1;
    }
    for &b in b" size=" {
        debug_msg[cursor] = b;
        cursor += 1;
    }
    let size = KERNEL_STACK_SIZE as u64;
    for i in (0..16).rev() {
        let nibble = ((size >> (i * 4)) & 0xF) as u8;
        debug_msg[cursor] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
        cursor += 1;
    }
    debug_msg[cursor] = b'\n';
    cursor += 1;
    log(core::str::from_utf8(&debug_msg[..cursor]).unwrap_or("???"));

    // Get memory map AFTER all UEFI allocations
    // CRITICAL: This must be the last step that performs UEFI allocations.
    // The memory map must accurately reflect page table pages, boot info pages,
    // and all other LOADER_DATA allocations so the kernel doesn't reclaim them
    // into the buddy allocator's free list (which would corrupt page tables).
    let mut memory_regions = get_memory_map();

    // [FIX] Verify stack allocation is in memory map — ColdCipher
    // UEFI sometimes reports LOADER_DATA starting AFTER our allocation,
    // leaving a gap that the kernel will try to use, causing corruption.
    let stack_end = stack_phys + KERNEL_STACK_SIZE as u64;
    let mut stack_covered = false;
    for region in &memory_regions {
        if region.ty == MemoryType::Bootloader {
            if region.start <= stack_phys && region.start + region.len >= stack_end {
                stack_covered = true;
                break;
            }
        }
    }

    if !stack_covered {
        log("[BOOT-FIX] Stack allocation NOT in memory map - manually adding it");
        // Insert stack region into memory map as Bootloader type
        memory_regions.push(MemoryRegion::new(
            stack_phys,
            KERNEL_STACK_SIZE as u64,
            MemoryType::Bootloader,
        ));
    }

    // Create boot info (no UEFI allocations - struct is on stack)
    // Extract RSDP physical address from UEFI configuration tables
    // — SableWire: tapping the firmware's ACPI root before we burn the bridge
    let rsdp_phys = find_rsdp_in_config_tables();

    let boot_info = create_boot_info(
        kernel_phys,
        elf_info.load_size,
        pml4_phys,
        &memory_regions,
        fb_info,
        video_modes,
        initramfs_phys,
        initramfs_size,
        rsdp_phys,
    );

    // Write boot info to the pre-allocated pages
    unsafe {
        ptr::write(boot_info_phys as *mut BootInfo, boot_info);
    }

    // Calculate addresses for kernel jump
    let kernel_entry_virt = KERNEL_VIRT_BASE + (elf_info.entry - elf_info.load_base);
    let boot_info_virt = PHYS_MAP_BASE + boot_info_phys;

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
            // Set up kernel stack (RSP must be set AFTER CR3 for virtual address)
            "mov rsp, rdx",
            // Jump to kernel with boot_info in rdi (System V ABI)
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

/// Clear the screen
fn clear_screen() {
    if let Some(mut st) = uefi::table::system_table_boot() {
        let _ = st.stdout().clear();
    }
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
    let logo_width = 500;
    let logo_height = 150;
    let start_x = (width - logo_width) / 2;
    let start_y = height / 3;

    // Modern color scheme - cyberpunk aesthetic
    // — NeonVale: these colors will burn their retinas in the best way possible
    let oxide_orange = BltPixel::new(255, 140, 0); // Primary brand color
    let accent_cyan = BltPixel::new(0, 255, 255); // Accent glow
    let bg_dark = BltPixel::new(10, 10, 15); // Deep background

    // Draw clean background
    for y in start_y..start_y + logo_height {
        for x in start_x..start_x + logo_width {
            let _ = gop.blt(BltOp::VideoFill {
                color: bg_dark,
                dest: (x, y),
                dims: (1, 1),
            });
        }
    }

    // Draw modern, clean OXIDE text with better spacing and proportions
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

    // Draw accent line underneath
    let line_y = start_y + logo_height - 20;
    for x in start_x + 20..start_x + logo_width - 20 {
        let _ = gop.blt(BltOp::VideoFill {
            color: accent_cyan,
            dest: (x, line_y),
            dims: (1, 2),
        });
    }
}

/// Modern letter drawing functions with clean, bold design
/// — NeonVale: each glyph is a statement, not an apology

fn draw_letter_o_modern(
    gop: &mut GraphicsOutput,
    x: usize,
    y: usize,
    primary: BltPixel,
    accent: BltPixel,
) {
    let width = 60;
    let height = 70;
    let thickness = 8;

    // Draw rounded O with clean lines
    for dy in 0..height {
        for dx in 0..width {
            let color = if dy < thickness || dy >= height - thickness {
                // Top and bottom bars
                if dx >= 10 && dx < width - 10 {
                    Some(primary)
                } else {
                    None
                }
            } else if dx < thickness || dx >= width - thickness {
                // Side bars
                Some(primary)
            } else {
                None
            };

            if let Some(c) = color {
                let _ = gop.blt(BltOp::VideoFill {
                    color: c,
                    dest: (x + dx, y + dy),
                    dims: (1, 1),
                });
            }
        }
    }
}

fn draw_letter_x_modern(
    gop: &mut GraphicsOutput,
    x: usize,
    y: usize,
    primary: BltPixel,
    accent: BltPixel,
) {
    let width = 60;
    let height = 70;
    let thickness = 8;

    // Draw clean X with proper diagonals
    for dy in 0..height {
        for dx in 0..width {
            let ratio = dy as f32 / height as f32;
            let diag1_x = (ratio * width as f32) as usize;
            let diag2_x = width - (ratio * width as f32) as usize;

            let color = if (dx >= diag1_x.saturating_sub(thickness / 2)
                && dx < diag1_x + thickness / 2)
                || (dx >= diag2_x.saturating_sub(thickness / 2) && dx < diag2_x + thickness / 2)
            {
                Some(primary)
            } else {
                None
            };

            if let Some(c) = color {
                let _ = gop.blt(BltOp::VideoFill {
                    color: c,
                    dest: (x + dx, y + dy),
                    dims: (1, 1),
                });
            }
        }
    }
}

fn draw_letter_i_modern(
    gop: &mut GraphicsOutput,
    x: usize,
    y: usize,
    primary: BltPixel,
    accent: BltPixel,
) {
    let width = 30;
    let height = 70;
    let thickness = 8;
    let bar_width = 12;

    // Draw clean I with top and bottom bars
    for dy in 0..height {
        for dx in 0..width {
            let color = if dy < thickness || dy >= height - thickness {
                // Top and bottom bars (full width)
                Some(primary)
            } else if dx >= (width - bar_width) / 2 && dx < (width + bar_width) / 2 {
                // Center vertical bar
                Some(primary)
            } else {
                None
            };

            if let Some(c) = color {
                let _ = gop.blt(BltOp::VideoFill {
                    color: c,
                    dest: (x + dx, y + dy),
                    dims: (1, 1),
                });
            }
        }
    }
}

fn draw_letter_d_modern(
    gop: &mut GraphicsOutput,
    x: usize,
    y: usize,
    primary: BltPixel,
    accent: BltPixel,
) {
    let width = 60;
    let height = 70;
    let thickness = 8;

    // Draw modern D shape
    for dy in 0..height {
        for dx in 0..width {
            let color = if dx < thickness {
                // Left vertical bar
                Some(primary)
            } else if dy < thickness || dy >= height - thickness {
                // Top and bottom bars
                if dx >= thickness && dx < width - 10 {
                    Some(primary)
                } else {
                    None
                }
            } else if dx >= width - thickness {
                // Right curved edge
                Some(primary)
            } else {
                None
            };

            if let Some(c) = color {
                let _ = gop.blt(BltOp::VideoFill {
                    color: c,
                    dest: (x + dx, y + dy),
                    dims: (1, 1),
                });
            }
        }
    }
}

fn draw_letter_e_modern(
    gop: &mut GraphicsOutput,
    x: usize,
    y: usize,
    primary: BltPixel,
    accent: BltPixel,
) {
    let width = 55;
    let height = 70;
    let thickness = 8;

    // Draw clean E with three horizontal bars
    for dy in 0..height {
        for dx in 0..width {
            let color = if dx < thickness {
                // Left vertical bar
                Some(primary)
            } else if dy < thickness
                || dy >= height - thickness
                || (dy >= height / 2 - thickness / 2 && dy < height / 2 + thickness / 2)
            {
                // Top, middle, and bottom bars
                Some(primary)
            } else {
                None
            };

            if let Some(c) = color {
                let _ = gop.blt(BltOp::VideoFill {
                    color: c,
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

    let mut initramfs_file = match initramfs_handle
        .into_type()
        .map_err(|_| "Invalid file type")?
    {
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
    let buffer =
        unsafe { core::slice::from_raw_parts_mut(phys_addr as *mut u8, file_size as usize) };
    initramfs_file
        .read(buffer)
        .map_err(|_| "Failed to read initramfs file")?;

    Ok((phys_addr, file_size))
}

/// Allocate pages of memory
fn allocate_pages(count: usize) -> Option<u64> {
    let st = uefi::table::system_table_boot()?;
    let bs = st.boot_services();

    bs.allocate_pages(AllocateType::AnyPages, UefiMemoryType::LOADER_DATA, count)
        .ok()
}

/// Get the memory map from UEFI
fn get_memory_map() -> Vec<MemoryRegion> {
    let st = uefi::table::system_table_boot().expect("Boot services not available");
    let bs = st.boot_services();

    let mmap = bs
        .memory_map(UefiMemoryType::LOADER_DATA)
        .expect("Failed to get memory map");

    let mut regions = Vec::new();

    for desc in mmap.entries() {
        let ty = match desc.ty {
            UefiMemoryType::CONVENTIONAL => MemoryType::Usable,
            UefiMemoryType::BOOT_SERVICES_CODE | UefiMemoryType::BOOT_SERVICES_DATA => {
                MemoryType::BootServices
            }
            UefiMemoryType::ACPI_RECLAIM => MemoryType::AcpiReclaimable,
            UefiMemoryType::ACPI_NON_VOLATILE => MemoryType::AcpiNvs,
            UefiMemoryType::LOADER_CODE | UefiMemoryType::LOADER_DATA => {
                // [DEBUG] Log LOADER_DATA regions — ColdCipher
                let start = desc.phys_start;
                let end = start + desc.page_count * PAGE_SIZE;
                if start <= 0x1c070000 && end >= 0x1c050000 {
                    let mut debug_msg = [0u8; 100];
                    let mut cursor = 0;
                    for &b in b"[BOOT-MMAP] LOADER_DATA: 0x" {
                        debug_msg[cursor] = b;
                        cursor += 1;
                    }
                    for i in (0..16).rev() {
                        let nibble = ((start >> (i * 4)) & 0xF) as u8;
                        debug_msg[cursor] = if nibble < 10 {
                            b'0' + nibble
                        } else {
                            b'a' + nibble - 10
                        };
                        cursor += 1;
                    }
                    for &b in b"-0x" {
                        debug_msg[cursor] = b;
                        cursor += 1;
                    }
                    for i in (0..16).rev() {
                        let nibble = ((end >> (i * 4)) & 0xF) as u8;
                        debug_msg[cursor] = if nibble < 10 {
                            b'0' + nibble
                        } else {
                            b'a' + nibble - 10
                        };
                        cursor += 1;
                    }
                    debug_msg[cursor] = b'\n';
                    cursor += 1;
                    log(core::str::from_utf8(&debug_msg[..cursor]).unwrap_or("???"));
                }
                MemoryType::Bootloader
            }
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
    let mut gop = bs
        .open_protocol_exclusive::<GraphicsOutput>(gop_handle)
        .ok()?;

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
    use boot_proto::{MAX_VIDEO_MODES, VideoMode, VideoModeList};

    let st = uefi::table::system_table_boot()?;
    let bs = st.boot_services();

    let gop_handle = bs.get_handle_for_protocol::<GraphicsOutput>().ok()?;
    let gop = bs
        .open_protocol_exclusive::<GraphicsOutput>(gop_handle)
        .ok()?;

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
            uefi::proto::console::gop::PixelFormat::Rgb
            | uefi::proto::console::gop::PixelFormat::Bgr => 32,
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

/// Find the ACPI RSDP physical address from UEFI configuration tables.
///
/// Prefers ACPI 2.0+ (XSDT capable) over legacy ACPI 1.0.
/// Returns 0 if no RSDP is found.
///
/// — SableWire: scanning the firmware config table for the ACPI anchor
fn find_rsdp_in_config_tables() -> u64 {
    use uefi::table::cfg::{ACPI_GUID, ACPI2_GUID};

    let st = match uefi::table::system_table_boot() {
        Some(st) => st,
        None => return 0,
    };

    let config_entries = st.config_table();

    // Prefer ACPI 2.0 RSDP (has XSDT with 64-bit pointers)
    for entry in config_entries {
        if entry.guid == ACPI2_GUID {
            let addr = entry.address as u64;
            log_fmt(format_args!("[ACPI] RSDP v2.0 found at 0x{:016x}", addr));
            return addr;
        }
    }

    // Fall back to ACPI 1.0 RSDP (RSDT with 32-bit pointers)
    for entry in config_entries {
        if entry.guid == ACPI_GUID {
            let addr = entry.address as u64;
            log_fmt(format_args!("[ACPI] RSDP v1.0 found at 0x{:016x}", addr));
            return addr;
        }
    }

    log("[ACPI] No RSDP found in UEFI config tables");
    0
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

/// Log a message to UEFI console
fn log(msg: &str) {
    if let Some(mut st) = uefi::table::system_table_boot() {
        let _ = st.stdout().write_str(msg);
        let _ = st.stdout().write_str("\r\n");
    }
}

/// Log a formatted message
pub fn log_fmt(args: core::fmt::Arguments) {
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
