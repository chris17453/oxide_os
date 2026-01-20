//! Process filesystem (procfs) for EFFLUX OS
//!
//! Provides /proc with process information.
//!
//! Structure:
//! - /proc/self -> symlink to current process
//! - /proc/meminfo - memory information
//! - /proc/[pid]/status - process status
//! - /proc/[pid]/cmdline - command line
//! - /proc/[pid]/exe - executable path (symlink)

#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;

use proc::process_table;
use proc_traits::ProcessState;
use proc_traits::Pid;
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

        // Handle "meminfo" file
        if name == "meminfo" {
            return Ok(Arc::new(ProcMeminfo { ino: 3 }));
        }

        // Try to parse as PID
        if let Ok(pid) = name.parse::<u32>() {
            let table = process_table();
            if table.get(pid).is_some() {
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

        // . entry
        if offset == 0 {
            return Ok(Some(DirEntry {
                name: ".".to_string(),
                ino: self.ino,
                file_type: VnodeType::Directory,
            }));
        }

        // .. entry
        if offset == 1 {
            return Ok(Some(DirEntry {
                name: "..".to_string(),
                ino: self.ino,
                file_type: VnodeType::Directory,
            }));
        }

        // "self" symlink
        if offset == 2 {
            return Ok(Some(DirEntry {
                name: "self".to_string(),
                ino: 2,
                file_type: VnodeType::Symlink,
            }));
        }

        // "meminfo" file
        if offset == 3 {
            return Ok(Some(DirEntry {
                name: "meminfo".to_string(),
                ino: 3,
                file_type: VnodeType::File,
            }));
        }

        // Process directories
        let table = process_table();
        let pids = table.all_pids();

        let pid_idx = offset - 4;
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
        Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino))
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
        // Read the symlink target (current PID)
        let pid = process_table().current_pid();
        let target = format!("{}", pid);
        let target_bytes = target.as_bytes();

        let offset = offset as usize;
        if offset >= target_bytes.len() {
            return Ok(0);
        }

        let available = target_bytes.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&target_bytes[offset..offset + to_read]);
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
        let pid = process_table().current_pid();
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
        let entries = [".", "..", "status", "cmdline", "exe", "cwd"];
        let types = [
            VnodeType::Directory,
            VnodeType::Directory,
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
        let table = process_table();
        if table.get(self.pid).is_none() {
            return Err(VfsError::NotFound);
        }
        Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino))
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

    fn generate_content(&self) -> String {
        let table = process_table();

        if let Some(proc) = table.get(self.pid) {
            let p = proc.lock();
            let state = match p.state() {
                ProcessState::Ready => "R (running)",
                ProcessState::Running => "R (running)",
                ProcessState::Blocked => "S (sleeping)",
                ProcessState::Zombie => "Z (zombie)",
            };

            format!(
                "Name:\tinit\n\
                 State:\t{}\n\
                 Pid:\t{}\n\
                 PPid:\t{}\n\
                 Uid:\t{}\t{}\t{}\t{}\n\
                 Gid:\t{}\t{}\t{}\t{}\n",
                state,
                self.pid,
                p.ppid(),
                p.credentials().uid,
                p.credentials().euid,
                p.credentials().uid,
                p.credentials().uid,
                p.credentials().gid,
                p.credentials().egid,
                p.credentials().gid,
                p.credentials().gid,
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
        // For now, just return "init" as cmdline
        // In a real implementation, we'd store the actual command line
        let content = b"init\0";

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
        Ok(Stat::new(VnodeType::File, Mode::new(0o444), 5, self.ino))
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
        let cached_kb = 0u64;  // Not tracked
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
