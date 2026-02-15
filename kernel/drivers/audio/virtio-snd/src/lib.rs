//! VirtIO Sound Device Driver
//!
//! Implements the VirtIO sound device specification for audio
//! in virtualized environments.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use audio::{
    AudioDevice, AudioDeviceInfo, AudioError, AudioResult, RingBuffer, SampleFormat, StreamConfig,
    StreamState,
};
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use pci::{PciDevice, VirtioPciTransport};
use spin::Mutex;
/// — EchoFrame: shared virtio plumbing — one crate to carry the signal chain
use virtio_core::status as dev_status;
use virtio_core::virtqueue::desc_flags;
use virtio_core::{phys_to_virt, virt_to_phys, Virtqueue};

/// VirtIO sound control commands
mod cmd {
    pub const JACK_INFO: u32 = 0x0001;
    pub const JACK_REMAP: u32 = 0x0002;
    pub const PCM_INFO: u32 = 0x0100;
    pub const PCM_SET_PARAMS: u32 = 0x0101;
    pub const PCM_PREPARE: u32 = 0x0102;
    pub const PCM_RELEASE: u32 = 0x0103;
    pub const PCM_START: u32 = 0x0104;
    pub const PCM_STOP: u32 = 0x0105;
    pub const CHMAP_INFO: u32 = 0x0200;
}

/// VirtIO sound response codes
mod status {
    pub const OK: u32 = 0x8000;
    pub const BAD_MSG: u32 = 0x8001;
    pub const NOT_SUPP: u32 = 0x8002;
    pub const IO_ERR: u32 = 0x8003;
}

/// VirtIO sound PCM formats
mod pcm_fmt {
    pub const IMA_ADPCM: u8 = 0;
    pub const MU_LAW: u8 = 1;
    pub const A_LAW: u8 = 2;
    pub const S8: u8 = 3;
    pub const U8: u8 = 4;
    pub const S16: u8 = 5;
    pub const U16: u8 = 6;
    pub const S18_3: u8 = 7;
    pub const U18_3: u8 = 8;
    pub const S20_3: u8 = 9;
    pub const U20_3: u8 = 10;
    pub const S24_3: u8 = 11;
    pub const U24_3: u8 = 12;
    pub const S20: u8 = 13;
    pub const U20: u8 = 14;
    pub const S24: u8 = 15;
    pub const U24: u8 = 16;
    pub const S32: u8 = 17;
    pub const U32: u8 = 18;
    pub const FLOAT: u8 = 19;
    pub const FLOAT64: u8 = 20;
}

/// VirtIO sound PCM rates (as bit flags)
mod pcm_rate {
    pub const RATE_5512: u32 = 1 << 0;
    pub const RATE_8000: u32 = 1 << 1;
    pub const RATE_11025: u32 = 1 << 2;
    pub const RATE_16000: u32 = 1 << 3;
    pub const RATE_22050: u32 = 1 << 4;
    pub const RATE_32000: u32 = 1 << 5;
    pub const RATE_44100: u32 = 1 << 6;
    pub const RATE_48000: u32 = 1 << 7;
    pub const RATE_64000: u32 = 1 << 8;
    pub const RATE_88200: u32 = 1 << 9;
    pub const RATE_96000: u32 = 1 << 10;
    pub const RATE_176400: u32 = 1 << 11;
    pub const RATE_192000: u32 = 1 << 12;
}

/// Control message header
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CtrlHdr {
    code: u32,
}

/// PCM stream info request
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PcmInfoRequest {
    hdr: CtrlHdr,
    start_id: u32,
    count: u32,
}

/// PCM stream info
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PcmInfo {
    hdr_info: u32,
    hdr_size: u32,
    features: u32,
    formats: u64,
    rates: u64,
    direction: u8,
    channels_min: u8,
    channels_max: u8,
    _padding: [u8; 5],
}

/// PCM set params
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PcmSetParams {
    hdr: CtrlHdr,
    stream_id: u32,
    buffer_bytes: u32,
    period_bytes: u32,
    features: u32,
    channels: u8,
    format: u8,
    rate: u8,
    _padding: u8,
}

/// PCM stream operation (prepare/release/start/stop)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PcmOp {
    hdr: CtrlHdr,
    stream_id: u32,
}

/// PCM transfer header
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PcmXfer {
    stream_id: u32,
}

/// PCM transfer status
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PcmStatus {
    status: u32,
    latency_bytes: u32,
}

/// VirtIO MMIO register offsets
const VIRTIO_MMIO_MAGIC: usize = 0x000;
const VIRTIO_MMIO_VERSION: usize = 0x004;
const VIRTIO_MMIO_DEVICE_ID: usize = 0x008;
const VIRTIO_MMIO_DEVICE_FEATURES: usize = 0x010;
const VIRTIO_MMIO_DEVICE_FEATURES_SEL: usize = 0x014;
const VIRTIO_MMIO_DRIVER_FEATURES: usize = 0x020;
const VIRTIO_MMIO_DRIVER_FEATURES_SEL: usize = 0x024;
const VIRTIO_MMIO_QUEUE_SEL: usize = 0x030;
const VIRTIO_MMIO_QUEUE_NUM_MAX: usize = 0x034;
const VIRTIO_MMIO_QUEUE_NUM: usize = 0x038;
const VIRTIO_MMIO_QUEUE_READY: usize = 0x044;
const VIRTIO_MMIO_QUEUE_NOTIFY: usize = 0x050;
const VIRTIO_MMIO_INTERRUPT_STATUS: usize = 0x060;
const VIRTIO_MMIO_INTERRUPT_ACK: usize = 0x064;
const VIRTIO_MMIO_STATUS: usize = 0x070;
const VIRTIO_MMIO_QUEUE_DESC_LOW: usize = 0x080;
const VIRTIO_MMIO_QUEUE_DESC_HIGH: usize = 0x084;
const VIRTIO_MMIO_QUEUE_AVAIL_LOW: usize = 0x090;
const VIRTIO_MMIO_QUEUE_AVAIL_HIGH: usize = 0x094;
const VIRTIO_MMIO_QUEUE_USED_LOW: usize = 0x0a0;
const VIRTIO_MMIO_QUEUE_USED_HIGH: usize = 0x0a4;
const VIRTIO_MMIO_CONFIG: usize = 0x100;

/// — EchoFrame: device status bits delegated to virtio-core — no more copy-paste liturgy

/// Queue indices
const CONTROLQ: u32 = 0;
const EVENTQ: u32 = 1;
const TXQ: u32 = 2;
const RXQ: u32 = 3;

/// — EchoFrame: descriptor flags and ring structs live in virtio-core now —
/// no more hand-rolled VirtqDesc nightmares in every driver

/// VirtIO sound configuration
#[repr(C)]
struct VirtioSndConfig {
    jacks: u32,
    streams: u32,
    chmaps: u32,
}

/// PCM stream info parsed
struct StreamInfo {
    direction: u8, // 0 = output, 1 = input
    channels_min: u8,
    channels_max: u8,
    formats: u64,
    rates: u64,
}

/// — EchoFrame: queue size inherits from the shared virtqueue spec — 256 slots of sonic potential
const QUEUE_SIZE: usize = virtio_core::virtqueue::MAX_QUEUE_SIZE;

/// VirtIO sound device
/// — EchoFrame: the whole audio pipeline, wired through shared virtqueues now
pub struct VirtioSnd {
    /// MMIO base address (0 when using PCI transport)
    mmio_base: usize,
    /// PCI transport (None for MMIO mode)
    transport: Option<VirtioPciTransport>,
    /// Control virtqueue — shared infrastructure, no more raw pointer rodeos
    ctrl_queue: Option<Virtqueue>,
    /// TX virtqueue — audio data flows through here
    tx_queue: Option<Virtqueue>,
    /// Number of streams
    num_streams: u32,
    /// Stream info
    streams: Vec<StreamInfo>,
    /// Current stream config
    config: Option<StreamConfig>,
    /// Stream state
    state: StreamState,
    /// Volume
    volume: AtomicU8,
    /// Position in frames
    position: AtomicU64,
    /// Audio buffer
    buffer: Option<RingBuffer>,
}

impl VirtioSnd {
    /// Construct from a PCI device — walks capabilities, builds transport
    /// — EchoFrame: sound needs silicon, PCI delivers
    pub fn from_pci(pci_dev: &PciDevice) -> Option<Self> {
        if !pci_dev.is_virtio_snd() {
            return None;
        }

        pci::enable_bus_master(pci_dev.address);
        pci::enable_memory_space(pci_dev.address);

        let caps = pci::find_virtio_caps(pci_dev);
        let transport = VirtioPciTransport::from_caps(pci_dev, &caps)?;

        Some(VirtioSnd {
            mmio_base: 0,
            transport: Some(transport),
            ctrl_queue: None,
            tx_queue: None,
            num_streams: 0,
            streams: Vec::new(),
            config: None,
            state: StreamState::Setup,
            volume: AtomicU8::new(100),
            position: AtomicU64::new(0),
            buffer: None,
        })
    }

    /// Probe for VirtIO sound device (MMIO path)
    pub fn probe(mmio_base: usize) -> Option<Self> {
        let magic = unsafe { read_volatile((mmio_base + VIRTIO_MMIO_MAGIC) as *const u32) };
        if magic != 0x74726976 {
            return None;
        }

        let version = unsafe { read_volatile((mmio_base + VIRTIO_MMIO_VERSION) as *const u32) };
        if version != 2 {
            return None;
        }

        let device_id = unsafe { read_volatile((mmio_base + VIRTIO_MMIO_DEVICE_ID) as *const u32) };
        if device_id != 25 {
            // Not a sound device
            return None;
        }

        Some(VirtioSnd {
            mmio_base,
            transport: None,
            ctrl_queue: None,
            tx_queue: None,
            num_streams: 0,
            streams: Vec::new(),
            config: None,
            state: StreamState::Setup,
            volume: AtomicU8::new(100),
            position: AtomicU64::new(0),
            buffer: None,
        })
    }

    /// Initialize the sound device (dual PCI/MMIO transport)
    /// — EchoFrame: boot the sound hardware, whatever wire it rides
    pub fn init(&mut self) -> Result<(), &'static str> {
        let has_pci = self.transport.is_some();

        if has_pci {
            self.init_pci_handshake()?;
        } else {
            self.init_mmio_handshake()?;
        }

        // Query stream info
        self.query_streams()?;

        // Create audio buffer (64KB ring)
        self.buffer = Some(RingBuffer::new(65536));

        Ok(())
    }

    /// PCI transport VirtIO handshake
    /// — EchoFrame: the sacred PCI init dance — acknowledge, negotiate, pray
    fn init_pci_handshake(&mut self) -> Result<(), &'static str> {
        let t = self.transport.as_ref().ok_or("No PCI transport")?;

        // Reset
        t.write_status(0);
        t.write_status(dev_status::ACKNOWLEDGE);
        t.write_status(dev_status::ACKNOWLEDGE | dev_status::DRIVER);

        // Features
        let _features = t.read_device_features(0);
        t.write_driver_features(0, 0);

        t.write_status(
            dev_status::ACKNOWLEDGE | dev_status::DRIVER | dev_status::FEATURES_OK,
        );

        let status = t.read_status();
        if status & dev_status::FEATURES_OK == 0 {
            t.write_status(dev_status::FAILED);
            return Err("Features not accepted");
        }

        // Read sound-specific config (streams count) from device config region
        self.num_streams = t.read_device_config_u32(4); // offset 4 = streams field

        // Release borrow for queue init
        let _ = t;

        // — EchoFrame: initialize queues via PCI transport using shared Virtqueue
        self.init_queue_pci(CONTROLQ)?;
        self.init_queue_pci(TXQ)?;

        // Driver ready
        let t = self.transport.as_ref().ok_or("No PCI transport")?;
        t.write_status(
            dev_status::ACKNOWLEDGE
                | dev_status::DRIVER
                | dev_status::FEATURES_OK
                | dev_status::DRIVER_OK,
        );

        Ok(())
    }

    /// MMIO transport VirtIO handshake
    /// — EchoFrame: MMIO path — same ritual, different wire
    fn init_mmio_handshake(&mut self) -> Result<(), &'static str> {
        self.write_reg(VIRTIO_MMIO_STATUS, 0);
        self.write_reg(VIRTIO_MMIO_STATUS, dev_status::ACKNOWLEDGE as u32);
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            (dev_status::ACKNOWLEDGE | dev_status::DRIVER) as u32,
        );

        self.write_reg(VIRTIO_MMIO_DEVICE_FEATURES_SEL, 0);
        let _features = self.read_reg(VIRTIO_MMIO_DEVICE_FEATURES);

        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES_SEL, 0);
        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES, 0);

        self.write_reg(
            VIRTIO_MMIO_STATUS,
            (dev_status::ACKNOWLEDGE | dev_status::DRIVER | dev_status::FEATURES_OK) as u32,
        );

        let status = self.read_reg(VIRTIO_MMIO_STATUS);
        if status & dev_status::FEATURES_OK as u32 == 0 {
            self.write_reg(VIRTIO_MMIO_STATUS, dev_status::FAILED as u32);
            return Err("Features not accepted");
        }

        // Read config
        let config = (self.mmio_base + VIRTIO_MMIO_CONFIG) as *const VirtioSndConfig;
        self.num_streams = unsafe { read_volatile(&(*config).streams) };

        self.init_queue(CONTROLQ)?;
        self.init_queue(TXQ)?;

        self.write_reg(
            VIRTIO_MMIO_STATUS,
            (dev_status::ACKNOWLEDGE
                | dev_status::DRIVER
                | dev_status::FEATURES_OK
                | dev_status::DRIVER_OK) as u32,
        );

        Ok(())
    }

    /// Initialize a virtqueue via PCI transport using shared Virtqueue
    /// — EchoFrame: let virtio-core handle the descriptor table swamp
    fn init_queue_pci(&mut self, queue_idx: u32) -> Result<(), &'static str> {
        let t = self.transport.as_ref().ok_or("No PCI transport")?;

        t.select_queue(queue_idx as u16);

        let max_size = t.queue_max_size();
        if max_size == 0 {
            return Err("Queue not available");
        }

        let size = max_size.min(64);
        t.set_queue_size(size);

        // — EchoFrame: Virtqueue::new handles all the alloc/alignment nightmares for us
        let queue = unsafe { Virtqueue::new(size) }
            .ok_or("Failed to allocate virtqueue")?;

        let (desc_phys, avail_phys, used_phys) = queue.physical_addresses();

        t.set_queue_desc(desc_phys);
        t.set_queue_avail(avail_phys);
        t.set_queue_used(used_phys);
        t.enable_queue();

        match queue_idx {
            CONTROLQ => { self.ctrl_queue = Some(queue); }
            TXQ => { self.tx_queue = Some(queue); }
            _ => {}
        }

        Ok(())
    }

    /// Initialize a virtqueue (MMIO path) using shared Virtqueue
    /// — EchoFrame: MMIO queues get the same virtio-core treatment
    fn init_queue(&mut self, queue_idx: u32) -> Result<(), &'static str> {
        self.write_reg(VIRTIO_MMIO_QUEUE_SEL, queue_idx);

        let max_size = self.read_reg(VIRTIO_MMIO_QUEUE_NUM_MAX) as u16;
        if max_size == 0 {
            return Err("Queue not available");
        }

        let size = max_size.min(64);
        self.write_reg(VIRTIO_MMIO_QUEUE_NUM, size as u32);

        // — EchoFrame: shared Virtqueue handles the allocation graveyard
        let queue = unsafe { Virtqueue::new(size) }
            .ok_or("Failed to allocate virtqueue")?;

        let (desc_phys, avail_phys, used_phys) = queue.physical_addresses();

        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_LOW, desc_phys as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_HIGH, (desc_phys >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_LOW, avail_phys as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_HIGH, (avail_phys >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_LOW, used_phys as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_HIGH, (used_phys >> 32) as u32);

        self.write_reg(VIRTIO_MMIO_QUEUE_READY, 1);

        match queue_idx {
            CONTROLQ => { self.ctrl_queue = Some(queue); }
            TXQ => { self.tx_queue = Some(queue); }
            _ => {}
        }

        Ok(())
    }

    /// Query stream information
    fn query_streams(&mut self) -> Result<(), &'static str> {
        if self.num_streams == 0 {
            return Ok(());
        }

        // For simplicity, assume standard stereo output support
        self.streams.push(StreamInfo {
            direction: 0,
            channels_min: 1,
            channels_max: 2,
            formats: (1 << pcm_fmt::S16) | (1 << pcm_fmt::S32),
            rates: pcm_rate::RATE_44100 as u64 | pcm_rate::RATE_48000 as u64,
        });

        Ok(())
    }

    /// Convert SampleFormat to virtio format
    fn sample_format_to_virtio(fmt: SampleFormat) -> u8 {
        match fmt {
            SampleFormat::S8 => pcm_fmt::S8,
            SampleFormat::U8 => pcm_fmt::U8,
            SampleFormat::S16LE | SampleFormat::S16BE => pcm_fmt::S16,
            SampleFormat::S24LE | SampleFormat::S24BE => pcm_fmt::S24_3,
            SampleFormat::S32LE | SampleFormat::S32BE => pcm_fmt::S32,
            SampleFormat::F32LE | SampleFormat::F32BE => pcm_fmt::FLOAT,
            SampleFormat::F64LE | SampleFormat::F64BE => pcm_fmt::FLOAT64,
            SampleFormat::MuLaw => pcm_fmt::MU_LAW,
            SampleFormat::ALaw => pcm_fmt::A_LAW,
            _ => pcm_fmt::S16,
        }
    }

    /// Convert sample rate to virtio rate index
    fn rate_to_virtio(rate: u32) -> u8 {
        match rate {
            5512 => 0,
            8000 => 1,
            11025 => 2,
            16000 => 3,
            22050 => 4,
            32000 => 5,
            44100 => 6,
            48000 => 7,
            64000 => 8,
            88200 => 9,
            96000 => 10,
            176400 => 11,
            192000 => 12,
            _ => 7, // Default to 48000
        }
    }

    /// Send control command (dual PCI/MMIO path)
    /// — EchoFrame: push commands through the control queue — now via shared Virtqueue API
    fn send_ctrl<C, R>(&mut self, cmd: &C, resp: &mut R) -> Result<(), &'static str> {
        let cmd_size = core::mem::size_of::<C>();
        let resp_size = core::mem::size_of::<R>();

        let queue = self.ctrl_queue.as_mut().ok_or("Control queue not initialized")?;

        // — EchoFrame: allocate two descriptors from the shared free list
        let desc0 = queue.alloc_desc().ok_or("No free descriptors")?;
        let desc1 = queue.alloc_desc().ok_or("No free descriptors")?;

        // — EchoFrame: virt_to_phys handles the address translation — no more manual PHYS_MAP_BASE arithmetic
        let cmd_phys = virt_to_phys(cmd as *const C as u64);
        let resp_phys = virt_to_phys(resp as *mut R as u64);

        unsafe {
            // Command buffer: device-readable, chained to response descriptor
            queue.write_desc(desc0, cmd_phys, cmd_size as u32, desc_flags::NEXT, desc1);
            // Response buffer: device-writable, end of chain
            queue.write_desc(desc1, resp_phys, resp_size as u32, desc_flags::WRITE, 0);
        }

        // Submit the chain head to the available ring
        queue.add_available(desc0);

        // Notify — PCI transport or MMIO register
        if let Some(ref t) = self.transport {
            t.notify_queue(CONTROLQ as u16);
        } else {
            self.write_reg(VIRTIO_MMIO_QUEUE_NOTIFY, CONTROLQ);
        }

        // — EchoFrame: spin until the device processes our command — has_completed() watches the used ring
        let queue = self.ctrl_queue.as_mut().ok_or("Control queue not initialized")?;
        loop {
            if queue.has_completed() {
                // Drain the completed entry and free the descriptor chain
                if let Some((head, _len)) = queue.pop_used() {
                    queue.free_chain(head);
                }
                break;
            }
            core::hint::spin_loop();
        }

        Ok(())
    }

    fn read_reg(&self, offset: usize) -> u32 {
        unsafe { read_volatile((self.mmio_base + offset) as *const u32) }
    }

    fn write_reg(&self, offset: usize, value: u32) {
        unsafe { write_volatile((self.mmio_base + offset) as *mut u32, value) }
    }
}

impl AudioDevice for VirtioSnd {
    fn info(&self) -> AudioDeviceInfo {
        AudioDeviceInfo {
            name: String::from("VirtIO Sound"),
            description: String::from("VirtIO sound device"),
            sample_rates: alloc::vec![44100, 48000],
            channels: alloc::vec![1, 2],
            formats: alloc::vec![SampleFormat::S16LE, SampleFormat::S32LE],
            max_buffer_frames: 8192,
            min_buffer_frames: 256,
        }
    }

    fn supports_playback(&self) -> bool {
        self.streams.iter().any(|s| s.direction == 0)
    }

    fn supports_capture(&self) -> bool {
        self.streams.iter().any(|s| s.direction == 1)
    }

    fn state(&self) -> StreamState {
        self.state
    }

    fn configure(&self, config: StreamConfig) -> AudioResult<()> {
        if self.state != StreamState::Setup && self.state != StreamState::Stopped {
            return Err(AudioError::InvalidState);
        }

        // Validate config
        if config.channels < 1 || config.channels > 8 {
            return Err(AudioError::InvalidConfig);
        }

        // Store config (need interior mutability in practice)
        Ok(())
    }

    fn prepare(&self) -> AudioResult<()> {
        if self.state != StreamState::Setup {
            return Err(AudioError::InvalidState);
        }
        Ok(())
    }

    fn start(&self) -> AudioResult<()> {
        if self.state != StreamState::Prepared && self.state != StreamState::Stopped {
            return Err(AudioError::InvalidState);
        }
        Ok(())
    }

    fn stop(&self) -> AudioResult<()> {
        if self.state != StreamState::Running {
            return Err(AudioError::InvalidState);
        }
        Ok(())
    }

    fn release(&self) -> AudioResult<()> {
        Ok(())
    }

    fn write(&self, data: &[u8]) -> AudioResult<usize> {
        if let Some(ref buffer) = self.buffer {
            Ok(buffer.write(data))
        } else {
            Err(AudioError::IoError)
        }
    }

    fn read(&self, _data: &mut [u8]) -> AudioResult<usize> {
        Err(AudioError::NotSupported)
    }

    fn write_available(&self) -> usize {
        self.buffer
            .as_ref()
            .map(|b| b.write_available())
            .unwrap_or(0)
    }

    fn read_available(&self) -> usize {
        0
    }

    fn get_volume(&self) -> u8 {
        self.volume.load(Ordering::SeqCst)
    }

    fn set_volume(&self, volume: u8) -> AudioResult<()> {
        self.volume.store(volume.min(100), Ordering::SeqCst);
        Ok(())
    }

    fn get_position(&self) -> u64 {
        self.position.load(Ordering::SeqCst)
    }
}

unsafe impl Send for VirtioSnd {}
unsafe impl Sync for VirtioSnd {}

/// Global VirtIO sound device (shared with audio subsystem via Arc)
/// — EchoFrame: one device, many listeners
static VIRTIO_SND: Mutex<Option<Arc<VirtioSnd>>> = Mutex::new(None);

/// Initialize VirtIO sound from PCI device
/// — EchoFrame: from PCI probe to registered audio device in one shot
pub fn init_from_pci(pci_dev: &PciDevice) -> Result<(), &'static str> {
    let mut snd = VirtioSnd::from_pci(pci_dev).ok_or("VirtIO sound PCI probe failed")?;
    snd.init()?;

    let arc_snd = Arc::new(snd);

    // Register with audio subsystem
    audio::init();
    audio::register_device(arc_snd.clone());

    *VIRTIO_SND.lock() = Some(arc_snd);
    Ok(())
}

/// Initialize VirtIO sound from MMIO address
pub fn init(mmio_base: usize) -> Result<(), &'static str> {
    let mut snd = VirtioSnd::probe(mmio_base).ok_or("VirtIO sound not found")?;
    snd.init()?;

    let arc_snd = Arc::new(snd);

    audio::init();
    audio::register_device(arc_snd.clone());

    *VIRTIO_SND.lock() = Some(arc_snd);
    Ok(())
}

/// Get device count
pub fn device_count() -> usize {
    if VIRTIO_SND.lock().is_some() { 1 } else { 0 }
}

// ============================================================================
// PciDriver Implementation for Dynamic Driver Loading
// ============================================================================
// — SoftGlyph: audio driver, auto-probed

use driver_core::{PciDriver, PciDeviceId, DriverError, DriverBindingData};

/// Device ID table for VirtIO sound devices
static VIRTIO_SND_IDS: &[PciDeviceId] = &[
    PciDeviceId::new(pci::vendor::VIRTIO, pci::virtio_modern::SOUND),   // Modern only
];

/// VirtIO sound driver for driver-core system
struct VirtioSndDriver;

impl PciDriver for VirtioSndDriver {
    fn name(&self) -> &'static str {
        "virtio-snd"
    }

    fn id_table(&self) -> &'static [PciDeviceId] {
        VIRTIO_SND_IDS
    }

    fn probe(&self, dev: &pci::PciDevice, _id: &PciDeviceId) -> Result<DriverBindingData, DriverError> {
        // SAFETY: PCI device is valid and matches our ID table
        let _ = unsafe { init_from_pci(dev) }
            .map_err(|_| DriverError::InitFailed)?;

        Ok(DriverBindingData::new(0))
    }

    unsafe fn remove(&self, _dev: &pci::PciDevice, _binding_data: DriverBindingData) {
        // TODO: Implement proper audio device removal
    }
}

/// Static driver instance for registration
static VIRTIO_SND_DRIVER: VirtioSndDriver = VirtioSndDriver;

// Register driver via compile-time linker section
driver_core::register_pci_driver!(VIRTIO_SND_DRIVER);
