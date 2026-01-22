//! OXIDE UEFI Bootloader
//!
//! Loads the OXIDE kernel and transfers control to it.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr;

use boot_proto::{
    BootInfo, FramebufferInfo, MemoryRegion, MemoryType, PixelFormat,
    BOOT_INFO_MAGIC, MAX_MEMORY_REGIONS, KERNEL_VIRT_BASE, PHYS_MAP_BASE,
};
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{AllocateType, MemoryType as UefiMemoryType};
use uefi::mem::memory_map::MemoryMap;

mod elf;
mod paging;

/// Kernel file path on the EFI partition
const KERNEL_PATH: &str = "\\EFI\\OXIDE\\kernel.elf";

/// Initramfs file path on the EFI partition
const INITRAMFS_PATH: &str = "\\EFI\\OXIDE\\initramfs.cpio";

/// Page size
const PAGE_SIZE: u64 = 4096;

#[entry]
fn main() -> Status {
    // Initialize UEFI services
    uefi::helpers::init().expect("Failed to initialize UEFI helpers");

    // Clear screen and display logo
    clear_screen();
    display_logo();
    
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

/// Display the OXIDE OS logo
fn display_logo() {
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
