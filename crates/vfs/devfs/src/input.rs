//! Input device nodes for /dev/input/
//!
//! Provides /dev/input/event0 (keyboard), /dev/input/event1 (mouse),
//! and /dev/input/mice (aggregated mouse events).

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::mem;
use spin::Mutex;

use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

/// Input event struct size (must match input::InputEvent layout)
/// Layout: Timestamp(u64 sec + u64 usec) + u16 type + u16 code + i32 value = 24 bytes
const INPUT_EVENT_SIZE: usize = 24;

/// IOCTL commands for input devices (Linux evdev compatible)
mod ioctl {
    /// Get device ID (struct input_id)
    pub const EVIOCGID: u64 = 0x02;
    /// Get device name
    pub const EVIOCGNAME: u64 = 0x06;
    /// Get physical location
    pub const EVIOCGPHYS: u64 = 0x07;
    /// Get unique identifier
    pub const EVIOCGUNIQ: u64 = 0x08;
    /// Flush event queue (custom, not in Linux evdev)
    pub const EVIOCFLUSH: u64 = 0x100;
}

/// Input device ID struct (Linux compatible)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct InputId {
    /// Bus type
    bustype: u16,
    /// Vendor ID
    vendor: u16,
    /// Product ID
    product: u16,
    /// Version
    version: u16,
}

/// BUS_I8042 — standard PS/2 bus type
const BUS_I8042: u16 = 0x11;

/// /dev/input/eventN — reads input events from a specific device
///
/// Each read returns one or more `InputEvent` structs (24 bytes each).
/// When no events are queued, read blocks until events arrive.
pub struct InputEventDevice {
    /// Input subsystem device index
    device_index: usize,
    /// Inode number
    ino: u64,
}

impl InputEventDevice {
    pub fn new(device_index: usize, ino: u64) -> Self {
        InputEventDevice { device_index, ino }
    }

    /// Get the input device handle from the input subsystem
    fn get_handle(&self) -> Option<Arc<input::InputDeviceHandle>> {
        input::get_device(self.device_index)
    }
}

impl VnodeOps for InputEventDevice {
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
        let handle = self.get_handle().ok_or(VfsError::IoError)?;

        if buf.len() < INPUT_EVENT_SIZE {
            return Err(VfsError::InvalidArgument);
        }

        // Calculate how many events we can fit
        let max_events = buf.len() / INPUT_EVENT_SIZE;
        let mut count = 0;

        // Try to pop events from the queue
        loop {
            if let Some(event) = handle.pop_event() {
                // Copy the InputEvent bytes directly into the buffer
                let event_bytes = unsafe {
                    core::slice::from_raw_parts(
                        &event as *const input::InputEvent as *const u8,
                        INPUT_EVENT_SIZE,
                    )
                };
                let offset = count * INPUT_EVENT_SIZE;
                buf[offset..offset + INPUT_EVENT_SIZE].copy_from_slice(event_bytes);
                count += 1;

                if count >= max_events {
                    break;
                }

                // If more events are available, keep draining without blocking
                if !handle.has_events() {
                    break;
                }
            } else if count > 0 {
                // We have some events, return them
                break;
            } else {
                // No events available — block until input arrives
                // Register as blocked reader on this device
                if let Some(pid) = sched::current_pid() {
                    input::set_blocked_reader(self.device_index, pid);
                }

                sched::block_current(sched::TaskState::TASK_INTERRUPTIBLE);

                // Race check: event may have arrived before we blocked
                if handle.has_events() {
                    if let Some(pid) = sched::current_pid_lockfree() {
                        sched::wake_up(pid);
                    }
                    continue;
                }

                // Halt until woken
                unsafe {
                    core::arch::asm!("sti", "hlt", options(nomem, nostack));
                }
            }
        }

        Ok(count * INPUT_EVENT_SIZE)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        // Input event devices are read-only
        Err(VfsError::PermissionDenied)
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
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o660), 0, self.ino);
        // Major 13 (input), minor = 64 + device_index (Linux convention for eventN)
        stat.rdev = make_dev(13, 64 + self.device_index as u64);
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::InvalidArgument)
    }

    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        let handle = self.get_handle().ok_or(VfsError::IoError)?;

        match request {
            ioctl::EVIOCGID => {
                // Return input device ID
                if arg == 0 {
                    return Err(VfsError::InvalidArgument);
                }
                let id = InputId {
                    bustype: BUS_I8042,
                    vendor: handle.info.vendor,
                    product: handle.info.product,
                    version: handle.info.version,
                };
                let ptr = arg as *mut InputId;
                unsafe {
                    *ptr = id;
                }
                Ok(0)
            }
            ioctl::EVIOCGNAME => {
                // Return device name string
                if arg == 0 {
                    return Err(VfsError::InvalidArgument);
                }
                let name = &handle.info.name;
                let name_bytes = name.as_bytes();
                // The arg is a pointer to a buffer; we write up to 256 bytes
                let ptr = arg as *mut u8;
                let len = name_bytes.len().min(255);
                unsafe {
                    core::ptr::copy_nonoverlapping(name_bytes.as_ptr(), ptr, len);
                    // Null-terminate
                    *ptr.add(len) = 0;
                }
                Ok(len as i64)
            }
            ioctl::EVIOCGPHYS => {
                // Return physical location string
                if arg == 0 {
                    return Err(VfsError::InvalidArgument);
                }
                let phys = &handle.info.phys;
                let phys_bytes = phys.as_bytes();
                let ptr = arg as *mut u8;
                let len = phys_bytes.len().min(255);
                unsafe {
                    core::ptr::copy_nonoverlapping(phys_bytes.as_ptr(), ptr, len);
                    *ptr.add(len) = 0;
                }
                Ok(len as i64)
            }
            ioctl::EVIOCGUNIQ => {
                // Return unique identifier string
                if arg == 0 {
                    return Err(VfsError::InvalidArgument);
                }
                let uniq = &handle.info.uniq;
                let uniq_bytes = uniq.as_bytes();
                let ptr = arg as *mut u8;
                let len = uniq_bytes.len().min(255);
                unsafe {
                    core::ptr::copy_nonoverlapping(uniq_bytes.as_ptr(), ptr, len);
                    *ptr.add(len) = 0;
                }
                Ok(len as i64)
            }
            ioctl::EVIOCFLUSH => {
                // Flush event queue (clear all pending events)
                handle.clear_events();
                Ok(0)
            }
            _ => Err(VfsError::NotSupported),
        }
    }

    fn poll_read_ready(&self) -> bool {
        self.get_handle().map(|h| h.has_events()).unwrap_or(false)
    }
}

/// /dev/input/mice — aggregated mouse events (PS/2 protocol format)
///
/// Reads return raw 3-byte PS/2 mouse packets aggregated from all mouse devices.
/// This is a simplified compatibility device; prefer /dev/input/eventN for full events.
pub struct MiceDevice {
    ino: u64,
}

impl MiceDevice {
    pub fn new(ino: u64) -> Self {
        MiceDevice { ino }
    }
}

impl VnodeOps for MiceDevice {
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
        // /dev/input/mice returns 3-byte PS/2 mouse packets
        // For now, return events from device 1 (mouse) converted to PS/2 format
        if buf.len() < 3 {
            return Err(VfsError::InvalidArgument);
        }

        let handle = match input::get_device(1) {
            Some(h) => h,
            None => return Ok(0),
        };

        // Wait for events
        loop {
            // Collect movement from available events
            let mut dx: i32 = 0;
            let mut dy: i32 = 0;
            let mut buttons: u8 = 0;
            let mut got_event = false;

            while let Some(event) = handle.pop_event() {
                match event.event_type() {
                    input::EventType::Rel => {
                        if event.code == input::REL_X {
                            dx += event.value;
                            got_event = true;
                        } else if event.code == input::REL_Y {
                            dy += event.value;
                            got_event = true;
                        }
                    }
                    input::EventType::Key => {
                        got_event = true;
                        let pressed = event.value != 0;
                        match event.code {
                            0x110 => {
                                if pressed {
                                    buttons |= 0x01;
                                }
                            } // BTN_LEFT
                            0x111 => {
                                if pressed {
                                    buttons |= 0x02;
                                }
                            } // BTN_RIGHT
                            0x112 => {
                                if pressed {
                                    buttons |= 0x04;
                                }
                            } // BTN_MIDDLE
                            _ => {}
                        }
                    }
                    input::EventType::Syn => {
                        // End of event batch — emit packet if we have data
                        if got_event {
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if got_event {
                // Clamp to PS/2 range (-128..127)
                let dx_clamped = dx.clamp(-128, 127) as i8;
                let dy_clamped = dy.clamp(-128, 127) as i8;

                // Build PS/2 3-byte packet
                // Byte 0: buttons + sign/overflow bits
                let mut byte0: u8 = buttons & 0x07;
                if dx_clamped < 0 {
                    byte0 |= 0x10;
                } // X sign bit
                if dy_clamped < 0 {
                    byte0 |= 0x20;
                } // Y sign bit
                byte0 |= 0x08; // Always-set bit

                buf[0] = byte0;
                buf[1] = dx_clamped as u8;
                buf[2] = dy_clamped as u8;
                return Ok(3);
            }

            // No events — block
            if let Some(pid) = sched::current_pid() {
                input::set_blocked_reader(1, pid);
            }

            sched::block_current(sched::TaskState::TASK_INTERRUPTIBLE);

            if handle.has_events() {
                if let Some(pid) = sched::current_pid_lockfree() {
                    sched::wake_up(pid);
                }
                continue;
            }

            unsafe {
                core::arch::asm!("sti", "hlt", options(nomem, nostack));
            }
        }
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::PermissionDenied)
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
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o660), 0, self.ino);
        // Major 13 (input), minor 63 (mice)
        stat.rdev = make_dev(13, 63);
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::InvalidArgument)
    }

    fn poll_read_ready(&self) -> bool {
        input::get_device(1)
            .map(|h| h.has_events())
            .unwrap_or(false)
    }
}

/// /dev/input/ directory node
///
/// Contains event0, event1, mice entries.
pub struct InputDirNode {
    /// Child devices
    children: BTreeMap<String, Arc<dyn VnodeOps>>,
    /// Inode number
    ino: u64,
}

impl InputDirNode {
    pub fn new(ino: u64) -> Self {
        let mut children = BTreeMap::new();

        // event0 = keyboard (device index 0), ino 100+
        children.insert(
            "event0".to_string(),
            Arc::new(InputEventDevice::new(0, ino + 1)) as Arc<dyn VnodeOps>,
        );
        // event1 = mouse (device index 1)
        children.insert(
            "event1".to_string(),
            Arc::new(InputEventDevice::new(1, ino + 2)) as Arc<dyn VnodeOps>,
        );
        // mice = aggregated mouse
        children.insert(
            "mice".to_string(),
            Arc::new(MiceDevice::new(ino + 3)) as Arc<dyn VnodeOps>,
        );

        InputDirNode { children, ino }
    }
}

impl VnodeOps for InputDirNode {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        self.children.get(name).cloned().ok_or(VfsError::NotFound)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::PermissionDenied)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let offset = offset as usize;

        // . and ..
        if offset == 0 {
            return Ok(Some(DirEntry {
                name: ".".to_string(),
                ino: self.ino,
                file_type: VnodeType::Directory,
            }));
        }
        if offset == 1 {
            return Ok(Some(DirEntry {
                name: "..".to_string(),
                ino: 1, // parent devfs ino
                file_type: VnodeType::Directory,
            }));
        }

        // Device entries start at offset 2
        let entries: Vec<_> = self.children.iter().collect();
        let idx = offset - 2;
        if idx < entries.len() {
            let (name, vnode) = &entries[idx];
            return Ok(Some(DirEntry {
                name: (*name).clone(),
                ino: vnode.stat().map(|s| s.ino).unwrap_or(0),
                file_type: vnode.vtype(),
            }));
        }

        Ok(None)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::PermissionDenied)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::PermissionDenied)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::PermissionDenied)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::PermissionDenied)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(
            VnodeType::Directory,
            Mode::DEFAULT_DIR,
            0,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::IsDirectory)
    }
}

/// Create a device number from major and minor numbers
fn make_dev(major: u64, minor: u64) -> u64 {
    (major << 8) | (minor & 0xFF)
}
