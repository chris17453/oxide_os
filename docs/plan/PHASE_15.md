# Phase 15: Audio

**Stage:** 3 - Hardware
**Status:** Not Started
**Dependencies:** Phase 10 (Modules)

---

## Goal

Implement audio subsystem with PCM playback.

---

## Deliverables

| Item | Status |
|------|--------|
| Audio device interface | [ ] |
| virtio-snd driver | [ ] |
| PCM playback | [ ] |
| PCM capture (optional) | [ ] |
| Volume mixer | [ ] |
| /dev/dsp compatibility | [ ] |

---

## Architecture Status

| Arch | Audio Core | virtio-snd | Mixer | Done |
|------|------------|------------|-------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] |

---

## Audio Interface

```rust
pub trait AudioDevice: Send + Sync {
    /// Get supported formats
    fn supported_formats(&self) -> Vec<AudioFormat>;

    /// Configure stream
    fn configure(&self, config: StreamConfig) -> Result<()>;

    /// Start playback/capture
    fn start(&self) -> Result<()>;

    /// Stop playback/capture
    fn stop(&self) -> Result<()>;

    /// Write PCM data (playback)
    fn write(&self, data: &[u8]) -> Result<usize>;

    /// Read PCM data (capture)
    fn read(&self, data: &mut [u8]) -> Result<usize>;

    /// Get/set volume (0-100)
    fn get_volume(&self) -> u8;
    fn set_volume(&self, volume: u8);
}

pub struct StreamConfig {
    pub channels: u8,       // 1=mono, 2=stereo
    pub sample_rate: u32,   // 44100, 48000, etc.
    pub format: SampleFormat,
    pub buffer_size: u32,   // In frames
}

pub enum SampleFormat {
    S16LE,      // Signed 16-bit little-endian
    S16BE,      // Signed 16-bit big-endian
    S32LE,      // Signed 32-bit little-endian
    F32LE,      // Float 32-bit little-endian
}
```

---

## Audio Stack

```
┌─────────────────────────────┐
│      Application            │
│   (write to /dev/dsp)       │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│       Audio Core            │
│  - Format conversion        │
│  - Mixing                   │
│  - Volume control           │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│      Audio Driver           │
│    (virtio-snd, HDA)        │
└─────────────────────────────┘
```

---

## virtio-snd

```rust
// virtio-snd structures
#[repr(C)]
struct VirtioSndConfig {
    jacks: u32,     // Number of jacks
    streams: u32,   // Number of PCM streams
    chmaps: u32,    // Number of channel maps
}

// Control virtqueue commands
const VIRTIO_SND_R_PCM_INFO: u32 = 0x0100;
const VIRTIO_SND_R_PCM_SET_PARAMS: u32 = 0x0101;
const VIRTIO_SND_R_PCM_PREPARE: u32 = 0x0102;
const VIRTIO_SND_R_PCM_RELEASE: u32 = 0x0103;
const VIRTIO_SND_R_PCM_START: u32 = 0x0104;
const VIRTIO_SND_R_PCM_STOP: u32 = 0x0105;

// TX/RX virtqueues for PCM data
#[repr(C)]
struct VirtioSndPcmXfer {
    stream_id: u32,
}

#[repr(C)]
struct VirtioSndPcmStatus {
    status: u32,
    latency_bytes: u32,
}
```

---

## PCM Stream States

```
        ┌─────────────┐
        │   SETUP     │◄──── Initial state
        └──────┬──────┘
               │ SET_PARAMS
               ▼
        ┌─────────────┐
        │  PREPARED   │◄──── Ready to start
        └──────┬──────┘
               │ START
               ▼
        ┌─────────────┐
        │   RUNNING   │◄──── Playing/recording
        └──────┬──────┘
               │ STOP
               ▼
        ┌─────────────┐
        │   STOPPED   │
        └──────┬──────┘
               │ RELEASE
               ▼
        ┌─────────────┐
        │   SETUP     │
        └─────────────┘
```

---

## Mixer

```rust
pub struct Mixer {
    master_volume: AtomicU8,
    channels: Vec<MixerChannel>,
}

pub struct MixerChannel {
    name: String,
    volume: AtomicU8,
    muted: AtomicBool,
}

// /dev/mixer ioctl
const SOUND_MIXER_READ_VOLUME: u32 = 0x80044D00;
const SOUND_MIXER_WRITE_VOLUME: u32 = 0xC0044D00;
```

---

## Key Files

```
crates/audio/efflux-audio/src/
├── lib.rs
├── device.rs          # Audio device trait
├── format.rs          # Audio formats
├── mixer.rs           # Mixer implementation
└── buffer.rs          # Ring buffer for PCM

crates/drivers/audio/efflux-virtio-snd/src/
├── lib.rs
├── control.rs         # Control queue
├── pcm.rs             # PCM handling
└── jack.rs            # Jack detection
```

---

## /dev/dsp Interface

```c
// Open audio device
int fd = open("/dev/dsp", O_WRONLY);

// Set format
int format = AFMT_S16_LE;
ioctl(fd, SNDCTL_DSP_SETFMT, &format);

// Set channels
int channels = 2;
ioctl(fd, SNDCTL_DSP_CHANNELS, &channels);

// Set sample rate
int rate = 44100;
ioctl(fd, SNDCTL_DSP_SPEED, &rate);

// Write PCM data
write(fd, pcm_data, pcm_size);

close(fd);
```

---

## Exit Criteria

- [ ] virtio-snd driver detects device
- [ ] PCM playback produces audio
- [ ] Volume control works
- [ ] /dev/dsp write plays audio
- [ ] Multiple formats supported (S16LE, etc.)
- [ ] Works on all 8 architectures

---

## Test Program

```c
#include <math.h>

int main() {
    int fd = open("/dev/dsp", O_WRONLY);

    // Configure: 16-bit stereo 44100 Hz
    int format = AFMT_S16_LE;
    int channels = 2;
    int rate = 44100;

    ioctl(fd, SNDCTL_DSP_SETFMT, &format);
    ioctl(fd, SNDCTL_DSP_CHANNELS, &channels);
    ioctl(fd, SNDCTL_DSP_SPEED, &rate);

    // Generate 1 second of 440 Hz sine wave
    int samples = rate;
    int16_t *buf = malloc(samples * channels * sizeof(int16_t));

    for (int i = 0; i < samples; i++) {
        double t = (double)i / rate;
        int16_t sample = (int16_t)(sin(2 * M_PI * 440 * t) * 32767);
        buf[i * 2] = sample;      // Left
        buf[i * 2 + 1] = sample;  // Right
    }

    write(fd, buf, samples * channels * sizeof(int16_t));

    free(buf);
    close(fd);
    return 0;
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 15 of EFFLUX Implementation*
