# Graphics and Sound Integration for DOOM

## Graphics Subsystem Integration

### Framebuffer Device (`/dev/fb0`)

DOOM uses the existing OXIDE OS framebuffer infrastructure:

**Kernel Components:**
- `kernel/graphics/fb/` - Framebuffer abstraction layer
- `kernel/vfs/devfs/` - Device filesystem with `/dev/fb0` support
- `kernel/drivers/gpu/virtio-gpu/` - VirtIO GPU driver for QEMU

**DOOM Integration:**
1. Opens `/dev/fb0` with `O_RDWR` flags
2. Gets screen info via `FBIOGET_VSCREENINFO` ioctl
3. Gets fixed info via `FBIOGET_FSCREENINFO` ioctl
4. Memory-maps framebuffer using `mmap(PROT_READ|PROT_WRITE, MAP_SHARED)`
5. Writes pixels directly to mapped memory
6. Converts indexed color (256 colors) to RGB32 format

### IOCTLs Used

```rust
const FBIOGET_VSCREENINFO: u64 = 0x4600;  // Variable screen info
const FBIOGET_FSCREENINFO: u64 = 0x4602;  // Fixed screen info
```

**Variable Screen Info Structure:**
- Resolution (xres, yres)
- Virtual resolution
- Bits per pixel
- Color channel offsets and lengths

**Fixed Screen Info Structure:**
- Physical memory start address
- Memory size
- Line length (stride)
- Visual type

### Rendering Pipeline

```
Game State → Raycasting → Indexed Color Buffer → RGB Conversion → Framebuffer
```

1. **Raycasting**: Cast rays for each screen column
2. **Wall Rendering**: Calculate wall heights and shading
3. **Indexed Color**: Use 256-color palette (retro style)
4. **RGB Conversion**: Convert palette index to RGB32
5. **Framebuffer Blit**: Write to mmap'd framebuffer

### Performance Characteristics

- **Direct Memory Access**: No syscalls per pixel
- **Optimal for Software Rendering**: Ideal for raycasting
- **Cache-Friendly**: Linear memory writes
- **35 FPS Target**: 28ms per frame

## Sound Subsystem Integration

### Sound Daemon Connection

DOOM connects to the `soundd` daemon via Unix socket:

**Socket Path:** `/run/soundd.sock`

**Connection Flow:**
1. Create Unix domain socket (`AF_UNIX`, `SOCK_STREAM`)
2. Connect to `/run/soundd.sock`
3. Send text-based commands
4. Receive status responses

### Sound Commands

```
VOLUME:<0-100>    # Set master volume
MUTE              # Mute audio
UNMUTE            # Unmute audio
STATUS            # Query sound system
```

### Integration Points

**Kernel Components:**
- `kernel/audio/audio/` - Audio subsystem
- `kernel/drivers/audio/virtio-snd/` - VirtIO sound driver
- `kernel/drivers/audio/intel-hda/` - Intel HDA driver

**Userspace Components:**
- `userspace/services/soundd/` - Sound daemon
- Device nodes: `/dev/dsp*`, `/dev/audio*`

### Audio Architecture

```
DOOM → Unix Socket → soundd → Audio Device → Hardware
```

1. **DOOM**: Generates sound events (firing, movement, etc.)
2. **Socket IPC**: Sends commands to soundd
3. **soundd**: Manages audio streams and mixing
4. **Audio Device**: Kernel driver interface
5. **Hardware**: Physical audio output

### Current Implementation

**Implemented:**
- ✅ Socket connection to soundd
- ✅ Volume control
- ✅ Connection status checking
- ✅ Error handling

**Planned:**
- 🔄 Sound effect playback
- 🔄 PCM audio streaming
- 🔄 Music playback
- 🔄 Multiple audio channels

## Missing Functionality & Wiring

### Graphics - Nothing Missing! ✅

All required graphics functionality is present:
- ✅ Framebuffer device (`/dev/fb0`)
- ✅ IOCTLs for screen info
- ✅ mmap support for direct memory access
- ✅ RGB32 format support
- ✅ VirtIO GPU driver for QEMU

### Sound - Extension Needed 🔄

Current soundd supports volume control but needs extension for game audio:

**Missing in soundd:**
1. **Sound Effect API**: Commands to play sound samples
2. **PCM Streaming**: Binary protocol for raw audio data
3. **Multiple Channels**: Simultaneous sound effects
4. **Priority System**: Important sounds override background
5. **Format Negotiation**: Sample rate, bit depth

**Recommended Extensions:**

```
# New commands for soundd protocol
OPEN_STREAM:<format>:<rate>:<channels>  # Open audio stream
WRITE_AUDIO:<stream_id>:<data>          # Write PCM data
CLOSE_STREAM:<stream_id>                # Close stream
PLAY_SOUND:<sound_id>:<volume>          # Play pre-loaded sound
```

### Input - Complete ✅

- ✅ Keyboard input via stdin
- ✅ Non-blocking I/O
- ✅ ANSI escape sequence parsing
- ✅ Multiple control schemes (WASD + Arrows)

### File I/O - Complete ✅

- ✅ open/read/close syscalls
- ✅ lseek for file positioning
- ✅ VFS integration
- ✅ WAD file loading framework

## Testing DOOM

### Prerequisites

1. **Kernel**: Built with framebuffer and audio support
2. **Drivers**: VirtIO GPU and sound drivers loaded
3. **soundd**: Running in background
4. **Display**: QEMU with graphics enabled

### Running in QEMU

```bash
# Build everything
make build-full

# Run with graphics
make run

# In OXIDE OS shell:
soundd &          # Start sound daemon
doom              # Launch DOOM
```

### Expected Behavior

1. Framebuffer initializes (resolution detected)
2. Sound system connects (if soundd running)
3. Map loads (procedural 64x64 grid)
4. Game loop starts at 35 FPS
5. Player can move and interact

### Troubleshooting

**"Failed to open /dev/fb0":**
- Ensure VirtIO GPU driver is loaded
- Check devfs is mounted
- Verify framebuffer initialization in kernel

**"Sound system not available":**
- Start soundd daemon first
- Check `/run/soundd.sock` exists
- Verify Unix domain socket support

**Performance Issues:**
- Check QEMU acceleration (KVM)
- Reduce resolution if needed
- Verify no excessive syscalls in hot path

## Future Enhancements

### Graphics
- 🔄 Hardware acceleration hints
- 🔄 VSync support
- 🔄 Resolution switching
- 🔄 Multiple framebuffers (double buffering)

### Sound
- 🔄 Full audio streaming API
- 🔄 3D positional audio
- 🔄 Music playback (MIDI/OGG)
- 🔄 Sound effect library

### Input
- 🔄 Mouse support
- 🔄 Gamepad support
- 🔄 Key remapping

## Documentation References

- Framebuffer: `kernel/graphics/fb/src/framebuffer.rs`
- Sound daemon: `userspace/services/soundd/README.md`
- Audio subsystem: `docs/SOUND_MANAGER_IMPLEMENTATION.md`
- VirtIO GPU: `kernel/drivers/gpu/virtio-gpu/`
- Device filesystem: `kernel/vfs/devfs/src/devices.rs`

---

*— GlassSignal: Graphics pipeline wired and tested*
*— EchoFrame: Audio subsystem integration documented*
*— SableWire: Hardware interface layer complete*
