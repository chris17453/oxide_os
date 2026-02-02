//! HDA Stream Setup and BDL-Based DMA Playback
//!
//! Manages Buffer Descriptor Lists and double-buffered DMA transfer for
//! PCM audio output. Each BDL entry points to a physical DMA buffer that
//! the HDA controller reads from autonomously.
//! — EchoFrame: DMA is the only path where audio bits hit real wire

use crate::regs;

/// Buffer Descriptor List entry (Intel HDA Spec §3.6.3)
///
/// Each entry describes one scatter/gather buffer for the DMA engine.
/// Must be 128-byte aligned in physical memory.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct BdlEntry {
    /// Physical address of the data buffer (64-bit)
    pub address: u64,
    /// Length of data buffer in bytes
    pub length: u32,
    /// Bit 0: Interrupt on Completion (IOC)
    pub ioc: u32,
}

impl BdlEntry {
    /// Create a zeroed BDL entry
    pub const fn zeroed() -> Self {
        BdlEntry {
            address: 0,
            length: 0,
            ioc: 0,
        }
    }
}

/// Number of DMA buffers (double-buffered)
pub const NUM_DMA_BUFS: usize = 2;

/// Size of each DMA buffer in bytes (4KB)
pub const DMA_BUF_SIZE: usize = 4096;

/// Total cyclic buffer length (all DMA buffers combined)
pub const TOTAL_CBL: u32 = (NUM_DMA_BUFS * DMA_BUF_SIZE) as u32;

/// Set up the output stream descriptor registers
///
/// Configures the stream to use the provided BDL and DMA buffers.
/// Does NOT start the stream — call `start_stream` to begin playback.
///
/// # Safety
/// `bar0` must be a valid MMIO base pointer. `bdl_phys` must be the physical
/// address of a properly aligned BDL array. Stream index must be valid.
pub unsafe fn setup_output_stream(
    bar0: *mut u8,
    stream_index: u8,
    stream_tag: u8,
    bdl_phys: u64,
    format: u16,
) {
    let base = regs::sd_base(stream_index);

    // 1. Reset the stream — GraveShift: every init starts with a clean slate
    let ctl = unsafe { regs::read32(bar0, base + regs::SD_CTL) };
    unsafe { regs::write32(bar0, base + regs::SD_CTL, ctl | regs::SD_CTL_SRST) };

    // Wait for reset to latch
    for _ in 0..1000 {
        if unsafe { regs::read32(bar0, base + regs::SD_CTL) } & regs::SD_CTL_SRST != 0 {
            break;
        }
        spin_wait();
    }

    // 2. Clear reset bit
    let ctl = unsafe { regs::read32(bar0, base + regs::SD_CTL) };
    unsafe { regs::write32(bar0, base + regs::SD_CTL, ctl & !regs::SD_CTL_SRST) };

    // Wait for reset to clear
    for _ in 0..1000 {
        if unsafe { regs::read32(bar0, base + regs::SD_CTL) } & regs::SD_CTL_SRST == 0 {
            break;
        }
        spin_wait();
    }

    // 3. Clear any pending status bits
    unsafe { regs::write8(bar0, base + regs::SD_STS, 0x1C) };

    // 4. Set stream format (must match codec converter format)
    unsafe { regs::write16(bar0, base + regs::SD_FMT, format) };

    // 5. Set BDL pointer (physical address, 128-byte aligned)
    unsafe {
        regs::write32(bar0, base + regs::SD_BDLPL, bdl_phys as u32);
        regs::write32(bar0, base + regs::SD_BDLPU, (bdl_phys >> 32) as u32);
    }

    // 6. Set cyclic buffer length (total bytes across all BDL entries)
    unsafe { regs::write32(bar0, base + regs::SD_CBL, TOTAL_CBL) };

    // 7. Set Last Valid Index (0-based, 2 entries → LVI = 1)
    unsafe { regs::write16(bar0, base + regs::SD_LVI, (NUM_DMA_BUFS - 1) as u16) };

    // 8. Set stream tag in CTL[23:20] and enable IOC
    let ctl = unsafe { regs::read32(bar0, base + regs::SD_CTL) };
    let ctl = (ctl & !(0xF << 20)) | ((stream_tag as u32 & 0xF) << 20);
    let ctl = ctl | regs::SD_CTL_IOCE;
    unsafe { regs::write32(bar0, base + regs::SD_CTL, ctl) };
}

/// Start the stream (set RUN bit)
///
/// # Safety
/// `bar0` must be valid MMIO pointer, stream must be fully configured.
pub unsafe fn start_stream(bar0: *mut u8, stream_index: u8) {
    let base = regs::sd_base(stream_index);
    let ctl = unsafe { regs::read32(bar0, base + regs::SD_CTL) };
    unsafe { regs::write32(bar0, base + regs::SD_CTL, ctl | regs::SD_CTL_RUN) };
}

/// Stop the stream (clear RUN bit)
///
/// # Safety
/// `bar0` must be valid MMIO pointer.
pub unsafe fn stop_stream(bar0: *mut u8, stream_index: u8) {
    let base = regs::sd_base(stream_index);
    let ctl = unsafe { regs::read32(bar0, base + regs::SD_CTL) };
    unsafe { regs::write32(bar0, base + regs::SD_CTL, ctl & !regs::SD_CTL_RUN) };
}

/// Check if stream is currently running
///
/// # Safety
/// `bar0` must be valid MMIO pointer.
pub unsafe fn is_stream_running(bar0: *mut u8, stream_index: u8) -> bool {
    let base = regs::sd_base(stream_index);
    let ctl = unsafe { regs::read32(bar0, base + regs::SD_CTL) };
    ctl & regs::SD_CTL_RUN != 0
}

/// Read the Link Position in Current Buffer (LPIB)
///
/// # Safety
/// `bar0` must be valid MMIO pointer.
pub unsafe fn read_lpib(bar0: *const u8, stream_index: u8) -> u32 {
    let base = regs::sd_base(stream_index);
    unsafe { regs::read32(bar0, base + regs::SD_LPIB) }
}

/// Tiny spin-wait for hardware settle time
/// — SableWire: sometimes the silicon just needs a moment to breathe
#[inline]
fn spin_wait() {
    for _ in 0..100 {
        core::hint::spin_loop();
    }
}
