//! Device implementations for devfs
//!
//! Provides /dev/null, /dev/zero, /dev/console, /dev/fb0, etc.

use alloc::sync::Arc;

use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

/// /dev/null - discards all writes, reads return EOF
pub struct NullDevice {
    ino: u64,
}

impl NullDevice {
    pub fn new(ino: u64) -> Self {
        NullDevice { ino }
    }
}

impl VnodeOps for NullDevice {
    fn vtype(&self) -> VnodeType {
        VnodeType::CharDevice
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        // Reading from /dev/null always returns EOF (0 bytes)
        Ok(0)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Writing to /dev/null always succeeds and discards the data
        Ok(buf.len())
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o666), 0, self.ino);
        stat.rdev = make_dev(1, 3); // Major 1, Minor 3 (standard /dev/null)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        // Truncating /dev/null is a no-op
        Ok(())
    }
}

/// /dev/zero - reads return zeros, writes are discarded
pub struct ZeroDevice {
    ino: u64,
}

impl ZeroDevice {
    pub fn new(ino: u64) -> Self {
        ZeroDevice { ino }
    }
}

impl VnodeOps for ZeroDevice {
    fn vtype(&self) -> VnodeType {
        VnodeType::CharDevice
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        // Reading from /dev/zero returns zeros
        buf.fill(0);
        Ok(buf.len())
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Writing to /dev/zero discards the data
        Ok(buf.len())
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o666), 0, self.ino);
        stat.rdev = make_dev(1, 5); // Major 1, Minor 5 (standard /dev/zero)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

/// Console write function type
pub type ConsoleFn = fn(&[u8]);

/// /dev/console - writes go to system console
pub struct ConsoleDevice {
    ino: u64,
}

impl ConsoleDevice {
    pub fn new(ino: u64) -> Self {
        ConsoleDevice { ino }
    }
}

/// Global console write function (set by kernel)
static mut CONSOLE_WRITE: Option<ConsoleFn> = None;

/// Set the console write function
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_console_write(f: ConsoleFn) {
    unsafe {
        CONSOLE_WRITE = Some(f);
    }
}

impl VnodeOps for ConsoleDevice {
    fn vtype(&self) -> VnodeType {
        VnodeType::CharDevice
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        // Console read would require keyboard input - return EOF for now
        Ok(0)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Write to the system console
        // Safety: We only read CONSOLE_WRITE
        unsafe {
            if let Some(write_fn) = CONSOLE_WRITE {
                write_fn(buf);
            }
        }
        Ok(buf.len())
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o620), 0, self.ino);
        stat.rdev = make_dev(5, 1); // Major 5, Minor 1 (console)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

/// Create a device number from major and minor numbers
fn make_dev(major: u64, minor: u64) -> u64 {
    (major << 8) | (minor & 0xFF)
}

// ============================================================================
// Framebuffer Device (/dev/fb0)
// ============================================================================

/// Framebuffer info structure for ioctl
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FbVarScreenInfo {
    pub xres: u32,
    pub yres: u32,
    pub xres_virtual: u32,
    pub yres_virtual: u32,
    pub xoffset: u32,
    pub yoffset: u32,
    pub bits_per_pixel: u32,
    pub grayscale: u32,
    // Bitfield info
    pub red_offset: u32,
    pub red_length: u32,
    pub green_offset: u32,
    pub green_length: u32,
    pub blue_offset: u32,
    pub blue_length: u32,
    pub transp_offset: u32,
    pub transp_length: u32,
    // Additional info
    pub nonstd: u32,
    pub activate: u32,
    pub height: u32,
    pub width: u32,
    pub accel_flags: u32,
    // Timing info (not used for UEFI GOP)
    pub pixclock: u32,
    pub left_margin: u32,
    pub right_margin: u32,
    pub upper_margin: u32,
    pub lower_margin: u32,
    pub hsync_len: u32,
    pub vsync_len: u32,
    pub sync: u32,
    pub vmode: u32,
    pub rotate: u32,
    pub colorspace: u32,
    pub reserved: [u32; 4],
}

impl Default for FbVarScreenInfo {
    fn default() -> Self {
        FbVarScreenInfo {
            xres: 0, yres: 0,
            xres_virtual: 0, yres_virtual: 0,
            xoffset: 0, yoffset: 0,
            bits_per_pixel: 0, grayscale: 0,
            red_offset: 0, red_length: 0,
            green_offset: 0, green_length: 0,
            blue_offset: 0, blue_length: 0,
            transp_offset: 0, transp_length: 0,
            nonstd: 0, activate: 0,
            height: 0, width: 0,
            accel_flags: 0,
            pixclock: 0,
            left_margin: 0, right_margin: 0,
            upper_margin: 0, lower_margin: 0,
            hsync_len: 0, vsync_len: 0,
            sync: 0, vmode: 0, rotate: 0, colorspace: 0,
            reserved: [0; 4],
        }
    }
}

/// Fixed screen info (Linux compatible)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FbFixScreenInfo {
    pub id: [u8; 16],
    pub smem_start: u64,
    pub smem_len: u32,
    pub fb_type: u32,
    pub type_aux: u32,
    pub visual: u32,
    pub xpanstep: u16,
    pub ypanstep: u16,
    pub ywrapstep: u16,
    pub _padding: u16,
    pub line_length: u32,
    pub mmio_start: u64,
    pub mmio_len: u32,
    pub accel: u32,
    pub capabilities: u16,
    pub reserved: [u16; 2],
}

impl Default for FbFixScreenInfo {
    fn default() -> Self {
        FbFixScreenInfo {
            id: [0; 16],
            smem_start: 0,
            smem_len: 0,
            fb_type: 0,
            type_aux: 0,
            visual: 2, // FB_VISUAL_TRUECOLOR
            xpanstep: 0,
            ypanstep: 0,
            ywrapstep: 0,
            _padding: 0,
            line_length: 0,
            mmio_start: 0,
            mmio_len: 0,
            accel: 0,
            capabilities: 0,
            reserved: [0; 2],
        }
    }
}

/// IOCTL commands for framebuffer
pub mod fb_ioctl {
    /// Get variable screen info
    pub const FBIOGET_VSCREENINFO: u64 = 0x4600;
    /// Put variable screen info
    pub const FBIOPUT_VSCREENINFO: u64 = 0x4601;
    /// Get fixed screen info
    pub const FBIOGET_FSCREENINFO: u64 = 0x4602;
    /// Get color map
    pub const FBIOGETCMAP: u64 = 0x4604;
    /// Put color map
    pub const FBIOPUTCMAP: u64 = 0x4605;
    /// Pan display
    pub const FBIOPAN_DISPLAY: u64 = 0x4606;
    /// Blank display
    pub const FBIOBLANK: u64 = 0x4611;

    // EFFLUX-specific extensions
    /// Get mode count
    pub const EFFLUX_FB_GET_MODE_COUNT: u64 = 0x4700;
    /// Get mode info by index
    pub const EFFLUX_FB_GET_MODE: u64 = 0x4701;
    /// Set display mode
    pub const EFFLUX_FB_SET_MODE: u64 = 0x4702;
}

/// Framebuffer info callback type
pub type FbInfoFn = fn() -> Option<FramebufferDeviceInfo>;

/// Framebuffer device info
#[derive(Debug, Clone, Copy)]
pub struct FramebufferDeviceInfo {
    pub base: usize,          // Virtual address
    pub phys_base: u64,       // Physical address
    pub size: usize,          // Total size in bytes
    pub width: u32,           // Width in pixels
    pub height: u32,          // Height in pixels
    pub stride: u32,          // Bytes per scanline
    pub bpp: u32,             // Bits per pixel
    pub is_bgr: bool,         // BGR vs RGB format
}

/// Global framebuffer info callback (set by kernel)
static mut FB_INFO_CALLBACK: Option<FbInfoFn> = None;

/// Set the framebuffer info callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_fb_info_callback(f: FbInfoFn) {
    unsafe {
        FB_INFO_CALLBACK = Some(f);
    }
}

/// /dev/fb0 - primary framebuffer device
pub struct FramebufferDevice {
    ino: u64,
}

impl FramebufferDevice {
    pub fn new(ino: u64) -> Self {
        FramebufferDevice { ino }
    }

    /// Get framebuffer info from kernel
    fn get_fb_info(&self) -> Option<FramebufferDeviceInfo> {
        unsafe {
            if let Some(callback) = FB_INFO_CALLBACK {
                callback()
            } else {
                None
            }
        }
    }
}

impl VnodeOps for FramebufferDevice {
    fn vtype(&self) -> VnodeType {
        VnodeType::CharDevice
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let info = self.get_fb_info().ok_or(VfsError::IoError)?;

        let offset = offset as usize;
        if offset >= info.size {
            return Ok(0); // EOF
        }

        let available = info.size - offset;
        let to_read = buf.len().min(available);

        // Read from framebuffer memory
        let fb_ptr = info.base as *const u8;
        unsafe {
            core::ptr::copy_nonoverlapping(fb_ptr.add(offset), buf.as_mut_ptr(), to_read);
        }

        Ok(to_read)
    }

    fn write(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        let info = self.get_fb_info().ok_or(VfsError::IoError)?;

        let offset = offset as usize;
        if offset >= info.size {
            return Ok(0); // Can't write past end
        }

        let available = info.size - offset;
        let to_write = buf.len().min(available);

        // Write to framebuffer memory
        let fb_ptr = info.base as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), fb_ptr.add(offset), to_write);
        }

        Ok(to_write)
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = self.get_fb_info().map(|i| i.size as u64).unwrap_or(0);
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o660), size, self.ino);
        stat.rdev = make_dev(29, 0); // Major 29, Minor 0 (standard /dev/fb0)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        // Can't truncate a framebuffer
        Err(VfsError::InvalidArgument)
    }

    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        let info = self.get_fb_info().ok_or(VfsError::IoError)?;

        match request {
            fb_ioctl::FBIOGET_VSCREENINFO => {
                if arg == 0 {
                    return Err(VfsError::InvalidArgument);
                }

                let var_info = FbVarScreenInfo {
                    xres: info.width,
                    yres: info.height,
                    xres_virtual: info.width,
                    yres_virtual: info.height,
                    xoffset: 0,
                    yoffset: 0,
                    bits_per_pixel: info.bpp,
                    grayscale: 0,
                    // For BGRA8888: B=0-7, G=8-15, R=16-23, A=24-31
                    red_offset: if info.is_bgr { 16 } else { 0 },
                    red_length: 8,
                    green_offset: 8,
                    green_length: 8,
                    blue_offset: if info.is_bgr { 0 } else { 16 },
                    blue_length: 8,
                    transp_offset: 24,
                    transp_length: 8,
                    ..Default::default()
                };

                let out_ptr = arg as *mut FbVarScreenInfo;
                unsafe {
                    *out_ptr = var_info;
                }
                Ok(0)
            }

            fb_ioctl::FBIOGET_FSCREENINFO => {
                if arg == 0 {
                    return Err(VfsError::InvalidArgument);
                }

                let mut fix_info = FbFixScreenInfo::default();
                // Set id to "EFFLUX FB"
                let id = b"EFFLUX FB\0\0\0\0\0\0\0";
                fix_info.id.copy_from_slice(id);
                fix_info.smem_start = info.phys_base;
                fix_info.smem_len = info.size as u32;
                fix_info.fb_type = 0; // FB_TYPE_PACKED_PIXELS
                fix_info.visual = 2;  // FB_VISUAL_TRUECOLOR
                fix_info.line_length = info.stride;

                let out_ptr = arg as *mut FbFixScreenInfo;
                unsafe {
                    *out_ptr = fix_info;
                }
                Ok(0)
            }

            fb_ioctl::FBIOBLANK => {
                // Blank/unblank not supported on UEFI GOP
                Ok(0)
            }

            fb_ioctl::EFFLUX_FB_GET_MODE_COUNT => {
                // For now, just 1 mode (current mode)
                Ok(1)
            }

            _ => Err(VfsError::NotSupported),
        }
    }
}
