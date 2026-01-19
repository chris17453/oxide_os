//! Password/user database

/// Password structure
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

/// Group structure
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

// Static storage for passwd entries
static mut PWD_STORAGE: PasswdStorage = PasswdStorage::new();

struct PasswdStorage {
    passwd: Passwd,
    name_buf: [u8; 64],
    passwd_buf: [u8; 64],
    gecos_buf: [u8; 128],
    dir_buf: [u8; 128],
    shell_buf: [u8; 64],
}

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

    fn setup_default(&mut self, uid: u32) {
        // Set up a default user entry
        self.name_buf[..4].copy_from_slice(b"user");
        self.name_buf[4] = 0;

        self.passwd_buf[0] = b'x';
        self.passwd_buf[1] = 0;

        self.gecos_buf[..4].copy_from_slice(b"User");
        self.gecos_buf[4] = 0;

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
}

/// Get password entry by uid
pub fn getpwuid(uid: u32) -> *mut Passwd {
    let storage = unsafe { &raw mut PWD_STORAGE };
    unsafe {
        if uid == 0 {
            (*storage).setup_root();
        } else {
            (*storage).setup_default(uid);
        }
        &raw mut (*storage).passwd
    }
}

/// Get password entry by name
pub fn getpwnam(name: *const u8) -> *mut Passwd {
    if name.is_null() {
        return core::ptr::null_mut();
    }

    let storage = unsafe { &raw mut PWD_STORAGE };
    unsafe {
        // Compare with "root"
        let root = b"root\0";
        let mut is_root = true;
        for i in 0..4 {
            if *name.add(i) != root[i] {
                is_root = false;
                break;
            }
        }
        if is_root && *name.add(4) == 0 {
            (*storage).setup_root();
            return &raw mut (*storage).passwd;
        }

        // Default user
        (*storage).setup_default(1000);
        &raw mut (*storage).passwd
    }
}

// Static storage for group entries
static mut GRP_STORAGE: GroupStorage = GroupStorage::new();

struct GroupStorage {
    group: Group,
    name_buf: [u8; 64],
    passwd_buf: [u8; 64],
    members: [*const u8; 2],
}

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
            members: [core::ptr::null(); 2],
        }
    }

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
}

/// Get group entry by gid
pub fn getgrgid(gid: u32) -> *mut Group {
    let storage = unsafe { &raw mut GRP_STORAGE };
    unsafe {
        if gid == 0 {
            (*storage).setup_root();
        } else {
            (*storage).setup_default(gid);
        }
        &raw mut (*storage).group
    }
}

/// Get group entry by name
pub fn getgrnam(name: *const u8) -> *mut Group {
    if name.is_null() {
        return core::ptr::null_mut();
    }

    let storage = unsafe { &raw mut GRP_STORAGE };
    unsafe {
        // Compare with "root"
        let root = b"root\0";
        let mut is_root = true;
        for i in 0..4 {
            if *name.add(i) != root[i] {
                is_root = false;
                break;
            }
        }
        if is_root && *name.add(4) == 0 {
            (*storage).setup_root();
            return &raw mut (*storage).group;
        }

        // Default group
        (*storage).setup_default(1000);
        &raw mut (*storage).group
    }
}

/// Get current user's uid
pub fn getuid() -> u32 {
    unsafe { crate::syscall::syscall0(crate::syscall::SYS_GETUID) as u32 }
}

/// Get current user's effective uid
pub fn geteuid() -> u32 {
    unsafe { crate::syscall::syscall0(crate::syscall::SYS_GETEUID) as u32 }
}

/// Get current user's gid
pub fn getgid() -> u32 {
    unsafe { crate::syscall::syscall0(crate::syscall::SYS_GETGID) as u32 }
}

/// Get current user's effective gid
pub fn getegid() -> u32 {
    unsafe { crate::syscall::syscall0(crate::syscall::SYS_GETEGID) as u32 }
}

/// Set uid
pub fn setuid(uid: u32) -> i32 {
    unsafe { crate::syscall::syscall1(crate::syscall::SYS_SETUID, uid as usize) as i32 }
}

/// Set gid
pub fn setgid(gid: u32) -> i32 {
    unsafe { crate::syscall::syscall1(crate::syscall::SYS_SETGID, gid as usize) as i32 }
}

/// Set effective uid
pub fn seteuid(uid: u32) -> i32 {
    unsafe { crate::syscall::syscall1(crate::syscall::SYS_SETEUID, uid as usize) as i32 }
}

/// Set effective gid
pub fn setegid(gid: u32) -> i32 {
    unsafe { crate::syscall::syscall1(crate::syscall::SYS_SETEGID, gid as usize) as i32 }
}
