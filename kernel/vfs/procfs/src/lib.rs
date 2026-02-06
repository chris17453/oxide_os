//! Process filesystem (procfs) for OXIDE OS
//!
//! Provides /proc with process information.
//!
//! Structure:
//! - /proc/self -> symlink to current process
//! - /proc/meminfo - memory information
//! - /proc/cpuinfo - CPU information
//! - /proc/uptime - system uptime
//! - /proc/loadavg - load average
//! - /proc/stat - system statistics
//! - /proc/version - kernel version
//! - /proc/devices - available devices
//! - /proc/filesystems - supported filesystems
//! - /proc/mounts -> symlink to /proc/self/mounts
//! - /proc/[pid]/status - process status
//! - /proc/[pid]/cmdline - command line
//! - /proc/[pid]/exe - executable path (symlink)
//! - /proc/[pid]/stat - process statistics
//! - /proc/[pid]/statm - process memory stats

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;

use proc_traits::Pid;
use proc_traits::ProcessState;
use sched::{self, TaskState as SchedTaskState};
use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

// ============================================================================
// Memory info callback
// ============================================================================

/// Memory statistics structure
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryStats {
    /// Total physical memory in bytes
    pub total_mem: u64,
    /// Free physical memory in bytes
    pub free_mem: u64,
    /// Total swap in bytes
    pub total_swap: u64,
    /// Free swap in bytes
    pub free_swap: u64,
    /// Kernel heap used in bytes
    pub heap_used: u64,
    /// Kernel heap free in bytes
    pub heap_free: u64,
}

/// Callback type for getting memory stats
pub type MemoryStatsCallback = fn() -> MemoryStats;

/// Global memory stats callback
static mut MEMORY_STATS_CALLBACK: Option<MemoryStatsCallback> = None;

/// Set the memory stats callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_memory_stats_callback(callback: MemoryStatsCallback) {
    unsafe {
        MEMORY_STATS_CALLBACK = Some(callback);
    }
}

/// Get current memory stats
fn get_memory_stats() -> MemoryStats {
    unsafe {
        if let Some(callback) = MEMORY_STATS_CALLBACK {
            callback()
        } else {
            MemoryStats::default()
        }
    }
}

/// The /proc root directory
pub struct ProcFs {
    ino: u64,
}

impl ProcFs {
    /// Create a new procfs
    pub fn new() -> Arc<Self> {
        Arc::new(ProcFs { ino: 1 })
    }
}

impl Default for ProcFs {
    fn default() -> Self {
        ProcFs { ino: 1 }
    }
}

impl VnodeOps for ProcFs {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        // Handle "self" symlink
        if name == "self" {
            return Ok(Arc::new(ProcSelf { ino: 2 }));
        }

        // Handle system information files
        match name {
            "meminfo" => return Ok(Arc::new(ProcMeminfo { ino: 3 })),
            "cpuinfo" => return Ok(Arc::new(ProcCpuinfo { ino: 4 })),
            "uptime" => return Ok(Arc::new(ProcUptime { ino: 5 })),
            "loadavg" => return Ok(Arc::new(ProcLoadavg { ino: 6 })),
            "stat" => return Ok(Arc::new(ProcStat { ino: 7 })),
            "version" => return Ok(Arc::new(ProcVersion { ino: 8 })),
            "devices" => return Ok(Arc::new(ProcDevices { ino: 9 })),
            "filesystems" => return Ok(Arc::new(ProcFilesystems { ino: 10 })),
            "mounts" => return Ok(Arc::new(ProcMounts { ino: 11 })),
            _ => {}
        }

        // Try to parse as PID
        if let Ok(pid) = name.parse::<u32>() {
            if sched::get_task_meta(pid).is_some() {
                return Ok(Arc::new(ProcPid::new(pid)));
            }
        }

        Err(VfsError::NotFound)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let offset = offset as usize;

        // Directory entries in order
        let entries: &[(&str, u64, VnodeType)] = &[
            (".", self.ino, VnodeType::Directory),
            ("..", self.ino, VnodeType::Directory),
            ("self", 2, VnodeType::Symlink),
            ("meminfo", 3, VnodeType::File),
            ("cpuinfo", 4, VnodeType::File),
            ("uptime", 5, VnodeType::File),
            ("loadavg", 6, VnodeType::File),
            ("stat", 7, VnodeType::File),
            ("version", 8, VnodeType::File),
            ("devices", 9, VnodeType::File),
            ("filesystems", 10, VnodeType::File),
            ("mounts", 11, VnodeType::Symlink),
        ];

        // Static entries
        if offset < entries.len() {
            let (name, ino, ftype) = entries[offset];
            return Ok(Some(DirEntry {
                name: name.to_string(),
                ino,
                file_type: ftype,
            }));
        }

        // Process directories
        let pids = sched::all_pids();
        let pid_idx = offset - entries.len();
        if pid_idx < pids.len() {
            let pid = pids[pid_idx];
            return Ok(Some(DirEntry {
                name: format!("{}", pid),
                ino: 1000 + pid as u64,
                file_type: VnodeType::Directory,
            }));
        }

        Ok(None)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(
            VnodeType::Directory,
            Mode::new(0o555),
            0,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::IsDirectory)
    }
}

/// /proc/self symlink - resolves to current process's /proc/[pid]
pub struct ProcSelf {
    ino: u64,
}

impl VnodeOps for ProcSelf {
    fn vtype(&self) -> VnodeType {
        VnodeType::Symlink
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        // Read the symlink target (current PID) without allocating
        let pid = sched::current_pid().unwrap_or(0);

        // Convert PID to string without allocation (max u32 = 10 digits)
        let mut pid_buf = [0u8; 12];
        let mut pid_len = 0;

        if pid == 0 {
            pid_buf[0] = b'0';
            pid_len = 1;
        } else {
            let mut n = pid;
            let mut temp = [0u8; 12];
            let mut temp_len = 0;
            while n > 0 {
                temp[temp_len] = b'0' + (n % 10) as u8;
                n /= 10;
                temp_len += 1;
            }
            // Reverse
            for i in 0..temp_len {
                pid_buf[i] = temp[temp_len - 1 - i];
            }
            pid_len = temp_len;
        }

        let offset = offset as usize;
        if offset >= pid_len {
            return Ok(0);
        }

        let available = pid_len - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&pid_buf[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let pid = sched::current_pid().unwrap_or(0);
        let target = format!("{}", pid);
        Ok(Stat::new(
            VnodeType::Symlink,
            Mode::new(0o777),
            target.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

/// /proc/[pid] directory
pub struct ProcPid {
    pid: Pid,
    ino: u64,
}

impl ProcPid {
    fn new(pid: Pid) -> Self {
        ProcPid {
            pid,
            ino: 1000 + pid as u64,
        }
    }
}

impl VnodeOps for ProcPid {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        match name {
            "status" => Ok(Arc::new(ProcPidStatus::new(self.pid))),
            "cmdline" => Ok(Arc::new(ProcPidCmdline::new(self.pid))),
            "stat" => Ok(Arc::new(ProcPidStat::new(self.pid))),
            "statm" => Ok(Arc::new(ProcPidStatm::new(self.pid))),
            "exe" => Ok(Arc::new(ProcPidExe::new(self.pid))),
            "cwd" => Ok(Arc::new(ProcPidCwd::new(self.pid))),
            _ => Err(VfsError::NotFound),
        }
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let entries = [".", "..", "status", "cmdline", "stat", "statm", "exe", "cwd"];
        let types = [
            VnodeType::Directory,
            VnodeType::Directory,
            VnodeType::File,
            VnodeType::File,
            VnodeType::File,
            VnodeType::File,
            VnodeType::Symlink,
            VnodeType::Symlink,
        ];

        let offset = offset as usize;
        if offset < entries.len() {
            return Ok(Some(DirEntry {
                name: entries[offset].to_string(),
                ino: self.ino + offset as u64,
                file_type: types[offset],
            }));
        }

        Ok(None)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        // Verify process exists
        if sched::get_task_meta(self.pid).is_none() {
            return Err(VfsError::NotFound);
        }
        Ok(Stat::new(
            VnodeType::Directory,
            Mode::new(0o555),
            0,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::IsDirectory)
    }
}

/// /proc/[pid]/status file
pub struct ProcPidStatus {
    pid: Pid,
    ino: u64,
}

impl ProcPidStatus {
    fn new(pid: Pid) -> Self {
        ProcPidStatus {
            pid,
            ino: 2000 + pid as u64,
        }
    }

    fn format_state(pid: Pid, fallback: ProcessState) -> &'static str {
        if let Some(ts) = sched::get_task_state(pid) {
            return match ts {
                s if s == SchedTaskState::TASK_RUNNING => "R (running)",
                s if s == SchedTaskState::TASK_INTERRUPTIBLE => "S (sleeping)",
                s if s == SchedTaskState::TASK_UNINTERRUPTIBLE => "D (disk sleep)",
                s if s == SchedTaskState::TASK_STOPPED => "T (stopped)",
                s if s == SchedTaskState::TASK_TRACED => "t (tracing stop)",
                s if s == SchedTaskState::TASK_ZOMBIE => "Z (zombie)",
                s if s == SchedTaskState::TASK_DEAD => "X (dead)",
                _ => "R (running)",
            };
        }

        match fallback {
            ProcessState::Ready | ProcessState::Running => "R (running)",
            ProcessState::Blocked => "S (sleeping)",
            ProcessState::Zombie => "Z (zombie)",
        }
    }

    fn generate_content(&self) -> String {
        if let Some(meta) = sched::get_task_meta(self.pid) {
            let m = meta.lock();
            let state = Self::format_state(self.pid, ProcessState::Running);
            let ppid = sched::get_task_ppid(self.pid).unwrap_or(0);

            // Get process name from cmdline (first arg, basename only)
            let name = if m.cmdline.is_empty() {
                "unknown".to_string()
            } else {
                // Get basename of first argument
                let arg0 = &m.cmdline[0];
                if let Some(pos) = arg0.rfind('/') {
                    arg0[pos + 1..].to_string()
                } else {
                    arg0.clone()
                }
            };

            format!(
                "Name:\t{}\n\
                 State:\t{}\n\
                 Pid:\t{}\n\
                 PPid:\t{}\n\
                 Uid:\t{}\t{}\t{}\t{}\n\
                 Gid:\t{}\t{}\t{}\t{}\n",
                name,
                state,
                self.pid,
                ppid,
                m.credentials.uid,
                m.credentials.euid,
                m.credentials.uid,
                m.credentials.uid,
                m.credentials.gid,
                m.credentials.egid,
                m.credentials.gid,
                m.credentials.gid,
            )
        } else {
            String::new()
        }
    }
}

impl VnodeOps for ProcPidStatus {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

/// /proc/[pid]/cmdline file
pub struct ProcPidCmdline {
    pid: Pid,
    ino: u64,
}

impl ProcPidCmdline {
    fn new(pid: Pid) -> Self {
        ProcPidCmdline {
            pid,
            ino: 3000 + pid as u64,
        }
    }
}

impl ProcPidCmdline {
    fn generate_content(&self) -> alloc::vec::Vec<u8> {
        if let Some(meta) = sched::get_task_meta(self.pid) {
            let m = meta.lock();

            if m.cmdline.is_empty() {
                return alloc::vec![0u8];
            }

            // Join args with NUL bytes, end with NUL
            let mut result = alloc::vec::Vec::new();
            for (i, arg) in m.cmdline.iter().enumerate() {
                if i > 0 {
                    result.push(0);
                }
                result.extend_from_slice(arg.as_bytes());
            }
            result.push(0);
            result
        } else {
            alloc::vec![0u8]
        }
    }
}

impl VnodeOps for ProcPidCmdline {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();

        let offset = offset as usize;
        if offset >= content.len() {
            return Ok(0);
        }

        let available = content.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&content[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

/// /proc/[pid]/exe symlink
pub struct ProcPidExe {
    pid: Pid,
    ino: u64,
}

impl ProcPidExe {
    fn new(pid: Pid) -> Self {
        ProcPidExe {
            pid,
            ino: 4000 + pid as u64,
        }
    }
}

impl VnodeOps for ProcPidExe {
    fn vtype(&self) -> VnodeType {
        VnodeType::Symlink
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        // Return path to executable
        // For embedded init, this would be something like "/init" or "[embedded]"
        let target = b"/init";

        let offset = offset as usize;
        if offset >= target.len() {
            return Ok(0);
        }

        let available = target.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&target[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::Symlink, Mode::new(0o777), 5, self.ino))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

/// /proc/[pid]/cwd symlink
pub struct ProcPidCwd {
    pid: Pid,
    ino: u64,
}

impl ProcPidCwd {
    fn new(pid: Pid) -> Self {
        ProcPidCwd {
            pid,
            ino: 5000 + pid as u64,
        }
    }
}

impl VnodeOps for ProcPidCwd {
    fn vtype(&self) -> VnodeType {
        VnodeType::Symlink
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        // Return current working directory
        // For now, just return "/"
        let target = b"/";

        let offset = offset as usize;
        if offset >= target.len() {
            return Ok(0);
        }

        let available = target.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&target[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::Symlink, Mode::new(0o777), 1, self.ino))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

/// /proc/[pid]/stat file - process statistics
/// Format compatible with Linux /proc/[pid]/stat
/// — GraveShift: The complete process vital signs
pub struct ProcPidStat {
    pid: Pid,
    ino: u64,
}

impl ProcPidStat {
    fn new(pid: Pid) -> Self {
        ProcPidStat {
            pid,
            ino: 6000 + pid as u64,
        }
    }

    /// Generate /proc/[pid]/stat content
    /// Format: pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt 
    /// cmajflt utime stime cutime cstime priority nice num_threads itrealvalue starttime 
    /// vsize rss rsslim startcode endcode startstack kstkesp kstkeip signal blocked sigignore 
    /// sigcatch wchan nswap cnswap exit_signal processor rt_priority policy 
    /// delayacct_blkio_ticks guest_time cguest_time start_data end_data start_brk arg_start 
    /// arg_end env_start env_end exit_code
    fn generate_content(&self) -> String {
        if let Some(meta) = sched::get_task_meta(self.pid) {
            let m = meta.lock();
            
            // Get task info for timing
            let (state_char, ppid, start_time, sum_runtime) = if let Some((task_state, task_ppid, task_start_time, task_sum_runtime)) = sched::get_task_timing_info(self.pid) {
                let state = match task_state {
                    s if s == sched::TaskState::TASK_RUNNING => 'R',
                    s if s == sched::TaskState::TASK_INTERRUPTIBLE => 'S',
                    s if s == sched::TaskState::TASK_UNINTERRUPTIBLE => 'D',
                    s if s == sched::TaskState::TASK_STOPPED => 'T',
                    s if s == sched::TaskState::TASK_TRACED => 't',
                    s if s == sched::TaskState::TASK_ZOMBIE => 'Z',
                    s if s == sched::TaskState::TASK_DEAD => 'X',
                    _ => 'R',
                };
                (state, task_ppid, task_start_time, task_sum_runtime)
            } else {
                ('R', 0, 0, 0)
            };

            // Get process name from cmdline (first arg, basename only)
            let name = if m.cmdline.is_empty() {
                "unknown".to_string()
            } else {
                let arg0 = &m.cmdline[0];
                if let Some(pos) = arg0.rfind('/') {
                    arg0[pos + 1..].to_string()
                } else {
                    arg0.clone()
                }
            };

            // Convert nanoseconds to jiffies (assuming 100 Hz, i.e., 10ms per jiffy)
            // 1 jiffy = 10,000,000 nanoseconds
            const NANOS_PER_JIFFY: u64 = 10_000_000;
            let utime = sum_runtime / NANOS_PER_JIFFY;
            let stime = 0u64; // Kernel time - not tracked separately yet
            let starttime = start_time / NANOS_PER_JIFFY;

            // Linux /proc/[pid]/stat format (52 fields minimum)
            format!(
                "{} ({}) {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
                self.pid,           // 1: pid
                name,               // 2: comm (in parentheses)
                state_char,         // 3: state
                ppid,               // 4: ppid
                m.pgid,             // 5: pgrp
                m.sid,              // 6: session
                m.tty_nr,           // 7: tty_nr
                m.pgid,             // 8: tpgid (foreground process group)
                0u64,               // 9: flags
                0u64,               // 10: minflt (minor faults)
                0u64,               // 11: cminflt (child minor faults)
                0u64,               // 12: majflt (major faults)
                0u64,               // 13: cmajflt (child major faults)
                utime,              // 14: utime (user mode jiffies)
                stime,              // 15: stime (kernel mode jiffies)
                0i64,               // 16: cutime (child user time)
                0i64,               // 17: cstime (child system time)
                20i64,              // 18: priority (static priority)
                0i64,               // 19: nice
                1u64,               // 20: num_threads
                0u64,               // 21: itrealvalue (obsolete)
                starttime,          // 22: starttime (jiffies since boot)
                0u64,               // 23: vsize (virtual memory size)
                0u64,               // 24: rss (resident set size in pages)
                !0u64,              // 25: rsslim (rss limit)
                0u64,               // 26: startcode
                0u64,               // 27: endcode
                0u64,               // 28: startstack
                0u64,               // 29: kstkesp
                0u64,               // 30: kstkeip
                0u64,               // 31: signal (pending signals bitmap)
                0u64,               // 32: blocked (blocked signals bitmap)
                0u64,               // 33: sigignore (ignored signals bitmap)
                0u64,               // 34: sigcatch (caught signals bitmap)
                0u64,               // 35: wchan (wait channel)
                0u64,               // 36: nswap (swapped pages)
                0u64,               // 37: cnswap (child swapped pages)
                0i32,               // 38: exit_signal
                0i32,               // 39: processor (CPU number)
                0u32,               // 40: rt_priority
                0u32,               // 41: policy
                0u64,               // 42: delayacct_blkio_ticks
                0u64,               // 43: guest_time
                0i64,               // 44: cguest_time
                0u64,               // 45: start_data
                0u64,               // 46: end_data
                m.program_break,    // 47: start_brk
                0u64,               // 48: arg_start
                0u64,               // 49: arg_end
                0u64,               // 50: env_start
                0u64,               // 51: env_end
                0i32,               // 52: exit_code
            )
        } else {
            String::new()
        }
    }
}

impl VnodeOps for ProcPidStat {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

/// /proc/[pid]/statm file - process memory statistics
/// Format: size resident shared text lib data dt
/// — GraveShift: Memory footprint snapshot
pub struct ProcPidStatm {
    pid: Pid,
    ino: u64,
}

impl ProcPidStatm {
    fn new(pid: Pid) -> Self {
        ProcPidStatm {
            pid,
            ino: 7000 + pid as u64,
        }
    }

    fn generate_content(&self) -> String {
        // For now, return zeros - proper implementation would query mm subsystem
        // Format: size resident shared text lib data dt
        // All values in pages
        format!("0 0 0 0 0 0 0\n")
    }
}

impl VnodeOps for ProcPidStatm {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

/// /proc/meminfo - memory information
pub struct ProcMeminfo {
    ino: u64,
}

impl ProcMeminfo {
    fn generate_content(&self) -> String {
        let stats = get_memory_stats();

        // Format in Linux /proc/meminfo style (values in kB)
        let total_kb = stats.total_mem / 1024;
        let free_kb = stats.free_mem / 1024;
        let used_kb = total_kb.saturating_sub(free_kb);
        let buffers_kb = 0u64; // Not tracked
        let cached_kb = 0u64; // Not tracked
        let swap_total_kb = stats.total_swap / 1024;
        let swap_free_kb = stats.free_swap / 1024;

        format!(
            "MemTotal:       {:8} kB\n\
             MemFree:        {:8} kB\n\
             MemAvailable:   {:8} kB\n\
             Buffers:        {:8} kB\n\
             Cached:         {:8} kB\n\
             SwapTotal:      {:8} kB\n\
             SwapFree:       {:8} kB\n\
             HeapUsed:       {:8} kB\n\
             HeapFree:       {:8} kB\n",
            total_kb,
            free_kb,
            free_kb, // MemAvailable ~= MemFree for now
            buffers_kb,
            cached_kb,
            swap_total_kb,
            swap_free_kb,
            stats.heap_used / 1024,
            stats.heap_free / 1024,
        )
    }
}

impl VnodeOps for ProcMeminfo {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// /proc/cpuinfo - CPU Information
// ============================================================================
// Provides CPU identification, features, and frequency information
// — WireSaint

/// /proc/cpuinfo - CPU information
pub struct ProcCpuinfo {
    ino: u64,
}

impl ProcCpuinfo {
    fn generate_content(&self) -> String {
        // Get CPU count (number of logical processors)
        let cpu_count = sched::num_cpus();

        let mut output = String::new();

        // Generate info for each CPU
        for cpu_id in 0..cpu_count {
            // Basic CPU information - on x86_64 we can get this via CPUID
            #[cfg(target_arch = "x86_64")]
            {
                use alloc::format;

                // CPUID leaf 0: Vendor ID
                let vendor = get_cpu_vendor();

                // CPUID leaf 1: Family, Model, Stepping
                let (family, model, stepping) = get_cpu_family_model_stepping();

                // CPUID leaf 0x80000002-0x80000004: Brand string
                let brand = get_cpu_brand_string();

                // Build model name from brand if available, otherwise construct it
                let model_name = if !brand.is_empty() {
                    brand
                } else {
                    format!("{} CPU @ Unknown MHz", vendor)
                };

                output.push_str(&format!(
                    "processor\t: {}\n\
                     vendor_id\t: {}\n\
                     cpu family\t: {}\n\
                     model\t\t: {}\n\
                     model name\t: {}\n\
                     stepping\t: {}\n\
                     microcode\t: 0x0\n\
                     cpu MHz\t\t: 0.000\n\
                     cache size\t: 0 KB\n\
                     physical id\t: 0\n\
                     siblings\t: {}\n\
                     core id\t\t: {}\n\
                     cpu cores\t: {}\n\
                     apicid\t\t: {}\n\
                     initial apicid\t: {}\n\
                     fpu\t\t: yes\n\
                     fpu_exception\t: yes\n\
                     cpuid level\t: 13\n\
                     wp\t\t: yes\n\
                     flags\t\t: fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush mmx fxsr sse sse2\n\
                     bugs\t\t:\n\
                     bogomips\t: 0.00\n\
                     clflush size\t: 64\n\
                     cache_alignment\t: 64\n\
                     address sizes\t: 46 bits physical, 48 bits virtual\n\
                     power management:\n\n",
                    cpu_id, vendor, family, model, model_name, stepping,
                    cpu_count, cpu_id, cpu_count, cpu_id, cpu_id
                ));
            }

            #[cfg(not(target_arch = "x86_64"))]
            {
                output.push_str(&format!(
                    "processor\t: {}\n\
                     vendor_id\t: Unknown\n\
                     model name\t: Unknown CPU\n\n",
                    cpu_id
                ));
            }
        }

        output
    }
}

#[cfg(target_arch = "x86_64")]
fn get_cpu_vendor() -> &'static str {
    // CPUID leaf 0 - Vendor ID string
    let mut ebx: u32;
    let mut ecx: u32;
    let mut edx: u32;

    unsafe {
        core::arch::asm!(
            "push rbx",
            "mov eax, 0",
            "cpuid",
            "mov {0:e}, ebx",
            "pop rbx",
            out(reg) ebx,
            out("ecx") ecx,
            out("edx") edx,
            out("eax") _,
        );
    }

    // Vendor string is EBX+EDX+ECX (12 bytes)
    let bytes = [
        (ebx & 0xFF) as u8,
        ((ebx >> 8) & 0xFF) as u8,
        ((ebx >> 16) & 0xFF) as u8,
        ((ebx >> 24) & 0xFF) as u8,
        (edx & 0xFF) as u8,
        ((edx >> 8) & 0xFF) as u8,
        ((edx >> 16) & 0xFF) as u8,
        ((edx >> 24) & 0xFF) as u8,
        (ecx & 0xFF) as u8,
        ((ecx >> 8) & 0xFF) as u8,
        ((ecx >> 16) & 0xFF) as u8,
        ((ecx >> 24) & 0xFF) as u8,
    ];

    // Check common vendors
    if &bytes == b"GenuineIntel" {
        "GenuineIntel"
    } else if &bytes == b"AuthenticAMD" {
        "AuthenticAMD"
    } else {
        "Unknown"
    }
}

#[cfg(target_arch = "x86_64")]
fn get_cpu_family_model_stepping() -> (u32, u32, u32) {
    let eax: u32;

    unsafe {
        core::arch::asm!(
            "push rbx",
            "mov eax, 1",
            "cpuid",
            "pop rbx",
            out("eax") eax,
            out("ecx") _,
            out("edx") _,
        );
    }

    let stepping = eax & 0xF;
    let base_model = (eax >> 4) & 0xF;
    let base_family = (eax >> 8) & 0xF;
    let ext_model = (eax >> 16) & 0xF;
    let ext_family = (eax >> 20) & 0xFF;

    let family = if base_family == 0xF {
        base_family + ext_family
    } else {
        base_family
    };

    let model = if base_family == 0x6 || base_family == 0xF {
        (ext_model << 4) | base_model
    } else {
        base_model
    };

    (family, model, stepping)
}

#[cfg(target_arch = "x86_64")]
fn get_cpu_brand_string() -> String {
    // CPUID leaves 0x80000002-0x80000004 contain 48-byte brand string
    let mut brand_bytes = [0u8; 48];

    for i in 0..3 {
        let leaf = 0x80000002u32 + i;
        let eax: u32;
        let ebx: u32;
        let ecx: u32;
        let edx: u32;

        unsafe {
            core::arch::asm!(
                "push rbx",
                "mov eax, {1:e}",
                "cpuid",
                "mov {0:e}, ebx",
                "pop rbx",
                out(reg) ebx,
                in(reg) leaf,
                out("eax") eax,
                out("ecx") ecx,
                out("edx") edx,
            );
        }

        let offset = (i * 16) as usize;
        brand_bytes[offset..offset + 4].copy_from_slice(&eax.to_le_bytes());
        brand_bytes[offset + 4..offset + 8].copy_from_slice(&ebx.to_le_bytes());
        brand_bytes[offset + 8..offset + 12].copy_from_slice(&ecx.to_le_bytes());
        brand_bytes[offset + 12..offset + 16].copy_from_slice(&edx.to_le_bytes());
    }

    // Convert to string, trim nulls and whitespace
    String::from_utf8_lossy(&brand_bytes)
        .trim_matches('\0')
        .trim()
        .to_string()
}

impl VnodeOps for ProcCpuinfo {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// /proc/uptime - System Uptime
// ============================================================================
// Format: <uptime_seconds> <idle_seconds>
// — GraveShift

/// /proc/uptime - system uptime
pub struct ProcUptime {
    ino: u64,
}

impl ProcUptime {
    fn generate_content(&self) -> String {
        // Get uptime from timer ticks
        // Timer runs at 100 Hz (each tick = 10ms)
        let ticks = arch_x86_64::timer_ticks();
        let uptime_ms = ticks * 10;
        let uptime_secs = uptime_ms / 1000;
        let uptime_frac = (uptime_ms % 1000) / 10; // Two decimal places

        // Idle time - for now just report 0 (we don't track idle time yet)
        let idle_secs = 0u64;
        let idle_frac = 0u64;

        format!(
            "{}.{:02} {}.{:02}\n",
            uptime_secs, uptime_frac, idle_secs, idle_frac
        )
    }
}

impl VnodeOps for ProcUptime {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// /proc/loadavg - Load Average
// ============================================================================
// Format: <1min> <5min> <15min> <running>/<total> <last_pid>
// — StackTrace

/// /proc/loadavg - system load average
pub struct ProcLoadavg {
    ino: u64,
}

impl ProcLoadavg {
    fn generate_content(&self) -> String {
        // For now, report zeros for load averages (not yet tracked)
        // Get running/total process counts
        let pids = sched::all_pids();
        let total_procs = pids.len();

        // Count running processes
        let mut running = 0;
        for &pid in &pids {
            if let Some(state) = sched::get_task_state(pid) {
                if state == sched::TaskState::TASK_RUNNING {
                    running += 1;
                }
            }
        }

        // Last PID is the highest one
        let last_pid = pids.iter().max().copied().unwrap_or(0);

        format!("0.00 0.00 0.00 {}/{} {}\n", running, total_procs, last_pid)
    }
}

impl VnodeOps for ProcLoadavg {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// /proc/stat - System Statistics
// ============================================================================
// Provides system-wide CPU and process statistics
// — GraveShift

/// /proc/stat - system statistics
pub struct ProcStat {
    ino: u64,
}

impl ProcStat {
    fn generate_content(&self) -> String {
        let mut output = String::new();

        // — GraveShift: Real CPU tick data from scheduler accounting.
        // Convert nanoseconds to jiffies (10ms ticks, like Linux CLK_TCK=100).
        let cpu_count = sched::num_cpus();
        let mut total_user: u64 = 0;
        let mut total_system: u64 = 0;
        let mut total_idle: u64 = 0;

        for cpu_id in 0..cpu_count {
            let (user_ns, sys_ns, idle_ns) = sched::get_cpu_times(cpu_id);
            total_user += user_ns;
            total_system += sys_ns;
            total_idle += idle_ns;
        }

        // Aggregate CPU line (sum of all CPUs), ns → jiffies (/ 10_000_000)
        let agg_user = total_user / 10_000_000;
        let agg_sys = total_system / 10_000_000;
        let agg_idle = total_idle / 10_000_000;
        output.push_str(&format!("cpu  {} 0 {} {} 0 0 0 0 0 0\n", agg_user, agg_sys, agg_idle));

        // Per-CPU statistics
        for cpu_id in 0..cpu_count {
            let (user_ns, sys_ns, idle_ns) = sched::get_cpu_times(cpu_id);
            let u = user_ns / 10_000_000;
            let s = sys_ns / 10_000_000;
            let i = idle_ns / 10_000_000;
            output.push_str(&format!("cpu{} {} 0 {} {} 0 0 0 0 0 0\n", cpu_id, u, s, i));
        }

        // Interrupt statistics
        output.push_str("intr 0\n");
        output.push_str("ctxt 0\n");

        // Boot time (Unix timestamp) - use a fixed value for now
        let boot_time = 1704067200u64; // 2024-01-01 00:00:00 UTC
        output.push_str(&format!("btime {}\n", boot_time));

        // Process statistics
        let pids = sched::all_pids();
        let total_procs = pids.len();
        output.push_str(&format!("processes {}\n", total_procs));

        // Running processes count
        let mut running = 0;
        for &pid in &pids {
            if let Some(state) = sched::get_task_state(pid) {
                if state == sched::TaskState::TASK_RUNNING {
                    running += 1;
                }
            }
        }
        output.push_str(&format!("procs_running {}\n", running));

        // Blocked processes (in uninterruptible sleep)
        let mut blocked = 0;
        for &pid in &pids {
            if let Some(state) = sched::get_task_state(pid) {
                if state == sched::TaskState::TASK_UNINTERRUPTIBLE {
                    blocked += 1;
                }
            }
        }
        output.push_str(&format!("procs_blocked {}\n", blocked));

        output
    }
}

impl VnodeOps for ProcStat {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// /proc/version - Kernel Version
// ============================================================================
// Shows kernel version and build information
// — NeonRoot

/// /proc/version - kernel version
pub struct ProcVersion {
    ino: u64,
}

impl ProcVersion {
    fn generate_content(&self) -> String {
        // Format similar to Linux: "Linux version <ver> (<compiler>) <build_date>"
        format!(
            "OXIDE version 0.1.0 (rustc) #1 SMP {}\n",
            env!("CARGO_PKG_VERSION")
        )
    }
}

impl VnodeOps for ProcVersion {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// /proc/devices - Available Devices
// ============================================================================
// Lists character and block devices
// — TorqueJax

/// /proc/devices - available devices
pub struct ProcDevices {
    ino: u64,
}

impl ProcDevices {
    fn generate_content(&self) -> String {
        // Basic device list - expand as more devices are added
        let mut output = String::new();

        output.push_str("Character devices:\n");
        output.push_str("  1 mem\n");
        output.push_str("  4 tty\n");
        output.push_str("  5 console\n");
        output.push_str(" 10 misc\n");

        output.push_str("\nBlock devices:\n");
        output.push_str("  1 ramdisk\n");
        output.push_str("  8 sd\n");

        output
    }
}

impl VnodeOps for ProcDevices {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// /proc/filesystems - Supported Filesystems
// ============================================================================
// Lists filesystem types supported by the kernel
// — WireSaint

/// /proc/filesystems - supported filesystems
pub struct ProcFilesystems {
    ino: u64,
}

impl ProcFilesystems {
    fn generate_content(&self) -> String {
        // List filesystem types we support
        let mut output = String::new();

        output.push_str("nodev\tsysfs\n");
        output.push_str("nodev\trootfs\n");
        output.push_str("nodev\ttmpfs\n");
        output.push_str("nodev\tdevfs\n");
        output.push_str("nodev\tprocfs\n");
        output.push_str("\toxidefs\n");
        output.push_str("\text2\n");

        output
    }
}

impl VnodeOps for ProcFilesystems {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        let offset = offset as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }

        let available = bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&bytes[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let content = self.generate_content();
        Ok(Stat::new(
            VnodeType::File,
            Mode::new(0o444),
            content.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// /proc/mounts - Currently Mounted Filesystems
// ============================================================================
// Symlink to /proc/self/mounts (not yet fully implemented)
// — WireSaint

/// /proc/mounts - symlink to /proc/self/mounts
pub struct ProcMounts {
    ino: u64,
}

impl VnodeOps for ProcMounts {
    fn vtype(&self) -> VnodeType {
        VnodeType::Symlink
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        // Symlink target
        let target = b"self/mounts";

        let offset = offset as usize;
        if offset >= target.len() {
            return Ok(0);
        }

        let available = target.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&target[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
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
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(
            VnodeType::Symlink,
            Mode::new(0o777),
            11,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}
