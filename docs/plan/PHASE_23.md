# Phase 23: External Media

**Stage:** 5 - Polish
**Status:** Complete (x86_64)
**Dependencies:** Phase 11 (Storage), Phase 21 (Security)

---

## Goal

Implement secure policies for USB drives and network shares.

---

## Deliverables

| Item | Status |
|------|--------|
| USB media detection | [x] |
| Network share mounting (SMB/NFS) | [x] |
| Read-only by default policy | [x] |
| User promotion workflow | [x] |
| Trust verification | [x] |
| Automount daemon | [x] |

---

## Architecture Status

| Arch | Detection | Policy | Promotion | Done |
|------|-----------|--------|-----------|------|
| x86_64 | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] |

---

## External Media Security Model

```
┌─────────────────────────────────────────────────────┐
│               External Media Flow                    │
│                                                      │
│  USB/Network Share Detected                          │
│           │                                          │
│           ▼                                          │
│  ┌─────────────────┐                                │
│  │ Check Trust DB  │                                │
│  └────────┬────────┘                                │
│           │                                          │
│     ┌─────┴─────┐                                   │
│     ▼           ▼                                   │
│  Trusted    Untrusted                               │
│     │           │                                   │
│     ▼           ▼                                   │
│  Mount RW   Mount RO                                │
│             + Quarantine                            │
│                 │                                   │
│                 ▼                                   │
│           User Prompt                               │
│           "Trust this device?"                      │
│                 │                                   │
│           ┌────┴────┐                               │
│           ▼         ▼                               │
│        Accept    Reject                             │
│           │         │                               │
│           ▼         ▼                               │
│     Promote RW  Keep RO/Eject                       │
└─────────────────────────────────────────────────────┘
```

---

## Device Trust Database

```rust
pub struct DeviceTrustDb {
    /// Trusted USB devices by serial/UUID
    usb_devices: HashMap<UsbId, DeviceTrust>,

    /// Trusted network shares by URL
    network_shares: HashMap<ShareUrl, DeviceTrust>,

    /// Default policy
    default_policy: MediaPolicy,
}

pub struct DeviceTrust {
    /// Device identifier
    id: DeviceId,

    /// Human-readable name
    name: String,

    /// Trust level
    trust: TrustLevel,

    /// Last seen timestamp
    last_seen: Timestamp,

    /// Number of connections
    connect_count: u32,
}

pub enum TrustLevel {
    /// Always trusted, mount read-write
    Trusted,

    /// Prompt on connect
    AskOnConnect,

    /// Always read-only
    ReadOnly,

    /// Block entirely
    Blocked,
}

pub struct MediaPolicy {
    /// Default mount mode for unknown devices
    default_mount: MountMode,

    /// Require password to promote to RW
    require_auth_for_rw: bool,

    /// Auto-eject after inactivity
    auto_eject_minutes: Option<u32>,

    /// Scan for signatures on executables
    verify_executables: bool,
}
```

---

## USB Device Detection

```rust
// USB device event
pub struct UsbEvent {
    pub event_type: UsbEventType,
    pub device: UsbDevice,
}

pub enum UsbEventType {
    Connected,
    Disconnected,
}

pub struct UsbDevice {
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub device_class: u8,
    pub partitions: Vec<Partition>,
}

// udev-like device management
pub struct MediaManager {
    /// Known devices
    devices: HashMap<DevicePath, UsbDevice>,

    /// Mount points
    mounts: HashMap<DevicePath, MountPoint>,

    /// Trust database
    trust_db: DeviceTrustDb,

    /// Event listeners
    listeners: Vec<Box<dyn MediaEventHandler>>,
}
```

---

## Network Share Support

```rust
// SMB share
pub struct SmbShare {
    pub server: String,
    pub share: String,
    pub username: Option<String>,
    pub domain: Option<String>,
    pub password: Option<SecureString>,
}

// NFS share
pub struct NfsShare {
    pub server: String,
    pub path: String,
    pub version: NfsVersion,
}

// Mount options
pub struct ShareMountOptions {
    pub read_only: bool,
    pub timeout: Duration,
    pub cache_mode: CacheMode,
    pub uid_map: Option<UidMap>,
}

// Network share mounting
pub fn mount_smb(share: &SmbShare, mountpoint: &Path, opts: &ShareMountOptions) -> Result<()>;
pub fn mount_nfs(share: &NfsShare, mountpoint: &Path, opts: &ShareMountOptions) -> Result<()>;
```

---

## Promotion Workflow

```
User requests write access to /media/usb0
                │
                ▼
┌────────────────────────────────┐
│    Authentication Required     │
│                                │
│    Enter password to enable    │
│    write access to:            │
│                                │
│    SanDisk Cruzer 32GB         │
│    Serial: ABC123              │
│                                │
│    [Password: ________]        │
│                                │
│    [ ] Remember this device    │
│                                │
│    [Cancel]  [OK]              │
└────────────────────────────────┘
                │
                ▼ (on success)
        Remount read-write
        Update trust DB if checked
```

---

## Automount Daemon

```rust
pub struct AutomountDaemon {
    /// Media manager
    media: MediaManager,

    /// Mount point base
    mount_base: PathBuf,  // /media/

    /// Configuration
    config: AutomountConfig,

    /// Active mounts
    active: HashMap<DeviceId, ActiveMount>,
}

impl AutomountDaemon {
    /// Handle device connection
    fn on_device_connected(&mut self, device: UsbDevice);

    /// Handle device disconnection
    fn on_device_disconnected(&mut self, device_path: &Path);

    /// Handle promotion request
    fn handle_promotion(&mut self, mount: &Path, auth: &AuthToken) -> Result<()>;

    /// Periodic cleanup
    fn cleanup_stale_mounts(&mut self);
}

// Systemd-style unit
// /etc/oxide/automount.conf
// [Automount]
// MountBase=/media
// DefaultMode=ro
// RequireAuthForRW=true
// AutoEject=30
```

---

## Key Files

```
crates/media/media/src/
├── lib.rs
├── manager.rs         # Media manager
├── usb.rs             # USB device handling
├── trust.rs           # Trust database
└── policy.rs          # Mount policies

crates/media/automount/src/
├── lib.rs
├── daemon.rs          # Automount daemon
├── mount.rs           # Mount operations
└── config.rs          # Configuration

crates/net/smb/src/
├── lib.rs
└── mount.rs           # SMB mounting

crates/net/nfs/src/
├── lib.rs
└── mount.rs           # NFS mounting

userspace/media/
├── mount       # Manual mount tool
├── eject       # Eject tool
└── media-ctl   # Media control
```

---

## Exit Criteria

- [x] USB device auto-detected on insert
- [x] Devices mount read-only by default
- [x] User can promote to read-write with auth
- [x] Trusted devices mount read-write automatically
- [x] SMB shares mountable
- [x] NFS shares mountable
- [x] Executables verified before running
- [ ] Works on all 8 architectures

---

## Test: USB Workflow

```bash
# Insert USB drive (auto-detected)
[automount] New device: SanDisk Cruzer (untrusted)
[automount] Mounted /dev/sda1 at /media/usb0 (read-only)

# Try to write
$ touch /media/usb0/test.txt
touch: cannot touch '/media/usb0/test.txt': Read-only file system

# Promote to read-write
$ media-ctl promote /media/usb0
Password: ******
[automount] Promoted /media/usb0 to read-write

# Now writing works
$ touch /media/usb0/test.txt
$ ls /media/usb0/test.txt
/media/usb0/test.txt

# Trust this device for future use
$ media-ctl trust /media/usb0
Device "SanDisk Cruzer" (serial: ABC123) added to trusted list.

# Eject
$ eject /media/usb0
Unmounted /media/usb0
Safe to remove device.
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 23 of OXIDE Implementation*
