//! Password/user database
//!
//! Provides access to user and group information via /etc/passwd and /etc/group,
//! with fallback to hardcoded defaults if files don't exist.

use core::cell::UnsafeCell;

/// Maximum line length in passwd/group files
const MAX_LINE_LEN: usize = 512;

/// Password structure (POSIX)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Passwd {
    /// Username
    pub pw_name: *const u8,
    /// Password (usually "x")
    pub pw_passwd: *const u8,
    /// User ID
    pub pw_uid: u32,
    /// Group ID
    pub pw_gid: u32,
    /// Real name/comment
    pub pw_gecos: *const u8,
    /// Home directory
    pub pw_dir: *const u8,
    /// Shell
    pub pw_shell: *const u8,
}

/// Group structure (POSIX)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Group {
    /// Group name
    pub gr_name: *const u8,
    /// Group password
    pub gr_passwd: *const u8,
    /// Group ID
    pub gr_gid: u32,
    /// Group members
    pub gr_mem: *const *const u8,
}

// =============================================================================
// Password database storage
// =============================================================================

/// Storage for a parsed passwd entry
struct PasswdStorage {
    passwd: Passwd,
    name_buf: [u8; 64],
    passwd_buf: [u8; 64],
    gecos_buf: [u8; 128],
    dir_buf: [u8; 128],
    shell_buf: [u8; 64],
}

/// Thread-safe wrapper for PasswdStorage
struct PasswdStorageCell {
    inner: UnsafeCell<PasswdStorage>,
}

unsafe impl Sync for PasswdStorageCell {}

impl PasswdStorage {
    const fn new() -> Self {
        PasswdStorage {
            passwd: Passwd {
                pw_name: core::ptr::null(),
                pw_passwd: core::ptr::null(),
                pw_uid: 0,
                pw_gid: 0,
                pw_gecos: core::ptr::null(),
                pw_dir: core::ptr::null(),
                pw_shell: core::ptr::null(),
            },
            name_buf: [0; 64],
            passwd_buf: [0; 64],
            gecos_buf: [0; 128],
            dir_buf: [0; 128],
            shell_buf: [0; 64],
        }
    }

    /// Parse a passwd line: name:passwd:uid:gid:gecos:dir:shell
    fn parse_line(&mut self, line: &[u8]) -> bool {
        let fields: [&[u8]; 7] = match split_colon_fields(line) {
            Some(f) => f,
            None => return false,
        };

        // Copy name
        let name_len = fields[0].len().min(self.name_buf.len() - 1);
        self.name_buf[..name_len].copy_from_slice(&fields[0][..name_len]);
        self.name_buf[name_len] = 0;

        // Copy password
        let passwd_len = fields[1].len().min(self.passwd_buf.len() - 1);
        self.passwd_buf[..passwd_len].copy_from_slice(&fields[1][..passwd_len]);
        self.passwd_buf[passwd_len] = 0;

        // Parse UID
        let uid = match parse_u32(fields[2]) {
            Some(u) => u,
            None => return false,
        };

        // Parse GID
        let gid = match parse_u32(fields[3]) {
            Some(g) => g,
            None => return false,
        };

        // Copy gecos
        let gecos_len = fields[4].len().min(self.gecos_buf.len() - 1);
        self.gecos_buf[..gecos_len].copy_from_slice(&fields[4][..gecos_len]);
        self.gecos_buf[gecos_len] = 0;

        // Copy home dir
        let dir_len = fields[5].len().min(self.dir_buf.len() - 1);
        self.dir_buf[..dir_len].copy_from_slice(&fields[5][..dir_len]);
        self.dir_buf[dir_len] = 0;

        // Copy shell
        let shell_len = fields[6].len().min(self.shell_buf.len() - 1);
        self.shell_buf[..shell_len].copy_from_slice(&fields[6][..shell_len]);
        self.shell_buf[shell_len] = 0;

        // Set up passwd struct
        self.passwd.pw_name = self.name_buf.as_ptr();
        self.passwd.pw_passwd = self.passwd_buf.as_ptr();
        self.passwd.pw_uid = uid;
        self.passwd.pw_gid = gid;
        self.passwd.pw_gecos = self.gecos_buf.as_ptr();
        self.passwd.pw_dir = self.dir_buf.as_ptr();
        self.passwd.pw_shell = self.shell_buf.as_ptr();

        true
    }

    /// Set up hardcoded root entry
    fn setup_root(&mut self) {
        self.name_buf[..4].copy_from_slice(b"root");
        self.name_buf[4] = 0;

        self.passwd_buf[0] = b'x';
        self.passwd_buf[1] = 0;

        self.gecos_buf[..4].copy_from_slice(b"root");
        self.gecos_buf[4] = 0;

        self.dir_buf[..5].copy_from_slice(b"/root");
        self.dir_buf[5] = 0;

        self.shell_buf[..7].copy_from_slice(b"/bin/sh");
        self.shell_buf[7] = 0;

        self.passwd.pw_name = self.name_buf.as_ptr();
        self.passwd.pw_passwd = self.passwd_buf.as_ptr();
        self.passwd.pw_uid = 0;
        self.passwd.pw_gid = 0;
        self.passwd.pw_gecos = self.gecos_buf.as_ptr();
        self.passwd.pw_dir = self.dir_buf.as_ptr();
        self.passwd.pw_shell = self.shell_buf.as_ptr();
    }

    /// Set up hardcoded default user entry
    fn setup_default(&mut self, uid: u32) {
        self.name_buf[..7].copy_from_slice(b"default");
        self.name_buf[7] = 0;

        self.passwd_buf[0] = b'x';
        self.passwd_buf[1] = 0;

        self.gecos_buf[..12].copy_from_slice(b"Default User");
        self.gecos_buf[12] = 0;

        self.dir_buf[..5].copy_from_slice(b"/home");
        self.dir_buf[5] = 0;

        self.shell_buf[..7].copy_from_slice(b"/bin/sh");
        self.shell_buf[7] = 0;

        self.passwd.pw_name = self.name_buf.as_ptr();
        self.passwd.pw_passwd = self.passwd_buf.as_ptr();
        self.passwd.pw_uid = uid;
        self.passwd.pw_gid = uid;
        self.passwd.pw_gecos = self.gecos_buf.as_ptr();
        self.passwd.pw_dir = self.dir_buf.as_ptr();
        self.passwd.pw_shell = self.shell_buf.as_ptr();
    }
}

static PWD_STORAGE: PasswdStorageCell = PasswdStorageCell {
    inner: UnsafeCell::new(PasswdStorage::new()),
};

// =============================================================================
// Group database storage
// =============================================================================

/// Storage for a parsed group entry
struct GroupStorage {
    group: Group,
    name_buf: [u8; 64],
    passwd_buf: [u8; 64],
    members: [*const u8; 16],
    member_bufs: [[u8; 32]; 16],
}

/// Thread-safe wrapper for GroupStorage
struct GroupStorageCell {
    inner: UnsafeCell<GroupStorage>,
}

unsafe impl Sync for GroupStorageCell {}

impl GroupStorage {
    const fn new() -> Self {
        GroupStorage {
            group: Group {
                gr_name: core::ptr::null(),
                gr_passwd: core::ptr::null(),
                gr_gid: 0,
                gr_mem: core::ptr::null(),
            },
            name_buf: [0; 64],
            passwd_buf: [0; 64],
            members: [core::ptr::null(); 16],
            member_bufs: [[0; 32]; 16],
        }
    }

    /// Parse a group line: name:passwd:gid:members
    fn parse_line(&mut self, line: &[u8]) -> bool {
        // Find fields by splitting on colons
        let mut field_starts = [0usize; 4];
        let mut field_ends = [0usize; 4];
        let mut field_idx = 0;
        let mut start = 0;

        for i in 0..line.len() {
            if line[i] == b':' || line[i] == b'\n' {
                if field_idx < 4 {
                    field_starts[field_idx] = start;
                    field_ends[field_idx] = i;
                    field_idx += 1;
                    start = i + 1;
                }
            }
        }
        // Handle last field if no trailing colon/newline
        if field_idx < 4 && start <= line.len() {
            field_starts[field_idx] = start;
            field_ends[field_idx] = line.len();
            field_idx += 1;
        }

        if field_idx < 4 {
            return false;
        }

        // Copy name
        let name_len = (field_ends[0] - field_starts[0]).min(self.name_buf.len() - 1);
        self.name_buf[..name_len]
            .copy_from_slice(&line[field_starts[0]..field_starts[0] + name_len]);
        self.name_buf[name_len] = 0;

        // Copy password
        let passwd_len = (field_ends[1] - field_starts[1]).min(self.passwd_buf.len() - 1);
        self.passwd_buf[..passwd_len]
            .copy_from_slice(&line[field_starts[1]..field_starts[1] + passwd_len]);
        self.passwd_buf[passwd_len] = 0;

        // Parse GID
        let gid_field = &line[field_starts[2]..field_ends[2]];
        let gid = match parse_u32(gid_field) {
            Some(g) => g,
            None => return false,
        };

        // Parse members (comma-separated)
        let members_field = &line[field_starts[3]..field_ends[3]];
        let mut member_idx = 0;
        let mut member_start = 0;

        for i in 0..=members_field.len() {
            if i == members_field.len() || members_field[i] == b',' {
                if member_idx < self.member_bufs.len() && i > member_start {
                    let member_len = (i - member_start).min(self.member_bufs[0].len() - 1);
                    self.member_bufs[member_idx][..member_len]
                        .copy_from_slice(&members_field[member_start..member_start + member_len]);
                    self.member_bufs[member_idx][member_len] = 0;
                    self.members[member_idx] = self.member_bufs[member_idx].as_ptr();
                    member_idx += 1;
                }
                member_start = i + 1;
            }
        }
        // Null-terminate members list
        if member_idx < self.members.len() {
            self.members[member_idx] = core::ptr::null();
        }

        // Set up group struct
        self.group.gr_name = self.name_buf.as_ptr();
        self.group.gr_passwd = self.passwd_buf.as_ptr();
        self.group.gr_gid = gid;
        self.group.gr_mem = self.members.as_ptr();

        true
    }

    /// Set up hardcoded root group
    fn setup_root(&mut self) {
        self.name_buf[..4].copy_from_slice(b"root");
        self.name_buf[4] = 0;

        self.passwd_buf[0] = b'x';
        self.passwd_buf[1] = 0;

        self.members[0] = core::ptr::null();

        self.group.gr_name = self.name_buf.as_ptr();
        self.group.gr_passwd = self.passwd_buf.as_ptr();
        self.group.gr_gid = 0;
        self.group.gr_mem = self.members.as_ptr();
    }

    /// Set up hardcoded default group
    fn setup_default(&mut self, gid: u32) {
        self.name_buf[..5].copy_from_slice(b"users");
        self.name_buf[5] = 0;

        self.passwd_buf[0] = b'x';
        self.passwd_buf[1] = 0;

        self.members[0] = core::ptr::null();

        self.group.gr_name = self.name_buf.as_ptr();
        self.group.gr_passwd = self.passwd_buf.as_ptr();
        self.group.gr_gid = gid;
        self.group.gr_mem = self.members.as_ptr();
    }
}

static GRP_STORAGE: GroupStorageCell = GroupStorageCell {
    inner: UnsafeCell::new(GroupStorage::new()),
};

// =============================================================================
// Helper functions
// =============================================================================

/// Split a line into exactly 7 colon-separated fields (for passwd)
fn split_colon_fields(line: &[u8]) -> Option<[&[u8]; 7]> {
    let mut fields: [&[u8]; 7] = [&[]; 7];
    let mut field_idx = 0;
    let mut start = 0;

    for i in 0..line.len() {
        if line[i] == b':' || line[i] == b'\n' {
            if field_idx < 7 {
                fields[field_idx] = &line[start..i];
                field_idx += 1;
                start = i + 1;
            }
        }
    }
    // Handle last field
    if field_idx < 7 && start <= line.len() {
        // Trim trailing newline if present
        let end = if line.last() == Some(&b'\n') {
            line.len() - 1
        } else {
            line.len()
        };
        if start < end {
            fields[field_idx] = &line[start..end];
            field_idx += 1;
        } else if start == end {
            fields[field_idx] = &[];
            field_idx += 1;
        }
    }

    if field_idx == 7 { Some(fields) } else { None }
}

/// Parse a u32 from ASCII bytes
fn parse_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }

    let mut result: u32 = 0;
    for &b in bytes {
        if b < b'0' || b > b'9' {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((b - b'0') as u32)?;
    }
    Some(result)
}

/// Read a file into buffer
fn read_file_to_buf(path: &str, buf: &mut [u8]) -> Option<usize> {
    use crate::{O_RDONLY, close, open, read};

    let fd = open(path, O_RDONLY as u32, 0);
    if fd < 0 {
        return None;
    }

    let n = read(fd, buf);
    close(fd);

    if n > 0 { Some(n as usize) } else { None }
}

/// Line iterator for a buffer
struct LineIter<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> LineIter<'a> {
    fn new(data: &'a [u8]) -> Self {
        LineIter { data, pos: 0 }
    }
}

impl<'a> Iterator for LineIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.data.len() {
            return None;
        }

        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != b'\n' {
            self.pos += 1;
        }

        let line = &self.data[start..self.pos];

        // Skip the newline
        if self.pos < self.data.len() {
            self.pos += 1;
        }

        Some(line)
    }
}

/// Compare a null-terminated C string with a byte slice
fn str_eq_cstr(cstr: *const u8, bytes: &[u8]) -> bool {
    if cstr.is_null() {
        return false;
    }

    unsafe {
        for (i, &b) in bytes.iter().enumerate() {
            if *cstr.add(i) != b {
                return false;
            }
        }
        // Check null terminator
        *cstr.add(bytes.len()) == 0
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Get password entry by UID
///
/// Searches /etc/passwd for the entry matching the given UID.
/// Falls back to hardcoded defaults if file doesn't exist.
pub fn getpwuid(uid: u32) -> *mut Passwd {
    let storage = unsafe { &mut *PWD_STORAGE.inner.get() };

    // Try to read /etc/passwd
    let mut file_buf = [0u8; 4096];
    if let Some(len) = read_file_to_buf("/etc/passwd", &mut file_buf) {
        for line in LineIter::new(&file_buf[..len]) {
            if line.is_empty() || line[0] == b'#' {
                continue;
            }
            if storage.parse_line(line) && storage.passwd.pw_uid == uid {
                return &mut storage.passwd;
            }
        }
    }

    // Fall back to hardcoded entries
    if uid == 0 {
        storage.setup_root();
    } else {
        storage.setup_default(uid);
    }
    &mut storage.passwd
}

/// Get password entry by name
///
/// Searches /etc/passwd for the entry matching the given username.
/// Falls back to hardcoded defaults if file doesn't exist.
pub fn getpwnam(name: *const u8) -> *mut Passwd {
    if name.is_null() {
        return core::ptr::null_mut();
    }

    let storage = unsafe { &mut *PWD_STORAGE.inner.get() };

    // Try to read /etc/passwd
    let mut file_buf = [0u8; 4096];
    if let Some(len) = read_file_to_buf("/etc/passwd", &mut file_buf) {
        for line in LineIter::new(&file_buf[..len]) {
            if line.is_empty() || line[0] == b'#' {
                continue;
            }
            if storage.parse_line(line) {
                // Find the name field in the line (before first colon)
                let name_end = line.iter().position(|&b| b == b':').unwrap_or(line.len());
                let line_name = &line[..name_end];

                if str_eq_cstr(name, line_name) {
                    return &mut storage.passwd;
                }
            }
        }
    }

    // Fall back to hardcoded entries
    unsafe {
        // Check for "root"
        if str_eq_cstr(name, b"root") {
            storage.setup_root();
            return &mut storage.passwd;
        }

        // Default user
        storage.setup_default(1000);
        &mut storage.passwd
    }
}

/// Get group entry by GID
///
/// Searches /etc/group for the entry matching the given GID.
/// Falls back to hardcoded defaults if file doesn't exist.
pub fn getgrgid(gid: u32) -> *mut Group {
    let storage = unsafe { &mut *GRP_STORAGE.inner.get() };

    // Try to read /etc/group
    let mut file_buf = [0u8; 4096];
    if let Some(len) = read_file_to_buf("/etc/group", &mut file_buf) {
        for line in LineIter::new(&file_buf[..len]) {
            if line.is_empty() || line[0] == b'#' {
                continue;
            }
            if storage.parse_line(line) && storage.group.gr_gid == gid {
                return &mut storage.group;
            }
        }
    }

    // Fall back to hardcoded entries
    if gid == 0 {
        storage.setup_root();
    } else {
        storage.setup_default(gid);
    }
    &mut storage.group
}

/// Get group entry by name
///
/// Searches /etc/group for the entry matching the given group name.
/// Falls back to hardcoded defaults if file doesn't exist.
pub fn getgrnam(name: *const u8) -> *mut Group {
    if name.is_null() {
        return core::ptr::null_mut();
    }

    let storage = unsafe { &mut *GRP_STORAGE.inner.get() };

    // Try to read /etc/group
    let mut file_buf = [0u8; 4096];
    if let Some(len) = read_file_to_buf("/etc/group", &mut file_buf) {
        for line in LineIter::new(&file_buf[..len]) {
            if line.is_empty() || line[0] == b'#' {
                continue;
            }
            if storage.parse_line(line) {
                // Find the name field in the line (before first colon)
                let name_end = line.iter().position(|&b| b == b':').unwrap_or(line.len());
                let line_name = &line[..name_end];

                if str_eq_cstr(name, line_name) {
                    return &mut storage.group;
                }
            }
        }
    }

    // Fall back to hardcoded entries
    unsafe {
        if str_eq_cstr(name, b"root") {
            storage.setup_root();
            return &mut storage.group;
        }

        storage.setup_default(1000);
        &mut storage.group
    }
}

// =============================================================================
// Enumeration API (setpwent, getpwent, endpwent)
// =============================================================================

/// State for passwd enumeration
struct PwentState {
    file_buf: [u8; 4096],
    file_len: usize,
    pos: usize,
    initialized: bool,
}

struct PwentStateCell {
    inner: UnsafeCell<PwentState>,
}

unsafe impl Sync for PwentStateCell {}

static PWENT_STATE: PwentStateCell = PwentStateCell {
    inner: UnsafeCell::new(PwentState {
        file_buf: [0; 4096],
        file_len: 0,
        pos: 0,
        initialized: false,
    }),
};

/// Rewind to beginning of passwd database
pub fn setpwent() {
    let state = unsafe { &mut *PWENT_STATE.inner.get() };
    state.pos = 0;

    if !state.initialized {
        if let Some(len) = read_file_to_buf("/etc/passwd", &mut state.file_buf) {
            state.file_len = len;
        } else {
            state.file_len = 0;
        }
        state.initialized = true;
    }
}

/// Get next passwd entry
pub fn getpwent() -> *mut Passwd {
    let state = unsafe { &mut *PWENT_STATE.inner.get() };
    let storage = unsafe { &mut *PWD_STORAGE.inner.get() };

    if !state.initialized {
        setpwent();
    }

    // Find next line
    while state.pos < state.file_len {
        let start = state.pos;
        while state.pos < state.file_len && state.file_buf[state.pos] != b'\n' {
            state.pos += 1;
        }

        let line = &state.file_buf[start..state.pos];

        // Skip newline
        if state.pos < state.file_len {
            state.pos += 1;
        }

        // Skip empty lines and comments
        if line.is_empty() || line[0] == b'#' {
            continue;
        }

        if storage.parse_line(line) {
            return &mut storage.passwd;
        }
    }

    core::ptr::null_mut()
}

/// Close passwd database
pub fn endpwent() {
    let state = unsafe { &mut *PWENT_STATE.inner.get() };
    state.pos = 0;
    state.initialized = false;
}

// =============================================================================
// Syscall wrappers
// =============================================================================

/// Get current user's UID
pub fn getuid() -> u32 {
    unsafe { crate::syscall::syscall0(crate::syscall::SYS_GETUID) as u32 }
}

/// Get current user's effective UID
pub fn geteuid() -> u32 {
    unsafe { crate::syscall::syscall0(crate::syscall::SYS_GETEUID) as u32 }
}

/// Get current user's GID
pub fn getgid() -> u32 {
    unsafe { crate::syscall::syscall0(crate::syscall::SYS_GETGID) as u32 }
}

/// Get current user's effective GID
pub fn getegid() -> u32 {
    unsafe { crate::syscall::syscall0(crate::syscall::SYS_GETEGID) as u32 }
}

/// Set UID
pub fn setuid(uid: u32) -> i32 {
    unsafe { crate::syscall::syscall1(crate::syscall::SYS_SETUID, uid as usize) as i32 }
}

/// Set GID
pub fn setgid(gid: u32) -> i32 {
    unsafe { crate::syscall::syscall1(crate::syscall::SYS_SETGID, gid as usize) as i32 }
}

/// Set effective UID
pub fn seteuid(uid: u32) -> i32 {
    unsafe { crate::syscall::syscall1(crate::syscall::SYS_SETEUID, uid as usize) as i32 }
}

/// Set effective GID
pub fn setegid(gid: u32) -> i32 {
    unsafe { crate::syscall::syscall1(crate::syscall::SYS_SETEGID, gid as usize) as i32 }
}

/// Get login name
///
/// Returns pointer to a static buffer containing the login name,
/// or null if unavailable.
pub fn getlogin() -> *const u8 {
    let uid = getuid();
    let passwd = getpwuid(uid);

    if passwd.is_null() {
        return core::ptr::null();
    }

    unsafe { (*passwd).pw_name }
}

/// Get login name (reentrant version)
///
/// Copies the login name into the provided buffer.
/// Returns 0 on success, -1 on error.
pub fn getlogin_r(buf: &mut [u8]) -> i32 {
    let uid = getuid();
    let passwd = getpwuid(uid);

    if passwd.is_null() {
        return -1;
    }

    unsafe {
        let name = (*passwd).pw_name;
        if name.is_null() {
            return -1;
        }

        // Find length
        let mut len = 0;
        while *name.add(len) != 0 && len < buf.len() - 1 {
            buf[len] = *name.add(len);
            len += 1;
        }
        buf[len] = 0;
    }

    0
}
