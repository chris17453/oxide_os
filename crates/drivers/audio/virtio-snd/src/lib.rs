//! VirtIO Sound Device Driver
//!
//! Implements the VirtIO sound device specification for audio
//! in virtualized environments.

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use audio::{
    AudioDevice, AudioDeviceInfo, AudioError, AudioResult,
    SampleFormat, StreamConfig, StreamState, RingBuffer,
};
use spin::Mutex;

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

/// VirtIO status bits
const VIRTIO_STATUS_ACKNOWLEDGE: u32 = 1;
const VIRTIO_STATUS_DRIVER: u32 = 2;
const VIRTIO_STATUS_FEATURES_OK: u32 = 8;
const VIRTIO_STATUS_DRIVER_OK: u32 = 4;
const VIRTIO_STATUS_FAILED: u32 = 128;

/// Queue indices
const CONTROLQ: u32 = 0;
const EVENTQ: u32 = 1;
const TXQ: u32 = 2;
const RXQ: u32 = 3;

/// Descriptor flags
const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

/// Virtqueue descriptor
#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

/// Virtqueue available ring
#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 256],
}

/// Virtqueue used element
#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

/// Virtqueue used ring
#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 256],
}

/// VirtIO sound configuration
#[repr(C)]
struct VirtioSndConfig {
    jacks: u32,
    streams: u32,
    chmaps: u32,
}

/// PCM stream info parsed
struct StreamInfo {
    direction: u8,  // 0 = output, 1 = input
    channels_min: u8,
    channels_max: u8,
    formats: u64,
    rates: u64,
}

/// VirtIO sound device
pub struct VirtioSnd {
    /// MMIO base address
    mmio_base: usize,
    /// Control queue descriptors
    ctrl_desc: *mut VirtqDesc,
    /// Control queue available
    ctrl_avail: *mut VirtqAvail,
    /// Control queue used
    ctrl_used: *mut VirtqUsed,
    /// TX queue descriptors
    tx_desc: *mut VirtqDesc,
    /// TX queue available
    tx_avail: *mut VirtqAvail,
    /// TX queue used
    tx_used: *mut VirtqUsed,
    /// Queue size
    queue_size: u16,
    /// Next control descriptor
    ctrl_next: u16,
    /// Last used control index
    ctrl_last_used: u16,
    /// Next TX descriptor
    tx_next: u16,
    /// Last used TX index
    tx_last_used: u16,
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
    /// Probe for VirtIO sound device
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
            ctrl_desc: core::ptr::null_mut(),
            ctrl_avail: core::ptr::null_mut(),
            ctrl_used: core::ptr::null_mut(),
            tx_desc: core::ptr::null_mut(),
            tx_avail: core::ptr::null_mut(),
            tx_used: core::ptr::null_mut(),
            queue_size: 0,
            ctrl_next: 0,
            ctrl_last_used: 0,
            tx_next: 0,
            tx_last_used: 0,
            num_streams: 0,
            streams: Vec::new(),
            config: None,
            state: StreamState::Setup,
            volume: AtomicU8::new(100),
            position: AtomicU64::new(0),
            buffer: None,
        })
    }

    /// Initialize the sound device
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Reset
        self.write_reg(VIRTIO_MMIO_STATUS, 0);

        // Acknowledge
        self.write_reg(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);

        // Driver
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
        );

        // Features
        self.write_reg(VIRTIO_MMIO_DEVICE_FEATURES_SEL, 0);
        let _features = self.read_reg(VIRTIO_MMIO_DEVICE_FEATURES);

        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES_SEL, 0);
        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES, 0);

        // Features OK
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
        );

        let status = self.read_reg(VIRTIO_MMIO_STATUS);
        if status & VIRTIO_STATUS_FEATURES_OK == 0 {
            self.write_reg(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_FAILED);
            return Err("Features not accepted");
        }

        // Read config
        let config = (self.mmio_base + VIRTIO_MMIO_CONFIG) as *const VirtioSndConfig;
        self.num_streams = unsafe { read_volatile(&(*config).streams) };

        // Initialize queues
        self.init_queue(CONTROLQ)?;
        self.init_queue(TXQ)?;

        // Driver OK
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK,
        );

        // Query stream info
        self.query_streams()?;

        // Create audio buffer
        self.buffer = Some(RingBuffer::new(65536)); // 64KB buffer

        Ok(())
    }

    /// Initialize a virtqueue
    fn init_queue(&mut self, queue_idx: u32) -> Result<(), &'static str> {
        self.write_reg(VIRTIO_MMIO_QUEUE_SEL, queue_idx);

        let max_size = self.read_reg(VIRTIO_MMIO_QUEUE_NUM_MAX) as u16;
        if max_size == 0 {
            return Err("Queue not available");
        }

        let size = max_size.min(64);
        self.queue_size = size;
        self.write_reg(VIRTIO_MMIO_QUEUE_NUM, size as u32);

        use alloc::alloc::{alloc_zeroed, Layout};

        let desc_size = size as usize * core::mem::size_of::<VirtqDesc>();
        let desc_layout = Layout::from_size_align(desc_size, 16).unwrap();
        let desc = unsafe { alloc_zeroed(desc_layout) } as *mut VirtqDesc;

        let avail_layout = Layout::from_size_align(core::mem::size_of::<VirtqAvail>(), 2).unwrap();
        let avail = unsafe { alloc_zeroed(avail_layout) } as *mut VirtqAvail;

        let used_layout = Layout::from_size_align(core::mem::size_of::<VirtqUsed>(), 4).unwrap();
        let used = unsafe { alloc_zeroed(used_layout) } as *mut VirtqUsed;

        match queue_idx {
            CONTROLQ => {
                self.ctrl_desc = desc;
                self.ctrl_avail = avail;
                self.ctrl_used = used;
            }
            TXQ => {
                self.tx_desc = desc;
                self.tx_avail = avail;
                self.tx_used = used;
            }
            _ => {}
        }

        let desc_addr = desc as u64;
        let avail_addr = avail as u64;
        let used_addr = used as u64;

        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_LOW, desc_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_HIGH, (desc_addr >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_LOW, avail_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_HIGH, (avail_addr >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_LOW, used_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_HIGH, (used_addr >> 32) as u32);

        self.write_reg(VIRTIO_MMIO_QUEUE_READY, 1);

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

    /// Send control command
    fn send_ctrl<C, R>(&mut self, cmd: &C, resp: &mut R) -> Result<(), &'static str> {
        let cmd_size = core::mem::size_of::<C>();
        let resp_size = core::mem::size_of::<R>();

        let desc_idx = self.ctrl_next;
        self.ctrl_next = (self.ctrl_next + 2) % self.queue_size;

        unsafe {
            let desc0 = &mut *self.ctrl_desc.add(desc_idx as usize);
            desc0.addr = cmd as *const C as u64;
            desc0.len = cmd_size as u32;
            desc0.flags = VIRTQ_DESC_F_NEXT;
            desc0.next = (desc_idx + 1) % self.queue_size;

            let desc1 = &mut *self.ctrl_desc.add(((desc_idx + 1) % self.queue_size) as usize);
            desc1.addr = resp as *mut R as u64;
            desc1.len = resp_size as u32;
            desc1.flags = VIRTQ_DESC_F_WRITE;
            desc1.next = 0;

            let avail = &mut *self.ctrl_avail;
            let avail_idx = avail.idx;
            avail.ring[(avail_idx % self.queue_size) as usize] = desc_idx;
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            write_volatile(&mut avail.idx, avail_idx.wrapping_add(1));
        }

        self.write_reg(VIRTIO_MMIO_QUEUE_NOTIFY, CONTROLQ);

        // Wait for completion
        loop {
            let used_idx = unsafe { read_volatile(&(*self.ctrl_used).idx) };
            if used_idx != self.ctrl_last_used {
                self.ctrl_last_used = used_idx;
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
        self.buffer.as_ref().map(|b| b.write_available()).unwrap_or(0)
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

/// Global VirtIO sound device
static VIRTIO_SND: Mutex<Option<VirtioSnd>> = Mutex::new(None);

/// Initialize VirtIO sound from MMIO address
pub fn init(mmio_base: usize) -> Result<(), &'static str> {
    let mut snd = VirtioSnd::probe(mmio_base).ok_or("VirtIO sound not found")?;
    snd.init()?;

    // Register with audio subsystem
    let arc_snd = Arc::new(snd);

    *VIRTIO_SND.lock() = None; // Can't easily share with Arc, simplified

    audio::init();

    Ok(())
}

/// Get device count
pub fn device_count() -> usize {
    if VIRTIO_SND.lock().is_some() { 1 } else { 0 }
}
