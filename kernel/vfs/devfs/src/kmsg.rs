//! /dev/kmsg - Kernel message log device
//!
//! Fixed ring buffer that stores timestamped, PID-tagged log entries.
//! Writers send: `<priority>,<tag>;<message>\n`
//! Kernel stores: `<priority>,<sec>.<ms>,<pid>,<tag>;<message>\n`
//!
//! Unstructured writes (no comma before semicolon) are auto-tagged with
//! priority 6 (INFO) and the process name from task metadata.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

// ============================================================================
// Ring Buffer Configuration
// ============================================================================

/// Maximum number of entries in the ring buffer
const RING_SIZE: usize = 1024;

/// Maximum size of a single log entry (bytes)
const ENTRY_SIZE: usize = 512;

/// Total ring buffer size: 1024 * 512 = 512KB
const RING_BYTES: usize = RING_SIZE * ENTRY_SIZE;

/// Default priority for unstructured writes
const DEFAULT_PRIORITY: u8 = 6; // INFO

// ============================================================================
// Callback Types (set by kernel during init)
// ============================================================================

/// Get current PID (returns 0 if unavailable)
pub type GetPidFn = fn() -> u32;

/// Get uptime in milliseconds
pub type GetUptimeMsFn = fn() -> u64;

/// Get process name for a PID (writes into buffer, returns length)
pub type GetProcNameFn = fn(u32, &mut [u8]) -> usize;

static mut GET_PID: Option<GetPidFn> = None;
static mut GET_UPTIME_MS: Option<GetUptimeMsFn> = None;
static mut GET_PROC_NAME: Option<GetProcNameFn> = None;

/// Set the PID callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_pid_callback(f: GetPidFn) {
    unsafe {
        GET_PID = Some(f);
    }
}

/// Set the uptime callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_uptime_callback(f: GetUptimeMsFn) {
    unsafe {
        GET_UPTIME_MS = Some(f);
    }
}

/// Set the process name callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_proc_name_callback(f: GetProcNameFn) {
    unsafe {
        GET_PROC_NAME = Some(f);
    }
}

fn current_pid() -> u32 {
    unsafe { GET_PID.map(|f| f()).unwrap_or(0) }
}

fn uptime_ms() -> u64 {
    unsafe { GET_UPTIME_MS.map(|f| f()).unwrap_or(0) }
}

fn proc_name(pid: u32, buf: &mut [u8]) -> usize {
    unsafe { GET_PROC_NAME.map(|f| f(pid, buf)).unwrap_or(0) }
}

// ============================================================================
// Ring Buffer Entry
// ============================================================================

/// A single log entry stored in the ring buffer
struct RingEntry {
    /// Entry data (formatted log line)
    data: [u8; ENTRY_SIZE],
    /// Length of valid data in `data`
    len: usize,
}

impl RingEntry {
    const fn empty() -> Self {
        RingEntry {
            data: [0; ENTRY_SIZE],
            len: 0,
        }
    }
}

// ============================================================================
// Ring Buffer
// ============================================================================

/// The global kernel message ring buffer
struct KmsgRing {
    /// Ring buffer entries
    entries: [RingEntry; RING_SIZE],
    /// Write sequence number (monotonically increasing)
    write_seq: u64,
}

impl KmsgRing {
    const fn new() -> Self {
        KmsgRing {
            entries: [const { RingEntry::empty() }; RING_SIZE],
            write_seq: 0,
        }
    }

    /// Write a formatted entry into the ring buffer
    fn write_entry(&mut self, data: &[u8]) {
        let idx = (self.write_seq as usize) % RING_SIZE;
        let entry = &mut self.entries[idx];
        let copy_len = data.len().min(ENTRY_SIZE);
        entry.data[..copy_len].copy_from_slice(&data[..copy_len]);
        entry.len = copy_len;
        self.write_seq += 1;
    }

    /// Read entry at the given sequence number
    /// Returns None if the entry has been overwritten or doesn't exist yet
    fn read_entry(&self, seq: u64) -> Option<&[u8]> {
        if seq >= self.write_seq {
            return None; // Not written yet
        }
        // Check if entry has been overwritten
        if self.write_seq - seq > RING_SIZE as u64 {
            return None; // Overwritten
        }
        let idx = (seq as usize) % RING_SIZE;
        let entry = &self.entries[idx];
        if entry.len == 0 {
            return None;
        }
        Some(&entry.data[..entry.len])
    }

    /// Get the oldest valid sequence number
    fn oldest_seq(&self) -> u64 {
        if self.write_seq <= RING_SIZE as u64 {
            0
        } else {
            self.write_seq - RING_SIZE as u64
        }
    }
}

static KMSG_RING: Mutex<KmsgRing> = Mutex::new(KmsgRing::new());

/// Global reader cursor for the single /dev/kmsg reader
static READER_SEQ: AtomicU64 = AtomicU64::new(0);

// ============================================================================
// Public kernel API
// ============================================================================

/// Write a kernel log message (PID 0, tag "kernel")
///
/// Called from kernel code to log messages to the ring buffer.
pub fn kmsg_write(priority: u8, message: &[u8]) {
    let ms = uptime_ms();
    let sec = ms / 1000;
    let frac = ms % 1000;

    let mut buf = [0u8; ENTRY_SIZE];
    let mut pos = 0;

    // Format: <priority>,<sec>.<ms>,0,kernel;<message>\n
    pos += write_u8(&mut buf[pos..], priority);
    buf[pos] = b',';
    pos += 1;
    pos += write_u64(&mut buf[pos..], sec);
    buf[pos] = b'.';
    pos += 1;
    pos += write_u64_padded3(&mut buf[pos..], frac);
    buf[pos] = b',';
    pos += 1;
    buf[pos] = b'0';
    pos += 1;
    buf[pos] = b',';
    pos += 1;
    let tag = b"kernel";
    let tag_len = tag.len().min(buf.len() - pos - 2);
    buf[pos..pos + tag_len].copy_from_slice(&tag[..tag_len]);
    pos += tag_len;
    buf[pos] = b';';
    pos += 1;

    let msg_len = message.len().min(buf.len() - pos - 1);
    buf[pos..pos + msg_len].copy_from_slice(&message[..msg_len]);
    pos += msg_len;
    buf[pos] = b'\n';
    pos += 1;

    KMSG_RING.lock().write_entry(&buf[..pos]);
}

/// Write a raw string as a kernel log message
pub fn kmsg_write_str(priority: u8, message: &str) {
    kmsg_write(priority, message.as_bytes());
}

// ============================================================================
// /dev/kmsg Device
// ============================================================================

/// The /dev/kmsg character device
pub struct KmsgDevice {
    ino: u64,
}

impl KmsgDevice {
    pub fn new(ino: u64) -> Self {
        KmsgDevice { ino }
    }

    /// Process a write from userspace, stamp with PID and timestamp
    fn process_write(&self, input: &[u8]) -> VfsResult<usize> {
        let pid = current_pid();
        let ms = uptime_ms();
        let sec = ms / 1000;
        let frac = ms % 1000;

        // Strip trailing newline from input for processing
        let input = if input.last() == Some(&b'\n') {
            &input[..input.len() - 1]
        } else {
            input
        };

        if input.is_empty() {
            return Ok(0);
        }

        // Detect structured vs raw format
        // Structured: <priority>,<tag>;<message>
        // Raw: anything else (auto-tagged with INFO and process name)
        let (priority, tag, message) = if let Some(semi_pos) = input.iter().position(|&b| b == b';')
        {
            // Has semicolon — check if prefix is structured: N,tag
            let prefix = &input[..semi_pos];
            let msg = &input[semi_pos + 1..];

            if let Some(comma_pos) = prefix.iter().position(|&b| b == b',') {
                // Parse priority digit
                let prio_slice = &prefix[..comma_pos];
                let tag_slice = &prefix[comma_pos + 1..];
                if prio_slice.len() == 1 && prio_slice[0].is_ascii_digit() {
                    (prio_slice[0] - b'0', tag_slice, msg)
                } else {
                    // Not valid structured format, treat as raw
                    let mut name_buf = [0u8; 32];
                    let name_len = proc_name(pid, &mut name_buf);
                    let tag = if name_len > 0 {
                        &name_buf[..name_len]
                    } else {
                        b"unknown" as &[u8]
                    };
                    // We need to store tag on stack — copy it
                    return self.write_raw_entry(pid, sec, frac, DEFAULT_PRIORITY, tag, input);
                }
            } else {
                // No comma before semicolon — raw
                let mut name_buf = [0u8; 32];
                let name_len = proc_name(pid, &mut name_buf);
                let tag = if name_len > 0 {
                    &name_buf[..name_len]
                } else {
                    b"unknown" as &[u8]
                };
                return self.write_raw_entry(pid, sec, frac, DEFAULT_PRIORITY, tag, input);
            }
        } else {
            // No semicolon at all — raw write (e.g. stdout from a service)
            let mut name_buf = [0u8; 32];
            let name_len = proc_name(pid, &mut name_buf);
            let tag = if name_len > 0 {
                &name_buf[..name_len]
            } else {
                b"unknown" as &[u8]
            };
            return self.write_raw_entry(pid, sec, frac, DEFAULT_PRIORITY, tag, input);
        };

        // Format structured entry
        let mut buf = [0u8; ENTRY_SIZE];
        let mut pos = 0;

        pos += write_u8(&mut buf[pos..], priority);
        buf[pos] = b',';
        pos += 1;
        pos += write_u64(&mut buf[pos..], sec);
        buf[pos] = b'.';
        pos += 1;
        pos += write_u64_padded3(&mut buf[pos..], frac);
        buf[pos] = b',';
        pos += 1;
        pos += write_u32(&mut buf[pos..], pid);
        buf[pos] = b',';
        pos += 1;

        let tag_len = tag.len().min(buf.len() - pos - 2);
        buf[pos..pos + tag_len].copy_from_slice(&tag[..tag_len]);
        pos += tag_len;
        buf[pos] = b';';
        pos += 1;

        let msg_len = message.len().min(buf.len() - pos - 1);
        buf[pos..pos + msg_len].copy_from_slice(&message[..msg_len]);
        pos += msg_len;
        buf[pos] = b'\n';
        pos += 1;

        KMSG_RING.lock().write_entry(&buf[..pos]);
        Ok(input.len() + if input.last() != Some(&b'\n') { 0 } else { 1 })
    }

    fn write_raw_entry(
        &self,
        pid: u32,
        sec: u64,
        frac: u64,
        priority: u8,
        tag: &[u8],
        message: &[u8],
    ) -> VfsResult<usize> {
        let mut buf = [0u8; ENTRY_SIZE];
        let mut pos = 0;

        pos += write_u8(&mut buf[pos..], priority);
        buf[pos] = b',';
        pos += 1;
        pos += write_u64(&mut buf[pos..], sec);
        buf[pos] = b'.';
        pos += 1;
        pos += write_u64_padded3(&mut buf[pos..], frac);
        buf[pos] = b',';
        pos += 1;
        pos += write_u32(&mut buf[pos..], pid);
        buf[pos] = b',';
        pos += 1;

        let tag_len = tag.len().min(buf.len() - pos - 2);
        buf[pos..pos + tag_len].copy_from_slice(&tag[..tag_len]);
        pos += tag_len;
        buf[pos] = b';';
        pos += 1;

        let msg_len = message.len().min(buf.len() - pos - 1);
        buf[pos..pos + msg_len].copy_from_slice(&message[..msg_len]);
        pos += msg_len;
        buf[pos] = b'\n';
        pos += 1;

        KMSG_RING.lock().write_entry(&buf[..pos]);
        Ok(message.len())
    }
}

impl VnodeOps for KmsgDevice {
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
        let ring = KMSG_RING.lock();

        // Get current reader position
        let mut seq = READER_SEQ.load(Ordering::Relaxed);

        // If reader is behind the oldest entry, jump forward
        let oldest = ring.oldest_seq();
        if seq < oldest {
            seq = oldest;
        }

        // Read entries into buffer
        let mut written = 0;
        while written < buf.len() {
            if let Some(entry_data) = ring.read_entry(seq) {
                if written + entry_data.len() > buf.len() {
                    break; // Not enough space for this entry
                }
                buf[written..written + entry_data.len()].copy_from_slice(entry_data);
                written += entry_data.len();
                seq += 1;
            } else {
                break; // No more entries
            }
        }

        // Update reader cursor
        READER_SEQ.store(seq, Ordering::Relaxed);
        Ok(written)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Handle multi-line writes: split on newlines and process each line
        if buf.is_empty() {
            return Ok(0);
        }

        let mut start = 0;
        for i in 0..buf.len() {
            if buf[i] == b'\n' {
                if i > start {
                    self.process_write(&buf[start..i])?;
                }
                start = i + 1;
            }
        }
        // Handle trailing data without newline
        if start < buf.len() {
            self.process_write(&buf[start..])?;
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
        let mut stat = Stat::new(VnodeType::CharDevice, Mode::new(0o666), 0, self.ino);
        stat.rdev = make_dev(1, 11); // Major 1, Minor 11 (Linux /dev/kmsg)
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Ok(())
    }

    fn poll_read_ready(&self) -> bool {
        let seq = READER_SEQ.load(Ordering::Relaxed);
        let ring = KMSG_RING.lock();
        seq < ring.write_seq
    }
}

/// Create a device number from major and minor numbers
fn make_dev(major: u64, minor: u64) -> u64 {
    (major << 8) | (minor & 0xFF)
}

// ============================================================================
// Integer formatting helpers (no alloc)
// ============================================================================

fn write_u8(buf: &mut [u8], val: u8) -> usize {
    write_u64(buf, val as u64)
}

fn write_u32(buf: &mut [u8], val: u32) -> usize {
    write_u64(buf, val as u64)
}

fn write_u64(buf: &mut [u8], val: u64) -> usize {
    if val == 0 {
        if !buf.is_empty() {
            buf[0] = b'0';
        }
        return 1;
    }

    let mut tmp = [0u8; 20];
    let mut i = 0;
    let mut v = val;
    while v > 0 {
        tmp[i] = (v % 10) as u8 + b'0';
        v /= 10;
        i += 1;
    }

    let len = i.min(buf.len());
    for j in 0..len {
        buf[j] = tmp[i - 1 - j];
    }
    len
}

/// Write a u64 zero-padded to 3 digits (for milliseconds)
fn write_u64_padded3(buf: &mut [u8], val: u64) -> usize {
    if buf.len() < 3 {
        return 0;
    }
    let v = val % 1000;
    buf[0] = (v / 100) as u8 + b'0';
    buf[1] = ((v / 10) % 10) as u8 + b'0';
    buf[2] = (v % 10) as u8 + b'0';
    3
}
