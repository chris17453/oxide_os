//! Device implementations for devfs
//!
//! Provides /dev/null, /dev/zero, /dev/console, /dev/fb0, etc.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use spin::Mutex;

use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

// ============================================================================
// Console Keyboard Input Buffer
// ============================================================================

/// Maximum keyboard input buffer size
const KEYBOARD_BUFFER_SIZE: usize = 1024;

/// Keyboard input buffer for console
static KEYBOARD_BUFFER: Mutex<VecDeque<u8>> = Mutex::new(VecDeque::new());

/// EOF pending flag (set by Ctrl+D)
static CONSOLE_EOF_PENDING: Mutex<bool> = Mutex::new(false);

/// PID of task blocked waiting for console input (0 = none)
static CONSOLE_BLOCKED_READER: Mutex<u32> = Mutex::new(0);

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

/// Push a character to the console keyboard input buffer
///
/// Handles special characters:
/// - Ctrl+C (0x03): sends SIGINT to foreground process group
/// - Ctrl+D (0x04): sets EOF flag for next read
/// - Ctrl+\ (0x1C): sends SIGQUIT to foreground process group
///
/// If a task is blocked waiting for input, it will be woken up.
pub fn console_push_char(ch: u8) {
    match ch {
        0x03 => {
            // Ctrl+C: send SIGINT to foreground process group
            unsafe {
                if let Some(signal_fn) = SIGNAL_FG_CALLBACK {
                    signal_fn(SIGINT);
                }
            }
            // Clear the input buffer
            KEYBOARD_BUFFER.lock().clear();
        }
        0x04 => {
            // Ctrl+D: set EOF flag
            // If there's data in the buffer, commit it first (return what we have)
            // If buffer is empty, next read will return 0 (EOF)
            let buffer = KEYBOARD_BUFFER.lock();
            if buffer.is_empty() {
                *CONSOLE_EOF_PENDING.lock() = true;
            }
            // If buffer has data, the data will be returned and Ctrl+D is ignored
            // (matching Unix behavior where Ctrl+D commits the current line)
        }
        0x1C => {
            // Ctrl+\: send SIGQUIT to foreground process group
            unsafe {
                if let Some(signal_fn) = SIGNAL_FG_CALLBACK {
                    signal_fn(SIGQUIT);
                }
            }
            // Clear the input buffer
            KEYBOARD_BUFFER.lock().clear();
        }
        _ => {
            // Normal character: add to buffer
            let mut buffer = KEYBOARD_BUFFER.lock();
            if buffer.len() >= KEYBOARD_BUFFER_SIZE {
                buffer.pop_front(); // Drop oldest if full
            }
            buffer.push_back(ch);
        }
    }

    // Wake up any task blocked waiting for console input
    let blocked_pid = *CONSOLE_BLOCKED_READER.lock();
    if blocked_pid != 0 {
        sched::wake_up(blocked_pid);
        *CONSOLE_BLOCKED_READER.lock() = 0;
    }
}

/// Push a string to the console keyboard input buffer
pub fn console_push_str(s: &[u8]) {
    let mut buffer = KEYBOARD_BUFFER.lock();
    for &ch in s {
        if buffer.len() >= KEYBOARD_BUFFER_SIZE {
            buffer.pop_front();
        }
        buffer.push_back(ch);
    }
}

/// Pop a character from the console keyboard input buffer
fn console_pop_char() -> Option<u8> {
    KEYBOARD_BUFFER.lock().pop_front()
}

/// Check if keyboard input is available
pub fn console_has_input() -> bool {
    !KEYBOARD_BUFFER.lock().is_empty()
}

/// Simple buffer writer for debug output
struct BufWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> BufWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        BufWriter { buf, pos: 0 }
    }

    fn as_slice(&self) -> &[u8] {
        &self.buf[..self.pos]
    }
}

impl<'a> core::fmt::Write for BufWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len() - self.pos;
        let to_write = bytes.len().min(remaining);
        self.buf[self.pos..self.pos + to_write].copy_from_slice(&bytes[..to_write]);
        self.pos += to_write;
        Ok(())
    }
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

    fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Check for EOF (Ctrl+D on empty buffer)
        {
            let mut eof_pending = CONSOLE_EOF_PENDING.lock();
            if *eof_pending {
                *eof_pending = false;
                return Ok(0);
            }
        }

        // Blocking read - wait for input from keyboard buffer
        loop {
            if let Some(ch) = console_pop_char() {
                buf[0] = ch;
                // Got first byte; now drain whatever else is available
                let mut count = 1;
                while count < buf.len() {
                    if let Some(ch) = console_pop_char() {
                        buf[count] = ch;
                        count += 1;
                    } else {
                        break;
                    }
                }
                return Ok(count);
            }

            // No input available - block this task until input arrives
            // Register our PID so console_push_char() can wake us up
            if let Some(pid) = sched::current_pid() {
                *CONSOLE_BLOCKED_READER.lock() = pid;
            }

            // Block this task (marks it as INTERRUPTIBLE = not runnable)
            // The scheduler will then run other tasks, or the idle task which uses HLT
            sched::block_current(sched::TaskState::TASK_INTERRUPTIBLE);

            // When we wake up, check if input arrived while we were asleep
        }
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Write to serial for debugging (raw bytes)
        // Safety: We only read SERIAL_WRITE
        unsafe {
            if let Some(serial_fn) = SERIAL_WRITE {
                serial_fn(buf);
            }
        }

        // Write to terminal emulator for framebuffer with ANSI processing
        if terminal::is_initialized() {
            terminal::write(buf);
        } else {
            // Fallback to legacy console write (for early boot)
            unsafe {
                if let Some(write_fn) = CONSOLE_WRITE {
                    write_fn(buf);
                }
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
