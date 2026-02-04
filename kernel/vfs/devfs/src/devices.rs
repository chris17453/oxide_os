//! Device implementations for devfs
//!
//! Provides /dev/null, /dev/zero, /dev/console, /dev/fb0, etc.

use alloc::sync::Arc;
use spin::Mutex;

use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

/// — GraveShift: unconditional COM1 serial write for critical diagnostics
/// No feature gate — if this path fires, we must see it
fn raw_serial_str(s: &[u8]) {
    for &b in s {
        unsafe {
            let mut status: u8;
            loop {
                core::arch::asm!("in al, dx", out("al") status, in("dx") 0x3FDu16, options(nomem, nostack));
                if status & 0x20 != 0 {
                    break;
                }
            }
            core::arch::asm!("out dx, al", in("al") b, in("dx") 0x3F8u16, options(nomem, nostack));
        }
    }
}

/// Write a string to COM1 serial port (debug-console only)
#[cfg(feature = "debug-console")]
fn dbg_serial(s: &str) {
    for &b in s.as_bytes() {
        unsafe {
            let mut status: u8;
            loop {
                core::arch::asm!("in al, dx", out("al") status, in("dx") 0x3FDu16, options(nomem, nostack));
                if status & 0x20 != 0 {
                    break;
                }
            }
            core::arch::asm!("out dx, al", in("al") b, in("dx") 0x3F8u16, options(nomem, nostack));
        }
    }
}

/// Strip ANSI/CSI escape sequences from output for cleaner serial debug logs
#[cfg(feature = "debug-tty-read")]
fn strip_ansi_escapes(data: &[u8]) -> alloc::vec::Vec<u8> {
    use alloc::vec::Vec;
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        if i + 1 < data.len() && data[i] == 0x1B {
            // ESC
            // Check for CSI sequence: ESC [
            if data[i + 1] == b'[' {
                // Skip until we find the end of CSI sequence (letter A-Z, a-z)
                i += 2;
                while i < data.len() {
                    let c = data[i];
                    i += 1;
                    if (c >= b'A' && c <= b'Z') || (c >= b'a' && c <= b'z') {
                        break;
                    }
                }
                continue;
            }
            // Check for other escape sequences: ESC ?
            else if data[i + 1] == b'?' {
                // Skip ESC ? sequences
                i += 2;
                while i < data.len() {
                    let c = data[i];
                    i += 1;
                    if c == b'h' || c == b'l' {
                        break;
                    }
                }
                continue;
            }
        }

        result.push(data[i]);
        i += 1;
    }

    result
}

// ============================================================================
// Console → Active VT Delegation
// ============================================================================
//
// /dev/console is a thin indirection to the active VT device, just like
// Linux where /dev/console resolves to the kernel's configured console
// (typically tty0 which is the active VT). All reads/writes/ioctls are
// forwarded to the underlying VT device's TTY+line discipline.

/// The underlying VT device vnode that /dev/console delegates to.
/// Set during kernel init after VT devices are registered.
static CONSOLE_BACKEND: Mutex<Option<Arc<dyn VnodeOps>>> = Mutex::new(None);

/// Set the backend VT vnode for /dev/console
///
/// Called by the kernel during init to wire /dev/console to the active VT.
pub fn set_console_backend(vnode: Arc<dyn VnodeOps>) {
    #[cfg(feature = "debug-console")]
    dbg_serial("[CON] set_console_backend() called\n");
    *CONSOLE_BACKEND.lock() = Some(vnode);
}

/// Get the backend vnode (clone the Arc)
fn get_console_backend() -> Option<Arc<dyn VnodeOps>> {
    CONSOLE_BACKEND.lock().clone()
}

/// Callback type for sending signals to the foreground process group
pub type SignalFgFn = fn(i32);

/// Global signal callback (set by kernel)
static mut SIGNAL_FG_CALLBACK: Option<SignalFgFn> = None;

/// Set the signal callback for sending signals to foreground process
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_signal_fg_callback(f: SignalFgFn) {
    unsafe {
        SIGNAL_FG_CALLBACK = Some(f);
    }
}

/// Signal numbers
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;

/// Legacy: push a character to the console
///
/// Now a no-op — input flows through VT push_input() only.
/// Kept for API compatibility during transition.
pub fn console_push_char(_ch: u8) {
    // Input is handled by the VT subsystem's push_input().
    // /dev/console delegates to the active VT device.
}

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

/// Serial-only write function type (for raw debug output)
pub type SerialWriteFn = fn(&[u8]);

/// /dev/console - writes go to system console with ANSI processing
pub struct ConsoleDevice {
    ino: u64,
}

impl ConsoleDevice {
    pub fn new(ino: u64) -> Self {
        ConsoleDevice { ino }
    }
}

/// Global serial write function (set by kernel, for debug output)
static mut SERIAL_WRITE: Option<SerialWriteFn> = None;

/// Set the serial write function (for raw debug output)
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_serial_write(f: SerialWriteFn) {
    unsafe {
        SERIAL_WRITE = Some(f);
    }
}

/// Legacy console write function - now routes to terminal + serial
pub type ConsoleFn = fn(&[u8]);

/// Global console write function (kept for backwards compatibility)
static mut CONSOLE_WRITE: Option<ConsoleFn> = None;

/// Set the console write function (legacy)
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

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        #[cfg(feature = "debug-console")]
        dbg_serial("[CON] read() enter\n");
        // Delegate to the active VT device
        match get_console_backend() {
            Some(backend) => {
                #[cfg(feature = "debug-console")]
                dbg_serial("[CON] read() -> backend\n");
                backend.read(offset, buf)
            }
            None => {
                // — GraveShift: ALWAYS log this — the EIO path kills login reads
                // No cfg gate: if we hit this, we need to know unconditionally
                raw_serial_str(b"[CON:READ] NO BACKEND -> EIO\n");
                Err(VfsError::IoError)
            }
        }
    }

    fn write(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        #[cfg(feature = "debug-console")]
        dbg_serial("[CON] write() enter\n");
        // Delegate to the active VT device
        match get_console_backend() {
            Some(backend) => {
                #[cfg(feature = "debug-console")]
                dbg_serial("[CON] write() -> backend\n");
                let r = backend.write(offset, buf);
                #[cfg(feature = "debug-console")]
                dbg_serial("[CON] write() <- backend done\n");
                r
            }
            None => {
                // Fallback for early boot before VT is ready:
                // write directly to serial + terminal
                unsafe {
                    if let Some(serial_fn) = SERIAL_WRITE {
                        // Filter ANSI escape sequences for serial debug output
                        #[cfg(feature = "debug-tty-read")]
                        {
                            let filtered = strip_ansi_escapes(buf);
                            serial_fn(&filtered);
                        }
                        #[cfg(not(feature = "debug-tty-read"))]
                        {
                            serial_fn(buf);
                        }
                    }
                }
                if terminal::is_initialized() {
                    // Terminal needs escape sequences for rendering
                    terminal::write(buf);
                } else {
                    unsafe {
                        if let Some(write_fn) = CONSOLE_WRITE {
                            write_fn(buf);
                        }
                    }
                }
                Ok(buf.len())
            }
        }
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

    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        // Delegate to the active VT device
        match get_console_backend() {
            Some(backend) => backend.ioctl(request, arg),
            None => Err(VfsError::IoError),
        }
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
            xres: 0,
            yres: 0,
            xres_virtual: 0,
            yres_virtual: 0,
            xoffset: 0,
            yoffset: 0,
            bits_per_pixel: 0,
            grayscale: 0,
            red_offset: 0,
            red_length: 0,
            green_offset: 0,
            green_length: 0,
            blue_offset: 0,
            blue_length: 0,
            transp_offset: 0,
            transp_length: 0,
            nonstd: 0,
            activate: 0,
            height: 0,
            width: 0,
            accel_flags: 0,
            pixclock: 0,
            left_margin: 0,
            right_margin: 0,
            upper_margin: 0,
            lower_margin: 0,
            hsync_len: 0,
            vsync_len: 0,
            sync: 0,
            vmode: 0,
            rotate: 0,
            colorspace: 0,
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

    // OXIDE-specific extensions
    /// Get mode count
    pub const FB_GET_MODE_COUNT: u64 = 0x4700;
    /// Get mode info by index
    pub const FB_GET_MODE: u64 = 0x4701;
    /// Set display mode (index in boot_proto VideoModeList)
    pub const FB_SET_MODE: u64 = 0x4702;
}

/// Framebuffer info callback type
pub type FbInfoFn = fn() -> Option<FramebufferDeviceInfo>;

/// Mode count callback type
pub type FbModeCountFn = fn() -> u32;

/// Mode info callback type (index -> info)
pub type FbModeInfoFn = fn(u32) -> Option<VideoModeDeviceInfo>;
/// Mode set callback type (index -> new info)
pub type FbModeSetFn = fn(u32) -> Option<VideoModeDeviceInfo>;

/// Framebuffer device info
#[derive(Debug, Clone, Copy)]
pub struct FramebufferDeviceInfo {
    pub base: usize,    // Virtual address
    pub phys_base: u64, // Physical address
    pub size: usize,    // Total size in bytes
    pub width: u32,     // Width in pixels
    pub height: u32,    // Height in pixels
    pub stride: u32,    // Bytes per scanline
    pub bpp: u32,       // Bits per pixel
    pub is_bgr: bool,   // BGR vs RGB format
}

/// Video mode info for userspace
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VideoModeDeviceInfo {
    /// Mode number (used for set_mode)
    pub mode_number: u32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bits per pixel
    pub bpp: u32,
    /// Stride in bytes per scanline
    pub stride: u32,
    /// Framebuffer size for this mode
    pub framebuffer_size: u64,
    /// Is this BGR format (vs RGB)
    pub is_bgr: bool,
    /// Padding for alignment
    pub _pad: [u8; 7],
}

/// Global framebuffer info callback (set by kernel)
static mut FB_INFO_CALLBACK: Option<FbInfoFn> = None;

/// Global mode count callback
static mut FB_MODE_COUNT_CALLBACK: Option<FbModeCountFn> = None;

/// Global mode info callback
static mut FB_MODE_INFO_CALLBACK: Option<FbModeInfoFn> = None;
/// Global mode set callback
static mut FB_MODE_SET_CALLBACK: Option<FbModeSetFn> = None;

/// Debug: Framebuffer write count
static mut FB_WRITE_COUNT: usize = 0;

/// Debug: Total bytes written to framebuffer
static mut FB_TOTAL_BYTES: usize = 0;

/// Debug: Last framebuffer base address used
static mut FB_LAST_BASE: usize = 0;

/// Set the framebuffer info callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_fb_info_callback(f: FbInfoFn) {
    unsafe {
        FB_INFO_CALLBACK = Some(f);
    }
}

/// Set the mode count callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_fb_mode_count_callback(f: FbModeCountFn) {
    unsafe {
        FB_MODE_COUNT_CALLBACK = Some(f);
    }
}

/// Set the mode info callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_fb_mode_info_callback(f: FbModeInfoFn) {
    unsafe {
        FB_MODE_INFO_CALLBACK = Some(f);
    }
}

/// Set the mode set callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_fb_mode_set_callback(f: FbModeSetFn) {
    unsafe {
        FB_MODE_SET_CALLBACK = Some(f);
    }
}

/// Get framebuffer write statistics (for debugging)
///
/// Returns (write_count, total_bytes, last_base_address)
pub fn get_fb_write_stats() -> (usize, usize, usize) {
    unsafe { (FB_WRITE_COUNT, FB_TOTAL_BYTES, FB_LAST_BASE) }
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

        // Track write statistics for debugging
        unsafe {
            FB_WRITE_COUNT += 1;
            FB_TOTAL_BYTES += to_write;
            FB_LAST_BASE = info.base;
        }

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
                // Set id to "OXIDE FB"
                let id = b"OXIDE FB\0\0\0\0\0\0\0\0";
                fix_info.id.copy_from_slice(id);
                fix_info.smem_start = info.phys_base;
                fix_info.smem_len = info.size as u32;
                fix_info.fb_type = 0; // FB_TYPE_PACKED_PIXELS
                fix_info.visual = 2; // FB_VISUAL_TRUECOLOR
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

            fb_ioctl::FB_GET_MODE_COUNT => {
                // Get mode count from callback
                let count = unsafe {
                    if let Some(callback) = FB_MODE_COUNT_CALLBACK {
                        callback()
                    } else {
                        1 // Default to 1 (current mode)
                    }
                };
                Ok(count as i64)
            }

            fb_ioctl::FB_GET_MODE => {
                // arg is pointer to struct: { u32 index, [pad], VideoModeDeviceInfo info }
                // Note: VideoModeDeviceInfo has 8-byte alignment due to u64 field,
                // so there's padding between index and info
                if arg == 0 {
                    return Err(VfsError::InvalidArgument);
                }

                // Read the mode index from user (at offset 0)
                let index = unsafe { *(arg as *const u32) };

                // Get mode info from callback
                let mode_info = unsafe {
                    if let Some(callback) = FB_MODE_INFO_CALLBACK {
                        callback(index)
                    } else {
                        None
                    }
                };

                match mode_info {
                    Some(info) => {
                        // Write mode info at offset 8 (after index + 4 bytes padding for alignment)
                        // VideoModeDeviceInfo requires 8-byte alignment due to u64 field
                        let out_ptr =
                            unsafe { (arg as *mut u8).add(8) as *mut VideoModeDeviceInfo };
                        unsafe {
                            *out_ptr = info;
                        }
                        Ok(0)
                    }
                    None => Err(VfsError::InvalidArgument),
                }
            }

            fb_ioctl::FB_SET_MODE => {
                if arg == 0 {
                    return Err(VfsError::InvalidArgument);
                }
                let index = unsafe { *(arg as *const u32) };
                let mode_info = unsafe {
                    if let Some(callback) = FB_MODE_SET_CALLBACK {
                        callback(index)
                    } else {
                        None
                    }
                };
                match mode_info {
                    Some(info) => {
                        let out_ptr =
                            unsafe { (arg as *mut u8).add(8) as *mut VideoModeDeviceInfo };
                        unsafe { *out_ptr = info };
                        Ok(0)
                    }
                    None => Err(VfsError::InvalidArgument),
                }
            }

            _ => Err(VfsError::NotSupported),
        }
    }
}

// ============================================================================
// Random Device (/dev/urandom, /dev/random)
// ============================================================================

/// Random number callback type
pub type RandomFillFn = fn(&mut [u8]);

/// Global random fill callback (set by kernel)
static mut RANDOM_FILL_CALLBACK: Option<RandomFillFn> = None;

/// Set the random fill callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_random_fill_callback(f: RandomFillFn) {
    unsafe {
        RANDOM_FILL_CALLBACK = Some(f);
    }
}

/// /dev/urandom and /dev/random - cryptographically secure random bytes
///
/// Both devices behave identically in modern Linux (since 4.8), providing
/// high-quality random data from the kernel's CSPRNG.
pub struct RandomDevice {
    ino: u64,
    /// Whether this is /dev/random (true) or /dev/urandom (false)
    /// In our implementation they're identical, but stat() shows different minor numbers
    is_blocking: bool,
}

impl RandomDevice {
    /// Create /dev/urandom device
    pub fn new_urandom(ino: u64) -> Self {
        RandomDevice {
            ino,
            is_blocking: false,
        }
    }

    /// Create /dev/random device
    pub fn new_random(ino: u64) -> Self {
        RandomDevice {
            ino,
            is_blocking: true,
        }
    }
}

impl VnodeOps for RandomDevice {
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
        // Fill buffer with random bytes
        unsafe {
            if let Some(fill_fn) = RANDOM_FILL_CALLBACK {
                fill_fn(buf);
            } else {
                // Fallback: fill with zeros (should not happen in properly initialized system)
                buf.fill(0);
            }
        }
        Ok(buf.len())
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Writing to /dev/urandom adds entropy to the pool
        // We accept the write but don't actually need to do anything special
        // since our CSPRNG is always seeded
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
        // Major 1: memory devices
        // Minor 8: /dev/random, Minor 9: /dev/urandom
        stat.rdev = make_dev(1, if self.is_blocking { 8 } else { 9 });
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

// ============================================================================
// Audio DSP Device (/dev/dsp)
// ============================================================================
//
// OSS-compatible digital audio interface. Programs write raw PCM data here
// and it flows through the audio subsystem to the VirtIO sound device.
// Supports standard OSS ioctls for format/rate/channel configuration.
//
// — EchoFrame: /dev/dsp is the doorway from userspace into the audio stream

/// OSS ioctl command numbers
/// — EchoFrame: ancient protocol, still works
mod oss_ioctl {
    pub const SNDCTL_DSP_RESET: u64 = 0x5000;
    pub const SNDCTL_DSP_SPEED: u64 = 0xC004_5002;
    pub const SNDCTL_DSP_STEREO: u64 = 0xC004_5003;
    pub const SNDCTL_DSP_SETFMT: u64 = 0xC004_5005;
    pub const SNDCTL_DSP_CHANNELS: u64 = 0xC004_5006;
    pub const SNDCTL_DSP_GETFMTS: u64 = 0x8004_500B;
    pub const SNDCTL_DSP_GETOSPACE: u64 = 0x800C_500C;
}

/// OSS audio format constants
mod oss_fmt {
    pub const AFMT_MU_LAW: u32 = 0x0000_0001;
    pub const AFMT_A_LAW: u32 = 0x0000_0002;
    pub const AFMT_U8: u32 = 0x0000_0008;
    pub const AFMT_S16_LE: u32 = 0x0000_0010;
    pub const AFMT_S16_BE: u32 = 0x0000_0020;
    pub const AFMT_S32_LE: u32 = 0x0000_1000;
}

/// Audio buffer info (returned by GETOSPACE ioctl)
/// — EchoFrame: how much room is left in the audio pipeline
#[repr(C)]
struct AudioBufInfo {
    fragments: i32,
    fragstotal: i32,
    fragsize: i32,
    bytes: i32,
}

/// /dev/dsp — OSS-compatible PCM audio device
/// — EchoFrame: character device major 14 minor 3, the classic Unix audio interface
pub struct DspDevice {
    ino: u64,
}

impl DspDevice {
    pub fn new(ino: u64) -> Self {
        DspDevice { ino }
    }
}

impl VnodeOps for DspDevice {
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
        // Read PCM capture data from audio device 0
        match audio::get_device(0) {
            Some(dev) => match dev.read(buf) {
                Ok(n) => Ok(n),
                Err(_) => {
                    // No capture data — return silence
                    buf.fill(0);
                    Ok(buf.len())
                }
            },
            None => {
                // No audio device — return silence
                buf.fill(0);
                Ok(buf.len())
            }
        }
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Write PCM playback data to audio device 0
        match audio::get_device(0) {
            Some(dev) => match dev.write(buf) {
                Ok(n) => Ok(n),
                Err(_) => Ok(buf.len()), // Swallow data if device error
            },
            None => Ok(buf.len()), // Discard if no device
        }
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
        stat.rdev = make_dev(14, 3); // Major 14, Minor 3 (standard /dev/dsp)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }

    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        match request {
            oss_ioctl::SNDCTL_DSP_RESET => {
                // Stop and release — best-effort
                if let Some(dev) = audio::get_device(0) {
                    let _ = dev.stop();
                    let _ = dev.release();
                }
                Ok(0)
            }
            oss_ioctl::SNDCTL_DSP_SPEED => {
                // Set sample rate — write back actual rate
                if arg != 0 {
                    let rate = unsafe { *(arg as *const u32) };
                    // Clamp to supported rates
                    let actual = match rate {
                        0..=22050 => 22050u32,
                        22051..=44100 => 44100,
                        _ => 48000,
                    };
                    unsafe { *(arg as *mut u32) = actual };
                }
                Ok(0)
            }
            oss_ioctl::SNDCTL_DSP_STEREO => {
                // Set stereo/mono: 0=mono, 1=stereo
                if arg != 0 {
                    let stereo = unsafe { *(arg as *const u32) };
                    let actual = if stereo != 0 { 1u32 } else { 0 };
                    unsafe { *(arg as *mut u32) = actual };
                }
                Ok(0)
            }
            oss_ioctl::SNDCTL_DSP_SETFMT => {
                // Set sample format
                if arg != 0 {
                    let fmt = unsafe { *(arg as *const u32) };
                    // Accept S16LE as default, pass back what we actually support
                    let actual = if fmt == oss_fmt::AFMT_S32_LE {
                        oss_fmt::AFMT_S32_LE
                    } else {
                        oss_fmt::AFMT_S16_LE
                    };
                    unsafe { *(arg as *mut u32) = actual };
                }
                Ok(0)
            }
            oss_ioctl::SNDCTL_DSP_CHANNELS => {
                // Set channel count
                if arg != 0 {
                    let ch = unsafe { *(arg as *const u32) };
                    let actual = ch.clamp(1, 2);
                    unsafe { *(arg as *mut u32) = actual };
                }
                Ok(0)
            }
            oss_ioctl::SNDCTL_DSP_GETFMTS => {
                // Report supported formats as bitmask
                if arg != 0 {
                    let supported = oss_fmt::AFMT_S16_LE
                        | oss_fmt::AFMT_S32_LE
                        | oss_fmt::AFMT_U8
                        | oss_fmt::AFMT_MU_LAW
                        | oss_fmt::AFMT_A_LAW;
                    unsafe { *(arg as *mut u32) = supported };
                }
                Ok(0)
            }
            oss_ioctl::SNDCTL_DSP_GETOSPACE => {
                // Report available write space
                if arg != 0 {
                    let avail = audio::get_device(0)
                        .map(|d| d.write_available())
                        .unwrap_or(4096);
                    let info = AudioBufInfo {
                        fragments: (avail / 1024) as i32,
                        fragstotal: 64,
                        fragsize: 1024,
                        bytes: avail as i32,
                    };
                    unsafe { *(arg as *mut AudioBufInfo) = info };
                }
                Ok(0)
            }
            _ => Err(VfsError::NotSupported),
        }
    }
}

// ============================================================================
// Audio Mixer Device (/dev/mixer)
// ============================================================================
//
// OSS-compatible mixer control interface. Programs use ioctls to read/write
// volume levels. Delegates to the audio subsystem's global mixer.
//
// — EchoFrame: the volume knob of the kernel

/// OSS mixer ioctl numbers
mod mixer_ioctl {
    pub const SOUND_MIXER_READ_VOLUME: u64 = 0x8004_4D00;
    pub const SOUND_MIXER_WRITE_VOLUME: u64 = 0xC004_4D00;
    pub const SOUND_MIXER_READ_DEVMASK: u64 = 0x8004_4DFE;
    pub const SOUND_MIXER_READ_CAPS: u64 = 0x8004_4DFC;
}

/// /dev/mixer — OSS-compatible audio mixer
/// — EchoFrame: character device major 14 minor 0
pub struct MixerDevice {
    ino: u64,
}

impl MixerDevice {
    pub fn new(ino: u64) -> Self {
        MixerDevice { ino }
    }
}

impl VnodeOps for MixerDevice {
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
        Ok(0) // EOF
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
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
        stat.rdev = make_dev(14, 0); // Major 14, Minor 0 (standard /dev/mixer)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }

    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        match request {
            mixer_ioctl::SOUND_MIXER_READ_VOLUME => {
                // OSS packs left+right volume as two bytes: (right << 8) | left
                // Both 0-100 range
                if arg != 0 {
                    let vol = audio::get_master_volume() as u32;
                    let packed = (vol << 8) | vol; // stereo same level
                    unsafe { *(arg as *mut u32) = packed };
                }
                Ok(0)
            }
            mixer_ioctl::SOUND_MIXER_WRITE_VOLUME => {
                if arg != 0 {
                    let packed = unsafe { *(arg as *const u32) };
                    let left = (packed & 0xFF).min(100);
                    let right = ((packed >> 8) & 0xFF).min(100);
                    // Use average of left/right for master volume
                    let vol = ((left + right) / 2) as u8;
                    audio::set_master_volume(vol);
                    // Write back actual value
                    let actual = vol as u32;
                    unsafe { *(arg as *mut u32) = (actual << 8) | actual };
                }
                Ok(0)
            }
            mixer_ioctl::SOUND_MIXER_READ_DEVMASK => {
                // Report which mixer channels exist (bit 0 = master volume)
                if arg != 0 {
                    unsafe { *(arg as *mut u32) = 0x01 }; // SOUND_MASK_VOLUME
                }
                Ok(0)
            }
            mixer_ioctl::SOUND_MIXER_READ_CAPS => {
                if arg != 0 {
                    unsafe { *(arg as *mut u32) = 0 }; // No special capabilities
                }
                Ok(0)
            }
            _ => Err(VfsError::NotSupported),
        }
    }
}
