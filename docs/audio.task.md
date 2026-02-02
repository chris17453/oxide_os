# Audio Subsystem — Task Tracker

## Completed: Intel HDA Driver

Full Intel High Definition Audio controller driver for QEMU's `intel-hda` (ICH6, PCI `8086:2668`) with `hda-duplex` codec.

### What shipped
- Crate skeleton with register definitions, codec verbs, stream/DMA setup
- `HdaController` with CORB/RIRB command transport, codec widget discovery, output path config
- `AudioDevice` trait implementation — playback, volume control, 32KB ring buffer
- PCI discovery (`is_intel_hda`, `find_intel_hda`)
- Kernel init probe wired into boot sequence
- QEMU flags added to Makefile (`-device intel-hda -device hda-duplex`)
- All unsafe blocks compliant with Rust 2024 edition
- Clean build (0 errors, 0 warnings)

### Key files
| File | Role |
|------|------|
| `kernel/drivers/audio/intel-hda/src/regs.rs` | HDA MMIO register offsets (Intel HDA Spec Rev 1.0a) |
| `kernel/drivers/audio/intel-hda/src/codec.rs` | Codec verb definitions, widget type helpers |
| `kernel/drivers/audio/intel-hda/src/stream.rs` | BDL entries, DMA double-buffer, stream control |
| `kernel/drivers/audio/intel-hda/src/lib.rs` | HdaController, IntelHda, AudioDevice impl, init |
| `kernel/drivers/pci/src/lib.rs` | `is_intel_hda()`, `find_intel_hda()` |
| `kernel/src/init.rs` | Intel HDA probe after VirtIO sound section |

---

## Next Up

### Boot test
- `make run` and verify serial log shows `[SND] Intel HDA audio initialized`
- Confirm PCI enumeration lists `8086:2668`
- Verify `/dev/dsp` write doesn't panic

### Capture support
- Currently playback only; input stream path is stubbed
- Wire up input stream descriptor, ADC discovery, and `read()` path
- Connect to `/dev/dsp` read side

### Interrupt-driven DMA
- Current implementation is polling-based
- Hook HDA controller interrupt (INTCTL/INTSTS) for buffer completion callbacks
- Reduces CPU spin and improves latency

### Multiple codec/stream support
- Currently uses first discovered output path
- Enumerate all codecs and streams, expose as separate audio devices
- Support simultaneous playback + capture on different streams

### Sample format flexibility
- Currently hardcoded to 48kHz/16-bit/stereo (format 0x0011)
- Honor `StreamConfig` sample rate and format fields in `configure()`
- Add 44.1kHz, 8-bit, mono, and other common formats

### Mixer integration
- Wire `/dev/mixer` volume controls to HDA amp gain verbs
- Per-widget gain control (DAC, pin, mixer nodes)
- Mute/unmute support
