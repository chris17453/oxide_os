# EFFLUX Graphics & Display Specification

**Version:** 1.0
**Status:** Draft
**License:** MIT

---

## 0) Overview

EFFLUX provides a unified graphics architecture supporting multiple display modes:

| Mode | Description | Use Case |
|------|-------------|----------|
| **Console** | Framebuffer text rendering | Boot, recovery, servers |
| **Terminal** | PTY-based text (SSH, serial) | Remote admin, headless |
| **Desktop** | Full GUI with compositor | Interactive workstations |
| **Remote** | Network-based display (VNC/RDP) | Remote access, thin clients |
| **Headless** | No display output | Servers, containers |

All modes can coexist. A server might run headless locally but expose remote desktop.

---

## 1) Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Applications                                     │
│   ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐   │
│   │  Python  │  │ Terminal │  │  Editor  │  │   Desktop App        │   │
│   │  Canvas  │  │ Emulator │  │          │  │                      │   │
│   └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────────┬───────────┘   │
│        │             │             │                    │               │
├────────┴─────────────┴─────────────┴────────────────────┴───────────────┤
│                         Graphics API (libefflux-gfx)                     │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────────┐    │
│   │ 2D Drawing  │  │ Text/Fonts  │  │    Widget Toolkit           │    │
│   │ Primitives  │  │  Rendering  │  │    (efflux-ui)              │    │
│   └──────┬──────┘  └──────┬──────┘  └─────────────┬───────────────┘    │
│          │                │                        │                    │
├──────────┴────────────────┴────────────────────────┴────────────────────┤
│                         Display Server (efflux-display)                  │
│   ┌────────────────┐  ┌────────────────┐  ┌──────────────────────┐     │
│   │   Compositor   │  │ Window Manager │  │   Input Routing      │     │
│   │   (Wayland-ish)│  │                │  │   (kbd, mouse, touch)│     │
│   └───────┬────────┘  └───────┬────────┘  └──────────┬───────────┘     │
│           │                   │                       │                 │
├───────────┴───────────────────┴───────────────────────┴─────────────────┤
│                         Display Backends                                 │
│   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌───────────┐   │
│   │  Local  │  │ virtio- │  │ Intel   │  │  AMD    │  │  Remote   │   │
│   │ (GOP/FB)│  │  gpu    │  │  i915   │  │ amdgpu  │  │ (VNC/RDP) │   │
│   └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘  └─────┬─────┘   │
│        │            │            │            │              │          │
├────────┴────────────┴────────────┴────────────┴──────────────┴──────────┤
│                              Hardware / Network                          │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │
│   │ Framebuffer │  │  GPU MMIO   │  │   PCIe      │  │   TCP/UDP   │   │
│   └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 2) Display Modes

### 2.1 Console Mode

Hardware-rendered text using framebuffer.

```rust
pub struct Console {
    pub framebuffer: Arc<dyn Framebuffer>,
    pub font: BitmapFont,
    pub grid: Grid<Cell>,
    pub cursor: CursorPos,
    pub colors: ColorScheme,
    pub scroll_region: (u32, u32),
}

pub struct Cell {
    pub codepoint: char,
    pub fg: Color32,
    pub bg: Color32,
    pub attrs: CellAttrs,
}

bitflags! {
    pub struct CellAttrs: u8 {
        const BOLD      = 0x01;
        const ITALIC    = 0x02;
        const UNDERLINE = 0x04;
        const BLINK     = 0x08;
        const REVERSE   = 0x10;
        const STRIKE    = 0x20;
    }
}
```

Features:
- ANSI/VT100/xterm escape sequences
- 256-color and true-color support
- Unicode with fallback glyphs
- Hardware cursor (where supported)
- Multiple virtual consoles (Ctrl+Alt+F1-F12)

### 2.2 Terminal Mode

PTY-based text for remote access. No local display required.

```rust
pub struct TerminalSession {
    pub pty: Arc<Pty>,
    pub termios: Termios,
    pub winsize: Winsize,
    pub transport: TerminalTransport,
}

pub enum TerminalTransport {
    Local,                          // /dev/ttyN
    Serial { port: u16, baud: u32 },
    Ssh { session: SshSession },
    Telnet { socket: TcpStream },   // Discouraged, legacy
}
```

### 2.3 Desktop Mode

Full graphical environment with compositor.

```rust
pub struct Desktop {
    pub compositor: Compositor,
    pub window_manager: WindowManager,
    pub wallpaper: Option<Surface>,
    pub panels: Vec<Panel>,
    pub notifications: NotificationQueue,
}

pub struct Compositor {
    pub outputs: Vec<Output>,
    pub surfaces: Vec<Arc<Surface>>,
    pub damage: DamageTracker,
    pub render_backend: Box<dyn RenderBackend>,
}
```

### 2.4 Remote Desktop Mode

Network-based display for remote access.

```rust
pub struct RemoteDesktop {
    pub protocol: RemoteProtocol,
    pub encoder: Box<dyn VideoEncoder>,
    pub session: RemoteSession,
    pub input_channel: InputChannel,
    pub clipboard: SharedClipboard,
    pub audio: Option<AudioChannel>,
}

pub enum RemoteProtocol {
    Vnc { security: VncSecurity },
    Efflux { encryption: bool },      // Native protocol
    Spice { compression: SpiceCodec },
}
```

---

## 3) Display Server (efflux-display)

### 3.1 Compositor

Wayland-inspired compositor managing all surfaces.

```rust
pub trait Compositor {
    fn create_surface(&mut self) -> Result<SurfaceId>;
    fn destroy_surface(&mut self, id: SurfaceId);
    fn attach_buffer(&mut self, surface: SurfaceId, buffer: BufferHandle);
    fn commit(&mut self, surface: SurfaceId);
    fn damage(&mut self, surface: SurfaceId, region: Rect);
    fn set_position(&mut self, surface: SurfaceId, x: i32, y: i32);
    fn set_size(&mut self, surface: SurfaceId, width: u32, height: u32);
    fn set_z_order(&mut self, surface: SurfaceId, z: i32);
    fn render_frame(&mut self) -> Result<()>;
}

pub struct Surface {
    pub id: SurfaceId,
    pub buffer: Option<BufferHandle>,
    pub position: Point,
    pub size: Size,
    pub z_order: i32,
    pub opacity: f32,
    pub visible: bool,
    pub input_region: Option<Region>,
    pub opaque_region: Option<Region>,
}
```

### 3.2 Window Manager

Handles window placement, decorations, and user interaction.

```rust
pub trait WindowManager {
    fn map_window(&mut self, surface: SurfaceId, hints: WindowHints) -> WindowId;
    fn unmap_window(&mut self, window: WindowId);
    fn move_window(&mut self, window: WindowId, x: i32, y: i32);
    fn resize_window(&mut self, window: WindowId, width: u32, height: u32);
    fn focus_window(&mut self, window: WindowId);
    fn minimize(&mut self, window: WindowId);
    fn maximize(&mut self, window: WindowId);
    fn fullscreen(&mut self, window: WindowId, output: Option<OutputId>);
    fn close_window(&mut self, window: WindowId);
}

pub struct WindowHints {
    pub title: String,
    pub class: String,
    pub min_size: Option<Size>,
    pub max_size: Option<Size>,
    pub resizable: bool,
    pub decorations: bool,
    pub modal: bool,
    pub parent: Option<WindowId>,
}
```

### 3.3 Client Protocol

IPC between applications and display server.

```rust
// Messages from client to server
pub enum ClientMessage {
    CreateSurface,
    DestroySurface { id: SurfaceId },
    CreateBuffer { width: u32, height: u32, format: PixelFormat },
    DestroyBuffer { handle: BufferHandle },
    Attach { surface: SurfaceId, buffer: BufferHandle },
    Commit { surface: SurfaceId },
    Damage { surface: SurfaceId, rects: Vec<Rect> },
    SetTitle { surface: SurfaceId, title: String },
    SetCursor { surface: SurfaceId, cursor: CursorType },
    // ... more
}

// Messages from server to client
pub enum ServerMessage {
    SurfaceCreated { id: SurfaceId },
    BufferCreated { handle: BufferHandle, fd: RawFd },
    Configure { surface: SurfaceId, width: u32, height: u32 },
    Close { surface: SurfaceId },
    KeyPress { key: Keycode, mods: Modifiers },
    KeyRelease { key: Keycode, mods: Modifiers },
    MouseMove { x: i32, y: i32 },
    MouseButton { button: u8, pressed: bool },
    // ... more
}
```

---

## 4) Graphics API (libefflux-gfx)

### 4.1 Low-Level: Direct Buffer Access

```rust
pub struct GraphicsBuffer {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub data: *mut u8,
}

impl GraphicsBuffer {
    pub fn pixels_mut(&mut self) -> &mut [u32];
    pub fn put_pixel(&mut self, x: u32, y: u32, color: Color32);
    pub fn get_pixel(&self, x: u32, y: u32) -> Color32;
    pub fn fill(&mut self, color: Color32);
    pub fn blit(&mut self, src: &GraphicsBuffer, dst_x: i32, dst_y: i32);
    pub fn blit_scaled(&mut self, src: &GraphicsBuffer, dst: Rect, filter: ScaleFilter);
}

pub enum PixelFormat {
    ARGB8888,
    XRGB8888,
    RGB888,
    RGB565,
    RGBA8888,
    BGRA8888,
}
```

### 4.2 Mid-Level: 2D Drawing (Canvas API)

**This is what Python and other apps would use.**

```rust
pub struct Canvas {
    buffer: GraphicsBuffer,
    transform: Transform2D,
    clip: Option<Region>,
    stroke: StrokeStyle,
    fill: FillStyle,
    font: Font,
}

impl Canvas {
    // State
    pub fn save(&mut self);
    pub fn restore(&mut self);
    pub fn reset(&mut self);

    // Transform
    pub fn translate(&mut self, x: f32, y: f32);
    pub fn rotate(&mut self, angle: f32);
    pub fn scale(&mut self, sx: f32, sy: f32);
    pub fn set_transform(&mut self, matrix: &Transform2D);

    // Clipping
    pub fn clip_rect(&mut self, rect: Rect);
    pub fn clip_path(&mut self, path: &Path);
    pub fn reset_clip(&mut self);

    // Style
    pub fn set_stroke_color(&mut self, color: Color);
    pub fn set_stroke_width(&mut self, width: f32);
    pub fn set_line_cap(&mut self, cap: LineCap);
    pub fn set_line_join(&mut self, join: LineJoin);
    pub fn set_fill_color(&mut self, color: Color);
    pub fn set_fill_gradient(&mut self, gradient: &Gradient);
    pub fn set_fill_pattern(&mut self, pattern: &Pattern);

    // Drawing primitives
    pub fn clear(&mut self, color: Color);
    pub fn draw_point(&mut self, x: f32, y: f32);
    pub fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32);
    pub fn draw_rect(&mut self, rect: Rect);
    pub fn fill_rect(&mut self, rect: Rect);
    pub fn draw_rounded_rect(&mut self, rect: Rect, radius: f32);
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f32);
    pub fn draw_circle(&mut self, cx: f32, cy: f32, r: f32);
    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32);
    pub fn draw_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32);
    pub fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32);
    pub fn draw_arc(&mut self, cx: f32, cy: f32, r: f32, start: f32, end: f32);
    pub fn draw_polygon(&mut self, points: &[(f32, f32)]);
    pub fn fill_polygon(&mut self, points: &[(f32, f32)]);
    pub fn draw_polyline(&mut self, points: &[(f32, f32)]);
    pub fn draw_bezier(&mut self, points: &[(f32, f32)]);
    pub fn draw_path(&mut self, path: &Path);
    pub fn fill_path(&mut self, path: &Path);

    // Text
    pub fn set_font(&mut self, font: &Font);
    pub fn set_font_size(&mut self, size: f32);
    pub fn draw_text(&mut self, text: &str, x: f32, y: f32);
    pub fn measure_text(&self, text: &str) -> TextMetrics;

    // Images
    pub fn draw_image(&mut self, image: &Image, x: f32, y: f32);
    pub fn draw_image_scaled(&mut self, image: &Image, dst: Rect);
    pub fn draw_image_portion(&mut self, image: &Image, src: Rect, dst: Rect);

    // Pixel operations
    pub fn get_image_data(&self, rect: Rect) -> ImageData;
    pub fn put_image_data(&mut self, data: &ImageData, x: i32, y: i32);
}

// Path builder for complex shapes
pub struct Path {
    commands: Vec<PathCommand>,
}

impl Path {
    pub fn new() -> Self;
    pub fn move_to(&mut self, x: f32, y: f32);
    pub fn line_to(&mut self, x: f32, y: f32);
    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32);
    pub fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32);
    pub fn arc_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, r: f32);
    pub fn close(&mut self);
}
```

### 4.3 High-Level: Widget Toolkit (efflux-ui)

```rust
pub trait Widget {
    fn id(&self) -> WidgetId;
    fn bounds(&self) -> Rect;
    fn set_bounds(&mut self, rect: Rect);
    fn paint(&self, canvas: &mut Canvas);
    fn handle_event(&mut self, event: &Event) -> EventResult;
    fn children(&self) -> &[Box<dyn Widget>];
}

// Common widgets
pub struct Button { ... }
pub struct Label { ... }
pub struct TextInput { ... }
pub struct TextArea { ... }
pub struct Checkbox { ... }
pub struct RadioButton { ... }
pub struct Slider { ... }
pub struct ProgressBar { ... }
pub struct ScrollView { ... }
pub struct ListView { ... }
pub struct TreeView { ... }
pub struct TabView { ... }
pub struct MenuBar { ... }
pub struct ContextMenu { ... }
pub struct Dialog { ... }
pub struct FileDialog { ... }
pub struct ColorPicker { ... }

// Layout containers
pub struct HBox { ... }
pub struct VBox { ... }
pub struct Grid { ... }
pub struct Stack { ... }
pub struct Splitter { ... }
```

---

## 5) Display Backends

### 5.1 Local Framebuffer (GOP/VBE)

Basic framebuffer from UEFI GOP or legacy VBE.

```rust
pub struct GopFramebuffer {
    base: *mut u8,
    width: u32,
    height: u32,
    stride: u32,
    format: PixelFormat,
}

impl Framebuffer for GopFramebuffer {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn stride(&self) -> u32;
    fn format(&self) -> PixelFormat;
    fn map(&self) -> *mut u8;
    fn flush(&self);                    // No-op for direct mapping
    fn flush_rect(&self, rect: Rect);   // No-op for direct mapping
    fn set_mode(&mut self, width: u32, height: u32) -> Result<()>;
    fn available_modes(&self) -> Vec<DisplayMode>;
}
```

### 5.2 virtio-gpu (QEMU/KVM)

Virtual GPU for hypervisors.

```rust
pub struct VirtioGpu {
    device: VirtioDevice,
    scanouts: Vec<Scanout>,
    resources: HashMap<u32, GpuResource>,
    cursor: CursorState,
}

impl VirtioGpu {
    pub fn create_resource_2d(&mut self, id: u32, format: u32, width: u32, height: u32);
    pub fn resource_attach_backing(&mut self, id: u32, pages: &[PhysAddr]);
    pub fn set_scanout(&mut self, scanout: u32, resource: u32, rect: Rect);
    pub fn transfer_to_host(&mut self, resource: u32, rect: Rect);
    pub fn resource_flush(&mut self, resource: u32, rect: Rect);
    pub fn update_cursor(&mut self, scanout: u32, x: u32, y: u32, resource: u32);
}
```

### 5.3 Hardware GPU (DRM/KMS-like)

For real hardware: Intel, AMD, NVIDIA.

```rust
pub trait GpuDriver: Send + Sync {
    fn name(&self) -> &str;
    fn probe(&self, pci: &PciDevice) -> bool;
    fn init(&mut self, pci: &PciDevice) -> Result<()>;

    // Mode setting
    fn get_connectors(&self) -> Vec<Connector>;
    fn get_modes(&self, connector: ConnectorId) -> Vec<DisplayMode>;
    fn set_mode(&mut self, connector: ConnectorId, mode: &DisplayMode) -> Result<()>;

    // Buffer management
    fn create_dumb_buffer(&mut self, width: u32, height: u32, bpp: u32) -> Result<BufferHandle>;
    fn map_dumb_buffer(&self, handle: BufferHandle) -> Result<*mut u8>;
    fn destroy_dumb_buffer(&mut self, handle: BufferHandle);

    // Framebuffer
    fn create_fb(&mut self, buffer: BufferHandle, width: u32, height: u32,
                 stride: u32, format: u32) -> Result<FbId>;
    fn destroy_fb(&mut self, fb: FbId);

    // Page flip
    fn page_flip(&mut self, crtc: CrtcId, fb: FbId, flags: u32) -> Result<()>;

    // Cursor
    fn set_cursor(&mut self, crtc: CrtcId, buffer: Option<BufferHandle>,
                  width: u32, height: u32, hot_x: u32, hot_y: u32);
    fn move_cursor(&mut self, crtc: CrtcId, x: i32, y: i32);
}

pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh: u32,           // Hz
    pub clock: u32,             // kHz
    pub flags: ModeFlags,
}
```

### 5.4 Remote Display Backend

For VNC, RDP, and native EFFLUX remote protocol.

```rust
pub struct RemoteDisplayBackend {
    protocol: Box<dyn RemoteProtocolHandler>,
    encoder: Box<dyn FrameEncoder>,
    framebuffer: GraphicsBuffer,
    damage: DamageTracker,
    connection: TcpStream,
}

pub trait RemoteProtocolHandler {
    fn handshake(&mut self, stream: &mut TcpStream) -> Result<()>;
    fn send_frame(&mut self, buffer: &GraphicsBuffer, damage: &[Rect]) -> Result<()>;
    fn receive_input(&mut self) -> Result<Option<InputEvent>>;
    fn send_clipboard(&mut self, data: &str) -> Result<()>;
    fn receive_clipboard(&mut self) -> Result<Option<String>>;
}

// VNC (RFB) Protocol
pub struct VncProtocol {
    security: VncSecurity,
    encoding: VncEncoding,
    pixel_format: PixelFormat,
}

pub enum VncEncoding {
    Raw,
    CopyRect,
    RRE,
    Hextile,
    TRLE,
    ZRLE,
    Tight,
    TightPNG,
}

// Native EFFLUX Protocol (better compression, encryption)
pub struct EffluxRemoteProtocol {
    encryption: Option<ChaCha20Poly1305>,
    compression: ZstdCompressor,
    video_codec: Option<VideoCodec>,
    audio_channel: Option<AudioChannel>,
}
```

---

## 6) Remote Desktop

### 6.1 Server

```rust
pub struct RemoteDesktopServer {
    listener: TcpListener,
    sessions: Vec<RemoteSession>,
    auth: Box<dyn Authenticator>,
    config: RemoteConfig,
}

pub struct RemoteSession {
    id: SessionId,
    user: UserId,
    display: RemoteDisplayBackend,
    input: InputForwarder,
    clipboard: ClipboardSync,
    audio: Option<AudioStream>,
    file_transfer: Option<FileTransfer>,
}

impl RemoteDesktopServer {
    pub fn start(config: RemoteConfig) -> Result<Self>;
    pub fn accept_connection(&mut self) -> Result<SessionId>;
    pub fn broadcast_frame(&mut self, buffer: &GraphicsBuffer, damage: &[Rect]);
    pub fn handle_input(&mut self, session: SessionId, event: InputEvent);
}
```

### 6.2 Client

```rust
pub struct RemoteDesktopClient {
    connection: TcpStream,
    protocol: Box<dyn RemoteProtocolHandler>,
    decoder: Box<dyn FrameDecoder>,
    local_display: Box<dyn Framebuffer>,
    input_sender: InputSender,
}

impl RemoteDesktopClient {
    pub fn connect(host: &str, port: u16, auth: &Credentials) -> Result<Self>;
    pub fn receive_frame(&mut self) -> Result<()>;
    pub fn send_input(&mut self, event: InputEvent) -> Result<()>;
    pub fn set_clipboard(&mut self, data: &str) -> Result<()>;
    pub fn get_clipboard(&mut self) -> Result<String>;
}
```

### 6.3 Features

| Feature | VNC | EFFLUX Native | RDP (future) |
|---------|-----|---------------|--------------|
| Encryption | TLS wrap | Built-in ChaCha20 | TLS |
| Compression | Tight/ZRLE | Zstd + H.264 | RemoteFX |
| Audio | No | Yes | Yes |
| File Transfer | No | Yes | Yes |
| Clipboard | Basic | Full (files, images) | Full |
| Multi-monitor | Limited | Yes | Yes |
| Resize | Limited | Dynamic | Dynamic |

---

## 7) Font Rendering

```rust
pub struct FontManager {
    fonts: HashMap<String, Font>,
    fallback_chain: Vec<Font>,
    cache: GlyphCache,
}

pub struct Font {
    family: String,
    style: FontStyle,
    data: FontData,
}

pub struct GlyphCache {
    atlas: GraphicsBuffer,
    entries: HashMap<GlyphKey, GlyphEntry>,
    packer: AtlasPacker,
}

pub struct GlyphEntry {
    uv: Rect,               // Position in atlas
    advance: f32,
    bearing: (f32, f32),
    size: (u32, u32),
}

impl FontManager {
    pub fn load_font(&mut self, path: &Path) -> Result<FontId>;
    pub fn load_system_fonts(&mut self);
    pub fn render_glyph(&mut self, font: FontId, codepoint: char, size: f32) -> &GlyphEntry;
    pub fn shape_text(&self, font: FontId, text: &str, size: f32) -> ShapedText;
}
```

Supported formats:
- TrueType (.ttf)
- OpenType (.otf)
- Bitmap fonts (.bdf, .pcf) for console

---

## 8) Python Graphics Bindings

### 8.1 Python Module: `efflux.graphics`

```python
import efflux.graphics as gfx

# Create a window
window = gfx.Window("My App", 800, 600)

# Get canvas for drawing
canvas = window.canvas()

# Drawing operations
canvas.clear(gfx.Color(255, 255, 255))  # White background

# Draw shapes
canvas.set_fill_color(gfx.Color(255, 0, 0))
canvas.fill_rect(50, 50, 200, 100)

canvas.set_stroke_color(gfx.Color(0, 0, 255))
canvas.set_stroke_width(3)
canvas.draw_circle(400, 300, 50)

# Draw text
canvas.set_font("sans-serif", 24)
canvas.set_fill_color(gfx.Color(0, 0, 0))
canvas.draw_text("Hello, EFFLUX!", 100, 400)

# Draw image
img = gfx.Image.load("/path/to/image.png")
canvas.draw_image(img, 500, 100)

# Update display
window.present()

# Event loop
while window.is_open():
    event = window.poll_event()
    if event:
        if event.type == gfx.EventType.CLOSE:
            break
        elif event.type == gfx.EventType.KEY_PRESS:
            print(f"Key pressed: {event.key}")
        elif event.type == gfx.EventType.MOUSE_MOVE:
            print(f"Mouse at: {event.x}, {event.y}")
```

### 8.2 Turtle Graphics (for education)

```python
import efflux.turtle as turtle

t = turtle.Turtle()
t.speed(5)

for _ in range(4):
    t.forward(100)
    t.right(90)

t.penup()
t.goto(-50, -50)
t.pendown()
t.color("red")
t.circle(30)

turtle.done()
```

### 8.3 Matplotlib-like Plotting

```python
import efflux.plot as plt
import numpy as np

x = np.linspace(0, 2 * np.pi, 100)
y = np.sin(x)

plt.figure(figsize=(800, 600))
plt.plot(x, y, color='blue', linewidth=2)
plt.title("Sine Wave")
plt.xlabel("x")
plt.ylabel("sin(x)")
plt.grid(True)
plt.show()
```

---

## 9) Console Subsystem

### 9.1 Render Backend Trait

The console uses a **pluggable render backend** to support different hardware:

```rust
/// Render backend trait - implemented by each hardware driver
pub trait RenderBackend: Send + Sync {
    /// Backend identification
    fn name(&self) -> &'static str;
    fn backend_type(&self) -> RenderBackendType;

    /// Capabilities
    fn capabilities(&self) -> RenderCapabilities;

    /// Basic operations
    fn dimensions(&self) -> (u32, u32);
    fn set_mode(&mut self, width: u32, height: u32, bpp: u8) -> Result<()>;
    fn available_modes(&self) -> Vec<DisplayMode>;

    /// Pixel operations
    fn put_pixel(&mut self, x: u32, y: u32, color: Color32);
    fn get_pixel(&self, x: u32, y: u32) -> Color32;
    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color32);
    fn clear(&mut self, color: Color32);

    /// Blitting (copy regions)
    fn blit(&mut self, src: &[u8], src_stride: u32,
            dst_x: u32, dst_y: u32, w: u32, h: u32);
    fn blit_transparent(&mut self, src: &[u8], src_stride: u32,
                        dst_x: u32, dst_y: u32, w: u32, h: u32,
                        transparent: Color32);
    fn copy_rect(&mut self, src_x: u32, src_y: u32,
                 dst_x: u32, dst_y: u32, w: u32, h: u32);

    /// Line drawing
    fn draw_line(&mut self, x1: u32, y1: u32, x2: u32, y2: u32, color: Color32);
    fn draw_hline(&mut self, x: u32, y: u32, len: u32, color: Color32);
    fn draw_vline(&mut self, x: u32, y: u32, len: u32, color: Color32);

    /// Shape drawing (may be hardware accelerated)
    fn draw_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color32);
    fn draw_circle(&mut self, cx: u32, cy: u32, r: u32, color: Color32);
    fn fill_circle(&mut self, cx: u32, cy: u32, r: u32, color: Color32);

    /// Text rendering (glyph blitting)
    fn blit_glyph(&mut self, x: u32, y: u32, glyph: &Glyph, fg: Color32, bg: Color32);

    /// Hardware cursor (if supported)
    fn set_cursor_pos(&mut self, x: u32, y: u32);
    fn set_cursor_visible(&mut self, visible: bool);
    fn set_cursor_shape(&mut self, shape: &CursorShape);

    /// Synchronization
    fn flush(&mut self);                    // Commit all pending operations
    fn flush_rect(&mut self, rect: Rect);   // Flush specific region
    fn wait_vsync(&self);                   // Wait for vertical blank
}

pub enum RenderBackendType {
    Software,       // Pure software rendering to framebuffer
    GopFramebuffer, // UEFI GOP (basic hardware framebuffer)
    VbeFramebuffer, // Legacy VBE framebuffer
    VirtioGpu,      // QEMU/KVM virtio-gpu
    IntelGpu,       // Intel integrated graphics
    AmdGpu,         // AMD discrete/integrated
    NvidiaGpu,      // NVIDIA (nouveau or proprietary)
    VncRemote,      // Remote rendering via VNC
    EffluxRemote,   // Native EFFLUX remote protocol
}

bitflags! {
    pub struct RenderCapabilities: u32 {
        const HARDWARE_CURSOR   = 0x0001;   // Hardware cursor support
        const HARDWARE_BLIT     = 0x0002;   // Hardware blitting
        const HARDWARE_FILL     = 0x0004;   // Hardware rectangle fill
        const HARDWARE_LINE     = 0x0008;   // Hardware line drawing
        const HARDWARE_CIRCLE   = 0x0010;   // Hardware circle drawing
        const VSYNC             = 0x0020;   // VSync support
        const DOUBLE_BUFFER     = 0x0040;   // Double buffering
        const PAGE_FLIP         = 0x0080;   // Page flipping
        const ALPHA_BLEND       = 0x0100;   // Alpha blending
        const SCALING           = 0x0200;   // Hardware scaling
        const ROTATION          = 0x0400;   // Hardware rotation
        const MULTI_HEAD        = 0x0800;   // Multiple displays
    }
}
```

### 9.2 Software Render Backend (Fallback)

Always available, renders directly to any framebuffer:

```rust
pub struct SoftwareRenderer {
    framebuffer: *mut u8,
    width: u32,
    height: u32,
    stride: u32,
    format: PixelFormat,
}

impl RenderBackend for SoftwareRenderer {
    fn name(&self) -> &'static str { "software" }
    fn backend_type(&self) -> RenderBackendType { RenderBackendType::Software }

    fn capabilities(&self) -> RenderCapabilities {
        // Software can do everything, just not hardware accelerated
        RenderCapabilities::empty()
    }

    fn put_pixel(&mut self, x: u32, y: u32, color: Color32) {
        if x >= self.width || y >= self.height { return; }
        let offset = (y * self.stride + x * 4) as usize;
        unsafe {
            let ptr = self.framebuffer.add(offset) as *mut u32;
            *ptr = color.to_native(self.format);
        }
    }

    fn draw_line(&mut self, x1: u32, y1: u32, x2: u32, y2: u32, color: Color32) {
        // Bresenham's line algorithm
        let dx = (x2 as i32 - x1 as i32).abs();
        let dy = (y2 as i32 - y1 as i32).abs();
        let sx = if x1 < x2 { 1i32 } else { -1 };
        let sy = if y1 < y2 { 1i32 } else { -1 };
        let mut err = dx - dy;
        let mut x = x1 as i32;
        let mut y = y1 as i32;

        loop {
            self.put_pixel(x as u32, y as u32, color);
            if x == x2 as i32 && y == y2 as i32 { break; }
            let e2 = 2 * err;
            if e2 > -dy { err -= dy; x += sx; }
            if e2 < dx { err += dx; y += sy; }
        }
    }

    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color32) {
        for dy in 0..h {
            for dx in 0..w {
                self.put_pixel(x + dx, y + dy, color);
            }
        }
    }

    fn draw_circle(&mut self, cx: u32, cy: u32, r: u32, color: Color32) {
        // Midpoint circle algorithm
        let mut x = r as i32;
        let mut y = 0i32;
        let mut err = 0i32;

        while x >= y {
            self.put_pixel((cx as i32 + x) as u32, (cy as i32 + y) as u32, color);
            self.put_pixel((cx as i32 + y) as u32, (cy as i32 + x) as u32, color);
            self.put_pixel((cx as i32 - y) as u32, (cy as i32 + x) as u32, color);
            self.put_pixel((cx as i32 - x) as u32, (cy as i32 + y) as u32, color);
            self.put_pixel((cx as i32 - x) as u32, (cy as i32 - y) as u32, color);
            self.put_pixel((cx as i32 - y) as u32, (cy as i32 - x) as u32, color);
            self.put_pixel((cx as i32 + y) as u32, (cy as i32 - x) as u32, color);
            self.put_pixel((cx as i32 + x) as u32, (cy as i32 - y) as u32, color);

            y += 1;
            if err <= 0 {
                err += 2 * y + 1;
            } else {
                x -= 1;
                err += 2 * (y - x) + 1;
            }
        }
    }
    // ... other methods implemented in software
}
```

### 9.3 Hardware-Accelerated Backends

```rust
/// Intel GPU backend with hardware acceleration
pub struct IntelGpuRenderer {
    mmio_base: *mut u8,
    gtt: GlobalTranslationTable,
    ring_buffer: RingBuffer,
    framebuffer: GpuBuffer,
    capabilities: RenderCapabilities,
}

impl RenderBackend for IntelGpuRenderer {
    fn name(&self) -> &'static str { "intel" }
    fn backend_type(&self) -> RenderBackendType { RenderBackendType::IntelGpu }

    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::HARDWARE_BLIT |
        RenderCapabilities::HARDWARE_FILL |
        RenderCapabilities::HARDWARE_CURSOR |
        RenderCapabilities::VSYNC |
        RenderCapabilities::DOUBLE_BUFFER |
        RenderCapabilities::PAGE_FLIP
    }

    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color32) {
        // Use Intel 2D blitter engine
        self.emit_xy_color_blt(x, y, w, h, color);
    }

    fn blit(&mut self, src: &[u8], src_stride: u32,
            dst_x: u32, dst_y: u32, w: u32, h: u32) {
        // Use Intel 2D blitter engine
        self.emit_xy_src_copy_blt(src, src_stride, dst_x, dst_y, w, h);
    }

    fn copy_rect(&mut self, src_x: u32, src_y: u32,
                 dst_x: u32, dst_y: u32, w: u32, h: u32) {
        // Hardware screen-to-screen blit
        self.emit_xy_src_copy_blt_self(src_x, src_y, dst_x, dst_y, w, h);
    }

    fn flush(&mut self) {
        // Wait for ring buffer to drain
        self.ring_buffer.flush();
    }
}

/// virtio-gpu backend
pub struct VirtioGpuRenderer {
    device: VirtioDevice,
    resource_id: u32,
    scanout: u32,
    width: u32,
    height: u32,
    local_buffer: Vec<u8>,  // Software buffer, transferred to host
}

impl RenderBackend for VirtioGpuRenderer {
    fn name(&self) -> &'static str { "virtio-gpu" }
    fn backend_type(&self) -> RenderBackendType { RenderBackendType::VirtioGpu }

    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::HARDWARE_CURSOR |
        RenderCapabilities::VSYNC
    }

    // virtio-gpu is mostly software rendering to local buffer,
    // then transfer to host
    fn put_pixel(&mut self, x: u32, y: u32, color: Color32) {
        let offset = (y * self.width + x) as usize * 4;
        self.local_buffer[offset..offset+4].copy_from_slice(&color.to_bytes());
    }

    fn flush(&mut self) {
        // Transfer dirty region to host
        self.transfer_to_host_2d();
        // Tell host to display it
        self.resource_flush();
    }

    fn flush_rect(&mut self, rect: Rect) {
        // Only transfer the dirty rectangle
        self.transfer_to_host_2d_rect(rect);
        self.resource_flush_rect(rect);
    }
}
```

### 9.4 Console Using Render Backend

```rust
pub struct ConsoleDriver {
    /// The render backend (hardware-specific)
    renderer: Box<dyn RenderBackend>,

    /// Font for text rendering
    font: BitmapFont,

    /// Character grid
    screen: ScreenBuffer,

    /// Cursor state
    cursor: CursorState,

    /// Current colors
    fg: Color32,
    bg: Color32,

    /// Dirty tracking
    dirty_cells: BitSet,
}

impl ConsoleDriver {
    pub fn new(renderer: Box<dyn RenderBackend>, font: BitmapFont) -> Self {
        let (width, height) = renderer.dimensions();
        let cols = width / font.width;
        let rows = height / font.height;

        Self {
            renderer,
            font,
            screen: ScreenBuffer::new(cols, rows),
            cursor: CursorState::default(),
            fg: Color32::WHITE,
            bg: Color32::BLACK,
            dirty_cells: BitSet::new(cols * rows),
        }
    }

    /// Write a character at current cursor position
    pub fn put_char(&mut self, ch: char) {
        let x = self.cursor.x;
        let y = self.cursor.y;

        // Update screen buffer
        self.screen.set(x, y, Cell { ch, fg: self.fg, bg: self.bg });
        self.dirty_cells.set(y * self.screen.cols + x);

        // Advance cursor
        self.advance_cursor();
    }

    /// Render dirty cells to backend
    pub fn flush(&mut self) {
        for idx in self.dirty_cells.iter_set() {
            let x = idx % self.screen.cols;
            let y = idx / self.screen.cols;
            self.render_cell(x, y);
        }
        self.dirty_cells.clear();

        // Commit to hardware
        self.renderer.flush();
    }

    fn render_cell(&mut self, col: u32, row: u32) {
        let cell = self.screen.get(col, row);
        let px = col * self.font.width;
        let py = row * self.font.height;

        // Draw background
        self.renderer.fill_rect(px, py, self.font.width, self.font.height, cell.bg);

        // Draw glyph
        if let Some(glyph) = self.font.get_glyph(cell.ch) {
            self.renderer.blit_glyph(px, py, glyph, cell.fg, cell.bg);
        }
    }

    /// Scroll screen up
    pub fn scroll_up(&mut self, lines: u32) {
        // If backend supports hardware copy, use it
        if self.renderer.capabilities().contains(RenderCapabilities::HARDWARE_BLIT) {
            let h = self.screen.rows * self.font.height;
            let scroll_px = lines * self.font.height;
            self.renderer.copy_rect(
                0, scroll_px,           // source
                0, 0,                   // dest
                self.screen.cols * self.font.width,
                h - scroll_px
            );
            // Clear bottom area
            self.renderer.fill_rect(
                0, h - scroll_px,
                self.screen.cols * self.font.width, scroll_px,
                self.bg
            );
        } else {
            // Software scroll: mark everything dirty
            self.screen.scroll_up(lines);
            self.dirty_cells.set_all();
        }
    }
}
```

### 9.5 Virtual Console Manager

```rust
pub struct VirtualConsoleManager {
    consoles: [VirtualConsole; 12],     // F1-F12
    active: usize,
    renderer: Arc<Mutex<Box<dyn RenderBackend>>>,
}

pub struct VirtualConsole {
    index: u8,
    tty: Arc<Tty>,
    driver: ConsoleDriver,
    mode: ConsoleMode,
}

pub enum ConsoleMode {
    Text,           // Character grid
    Graphics,       // Direct framebuffer (for X/Wayland)
}
```

### 9.6 Console Switching

- **Ctrl+Alt+F1-F12**: Switch virtual consoles
- **Ctrl+Alt+F7** (default): Graphics/desktop session
- Console 1-6: Text login
- Console 7-12: Graphics sessions

---

## 10) Hardware Support Roadmap

### 10.1 Phase 1: Basic Display (Boot)

- UEFI GOP framebuffer
- Bitmap font rendering
- Text console with ANSI colors
- Single display

### 10.2 Phase 2: Virtual GPU

- virtio-gpu for QEMU
- Hardware cursor
- Multiple scanouts
- Basic 2D acceleration

### 10.3 Phase 3: Real Hardware (Intel)

- Intel i915 driver (integrated graphics)
- Mode setting (KMS)
- Page flipping
- VSync

### 10.4 Phase 4: Full Desktop

- Compositor (efflux-display)
- Window manager
- Widget toolkit
- Remote desktop server

### 10.5 Phase 5: Advanced GPU

- AMD amdgpu driver
- NVIDIA nouveau (basic)
- OpenGL ES 2.0 (software + GPU)
- Vulkan (future)

---

## 11) devfs Integration

```
/dev/
├── fb0                 # Primary framebuffer
├── fb1                 # Secondary framebuffer
├── dri/
│   ├── card0           # Primary GPU
│   ├── card1           # Secondary GPU
│   └── renderD128      # Render-only node
├── tty0                # Current virtual console
├── tty1-tty12          # Virtual consoles
├── console             # System console
└── input/
    ├── event0          # Keyboard
    ├── event1          # Mouse
    └── mice            # Multiplexed mouse
```

---

## 12) Exit Criteria

### Phase 14a: Basic Framebuffer
- [ ] GOP framebuffer working
- [ ] Text console with 256 colors
- [ ] Virtual console switching
- [ ] Cursor rendering

### Phase 14b: virtio-gpu
- [ ] virtio-gpu driver
- [ ] Multiple displays
- [ ] Hardware cursor
- [ ] Page flipping

### Phase 14c: Display Server
- [ ] Compositor running
- [ ] Window management
- [ ] Client applications connect
- [ ] Input routing

### Phase 14d: Remote Desktop
- [ ] VNC server working
- [ ] Native protocol working
- [ ] Audio streaming
- [ ] Clipboard sync

### Phase 14e: Python Graphics
- [ ] Canvas API from Python
- [ ] Window creation
- [ ] Event handling
- [ ] Image loading/saving

---

## 13) Security Considerations

1. **GPU Memory Isolation**: Prevent processes from reading other processes' GPU buffers
2. **DMA Protection**: IOMMU for GPU DMA
3. **Remote Desktop Auth**: Strong authentication, rate limiting
4. **Input Injection**: Prevent unauthorized input events
5. **Screen Capture**: Permission required for screenshots

---

*End of EFFLUX Graphics Specification*
