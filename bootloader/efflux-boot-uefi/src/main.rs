//! EFFLUX UEFI Bootloader
//!
//! Loads the EFFLUX kernel and transfers control to it.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr;

use efflux_boot_proto::{
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
const KERNEL_PATH: &str = "\\EFI\\EFFLUX\\kernel.elf";

/// Page size
const PAGE_SIZE: u64 = 4096;

#[entry]
fn main() -> Status {
    // Initialize UEFI services
    uefi::helpers::init().expect("Failed to initialize UEFI helpers");

    log("");
    log("========================================");
    log("  EFFLUX UEFI Bootloader");
    log("  Version 0.1.0");
    log("========================================");
    log("");

    // Load kernel
    log_fmt(format_args!("[BOOT] Loading kernel from {}...", KERNEL_PATH));
    let kernel_data = match load_kernel_file() {
        Ok(data) => data,
        Err(e) => {
            log_fmt(format_args!("[BOOT] ERROR: Failed to load kernel: {}", e));
            halt();
        }
    };
    log_fmt(format_args!("[BOOT] Kernel file loaded: {} bytes", kernel_data.len()));

    // Parse ELF
    log("[BOOT] Parsing ELF...");
    let elf_info = match elf::parse_elf(&kernel_data) {
        Ok(info) => info,
        Err(e) => {
            log_fmt(format_args!("[BOOT] ERROR: Failed to parse ELF: {}", e));
            halt();
        }
    };
    log_fmt(format_args!("[BOOT] Kernel entry point: {:#x}", elf_info.entry));
    log_fmt(format_args!("[BOOT] Kernel load address: {:#x}", elf_info.load_base));
    log_fmt(format_args!("[BOOT] Kernel size: {} bytes", elf_info.load_size));

    // Allocate memory for kernel
    log("[BOOT] Allocating memory for kernel...");
    let kernel_pages = (elf_info.load_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let kernel_phys = allocate_pages(kernel_pages as usize)
        .expect("Failed to allocate memory for kernel");
    log_fmt(format_args!("[BOOT] Kernel physical address: {:#x}", kernel_phys));

    // Load kernel segments
    log("[BOOT] Loading kernel segments...");
    elf::load_segments(&kernel_data, &elf_info, kernel_phys);
    log("[BOOT] Kernel loaded");

    // Get framebuffer info
    let fb_info = get_framebuffer_info();
    if let Some(ref fb) = fb_info {
        log_fmt(format_args!("[BOOT] Framebuffer: {}x{} @ {:#x}", fb.width, fb.height, fb.base));
    }

    // Set up page tables
    log("[BOOT] Setting up page tables...");
    let pml4_phys = paging::setup_page_tables(kernel_phys, elf_info.load_size);
    log_fmt(format_args!("[BOOT] PML4 at {:#x}", pml4_phys));

    // Get memory map
    log("[BOOT] Getting memory map...");
    let memory_regions = get_memory_map();
    log_fmt(format_args!("[BOOT] Found {} memory regions", memory_regions.len()));

    // Create boot info
    let boot_info = create_boot_info(
        kernel_phys,
        elf_info.load_size,
        pml4_phys,
        &memory_regions,
        fb_info,
    );

    // Allocate space for boot info in a safe location
    let boot_info_phys = allocate_pages(1).expect("Failed to allocate boot info page");
    unsafe {
        ptr::write(boot_info_phys as *mut BootInfo, boot_info);
    }

    log_fmt(format_args!("[BOOT] Boot info at {:#x}", boot_info_phys));

    // Calculate kernel entry virtual address
    let kernel_entry_virt = KERNEL_VIRT_BASE + (elf_info.entry - elf_info.load_base);

    // Boot info virtual address (through direct physical map)
    let boot_info_virt = PHYS_MAP_BASE + boot_info_phys;

    log_fmt(format_args!("[BOOT] Jumping to kernel at {:#x}...", kernel_entry_virt));
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
            cstr16!("\\EFI\\EFFLUX\\kernel.elf"),
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

/// Create boot info structure
fn create_boot_info(
    kernel_phys: u64,
    kernel_size: u64,
    pml4_phys: u64,
    memory_regions: &[MemoryRegion],
    framebuffer: Option<FramebufferInfo>,
) -> BootInfo {
    let mut info = BootInfo::empty();
    info.magic = BOOT_INFO_MAGIC;
    info.kernel_phys_base = kernel_phys;
    info.kernel_virt_base = KERNEL_VIRT_BASE;
    info.kernel_size = kernel_size;
    info.pml4_phys = pml4_phys;
    info.phys_map_base = PHYS_MAP_BASE;
    info.framebuffer = framebuffer;

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
