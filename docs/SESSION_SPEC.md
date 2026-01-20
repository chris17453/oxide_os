# OXIDE Session & Display Routing Specification

**Version:** 1.0
**Status:** Draft
**License:** MIT

---

## 0) Overview

This document explains how OXIDE routes display output and handles input across different session types, whether running on bare metal or in a VM.

**The key principle:** All process I/O goes through the TTY layer. The TTY is then connected to a *backend* (hardware console, PTY, serial, etc.).

---

## 1) Session Types

| Session Type | TTY Device | Backend | Use Case |
|--------------|------------|---------|----------|
| **Local Console** | /dev/tty1-12 | Framebuffer + Keyboard | Bare metal, direct access |
| **Serial Console** | /dev/ttyS0 | UART | Headless servers, debugging |
| **SSH/Remote Shell** | /dev/pts/N | PTY → Network | Remote administration |
| **VNC/Remote Desktop** | /dev/tty7+ | Framebuffer → Network | Remote GUI |
| **Local Desktop** | /dev/tty7 | Framebuffer + GPU | Local GUI session |
| **Container** | /dev/pts/N | PTY | Isolated environments |

---

## 2) Architecture: How Output Flows

### 2.1 Text Output (printf, echo, etc.)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                             │
│   User Process                                                              │
│   ┌──────────────────────────────────────────────────────────┐              │
│   │  printf("Hello\n");                                      │              │
│   │  write(1, "Hello\n", 6);  // fd 1 = stdout               │              │
│   └──────────────────────────┬───────────────────────────────┘              │
│                              │                                              │
│                              ▼                                              │
│   Kernel: File Descriptor Table                                             │
│   ┌──────────────────────────────────────────────────────────┐              │
│   │  fd 0 → /dev/tty  (stdin)                                │              │
│   │  fd 1 → /dev/tty  (stdout)  ◄── write() goes here        │              │
│   │  fd 2 → /dev/tty  (stderr)                               │              │
│   └──────────────────────────┬───────────────────────────────┘              │
│                              │                                              │
│                              ▼                                              │
│   TTY Layer (/dev/tty → actual tty device)                                  │
│   ┌──────────────────────────────────────────────────────────┐              │
│   │  • Line discipline processing                            │              │
│   │  • ANSI escape sequence parsing                          │              │
│   │  • Output buffering                                      │              │
│   └──────────────────────────┬───────────────────────────────┘              │
│                              │                                              │
│              ┌───────────────┼───────────────┐                              │
│              │               │               │                              │
│              ▼               ▼               ▼                              │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                      │
│   │  /dev/tty1   │  │ /dev/pts/0   │  │ /dev/ttyS0   │                      │
│   │  (Console)   │  │ (PTY Slave)  │  │ (Serial)     │                      │
│   └──────┬───────┘  └──────┬───────┘  └──────┬───────┘                      │
│          │                 │                 │                              │
│          ▼                 ▼                 ▼                              │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                      │
│   │  Framebuffer │  │  PTY Master  │  │  UART Driver │                      │
│   │  Console     │  │  (ssh/tmux)  │  │              │                      │
│   │  Driver      │  └──────┬───────┘  └──────┬───────┘                      │
│   └──────┬───────┘         │                 │                              │
│          │                 │                 │                              │
│          ▼                 ▼                 ▼                              │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                      │
│   │   Monitor    │  │   Network    │  │  Serial Port │                      │
│   │   (local)    │  │   (TCP)      │  │  (hardware)  │                      │
│   └──────────────┘  └──────────────┘  └──────────────┘                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Graphics Output (GUI applications)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                             │
│   GUI Application (Python with Canvas, Desktop App, etc.)                   │
│   ┌──────────────────────────────────────────────────────────┐              │
│   │  canvas.fill_rect(10, 10, 100, 100)                      │              │
│   │  window.present()                                        │              │
│   └──────────────────────────┬───────────────────────────────┘              │
│                              │                                              │
│                              ▼                                              │
│   Display Server Protocol (Unix socket: /run/display)                │
│   ┌──────────────────────────────────────────────────────────┐              │
│   │  Client → Server IPC messages                            │              │
│   │  • CreateBuffer, Attach, Commit, Damage                  │              │
│   └──────────────────────────┬───────────────────────────────┘              │
│                              │                                              │
│                              ▼                                              │
│   Display Server (display)                                           │
│   ┌──────────────────────────────────────────────────────────┐              │
│   │  Compositor                                              │              │
│   │  • Combines all window surfaces                          │              │
│   │  • Handles window stacking, transparency                 │              │
│   │  • Renders to output buffer                              │              │
│   └──────────────────────────┬───────────────────────────────┘              │
│                              │                                              │
│              ┌───────────────┼───────────────┐                              │
│              │               │               │                              │
│              ▼               ▼               ▼                              │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                      │
│   │  Local GPU   │  │  VNC Server  │  │  RDP Server  │                      │
│   │  Backend     │  │  Backend     │  │  Backend     │                      │
│   └──────┬───────┘  └──────┬───────┘  └──────┬───────┘                      │
│          │                 │                 │                              │
│          ▼                 ▼                 ▼                              │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                      │
│   │  Monitor     │  │  VNC Client  │  │  RDP Client  │                      │
│   │  (local)     │  │  (remote)    │  │  (remote)    │                      │
│   └──────────────┘  └──────────────┘  └──────────────┘                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3) Console Driver Architecture

### 3.1 The Console is Just a TTY + Renderer

```rust
/// The console combines:
/// 1. A TTY (for line discipline, termios, etc.)
/// 2. A renderer (writes characters to framebuffer)

pub struct ConsoleDriver {
    /// TTY device for this console (e.g., /dev/tty1)
    tty: Arc<Tty>,

    /// Framebuffer to render to
    framebuffer: Arc<dyn Framebuffer>,

    /// Font for rendering
    font: BitmapFont,

    /// Character grid (what's currently on screen)
    screen: ScreenBuffer,

    /// Cursor position
    cursor: CursorPos,

    /// ANSI escape sequence state machine
    ansi_parser: AnsiParser,
}

impl ConsoleDriver {
    /// Called when data is written to the TTY
    pub fn output(&mut self, data: &[u8]) {
        for byte in data {
            match self.ansi_parser.feed(*byte) {
                AnsiAction::Print(ch) => {
                    self.put_char(ch);
                }
                AnsiAction::MoveCursor(x, y) => {
                    self.cursor = CursorPos { x, y };
                }
                AnsiAction::SetColor(fg, bg) => {
                    self.current_fg = fg;
                    self.current_bg = bg;
                }
                AnsiAction::ClearScreen => {
                    self.clear();
                }
                AnsiAction::ScrollUp(lines) => {
                    self.scroll(lines);
                }
                // ... more escape sequences
            }
        }
        self.render_to_framebuffer();
    }

    fn put_char(&mut self, ch: char) {
        self.screen.set(self.cursor.x, self.cursor.y, Cell {
            character: ch,
            fg: self.current_fg,
            bg: self.current_bg,
        });
        self.advance_cursor();
    }

    fn render_to_framebuffer(&mut self) {
        // Only render dirty cells
        for (x, y, cell) in self.screen.dirty_cells() {
            self.render_cell(x, y, cell);
        }
        self.screen.clear_dirty();
    }

    fn render_cell(&mut self, x: u32, y: u32, cell: &Cell) {
        let px = x * self.font.width;
        let py = y * self.font.height;

        // Draw background
        self.framebuffer.fill_rect(px, py,
            self.font.width, self.font.height, cell.bg);

        // Draw glyph
        if let Some(glyph) = self.font.get_glyph(cell.character) {
            self.framebuffer.blit_glyph(px, py, glyph, cell.fg);
        }
    }
}
```

### 3.2 Multiple Virtual Consoles

```rust
pub struct VirtualConsoleManager {
    /// Consoles tty1-tty12
    consoles: [ConsoleDriver; 12],

    /// Which console is currently active (displayed)
    active: usize,

    /// The shared framebuffer (only one can use it at a time)
    framebuffer: Arc<dyn Framebuffer>,
}

impl VirtualConsoleManager {
    pub fn switch_to(&mut self, console: usize) {
        if console >= 12 { return; }

        // Save current console's screen state
        self.consoles[self.active].save_state();

        // Switch active console
        self.active = console;

        // Restore and redraw new console
        self.consoles[self.active].restore_state();
        self.consoles[self.active].full_redraw();
    }

    /// Called on Ctrl+Alt+F1-F12
    pub fn handle_console_switch(&mut self, key: Keycode) {
        match key {
            Keycode::F1 => self.switch_to(0),
            Keycode::F2 => self.switch_to(1),
            // ...
            Keycode::F12 => self.switch_to(11),
            _ => {}
        }
    }
}
```

---

## 4) How Different Backends Work

### 4.1 Bare Metal Boot

```
Boot sequence:
1. UEFI provides GOP framebuffer
2. Kernel takes over framebuffer
3. Console driver initialized on /dev/tty1
4. init spawns getty on tty1-tty6
5. User sees login prompt on monitor

Hardware path:
  Process → /dev/tty1 → ConsoleDriver → GOP Framebuffer → Monitor
```

### 4.2 VM Boot (QEMU with virtio-gpu)

```
Boot sequence:
1. QEMU provides virtio-gpu device
2. Kernel initializes virtio-gpu driver
3. Console driver uses virtio-gpu scanout
4. Output appears in QEMU window

Hardware path:
  Process → /dev/tty1 → ConsoleDriver → virtio-gpu → QEMU → Host Display
```

### 4.3 Headless Server (Serial Console)

```
Boot sequence:
1. No framebuffer (or unused)
2. Kernel initializes serial port (COM1/ttyS0)
3. init spawns getty on /dev/ttyS0
4. User connects via serial terminal

Hardware path:
  Process → /dev/ttyS0 → UART Driver → Serial Cable → Terminal Emulator
```

### 4.4 SSH Connection

```
Connection sequence:
1. sshd accepts connection
2. sshd allocates PTY pair (/dev/ptmx → /dev/pts/N)
3. sshd forks shell attached to /dev/pts/N
4. Shell I/O goes through PTY → SSH → Network

Path:
  Process → /dev/pts/0 → PTY Master → sshd → TCP → SSH Client → User Terminal
```

### 4.5 VNC Remote Desktop

```
Connection sequence:
1. VNC server captures compositor output
2. Encodes frames and sends over network
3. Receives input events from client
4. Routes input to display server

Path (output):
  GUI App → Display Server → VNC Encoder → TCP → VNC Client → Remote Screen

Path (input):
  Remote Keyboard → VNC Client → TCP → VNC Server → Display Server → GUI App
```

---

## 5) Backend Abstraction

### 5.1 Output Backend Trait

```rust
/// All display outputs implement this
pub trait DisplayBackend: Send + Sync {
    /// Get backend type
    fn backend_type(&self) -> BackendType;

    /// Get display dimensions
    fn dimensions(&self) -> (u32, u32);

    /// Get pixel format
    fn format(&self) -> PixelFormat;

    /// Render a frame
    fn present(&mut self, buffer: &GraphicsBuffer, damage: &[Rect]) -> Result<()>;

    /// Handle vsync (if applicable)
    fn wait_vsync(&self) -> Result<()>;

    /// Set display mode
    fn set_mode(&mut self, width: u32, height: u32) -> Result<()>;
}

pub enum BackendType {
    Framebuffer,    // Direct memory-mapped framebuffer (GOP, VBE)
    VirtioGpu,      // QEMU/KVM virtual GPU
    DrmKms,         // Real hardware via DRM/KMS
    VncServer,      // Remote via VNC protocol
    RdpServer,      // Remote via RDP protocol
    OxideRemote,   // Native OXIDE remote protocol
}
```

### 5.2 Backend Detection and Initialization

```rust
pub fn init_display_backends() -> Vec<Box<dyn DisplayBackend>> {
    let mut backends = Vec::new();

    // Try GOP framebuffer (always available after UEFI boot)
    if let Some(gop) = detect_gop_framebuffer() {
        backends.push(Box::new(GopBackend::new(gop)));
    }

    // Probe PCI for GPUs
    for device in pci_enumerate() {
        // virtio-gpu
        if device.vendor == VIRTIO_VENDOR && device.device == VIRTIO_GPU {
            if let Ok(vgpu) = VirtioGpuBackend::new(&device) {
                backends.push(Box::new(vgpu));
            }
        }

        // Intel integrated graphics
        if device.vendor == INTEL_VENDOR && is_intel_gpu(device.device) {
            if let Ok(i915) = IntelBackend::new(&device) {
                backends.push(Box::new(i915));
            }
        }

        // AMD GPU
        if device.vendor == AMD_VENDOR && is_amd_gpu(device.device) {
            if let Ok(amd) = AmdBackend::new(&device) {
                backends.push(Box::new(amd));
            }
        }
    }

    backends
}
```

### 5.3 Fallback Chain

```rust
pub fn select_primary_backend(backends: &[Box<dyn DisplayBackend>]) -> &dyn DisplayBackend {
    // Prefer real GPU over virtio over basic framebuffer
    for backend in backends {
        if matches!(backend.backend_type(), BackendType::DrmKms) {
            return backend.as_ref();
        }
    }
    for backend in backends {
        if matches!(backend.backend_type(), BackendType::VirtioGpu) {
            return backend.as_ref();
        }
    }
    // Fall back to GOP/basic framebuffer
    &backends[0]
}
```

---

## 6) Session Management

### 6.1 Session Types

```rust
pub enum SessionType {
    /// Local console session (tty1-tty6)
    Console { tty: u8 },

    /// Local graphical session (tty7+)
    Graphical { tty: u8, display: DisplayId },

    /// Remote shell (SSH, etc.)
    RemoteShell { pty: PtyId, peer: SocketAddr },

    /// Remote desktop (VNC, RDP)
    RemoteDesktop { display: DisplayId, peer: SocketAddr },

    /// Serial console
    Serial { port: SerialPort },
}

pub struct Session {
    pub id: SessionId,
    pub session_type: SessionType,
    pub user: Option<UserId>,
    pub created: Timestamp,
    pub leader: ProcessId,      // Session leader process
    pub foreground_pg: ProcessGroupId,
}
```

### 6.2 Session Lifecycle

```rust
impl SessionManager {
    /// Create a new console session
    pub fn create_console_session(&mut self, tty: u8) -> Result<SessionId> {
        let tty_path = format!("/dev/tty{}", tty);
        let tty = open(&tty_path, O_RDWR)?;

        // Create session
        let session = Session {
            id: self.next_id(),
            session_type: SessionType::Console { tty },
            user: None,
            created: now(),
            leader: 0,
            foreground_pg: 0,
        };

        self.sessions.insert(session.id, session);
        Ok(session.id)
    }

    /// Create SSH session with PTY
    pub fn create_ssh_session(&mut self, peer: SocketAddr) -> Result<(SessionId, PtyPair)> {
        // Allocate PTY
        let (master, slave) = pty_open()?;
        let pty_id = self.register_pty(master);

        let session = Session {
            id: self.next_id(),
            session_type: SessionType::RemoteShell { pty: pty_id, peer },
            user: None,
            created: now(),
            leader: 0,
            foreground_pg: 0,
        };

        self.sessions.insert(session.id, session);
        Ok((session.id, PtyPair { master, slave }))
    }

    /// Create VNC session
    pub fn create_vnc_session(&mut self, peer: SocketAddr) -> Result<SessionId> {
        // Allocate virtual display
        let display = self.create_virtual_display()?;

        // Start VNC encoder for this display
        let vnc = VncServerSession::new(display, peer)?;

        let session = Session {
            id: self.next_id(),
            session_type: SessionType::RemoteDesktop { display, peer },
            user: None,
            created: now(),
            leader: 0,
            foreground_pg: 0,
        };

        self.sessions.insert(session.id, session);
        Ok(session.id)
    }
}
```

---

## 7) Putting It All Together

### 7.1 Boot to Console (Bare Metal)

```
1. Kernel boots
2. Framebuffer initialized from UEFI GOP
3. ConsoleDriver created for /dev/tty1
4. init starts, opens /dev/console
5. init spawns: getty /dev/tty1
6. getty calls: setsid() → creates session
7. getty calls: ioctl(TIOCSCTTY) → makes tty1 controlling terminal
8. getty opens /dev/tty1 for stdin/stdout/stderr
9. getty prompts: "login: "
10. User types, characters flow:
    Keyboard IRQ → Keyboard Driver → /dev/tty1 (input buffer)
    → getty reads → getty writes prompt → /dev/tty1 (output)
    → ConsoleDriver → Framebuffer → Monitor
```

### 7.2 SSH Connection

```
1. sshd listening on port 22
2. Client connects
3. sshd authenticates user
4. sshd calls: openpty() → gets /dev/ptmx (master) and /dev/pts/0 (slave)
5. sshd forks
6. Child: setsid(), ioctl(TIOCSCTTY, pts/0), dup2 for stdin/stdout/stderr
7. Child: exec("/bin/bash")
8. Parent: reads from master, sends to network; reads from network, writes to master
9. Bash writes "$ " → pts/0 → master → sshd → network → SSH client
10. User types "ls" → SSH client → network → sshd → master → pts/0 → bash
```

### 7.3 Running GUI Application

```
1. Display server (display) running
2. User runs: python my_gui_app.py
3. App: window = gfx.Window("Title", 800, 600)
   → connects to /run/display socket
   → sends CreateSurface message
4. Display server: creates surface, allocates shared memory buffer
   → sends BufferCreated { handle, fd } to app
5. App: canvas.fill_rect(...)
   → draws to shared memory buffer
6. App: window.present()
   → sends Commit message to display server
7. Display server: composites all surfaces
   → renders to active backend (framebuffer/GPU/VNC)
8. Frame appears on screen (or sent to VNC client)
```

---

## 8) Configuration

### 8.1 Console Configuration

```toml
# /etc/oxide/console.conf

[console]
# Number of virtual consoles (1-12)
count = 6

# Font for console rendering
font = "/usr/share/fonts/console/terminus-16.psf"

# Default colors
foreground = "#FFFFFF"
background = "#000000"

# Console assigned to graphical session
graphics_console = 7

[serial]
# Enable serial console
enabled = true
port = "ttyS0"
baud = 115200
```

### 8.2 Display Server Configuration

```toml
# /etc/oxide/display.conf

[compositor]
# Backend priority (first available is used)
backends = ["drm", "virtio-gpu", "framebuffer"]

# VSync
vsync = true

# Double buffering
double_buffer = true

[remote]
# VNC server
vnc_enabled = true
vnc_port = 5900
vnc_password = ""  # Empty = no auth (dangerous!)

# Native OXIDE remote
remote_enabled = true
remote_port = 5800
remote_encryption = true
```

---

## 9) Exit Criteria

- [ ] Console output works on bare metal (GOP)
- [ ] Console output works in QEMU (virtio-gpu)
- [ ] Virtual console switching (Ctrl+Alt+F1-F12)
- [ ] PTY allocation and SSH sessions work
- [ ] VNC server can share desktop
- [ ] GUI applications can create windows
- [ ] Python can draw to screen via Canvas API
- [ ] Multiple simultaneous session types work

---

*End of OXIDE Session & Display Routing Specification*
