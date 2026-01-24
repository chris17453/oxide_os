//! OXIDE UEFI Bootloader
//!
//! Loads the OXIDE kernel and transfers control to it.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr;

use boot_proto::{
    BOOT_INFO_MAGIC, BootInfo, FramebufferInfo, KERNEL_VIRT_BASE, MAX_MEMORY_REGIONS, MemoryRegion,
    MemoryType, PHYS_MAP_BASE, PixelFormat,
};
use uefi::Char16;
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

#[derive(Clone, Copy)]
struct ProgressLayout {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    text_y: usize,
}

static mut PROGRESS_LAYOUT: Option<ProgressLayout> = None;

#[entry]
fn main() -> Status {
    // Initialize UEFI services
    uefi::helpers::init().expect("Failed to initialize UEFI helpers");

    // Show graphical logo (non-blocking) and continue boot
    let _ = display_graphical_logo();

    // Clear screen for boot process
    clear_screen();

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
    update_progress(
        &mut current_step,
        total_steps,
        "Allocating kernel memory...",
    );
    let kernel_pages = (elf_info.load_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let kernel_phys =
        allocate_pages(kernel_pages as usize).expect("Failed to allocate memory for kernel");

    // Step 4: Load kernel segments
    update_progress(&mut current_step, total_steps, "Loading kernel segments...");
    elf::load_segments(&kernel_data, &elf_info, kernel_phys);

    // Step 5: Load initramfs
    update_progress(&mut current_step, total_steps, "Loading initramfs...");
    let (initramfs_phys, initramfs_size) = match load_initramfs() {
        Ok((phys, size)) => (phys, size),
        Err(_) => (0, 0), // Non-fatal
    };

    // Step 6: Initialize graphics
    update_progress(&mut current_step, total_steps, "Initializing graphics...");
    let fb_info = get_framebuffer_info();

    // Step 7: Enumerate video modes
    update_progress(&mut current_step, total_steps, "Enumerating video modes...");
    let video_modes = enumerate_video_modes();

    // Step 8: Get memory map BEFORE page table setup
    // (to avoid buffer allocation corrupting page tables)
    update_progress(&mut current_step, total_steps, "Getting memory map...");
    let memory_regions = get_memory_map();

    // Step 9: Set up page tables (after memory map to avoid corruption)
    update_progress(&mut current_step, total_steps, "Setting up page tables...");
    let pml4_phys = paging::setup_page_tables(kernel_phys, elf_info.load_size);

    // Step 10: Create boot info
    update_progress(
        &mut current_step,
        total_steps,
        "Creating boot information...",
    );
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
    // BootInfo is larger than one page (~5KB due to memory_regions and video_modes arrays),
    // so we must allocate 2 pages to avoid overwriting adjacent page table memory
    update_progress(&mut current_step, total_steps, "Finalizing boot setup...");
    let boot_info_phys = allocate_pages(2).expect("Failed to allocate boot info pages");
    unsafe {
        ptr::write(boot_info_phys as *mut BootInfo, boot_info);
    }

    // Calculate addresses for kernel jump
    let kernel_entry_virt = KERNEL_VIRT_BASE + (elf_info.entry - elf_info.load_base);
    let boot_info_virt = PHYS_MAP_BASE + boot_info_phys;

    // Step 12: Transfer control to kernel
    update_progress(
        &mut current_step,
        total_steps,
        "Transferring control to kernel...",
    );

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

    // Prepare progress layout near bottom of screen
    let bar_width = width.saturating_mul(2) / 3;
    let bar_height = 24;
    let bar_x = (width - bar_width) / 2;
    let bar_y = height.saturating_sub(80);
    unsafe {
        PROGRESS_LAYOUT = Some(ProgressLayout {
            x: bar_x,
            y: bar_y,
            width: bar_width,
            height: bar_height,
            text_y: bar_y.saturating_sub(30),
        });
    }
    // Draw initial empty bar
    draw_progress_bar(&mut *gop, 0, 1, "Starting...");

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
    let bg_color = BltPixel::new(20, 20, 20); // Dark background

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
            if (dx == 0 || dx == 39 || dy == 0 || dy == 59)
                || (dx > 5 && dx < 35 && (dy < 8 || dy > 52))
            {
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
            if dx == 0
                || (dx > 20
                    && ((dy < 8 && dx < 35)
                        || (dy > 52 && dx < 35)
                        || (dy >= 8 && dy <= 52 && dx == 35)))
            {
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

    // Draw to serial (existing)
    if let Some(mut st) = uefi::table::system_table_boot() {
        let _ = st.stdout().write_str("\x1b[2K\r");
        let _ = st.stdout().write_str("\x1b[1A\x1b[2K\r");
        log_fmt(format_args!("{} {}% {}", progress_bar, percentage, message));
        log("");
    }

    // Draw to graphics if available
    if let Some(st) = uefi::table::system_table_boot() {
        let bs = st.boot_services();
        if let Ok(handle) = bs.get_handle_for_protocol::<GraphicsOutput>() {
            if let Ok(mut gop) = bs.open_protocol_exclusive::<GraphicsOutput>(handle) {
                draw_progress_bar(&mut *gop, current, total, message);
            }
        }
    }
}

/// Update progress and display current operation
fn update_progress(step: &mut usize, total: usize, message: &str) {
    *step += 1;
    display_progress(*step, total, message);
}

fn draw_progress_bar(gop: &mut GraphicsOutput, current: usize, total: usize, message: &str) {
    let mode = gop.current_mode_info();
    let (width, _height) = mode.resolution();
    let layout = unsafe {
        PROGRESS_LAYOUT.unwrap_or(ProgressLayout {
            x: width / 6,
            y: _height.saturating_sub(80),
            width: width * 2 / 3,
            height: 24,
            text_y: _height.saturating_sub(110),
        })
    };

    let bg = BltPixel::new(30, 30, 30);
    let fill = BltPixel::new(30, 180, 50);
    let border = BltPixel::new(80, 80, 80);
    let text_color = BltPixel::new(220, 220, 220);

    // Background and border
    fill_rect(gop, layout.x, layout.y, layout.width, layout.height, bg);
    fill_rect(gop, layout.x, layout.y, layout.width, 2, border);
    fill_rect(
        gop,
        layout.x,
        layout.y + layout.height - 2,
        layout.width,
        2,
        border,
    );
    fill_rect(gop, layout.x, layout.y, 2, layout.height, border);
    fill_rect(
        gop,
        layout.x + layout.width - 2,
        layout.y,
        2,
        layout.height,
        border,
    );

    // Fill bar
    let inner_x = layout.x + 3;
    let inner_y = layout.y + 3;
    let inner_w = layout.width.saturating_sub(6);
    let inner_h = layout.height.saturating_sub(6);
    let filled_w = if total == 0 {
        0
    } else {
        (inner_w * current.min(total)) / total
    };
    fill_rect(gop, inner_x, inner_y, inner_w, inner_h, bg);
    if filled_w > 0 {
        fill_rect(gop, inner_x, inner_y, filled_w, inner_h, fill);
    }

    // Render message and percentage
    let pct = if total == 0 {
        0
    } else {
        (current * 100) / total
    };
    let mut upper = String::new();
    for ch in message.chars() {
        upper.extend(ch.to_uppercase());
    }
    let msg = format!("{}% {}", pct, upper);
    let max_chars = layout.width / 8;
    let truncated = if msg.len() > max_chars {
        &msg[..max_chars]
    } else {
        &msg
    };
    draw_text(gop, layout.x, layout.text_y, truncated, text_color, bg);
}

fn fill_rect(gop: &mut GraphicsOutput, x: usize, y: usize, w: usize, h: usize, color: BltPixel) {
    for yy in y..y + h {
        for xx in x..x + w {
            let _ = gop.blt(BltOp::VideoFill {
                color,
                dest: (xx, yy),
                dims: (1, 1),
            });
        }
    }
}

fn draw_text(gop: &mut GraphicsOutput, x: usize, y: usize, text: &str, fg: BltPixel, bg: BltPixel) {
    let mut cursor_x = x;
    for ch in text.chars() {
        draw_char(gop, cursor_x, y, ch, fg, bg);
        cursor_x += 8;
    }
}

fn draw_char(gop: &mut GraphicsOutput, x: usize, y: usize, ch: char, fg: BltPixel, bg: BltPixel) {
    let glyph = glyph_for(ch);
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..5 {
            let color = if (bits >> (4 - col)) & 1 == 1 { fg } else { bg };
            let _ = gop.blt(BltOp::VideoFill {
                color,
                dest: (x + col, y + row),
                dims: (1, 1),
            });
        }
    }
}

fn glyph_for(ch: char) -> [u8; 7] {
    match ch {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100,
        ],
        'E' => [
            0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b00110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        ':' => [
            0b00000, 0b00100, 0b00000, 0b00000, 0b00000, 0b00100, 0b00000,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00000,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b01110, 0b00000, 0b00000, 0b00000,
        ],
        '/' => [
            0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b00000, 0b00000,
        ],
        ' ' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
        '_' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
        _ => [
            0b11111, 0b10001, 0b10101, 0b10001, 0b10101, 0b10001, 0b11111,
        ],
    }
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
