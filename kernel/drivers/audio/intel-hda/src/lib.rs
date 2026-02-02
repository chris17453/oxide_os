//! Intel High Definition Audio Controller Driver
//!
//! Drives the ICH6 HDA controller (PCI ID 8086:2668) present in QEMU's
//! `intel-hda` device with `hda-duplex` codec. Implements full CORB/RIRB
//! command transport, codec widget discovery, BDL-based DMA streaming,
//! and the OXIDE `AudioDevice` trait for integration with /dev/dsp.
//!
//! — EchoFrame: sound doesn't exist until hardware moves air molecules;
//!   this driver is the first link in that chain

#![no_std]

extern crate alloc;

pub mod codec;
pub mod regs;
pub mod stream;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use audio::{AudioDevice, AudioDeviceInfo, AudioError, AudioResult, RingBuffer, SampleFormat,
            StreamConfig, StreamState};
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use pci::PciDevice;
use spin::Mutex;
use stream::BdlEntry;

/// Physical memory direct-map base — same constant every OXIDE driver uses
/// — SableWire: the kernel's window into raw physical address space
const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Convert a kernel virtual address to a physical address
#[inline]
fn virt_to_phys(virt: u64) -> u64 {
    virt - PHYS_MAP_BASE
}

/// Convert a physical address to a kernel virtual address
#[inline]
fn phys_to_virt(phys: u64) -> u64 {
    phys + PHYS_MAP_BASE
}

// ============================================================================
// HDA Controller — raw hardware interface
// — GraveShift: the bare-metal layer between MMIO registers and audio data
// ============================================================================

/// Discovered output path through the codec widget tree
#[allow(dead_code)]
struct OutputPath {
    /// DAC (Audio Output) node ID
    dac_nid: u8,
    /// Pin Complex node ID
    pin_nid: u8,
    /// Codec address (0-14)
    codec_addr: u8,
}

/// Core HDA controller state
struct HdaController {
    /// MMIO base (kernel virtual address from BAR0)
    bar0: *mut u8,
    /// CORB buffer (256 entries × 4 bytes = 1KB)
    corb: *mut u32,
    /// RIRB buffer (256 entries × 8 bytes = 2KB)
    rirb: *mut u64,
    /// CORB physical address (for DMA)
    corb_phys: u64,
    /// RIRB physical address (for DMA)
    rirb_phys: u64,
    /// Current CORB write pointer
    corb_wp: u16,
    /// Number of output streams (from GCAP)
    num_oss: u8,
    /// Number of input streams (from GCAP)
    num_iss: u8,
    /// BDL entries (must outlive DMA)
    bdl: *mut BdlEntry,
    /// BDL physical address
    bdl_phys: u64,
    /// DMA buffers (double-buffered)
    dma_bufs: [*mut u8; stream::NUM_DMA_BUFS],
    /// DMA buffer physical addresses
    dma_phys: [u64; stream::NUM_DMA_BUFS],
    /// Output stream descriptor index
    stream_index: u8,
    /// Stream tag (1-15, encoded in SD_CTL and codec)
    stream_tag: u8,
    /// Current write position within the DMA double-buffer
    write_pos: usize,
    /// Discovered output path
    output_path: Option<OutputPath>,
    /// Whether stream hardware is configured
    stream_configured: bool,
}

// Safety: Controller is accessed only through Mutex<HdaController>
unsafe impl Send for HdaController {}
unsafe impl Sync for HdaController {}

impl HdaController {
    /// Reset the controller (HDA Spec §3.3.7)
    ///
    /// Clears GCTL.CRST, waits for the controller to enter reset,
    /// then sets CRST to bring it back, and waits for codec detection.
    /// — GraveShift: you can't build on broken foundations
    unsafe fn reset(&mut self) -> Result<(), &'static str> {
        let bar0 = self.bar0;

        // Enter reset: clear CRST
        // Safety: bar0 is a valid MMIO pointer from PCI BAR0
        let gctl = unsafe { regs::read32(bar0, regs::GCTL) };
        unsafe { regs::write32(bar0, regs::GCTL, gctl & !regs::GCTL_CRST) };

        // Wait for CRST to read back as 0
        for _ in 0..10_000 {
            if unsafe { regs::read32(bar0, regs::GCTL) } & regs::GCTL_CRST == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        if unsafe { regs::read32(bar0, regs::GCTL) } & regs::GCTL_CRST != 0 {
            return Err("HDA: controller did not enter reset");
        }

        // Small delay for hardware settle
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }

        // Exit reset: set CRST
        let gctl = unsafe { regs::read32(bar0, regs::GCTL) };
        unsafe { regs::write32(bar0, regs::GCTL, gctl | regs::GCTL_CRST) };

        // Wait for CRST to read back as 1
        for _ in 0..10_000 {
            if unsafe { regs::read32(bar0, regs::GCTL) } & regs::GCTL_CRST != 0 {
                break;
            }
            core::hint::spin_loop();
        }
        if unsafe { regs::read32(bar0, regs::GCTL) } & regs::GCTL_CRST == 0 {
            return Err("HDA: controller did not exit reset");
        }

        // Wait for codec detection (STATESTS should show at least one codec)
        for _ in 0..100_000 {
            let statests = unsafe { regs::read16(bar0, regs::STATESTS) };
            if statests != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Read capabilities
        let gcap = unsafe { regs::read16(bar0, regs::GCAP) };
        self.num_oss = ((gcap >> 12) & 0xF) as u8;
        self.num_iss = ((gcap >> 8) & 0xF) as u8;

        Ok(())
    }

    /// Initialize CORB (Command Output Ring Buffer) — HDA Spec §3.3.2
    ///
    /// Allocates a 1KB DMA buffer for 256 command entries, programs the
    /// controller with its physical address, and starts the CORB DMA engine.
    /// — EchoFrame: the outbound command pipeline to the codec
    unsafe fn init_corb(&mut self) -> Result<(), &'static str> {
        let bar0 = self.bar0;

        // Stop CORB if running
        // Safety: bar0 points to valid HDA MMIO register space
        unsafe { regs::write8(bar0, regs::CORBCTL, 0) };
        for _ in 0..1000 {
            if unsafe { regs::read8(bar0, regs::CORBCTL) } & regs::CORBCTL_RUN == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Allocate 1KB aligned buffer for 256 entries (4 bytes each)
        let layout = alloc::alloc::Layout::from_size_align(1024, 128)
            .map_err(|_| "HDA: CORB layout error")?;
        // Safety: layout is valid (non-zero size, power-of-two alignment)
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err("HDA: CORB allocation failed");
        }
        self.corb = ptr as *mut u32;
        self.corb_phys = virt_to_phys(ptr as u64);

        // Program CORB base address
        unsafe {
            regs::write32(bar0, regs::CORBLBASE, self.corb_phys as u32);
            regs::write32(bar0, regs::CORBUBASE, (self.corb_phys >> 32) as u32);
        }

        // Set size to 256 entries (bits [1:0] = 0b10)
        unsafe { regs::write8(bar0, regs::CORBSIZE, 0x02) };

        // Reset read pointer: write 1 to bit 15, then wait for it to read back 1
        unsafe { regs::write16(bar0, regs::CORBRP, 1 << 15) };
        for _ in 0..1000 {
            if unsafe { regs::read16(bar0, regs::CORBRP) } & (1 << 15) != 0 {
                break;
            }
            core::hint::spin_loop();
        }
        // Clear reset: write 0 to bit 15
        unsafe { regs::write16(bar0, regs::CORBRP, 0) };
        for _ in 0..1000 {
            if unsafe { regs::read16(bar0, regs::CORBRP) } & (1 << 15) == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Set write pointer to 0
        self.corb_wp = 0;
        unsafe { regs::write16(bar0, regs::CORBWP, 0) };

        // Start CORB DMA engine
        unsafe { regs::write8(bar0, regs::CORBCTL, regs::CORBCTL_RUN) };
        for _ in 0..1000 {
            if unsafe { regs::read8(bar0, regs::CORBCTL) } & regs::CORBCTL_RUN != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        Ok(())
    }

    /// Initialize RIRB (Response Input Ring Buffer) — HDA Spec §3.3.3
    ///
    /// Allocates a 2KB DMA buffer for 256 response entries, programs the
    /// controller, and starts the RIRB DMA engine.
    /// — EchoFrame: where codec responses materialize from silicon
    unsafe fn init_rirb(&mut self) -> Result<(), &'static str> {
        let bar0 = self.bar0;

        // Stop RIRB if running
        // Safety: bar0 points to valid HDA MMIO register space
        unsafe { regs::write8(bar0, regs::RIRBCTL, 0) };
        for _ in 0..1000 {
            if unsafe { regs::read8(bar0, regs::RIRBCTL) } & regs::RIRBCTL_DMAEN == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Allocate 2KB aligned buffer for 256 entries (8 bytes each: response + response_ex)
        let layout = alloc::alloc::Layout::from_size_align(2048, 128)
            .map_err(|_| "HDA: RIRB layout error")?;
        // Safety: layout is valid
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err("HDA: RIRB allocation failed");
        }
        self.rirb = ptr as *mut u64;
        self.rirb_phys = virt_to_phys(ptr as u64);

        // Program RIRB base address
        unsafe {
            regs::write32(bar0, regs::RIRBLBASE, self.rirb_phys as u32);
            regs::write32(bar0, regs::RIRBUBASE, (self.rirb_phys >> 32) as u32);
        }

        // Set size to 256 entries
        unsafe { regs::write8(bar0, regs::RIRBSIZE, 0x02) };

        // Reset write pointer
        unsafe { regs::write16(bar0, regs::RIRBWP, 1 << 15) };

        // Set response interrupt count (generate interrupt every response)
        unsafe { regs::write16(bar0, regs::RINTCNT, 1) };

        // Start RIRB DMA engine
        unsafe { regs::write8(bar0, regs::RIRBCTL, regs::RIRBCTL_DMAEN) };
        for _ in 0..1000 {
            if unsafe { regs::read8(bar0, regs::RIRBCTL) } & regs::RIRBCTL_DMAEN != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        Ok(())
    }

    /// Send a codec command via CORB and poll RIRB for the response
    ///
    /// Command format: (codec_addr << 28) | (nid << 20) | verb
    /// — EchoFrame: call-and-response, the oldest protocol in music
    unsafe fn send_command(&mut self, codec_addr: u8, nid: u8, verb: u32) -> Result<u32, &'static str> {
        let bar0 = self.bar0;

        // Build the full command word
        let cmd = ((codec_addr as u32) << 28) | ((nid as u32) << 20) | (verb & 0xFFFFF);

        // Advance write pointer (wrap at 255)
        self.corb_wp = (self.corb_wp + 1) % 256;

        // Write command to CORB buffer
        // Safety: corb is a valid allocated buffer, corb_wp is within bounds
        unsafe { core::ptr::write_volatile(self.corb.add(self.corb_wp as usize), cmd) };

        // Update CORB write pointer register to trigger DMA
        unsafe { regs::write16(bar0, regs::CORBWP, self.corb_wp) };

        // Poll RIRB for response — hardware writes RIRB WP when response arrives
        let rirb_wp_before = unsafe { regs::read16(bar0, regs::RIRBWP) } & 0xFF;

        for _ in 0..100_000 {
            let rirb_wp_now = unsafe { regs::read16(bar0, regs::RIRBWP) } & 0xFF;
            if rirb_wp_now != rirb_wp_before {
                // Response available — read from RIRB at the new write pointer
                // Safety: rirb is valid, rirb_wp_now is within 256-entry buffer
                let rirb_entry = unsafe {
                    core::ptr::read_volatile(self.rirb.add(rirb_wp_now as usize))
                };
                let response = rirb_entry as u32;

                // Clear RIRB interrupt status
                unsafe { regs::write8(bar0, regs::RIRBSTS, 0x05) };

                return Ok(response);
            }
            core::hint::spin_loop();
        }

        Err("HDA: RIRB timeout — no codec response")
    }

    /// Discover codecs and find an output path (DAC → Pin)
    ///
    /// Walks: root node → function group → widgets, looking for an Audio Output
    /// converter connected to a Line Out or Headphone Pin Complex.
    /// — EchoFrame: mapping the audio topology like sonar in a dark room
    unsafe fn discover_and_configure(&mut self) -> Result<(), &'static str> {
        let bar0 = self.bar0;
        let statests = unsafe { regs::read16(bar0, regs::STATESTS) };

        // Check each possible codec address (0-14)
        for codec_addr in 0..15u8 {
            if statests & (1 << codec_addr) == 0 {
                continue;
            }

            // Get root node sub-node count to find function groups
            // Safety: send_command requires valid CORB/RIRB which are initialized
            let sub_nodes = unsafe {
                self.send_command(
                    codec_addr, 0,
                    codec::GET_PARAM | codec::PARAM_SUB_NODE_COUNT,
                )?
            };

            let start_nid = ((sub_nodes >> 16) & 0xFF) as u8;
            let num_nodes = (sub_nodes & 0xFF) as u8;

            // Walk function groups looking for Audio Function Group
            for fg_offset in 0..num_nodes {
                let fg_nid = start_nid + fg_offset;

                let fg_type = unsafe {
                    self.send_command(
                        codec_addr, fg_nid,
                        codec::GET_PARAM | codec::PARAM_FN_GROUP_TYPE,
                    )?
                };

                if (fg_type & 0xFF) as u8 != codec::FN_GROUP_AUDIO {
                    continue;
                }

                // Power on the function group
                unsafe {
                    self.send_command(codec_addr, fg_nid, codec::SET_POWER_STATE | codec::POWER_D0)?;
                }

                // Get widgets in this function group
                let widget_nodes = unsafe {
                    self.send_command(
                        codec_addr, fg_nid,
                        codec::GET_PARAM | codec::PARAM_SUB_NODE_COUNT,
                    )?
                };
                let w_start = ((widget_nodes >> 16) & 0xFF) as u8;
                let w_count = (widget_nodes & 0xFF) as u8;

                // First pass: find a suitable output pin (Line Out or HP)
                let mut output_pin: Option<u8> = None;
                for w_offset in 0..w_count {
                    let nid = w_start + w_offset;
                    let caps = unsafe {
                        self.send_command(
                            codec_addr, nid,
                            codec::GET_PARAM | codec::PARAM_AUDIO_CAPS,
                        )?
                    };
                    let wtype = codec::widget_type(caps);

                    if wtype == codec::WIDGET_PIN_COMPLEX {
                        let config = unsafe {
                            self.send_command(
                                codec_addr, nid,
                                codec::GET_CONFIG_DEFAULT,
                            )?
                        };
                        let dev = codec::pin_default_device(config);
                        let conn = codec::pin_connectivity(config);

                        // Accept Line Out, Speaker, or Headphone that isn't disconnected
                        if conn != codec::PIN_CONN_NONE
                            && (dev == codec::PIN_DEV_LINE_OUT
                                || dev == codec::PIN_DEV_SPEAKER
                                || dev == codec::PIN_DEV_HP_OUT)
                        {
                            output_pin = Some(nid);
                            break;
                        }
                    }
                }

                let pin_nid = match output_pin {
                    Some(nid) => nid,
                    None => continue,
                };

                // Trace connection list from pin back to find a DAC
                let dac_nid = unsafe { self.find_dac(codec_addr, w_start, w_count, pin_nid)? };

                // Store the discovered path
                self.output_path = Some(OutputPath {
                    dac_nid,
                    pin_nid,
                    codec_addr,
                });

                // Configure the output path
                unsafe { self.configure_output(codec_addr, dac_nid, pin_nid)? };

                return Ok(());
            }
        }

        Err("HDA: no audio output path found")
    }

    /// Trace from a pin back through connection lists to find a DAC
    /// — EchoFrame: following the wires backward through the codec graph
    unsafe fn find_dac(
        &mut self,
        codec_addr: u8,
        w_start: u8,
        w_count: u8,
        pin_nid: u8,
    ) -> Result<u8, &'static str> {
        // Get pin's connection list
        let conn_len_raw = unsafe {
            self.send_command(
                codec_addr, pin_nid,
                codec::GET_PARAM | codec::PARAM_CONN_LIST_LEN,
            )?
        };
        let conn_len = (conn_len_raw & 0x7F) as u8;

        if conn_len == 0 {
            return Err("HDA: pin has no connections");
        }

        // Read connection list entries (up to 4 per verb response, short form)
        let conn_entries = unsafe {
            self.send_command(
                codec_addr, pin_nid,
                codec::GET_CONN_LIST,
            )?
        };

        // Check each connected node
        for i in 0..conn_len.min(4) {
            let connected_nid = ((conn_entries >> (i * 8)) & 0xFF) as u8;

            if connected_nid < w_start || connected_nid >= w_start + w_count {
                continue;
            }

            let caps = unsafe {
                self.send_command(
                    codec_addr, connected_nid,
                    codec::GET_PARAM | codec::PARAM_AUDIO_CAPS,
                )?
            };
            let wtype = codec::widget_type(caps);

            // Direct DAC connection — jackpot
            if wtype == codec::WIDGET_AUDIO_OUT {
                return Ok(connected_nid);
            }

            // Mixer or selector — recurse one level deeper
            if wtype == codec::WIDGET_AUDIO_MIXER || wtype == codec::WIDGET_AUDIO_SELECTOR {
                let sub_conn_len = unsafe {
                    self.send_command(
                        codec_addr, connected_nid,
                        codec::GET_PARAM | codec::PARAM_CONN_LIST_LEN,
                    )?
                };
                let sub_len = (sub_conn_len & 0x7F) as u8;
                if sub_len > 0 {
                    let sub_entries = unsafe {
                        self.send_command(
                            codec_addr, connected_nid,
                            codec::GET_CONN_LIST,
                        )?
                    };
                    for j in 0..sub_len.min(4) {
                        let sub_nid = ((sub_entries >> (j * 8)) & 0xFF) as u8;
                        if sub_nid < w_start || sub_nid >= w_start + w_count {
                            continue;
                        }
                        let sub_caps = unsafe {
                            self.send_command(
                                codec_addr, sub_nid,
                                codec::GET_PARAM | codec::PARAM_AUDIO_CAPS,
                            )?
                        };
                        if codec::widget_type(sub_caps) == codec::WIDGET_AUDIO_OUT {
                            return Ok(sub_nid);
                        }
                    }
                }
            }
        }

        Err("HDA: no DAC found in output path")
    }

    /// Configure the output path: power on, set pin mode, unmute amps, set format
    /// — EchoFrame: waking every link in the audio chain from silicon slumber
    unsafe fn configure_output(
        &mut self,
        codec_addr: u8,
        dac_nid: u8,
        pin_nid: u8,
    ) -> Result<(), &'static str> {
        // Safety: send_command requires valid CORB/RIRB, guaranteed by prior init

        // Power on DAC
        unsafe { self.send_command(codec_addr, dac_nid, codec::SET_POWER_STATE | codec::POWER_D0)? };

        // Power on Pin
        unsafe { self.send_command(codec_addr, pin_nid, codec::SET_POWER_STATE | codec::POWER_D0)? };

        // Enable pin as output
        unsafe {
            self.send_command(
                codec_addr, pin_nid,
                codec::SET_PIN_CTL | codec::PIN_CTL_OUT_EN as u32,
            )?;
        }

        // Set DAC converter stream tag (must match SD stream tag) and channel 0
        let stream_chan = ((self.stream_tag as u32) << 4) | 0;
        unsafe { self.send_command(codec_addr, dac_nid, codec::SET_STREAM_CHAN | stream_chan)? };

        // Set DAC converter format: 48kHz, 16-bit, stereo
        unsafe {
            self.send_command(
                codec_addr, dac_nid,
                codec::SET_CONV_FMT | regs::FMT_48KHZ_16BIT_STEREO as u32,
            )?;
        }

        // Unmute output amp on DAC (set left+right, output, gain=0x7F)
        let amp_cmd: u32 = 0xB000 | 0x7F;
        unsafe { self.send_command(codec_addr, dac_nid, codec::SET_AMP_GAIN | amp_cmd)? };

        // Unmute output amp on Pin
        unsafe { self.send_command(codec_addr, pin_nid, codec::SET_AMP_GAIN | amp_cmd)? };

        // Try EAPD enable (some codecs need it for speaker/HP)
        let _ = unsafe { self.send_command(codec_addr, pin_nid, codec::SET_EAPD | 0x02) };

        Ok(())
    }

    /// Allocate DMA buffers and BDL, then configure the output stream descriptor
    /// — EchoFrame: building the DMA pipeline from memory to controller to DAC
    unsafe fn setup_dma_and_stream(&mut self) -> Result<(), &'static str> {
        // Allocate BDL (needs 128-byte alignment, 2 entries × 16 bytes = 32 bytes)
        let bdl_layout = alloc::alloc::Layout::from_size_align(
            stream::NUM_DMA_BUFS * core::mem::size_of::<BdlEntry>(),
            128,
        ).map_err(|_| "HDA: BDL layout error")?;
        // Safety: layout is valid (non-zero size, power-of-two alignment)
        let bdl_ptr = unsafe { alloc::alloc::alloc_zeroed(bdl_layout) };
        if bdl_ptr.is_null() {
            return Err("HDA: BDL allocation failed");
        }
        self.bdl = bdl_ptr as *mut BdlEntry;
        self.bdl_phys = virt_to_phys(bdl_ptr as u64);

        // Allocate DMA buffers (4KB each, page-aligned for clean DMA)
        for i in 0..stream::NUM_DMA_BUFS {
            let buf_layout = alloc::alloc::Layout::from_size_align(stream::DMA_BUF_SIZE, 4096)
                .map_err(|_| "HDA: DMA buffer layout error")?;
            // Safety: layout is valid
            let buf_ptr = unsafe { alloc::alloc::alloc_zeroed(buf_layout) };
            if buf_ptr.is_null() {
                return Err("HDA: DMA buffer allocation failed");
            }
            self.dma_bufs[i] = buf_ptr;
            self.dma_phys[i] = virt_to_phys(buf_ptr as u64);

            // Fill BDL entry
            // Safety: bdl points to allocated BDL buffer, i < NUM_DMA_BUFS
            let entry = unsafe { &mut *self.bdl.add(i) };
            entry.address = self.dma_phys[i];
            entry.length = stream::DMA_BUF_SIZE as u32;
            entry.ioc = 1;
        }

        // The first output stream descriptor index = num_iss
        // (input streams occupy indices 0..num_iss-1)
        self.stream_index = self.num_iss;
        self.stream_tag = 1;

        // Configure the stream descriptor hardware
        // Safety: bar0 is valid, all buffer addresses are valid physical addresses
        unsafe {
            stream::setup_output_stream(
                self.bar0,
                self.stream_index,
                self.stream_tag,
                self.bdl_phys,
                regs::FMT_48KHZ_16BIT_STEREO,
            );
        }

        self.stream_configured = true;
        self.write_pos = 0;

        Ok(())
    }

    /// Write audio data into the DMA buffers
    ///
    /// Copies PCM data into the double-buffered DMA region. If the stream
    /// isn't running yet, starts it once we have data.
    /// — EchoFrame: feeding bytes into the DMA engine's hungry maw
    unsafe fn write_audio(&mut self, data: &[u8]) -> usize {
        if !self.stream_configured {
            return 0;
        }

        let total_dma_size = stream::NUM_DMA_BUFS * stream::DMA_BUF_SIZE;
        let mut written = 0usize;
        let mut remaining = data;

        while !remaining.is_empty() {
            let buf_idx = self.write_pos / stream::DMA_BUF_SIZE;
            let buf_offset = self.write_pos % stream::DMA_BUF_SIZE;
            let space = stream::DMA_BUF_SIZE - buf_offset;
            let chunk = remaining.len().min(space);

            if buf_idx >= stream::NUM_DMA_BUFS {
                self.write_pos = 0;
                continue;
            }

            // Safety: dma_bufs[buf_idx] is a valid allocated buffer, chunk fits within it
            unsafe {
                core::ptr::copy_nonoverlapping(
                    remaining.as_ptr(),
                    self.dma_bufs[buf_idx].add(buf_offset),
                    chunk,
                );
            }

            self.write_pos = (self.write_pos + chunk) % total_dma_size;
            written += chunk;
            remaining = &remaining[chunk..];
        }

        // Start stream if not already running
        // Safety: bar0 and stream_index are valid
        if !unsafe { stream::is_stream_running(self.bar0, self.stream_index) } && written > 0 {
            unsafe { stream::start_stream(self.bar0, self.stream_index) };
        }

        written
    }

    /// Set amplifier gain on the output path (0-127 mapped from 0-100)
    /// — EchoFrame: twisting the volume knob at the hardware level
    unsafe fn set_volume(&mut self, volume: u8) {
        if let Some(ref path) = self.output_path {
            let gain = ((volume as u32) * 0x7F) / 100;
            let amp_cmd: u32 = 0xB000 | (gain & 0x7F);
            // Safety: send_command uses initialized CORB/RIRB
            let _ = unsafe {
                self.send_command(path.codec_addr, path.dac_nid, codec::SET_AMP_GAIN | amp_cmd)
            };
        }
    }
}

// ============================================================================
// IntelHda — AudioDevice trait wrapper
// — EchoFrame: the bridge between raw hardware and the OXIDE audio subsystem
// ============================================================================

/// High-level Intel HDA audio device implementing `audio::AudioDevice`
pub struct IntelHda {
    /// Underlying controller, mutex-protected
    controller: Mutex<HdaController>,
    /// Software ring buffer for buffering writes from userspace
    ring_buffer: RingBuffer,
    /// Current stream state
    state: Mutex<StreamState>,
    /// Current configuration
    config: Mutex<Option<StreamConfig>>,
    /// Volume (0-100)
    volume: AtomicU8,
    /// Mute state
    muted: AtomicBool,
    /// Playback position counter (bytes)
    position: AtomicU64,
}

// Safety: All fields are independently thread-safe (Mutex, Atomic, or immutable)
unsafe impl Send for IntelHda {}
unsafe impl Sync for IntelHda {}

impl AudioDevice for IntelHda {
    fn info(&self) -> AudioDeviceInfo {
        AudioDeviceInfo {
            name: String::from("Intel HDA"),
            description: String::from("Intel High Definition Audio (ICH6)"),
            sample_rates: vec![44100, 48000],
            channels: vec![1, 2],
            formats: vec![SampleFormat::S16LE, SampleFormat::S16BE],
            max_buffer_frames: 8192,
            min_buffer_frames: 64,
        }
    }

    fn supports_playback(&self) -> bool {
        true
    }

    fn supports_capture(&self) -> bool {
        false
    }

    fn state(&self) -> StreamState {
        *self.state.lock()
    }

    fn configure(&self, config: StreamConfig) -> AudioResult<()> {
        let state = *self.state.lock();
        if state == StreamState::Running {
            return Err(AudioError::DeviceBusy);
        }
        *self.config.lock() = Some(config);
        *self.state.lock() = StreamState::Setup;
        Ok(())
    }

    fn prepare(&self) -> AudioResult<()> {
        let mut ctrl = self.controller.lock();
        if !ctrl.stream_configured {
            unsafe {
                ctrl.setup_dma_and_stream().map_err(|_| AudioError::IoError)?;
            }
        }
        *self.state.lock() = StreamState::Prepared;
        Ok(())
    }

    fn start(&self) -> AudioResult<()> {
        let ctrl = self.controller.lock();
        if ctrl.stream_configured {
            unsafe {
                stream::start_stream(ctrl.bar0, ctrl.stream_index);
            }
        }
        *self.state.lock() = StreamState::Running;
        Ok(())
    }

    fn stop(&self) -> AudioResult<()> {
        let ctrl = self.controller.lock();
        if ctrl.stream_configured {
            unsafe {
                stream::stop_stream(ctrl.bar0, ctrl.stream_index);
            }
        }
        *self.state.lock() = StreamState::Stopped;
        Ok(())
    }

    fn release(&self) -> AudioResult<()> {
        let ctrl = self.controller.lock();
        if ctrl.stream_configured {
            unsafe {
                stream::stop_stream(ctrl.bar0, ctrl.stream_index);
            }
        }
        self.ring_buffer.clear();
        *self.state.lock() = StreamState::Setup;
        Ok(())
    }

    fn write(&self, data: &[u8]) -> AudioResult<usize> {
        // Buffer into ring buffer, then drain to DMA
        let buffered = self.ring_buffer.write(data);

        // Drain ring buffer into DMA
        let mut drain_buf = [0u8; 4096];
        let available = self.ring_buffer.read_available().min(drain_buf.len());
        if available > 0 {
            let read = self.ring_buffer.read(&mut drain_buf[..available]);
            if read > 0 {
                let mut ctrl = self.controller.lock();
                let written = unsafe { ctrl.write_audio(&drain_buf[..read]) };
                self.position.fetch_add(written as u64, Ordering::Relaxed);
            }
        }

        if buffered > 0 {
            Ok(buffered)
        } else {
            Err(AudioError::BufferOverflow)
        }
    }

    fn read(&self, _data: &mut [u8]) -> AudioResult<usize> {
        Err(AudioError::NotSupported)
    }

    fn write_available(&self) -> usize {
        self.ring_buffer.write_available()
    }

    fn read_available(&self) -> usize {
        0
    }

    fn get_volume(&self) -> u8 {
        self.volume.load(Ordering::Relaxed)
    }

    fn set_volume(&self, volume: u8) -> AudioResult<()> {
        let vol = volume.min(100);
        self.volume.store(vol, Ordering::Relaxed);
        let mut ctrl = self.controller.lock();
        unsafe { ctrl.set_volume(vol) };
        Ok(())
    }

    fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    fn set_mute(&self, mute: bool) -> AudioResult<()> {
        self.muted.store(mute, Ordering::Relaxed);
        let mut ctrl = self.controller.lock();
        if mute {
            unsafe { ctrl.set_volume(0) };
        } else {
            let vol = self.volume.load(Ordering::Relaxed);
            unsafe { ctrl.set_volume(vol) };
        }
        Ok(())
    }

    fn get_position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }

    fn get_latency(&self) -> u32 {
        // Two 4KB buffers at 48kHz 16-bit stereo = ~21ms
        (stream::TOTAL_CBL / 4) as u32
    }
}

// ============================================================================
// Global State + PCI Init Entry Point
// — EchoFrame: the single static instance, born when PCI finds the hardware
// ============================================================================

/// Global Intel HDA device instance
static INTEL_HDA: Mutex<Option<Arc<IntelHda>>> = Mutex::new(None);

/// Initialize Intel HDA from a discovered PCI device
///
/// Full init sequence: PCI BAR → reset → CORB/RIRB → codec discovery →
/// output path config → DMA setup → register with audio subsystem.
/// — EchoFrame: from cold silicon to live audio pipeline in one function call
pub fn init_from_pci(pci_dev: &PciDevice) -> Result<(), &'static str> {
    // Get BAR0 physical address
    let bar0_phys = pci_dev.bar0_address().ok_or("HDA: no BAR0")?;
    let bar0_virt = phys_to_virt(bar0_phys) as *mut u8;

    // Enable PCI bus mastering and memory space
    pci::enable_bus_master(pci_dev.address);
    pci::enable_memory_space(pci_dev.address);

    // Build initial controller state
    let mut ctrl = HdaController {
        bar0: bar0_virt,
        corb: core::ptr::null_mut(),
        rirb: core::ptr::null_mut(),
        corb_phys: 0,
        rirb_phys: 0,
        corb_wp: 0,
        num_oss: 0,
        num_iss: 0,
        bdl: core::ptr::null_mut(),
        bdl_phys: 0,
        dma_bufs: [core::ptr::null_mut(); stream::NUM_DMA_BUFS],
        dma_phys: [0; stream::NUM_DMA_BUFS],
        stream_index: 0,
        stream_tag: 1,
        write_pos: 0,
        output_path: None,
        stream_configured: false,
    };

    // Init sequence — each step must succeed before the next
    unsafe {
        ctrl.reset()?;
        ctrl.init_corb()?;
        ctrl.init_rirb()?;
        ctrl.discover_and_configure()?;
        ctrl.setup_dma_and_stream()?;
    }

    // Wrap in the AudioDevice implementation
    let intel_hda = Arc::new(IntelHda {
        controller: Mutex::new(ctrl),
        ring_buffer: RingBuffer::new(32768),
        state: Mutex::new(StreamState::Prepared),
        config: Mutex::new(Some(StreamConfig {
            channels: 2,
            sample_rate: 48000,
            format: SampleFormat::S16LE,
            buffer_frames: 4096,
            period_frames: 1024,
        })),
        volume: AtomicU8::new(80),
        muted: AtomicBool::new(false),
        position: AtomicU64::new(0),
    });

    // Register with the audio subsystem
    audio::init();
    audio::register_device(intel_hda.clone());

    // Store global reference
    *INTEL_HDA.lock() = Some(intel_hda);

    Ok(())
}

/// Get the number of registered Intel HDA devices
pub fn device_count() -> usize {
    if INTEL_HDA.lock().is_some() { 1 } else { 0 }
}
