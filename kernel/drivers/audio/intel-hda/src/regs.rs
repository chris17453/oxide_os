//! Intel HDA Controller MMIO Register Definitions
//!
//! Register offsets per Intel High Definition Audio Specification Rev 1.0a §3.
//! Every constant is a byte offset from the controller's BAR0 MMIO base.
//! — SableWire: raw silicon addresses, no abstraction between you and the metal

#![allow(dead_code)]

// ============================================================================
// Global Registers (§3.3)
// ============================================================================

/// Global Capabilities — 16-bit read-only
/// Bits [3:0]: Number of Output Streams (OSS)
/// Bits [7:4]: Number of Input Streams (ISS)
/// Bits [11:8]: Number of Bidirectional Streams (BSS)
/// Bits [14:12]: Number of Serial Data Out signals (NSDO)
/// Bit [15]: 64-bit Address Supported (64OK)
pub const GCAP: usize = 0x00;

/// Minor Version — 8-bit read-only
pub const VMIN: usize = 0x02;

/// Major Version — 8-bit read-only
pub const VMAJ: usize = 0x03;

/// Output Payload Capability — 16-bit read-only
pub const OUTPAY: usize = 0x04;

/// Input Payload Capability — 16-bit read-only
pub const INPAY: usize = 0x06;

/// Global Control — 32-bit
/// Bit 0: Controller Reset (CRST) — 0=reset, 1=running
/// Bit 1: Flush Control
/// Bit 8: Accept Unsolicited Responses
pub const GCTL: usize = 0x08;

/// Wake Enable — 16-bit
pub const WAKEEN: usize = 0x0C;

/// State Change Status — 16-bit
/// Bit per codec (0-14): 1 = codec present/changed
pub const STATESTS: usize = 0x0E;

/// Global Status — 16-bit
pub const GSTS: usize = 0x10;

/// Interrupt Control — 32-bit
/// Bits [29:0]: Stream Interrupt Enable (one per stream)
/// Bit 30: Controller Interrupt Enable (CIE)
/// Bit 31: Global Interrupt Enable (GIE)
pub const INTCTL: usize = 0x20;

/// Interrupt Status — 32-bit
/// Bits [29:0]: Stream Interrupt Status
/// Bit 30: Controller Interrupt Status
/// Bit 31: Global Interrupt Status
pub const INTSTS: usize = 0x24;

// ============================================================================
// CORB Registers (§3.3.2) — Command Output Ring Buffer
// ============================================================================

/// CORB Lower Base Address — 32-bit (128-byte aligned)
pub const CORBLBASE: usize = 0x40;

/// CORB Upper Base Address — 32-bit (upper 32 bits for 64-bit systems)
pub const CORBUBASE: usize = 0x44;

/// CORB Write Pointer — 16-bit
/// Bits [7:0]: write pointer index
pub const CORBWP: usize = 0x48;

/// CORB Read Pointer — 16-bit
/// Bit 15: Read Pointer Reset (write 1 to reset, poll until reads back 1)
/// Bits [7:0]: read pointer (updated by controller)
pub const CORBRP: usize = 0x4A;

/// CORB Control — 8-bit
/// Bit 0: Memory Error Interrupt Enable
/// Bit 1: CORB DMA Engine Run (CORBRUN)
pub const CORBCTL: usize = 0x4C;

/// CORB Status — 8-bit
pub const CORBSTS: usize = 0x4D;

/// CORB Size — 8-bit
/// Bits [1:0]: Size Capability (00=2, 01=16, 10=256 entries)
/// Bits [5:4]: Size Select
pub const CORBSIZE: usize = 0x4E;

// ============================================================================
// RIRB Registers (§3.3.3) — Response Input Ring Buffer
// ============================================================================

/// RIRB Lower Base Address — 32-bit (128-byte aligned)
pub const RIRBLBASE: usize = 0x50;

/// RIRB Upper Base Address — 32-bit
pub const RIRBUBASE: usize = 0x54;

/// RIRB Write Pointer — 16-bit (updated by controller)
/// Bit 15: Write Pointer Reset (write 1 to reset)
/// Bits [7:0]: write pointer
pub const RIRBWP: usize = 0x58;

/// Response Interrupt Count — 16-bit
pub const RINTCNT: usize = 0x5A;

/// RIRB Control — 8-bit
/// Bit 0: Response Interrupt Control
/// Bit 1: RIRB DMA Enable (RIRBDMAEN)
/// Bit 2: Response Overrun Interrupt Control
pub const RIRBCTL: usize = 0x5C;

/// RIRB Status — 8-bit
/// Bit 0: Response Interrupt
/// Bit 2: Response Overrun Interrupt Status
pub const RIRBSTS: usize = 0x5D;

/// RIRB Size — 8-bit
pub const RIRBSIZE: usize = 0x5E;

// ============================================================================
// Stream Descriptor Registers (§3.3.5)
// ============================================================================
// Each stream descriptor block is 0x20 bytes.
// Output streams start at index num_iss, input streams at 0.
// Base address: 0x80 + (stream_index * 0x20)

/// Compute the base address for a given stream descriptor index
#[inline]
pub const fn sd_base(stream_index: u8) -> usize {
    0x80 + (stream_index as usize) * 0x20
}

/// Stream Descriptor Control — 24-bit (3 bytes at +0x00)
/// Bit 0: Stream Reset (SRST)
/// Bit 1: Stream Run (RUN)
/// Bit 2: Interrupt on Completion Enable (IOCE)
/// Bit 3: FIFO Error Interrupt Enable (FEIE)
/// Bit 4: Descriptor Error Interrupt Enable (DEIE)
/// Bits [23:20]: Stream Number (tag)
pub const SD_CTL: usize = 0x00;

/// Stream Descriptor Status — 8-bit at +0x03
/// Bit 2: Buffer Completion Interrupt Status (BCIS)
/// Bit 3: FIFO Error (FIFOE)
/// Bit 4: Descriptor Error (DESE)
/// Bit 5: FIFO Ready (FIFORDY)
pub const SD_STS: usize = 0x03;

/// Link Position in Buffer — 32-bit at +0x04
pub const SD_LPIB: usize = 0x04;

/// Cyclic Buffer Length — 32-bit at +0x08
/// Total length of all BDL entries in bytes
pub const SD_CBL: usize = 0x08;

/// Last Valid Index — 16-bit at +0x0C
/// Index of last valid BDL entry (0-based)
pub const SD_LVI: usize = 0x0C;

/// FIFO Size — 16-bit at +0x10
pub const SD_FIFOS: usize = 0x10;

/// Stream Format — 16-bit at +0x12
/// Bits [14]: Stream Type (0=PCM, 1=non-PCM)
/// Bits [13:11]: Sample Base Rate (0=48kHz, 1=44.1kHz)
/// Bits [10:8]: Sample Base Rate Multiple
/// Bits [7:5]: Reserved
/// Bits [4:1]: Bits Per Sample
/// Bits [3:0]: Number of Channels - 1
pub const SD_FMT: usize = 0x12;

/// Buffer Descriptor List Pointer — Lower 32 bits at +0x18
pub const SD_BDLPL: usize = 0x18;

/// Buffer Descriptor List Pointer — Upper 32 bits at +0x1C
pub const SD_BDLPU: usize = 0x1C;

// ============================================================================
// GCTL Register Bits
// ============================================================================

/// Controller Reset bit in GCTL
pub const GCTL_CRST: u32 = 1 << 0;

/// Flush Control bit in GCTL
pub const GCTL_FCNTRL: u32 = 1 << 1;

/// Accept Unsolicited Responses bit in GCTL
pub const GCTL_UNSOL: u32 = 1 << 8;

// ============================================================================
// CORBCTL / RIRBCTL Register Bits
// ============================================================================

/// CORB DMA Run bit
pub const CORBCTL_RUN: u8 = 1 << 1;

/// CORB Memory Error Interrupt Enable
pub const CORBCTL_MEIE: u8 = 1 << 0;

/// RIRB DMA Enable bit
pub const RIRBCTL_DMAEN: u8 = 1 << 1;

/// RIRB Response Overrun Interrupt Control
pub const RIRBCTL_OIC: u8 = 1 << 2;

// ============================================================================
// SD_CTL Register Bits
// ============================================================================

/// Stream Reset
pub const SD_CTL_SRST: u32 = 1 << 0;

/// Stream Run
pub const SD_CTL_RUN: u32 = 1 << 1;

/// Interrupt on Completion Enable
pub const SD_CTL_IOCE: u32 = 1 << 2;

/// FIFO Error Interrupt Enable
pub const SD_CTL_FEIE: u32 = 1 << 3;

/// Descriptor Error Interrupt Enable
pub const SD_CTL_DEIE: u32 = 1 << 4;

// ============================================================================
// SD_STS Register Bits
// ============================================================================

/// Buffer Completion Interrupt Status
pub const SD_STS_BCIS: u8 = 1 << 2;

/// FIFO Error
pub const SD_STS_FIFOE: u8 = 1 << 3;

/// Descriptor Error
pub const SD_STS_DESE: u8 = 1 << 4;

/// FIFO Ready
pub const SD_STS_FIFORDY: u8 = 1 << 5;

// ============================================================================
// Stream Format Encoding (SD_FMT)
// ============================================================================

/// 48kHz base rate, divide-by-1, 16-bit stereo (2ch)
/// Bits: [14]=0 (PCM), [13:11]=000 (48kHz base), [10:8]=000 (x1 mult),
///       [6:4]=001 (16-bit), [3:0]=0001 (2 channels)
pub const FMT_48KHZ_16BIT_STEREO: u16 = 0x0011;

/// 44.1kHz base rate, divide-by-1, 16-bit stereo
pub const FMT_44K1_16BIT_STEREO: u16 = 0x4011;

// ============================================================================
// Volatile MMIO Helpers
// — SableWire: direct memory-mapped I/O, no caching, no shortcuts
// ============================================================================

/// Read a 32-bit register at (base + offset)
///
/// # Safety
/// `base` must point to valid MMIO space. Caller must ensure the offset
/// is within the controller's BAR0 region.
#[inline]
pub unsafe fn read32(base: *const u8, offset: usize) -> u32 {
    unsafe { core::ptr::read_volatile(base.add(offset) as *const u32) }
}

/// Write a 32-bit register at (base + offset)
#[inline]
pub unsafe fn write32(base: *mut u8, offset: usize, val: u32) {
    unsafe { core::ptr::write_volatile(base.add(offset) as *mut u32, val) }
}

/// Read a 16-bit register at (base + offset)
#[inline]
pub unsafe fn read16(base: *const u8, offset: usize) -> u16 {
    unsafe { core::ptr::read_volatile(base.add(offset) as *const u16) }
}

/// Write a 16-bit register at (base + offset)
#[inline]
pub unsafe fn write16(base: *mut u8, offset: usize, val: u16) {
    unsafe { core::ptr::write_volatile(base.add(offset) as *mut u16, val) }
}

/// Read an 8-bit register at (base + offset)
#[inline]
pub unsafe fn read8(base: *const u8, offset: usize) -> u8 {
    unsafe { core::ptr::read_volatile(base.add(offset) as *const u8) }
}

/// Write an 8-bit register at (base + offset)
#[inline]
pub unsafe fn write8(base: *mut u8, offset: usize, val: u8) {
    unsafe { core::ptr::write_volatile(base.add(offset) as *mut u8, val) }
}
