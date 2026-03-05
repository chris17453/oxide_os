//! Bochs/QEMU Standard VGA Display Driver
//!
//! — NeonVale: The Bochs Display Adapter — QEMU's dirty little secret for
//! framebuffer access without the VirtIO circus. PCI vendor 0x1234, device
//! 0x1111, VBE DISPI registers at 0x01CE/0x01CF, BAR0 = linear framebuffer.
//! No virtqueues, no DMA, no handshakes. Just write mode registers and
//! start painting pixels. The kind of hardware that respects your time.

#![no_std]
#![allow(unused)]

extern crate alloc;

use driver_core::{DriverBindingData, DriverError, PciDeviceId, PciDriver};
use fb::{FramebufferInfo, PixelFormat};
use pci::PciDevice;
use spin::Mutex;

// — NeonVale: Bochs VBE DISPI register interface.
// Index register at 0x01CE, data register at 0x01CF.
// Write the register number to INDEX, then read/write DATA.
const VBE_DISPI_INDEX: u16 = 0x01CE;
const VBE_DISPI_DATA: u16 = 0x01CF;

// DISPI register indices
const VBE_DISPI_INDEX_ID: u16 = 0x00;
const VBE_DISPI_INDEX_XRES: u16 = 0x01;
const VBE_DISPI_INDEX_YRES: u16 = 0x02;
const VBE_DISPI_INDEX_BPP: u16 = 0x03;
const VBE_DISPI_INDEX_ENABLE: u16 = 0x04;
const VBE_DISPI_INDEX_BANK: u16 = 0x05;
const VBE_DISPI_INDEX_VIRT_WIDTH: u16 = 0x06;
const VBE_DISPI_INDEX_VIRT_HEIGHT: u16 = 0x07;
const VBE_DISPI_INDEX_X_OFFSET: u16 = 0x08;
const VBE_DISPI_INDEX_Y_OFFSET: u16 = 0x09;

// Enable flags
const VBE_DISPI_DISABLED: u16 = 0x00;
const VBE_DISPI_ENABLED: u16 = 0x01;
const VBE_DISPI_LFB_ENABLED: u16 = 0x40;
const VBE_DISPI_NOCLEARMEM: u16 = 0x80;

// PCI identity
const BOCHS_VENDOR: u16 = 0x1234;
const BOCHS_DEVICE: u16 = 0x1111;

/// PHYS_MAP_BASE — kernel direct physical memory map
const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Read a DISPI register
#[inline]
fn dispi_read(index: u16) -> u16 {
    unsafe {
        core::arch::asm!("out dx, ax", in("dx") VBE_DISPI_INDEX, in("ax") index, options(nomem, nostack));
        let val: u16;
        core::arch::asm!("in ax, dx", in("dx") VBE_DISPI_DATA, out("ax") val, options(nomem, nostack));
        val
    }
}

/// Write a DISPI register
#[inline]
fn dispi_write(index: u16, value: u16) {
    unsafe {
        core::arch::asm!("out dx, ax", in("dx") VBE_DISPI_INDEX, in("ax") index, options(nomem, nostack));
        core::arch::asm!("out dx, ax", in("dx") VBE_DISPI_DATA, in("ax") value, options(nomem, nostack));
    }
}

/// Bochs display state
struct BochsDisplay {
    /// BAR0 physical address (linear framebuffer)
    fb_phys: u64,
    /// Current width
    width: u32,
    /// Current height
    height: u32,
    /// Bytes per pixel (always 4 for 32bpp)
    bpp: u32,
}

impl BochsDisplay {
    /// — NeonVale: Set the display mode. Disables VBE, writes resolution
    /// and BPP, then re-enables with linear framebuffer. The NOCLEARMEM
    /// flag is intentionally NOT set — we want a clean black screen, not
    /// whatever garbage was in VRAM from the BIOS POST splash.
    fn set_mode(&mut self, width: u32, height: u32, bpp: u32) {
        dispi_write(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_DISABLED);
        dispi_write(VBE_DISPI_INDEX_XRES, width as u16);
        dispi_write(VBE_DISPI_INDEX_YRES, height as u16);
        dispi_write(VBE_DISPI_INDEX_BPP, bpp as u16);
        dispi_write(VBE_DISPI_INDEX_VIRT_WIDTH, width as u16);
        dispi_write(VBE_DISPI_INDEX_VIRT_HEIGHT, height as u16);
        dispi_write(VBE_DISPI_INDEX_X_OFFSET, 0);
        dispi_write(VBE_DISPI_INDEX_Y_OFFSET, 0);
        dispi_write(
            VBE_DISPI_INDEX_ENABLE,
            VBE_DISPI_ENABLED | VBE_DISPI_LFB_ENABLED,
        );

        self.width = width;
        self.height = height;
        self.bpp = bpp;
    }

    /// Get the FramebufferInfo for fb crate integration
    fn framebuffer_info(&self) -> FramebufferInfo {
        let virt_base = PHYS_MAP_BASE + self.fb_phys;
        let stride = self.width * (self.bpp / 8);
        let size = (stride * self.height) as usize;

        FramebufferInfo {
            base: virt_base as usize,
            size,
            width: self.width,
            height: self.height,
            stride,
            format: PixelFormat::BGRA8888, // — NeonVale: QEMU Bochs is always BGRA in 32bpp mode
        }
    }
}

/// Global Bochs display instance
static BOCHS_DISPLAY: Mutex<Option<BochsDisplay>> = Mutex::new(None);

/// Check if a Bochs display adapter is present on this PCI device
fn is_bochs_device(dev: &PciDevice) -> bool {
    dev.vendor_id == BOCHS_VENDOR && dev.device_id == BOCHS_DEVICE
}

/// Probe and initialize the Bochs display from a PCI device
pub fn init_from_pci(pci_dev: &PciDevice) -> Result<FramebufferInfo, &'static str> {
    if !is_bochs_device(pci_dev) {
        return Err("Not a Bochs display device");
    }

    // — NeonVale: Verify DISPI is responding. Read the ID register — should
    // return 0xB0C0..0xB0C5 for various Bochs VBE versions.
    let id = dispi_read(VBE_DISPI_INDEX_ID);
    if (id & 0xFFF0) != 0xB0C0 {
        unsafe {
            os_log::write_str_raw("[BOCHS] DISPI ID mismatch: 0x");
            os_log::write_u64_hex_raw(id as u64);
            os_log::write_str_raw("\n");
        }
        return Err("DISPI ID register not recognized");
    }

    // Get BAR0 = framebuffer physical address
    let fb_phys = pci_dev.bar0_address().ok_or("Bochs: no BAR0")?;

    unsafe {
        os_log::write_str_raw("[BOCHS] Found Bochs VGA (DISPI v0x");
        os_log::write_u64_hex_raw(id as u64);
        os_log::write_str_raw(") BAR0=0x");
        os_log::write_u64_hex_raw(fb_phys);
        os_log::write_str_raw("\n");
    }

    // — NeonVale: Enable PCI bus mastering and memory space access.
    // Without bit 1 (memory space), BAR0 reads return garbage.
    let cmd = pci::config_read32(pci_dev.address, 0x04) as u16;
    if cmd & 0x02 == 0 {
        pci::config_write16(pci_dev.address, 0x04, cmd | 0x02);
    }

    let mut display = BochsDisplay {
        fb_phys,
        width: 0,
        height: 0,
        bpp: 32,
    };

    // — NeonVale: Set mode to match whatever UEFI GOP was doing, or a sane
    // default. 1024x768x32 is universally supported and matches the common
    // OVMF GOP resolution for -vga std.
    display.set_mode(1024, 768, 32);

    let info = display.framebuffer_info();

    unsafe {
        os_log::write_str_raw("[BOCHS] Mode set: ");
        os_log::write_u32_raw(display.width);
        os_log::write_str_raw("x");
        os_log::write_u32_raw(display.height);
        os_log::write_str_raw("x");
        os_log::write_u32_raw(display.bpp);
        os_log::write_str_raw("\n");
    }

    *BOCHS_DISPLAY.lock() = Some(display);
    Ok(info)
}

/// Check if a Bochs display is active
pub fn is_active() -> bool {
    BOCHS_DISPLAY.lock().is_some()
}

// ============================================================================
// PciDriver Implementation
// ============================================================================

/// Device ID table for Bochs VGA
static BOCHS_IDS: &[PciDeviceId] = &[PciDeviceId::new(BOCHS_VENDOR, BOCHS_DEVICE)];

/// Bochs display driver for driver-core
struct BochsDisplayDriver;

impl PciDriver for BochsDisplayDriver {
    fn name(&self) -> &'static str {
        "bochs-display"
    }

    fn id_table(&self) -> &'static [PciDeviceId] {
        BOCHS_IDS
    }

    fn probe(
        &self,
        dev: &PciDevice,
        _id: &PciDeviceId,
    ) -> Result<DriverBindingData, DriverError> {
        // — NeonVale: If a working GOP framebuffer already exists, skip Bochs init.
        // The UEFI GOP is our primary display; Bochs takeover is for when we want
        // to own the display pipeline ourselves. The display_takeover() in init.rs
        // handles the actual switchover decision.
        if fb::framebuffer().is_some() {
            unsafe {
                os_log::write_str_raw(
                    "[BOCHS] GOP framebuffer active, skipping probe (use display_takeover)\n",
                );
            }
            return Ok(DriverBindingData::new(0));
        }

        init_from_pci(dev).map_err(|_| DriverError::InitFailed)?;
        Ok(DriverBindingData::new(1))
    }

    unsafe fn remove(&self, _dev: &PciDevice, _binding_data: DriverBindingData) {
        // — NeonVale: Bochs display removal is a no-op. The framebuffer memory
        // lives in PCI BAR0 space and doesn't need explicit cleanup.
    }
}

/// Static driver instance
static BOCHS_DRIVER: BochsDisplayDriver = BochsDisplayDriver;

// Register via linker section
driver_core::register_pci_driver!(BOCHS_DRIVER);
