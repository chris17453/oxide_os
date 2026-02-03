# OXIDE Sound Manager (soundd)

## Overview

The OXIDE Sound Manager (`soundd`) is a system daemon that provides sound management capabilities similar to PulseAudio or PipeWire. It enables per-user audio session management, stream mixing, and volume control.

## Features

- **Audio Device Management**: Automatically discovers and manages audio hardware devices (`/dev/dsp*`, `/dev/audio*`)
- **Per-User Sessions**: Isolates audio streams by user for privacy and security
- **Stream Mixing**: Multiplexes multiple client audio streams to hardware devices
- **Volume Control**: Master volume and per-stream volume adjustment
- **IPC Interface**: Socket-based communication protocol for audio clients
- **User Authentication**: Permission checks for audio device access

## Architecture

### Components

1. **Device Manager**: Enumerates and opens audio devices in `/dev`
2. **Session Manager**: Maintains per-user audio sessions with isolated mixing
3. **Stream Multiplexer**: Combines multiple client streams for hardware playback
4. **IPC Server**: Listens on `/run/soundd.sock` for client connections
5. **Mixer**: Applies volume adjustments and mixing operations

### Protocol

Clients connect to the sound daemon via Unix socket (`/run/soundd.sock`) and send text-based commands:

- `VOLUME:<0-100>` - Set master volume (0-100)
- `MUTE` - Mute audio output
- `UNMUTE` - Unmute audio output
- `STATUS` - Query current sound system status
- `OPEN_STREAM` - Open an audio stream (future)
- `WRITE_AUDIO` - Submit PCM audio data (future)
- `READ_AUDIO` - Read captured audio (future)
- `CLOSE_STREAM` - Close audio stream (future)

### Audio Format

Default configuration:
- Sample Rate: 48000 Hz
- Channels: 2 (stereo)
- Sample Format: 16-bit signed PCM (S16LE)
- Buffer Size: 4096 frames

## Usage

### Starting the Daemon

The sound daemon is typically started by the system init process:

```bash
soundd daemon
```

### Command Line Interface

Check sound system status:

```bash
soundd status
```

Set master volume (0-100):

```bash
soundd volume 75
```

Mute/unmute audio:

```bash
soundd mute
soundd unmute
```

### Client Integration

Applications can connect to soundd via Unix socket:

```rust
// Open socket connection
let fd = open("/run/soundd.sock", O_RDWR);

// Set volume to 80%
write(fd, b"VOLUME:80");

// Query status
write(fd, b"STATUS");
let mut buf = [0u8; 512];
let n = read(fd, &mut buf);
// Parse status response

close(fd);
```

## Configuration

The sound daemon operates with sensible defaults and minimal configuration:

- Device Directory: `/dev` (scans for `dsp*` and `audio*` devices)
- Socket Path: `/run/soundd.sock`
- Log File: `/var/log/soundd.log`
- Maximum Clients: 32 concurrent connections
- Streams per Client: 8 streams per client

## Implementation Notes

### Current Capabilities

- Device enumeration and management
- IPC infrastructure for client communication
- Master volume control
- User session framework
- Basic mixing infrastructure

### Future Enhancements

1. **Full Stream API**: Complete implementation of OPEN_STREAM, WRITE_AUDIO, etc.
2. **Real-time Mixing**: Active audio mixing from multiple client streams
3. **Hardware Integration**: Direct hardware device I/O with proper buffering
4. **Format Negotiation**: Support for various sample rates and formats
5. **Capture Support**: Audio input/recording functionality
6. **Network Audio**: Stream audio over network (AirPlay, Chromecast-like)
7. **Plugin System**: Effects and filters (EQ, reverb, compression)
8. **Device Hotplug**: Dynamic device add/remove support
9. **Binary Protocol**: More efficient binary protocol for audio data
10. **Advanced Routing**: Complex audio routing and mapping

### Security Considerations

- Per-user session isolation prevents audio eavesdropping
- Permission checks ensure only authorized users access devices
- Socket permissions restrict IPC access (future: use proper Unix credentials)
- No privileged operations required after startup

### Performance

- Low-latency design with configurable buffer sizes
- Non-blocking I/O for client connections
- Efficient mixing algorithms with saturating arithmetic
- Minimal memory footprint (< 1MB for daemon)

## Integration with OXIDE OS

The sound manager integrates with the broader OXIDE OS ecosystem:

- **Kernel Audio Subsystem**: Uses kernel audio device drivers (Intel HDA, VirtIO Sound)
- **VFS**: Accesses audio devices through standard `/dev` interfaces
- **Init System**: Launched by system init as a service
- **Service Manager**: Can be controlled via `service` command (future)
- **User Sessions**: Coordinates with session manager for per-user isolation

## Development

### Building

The sound daemon is built as part of the userspace packages:

```bash
make userspace-pkg PKG=soundd
```

Or as part of the full build:

```bash
make build-full
```

### Testing

Test the daemon in QEMU:

```bash
make run
# In QEMU:
soundd daemon &
soundd status
soundd volume 50
```

### Code Structure

```
userspace/services/soundd/
├── Cargo.toml           # Package manifest
└── src/
    └── main.rs          # Main implementation
```

Key data structures:
- `SoundDaemon`: Main daemon state
- `AudioDevice`: Hardware device representation
- `AudioStream`: Client audio stream
- `Client`: IPC client connection

## References

- Audio subsystem: `kernel/audio/audio/`
- Audio drivers: `kernel/drivers/audio/`
- VirtIO Sound: `kernel/drivers/audio/virtio-snd/`
- Intel HDA: `kernel/drivers/audio/intel-hda/`
- Audio task doc: `docs/audio.task.md`

## License

Part of OXIDE OS - see LICENSE in repository root.

---

*— EchoFrame: Audio + media subsystems*
