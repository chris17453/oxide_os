//! RDP Graphics Pipeline
//!
//! Provides screen capture, dirty region detection, and bitmap encoding
//! for the RDP server.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::vec;

mod capture;
mod compress;
mod convert;

pub use capture::FramebufferCaptureProvider;
pub use compress::{RleCompressor, rle_compress};
pub use convert::PixelConverter;

use alloc::sync::Arc;
use alloc::vec::Vec;
use rdp_proto::fast_path::{FastPathBitmapRect, FastPathBitmapUpdate};
use rdp_proto::pdu::{BitmapRectangle, BitmapUpdate};
use rdp_traits::{DirtyRegion, PixelFormat as RdpPixelFormat, RdpResult, ScreenCaptureProvider};
use spin::Mutex;

/// Block size for dirty detection (32x32 pixels)
pub const DIRTY_BLOCK_SIZE: u32 = 32;

/// Maximum rectangles per update (to avoid huge packets)
pub const MAX_RECTS_PER_UPDATE: usize = 64;

/// Configuration for the graphics pipeline
#[derive(Debug, Clone)]
pub struct GraphicsConfig {
    /// Target bits per pixel for wire format
    pub target_bpp: u16,
    /// Enable bitmap compression
    pub compression_enabled: bool,
    /// Maximum frame rate
    pub max_fps: u32,
    /// Block size for dirty detection
    pub block_size: u32,
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        Self {
            target_bpp: 32,
            compression_enabled: true,
            max_fps: 30,
            block_size: DIRTY_BLOCK_SIZE,
        }
    }
}

/// Graphics encoder for a session
pub struct GraphicsEncoder {
    /// Screen capture provider
    capture: Arc<Mutex<dyn ScreenCaptureProvider>>,
    /// Pixel converter
    converter: PixelConverter,
    /// RLE compressor
    compressor: RleCompressor,
    /// Configuration
    config: GraphicsConfig,
    /// Previous frame buffer for comparison
    prev_frame: Vec<u8>,
    /// Current frame buffer
    curr_frame: Vec<u8>,
    /// Screen dimensions
    width: u32,
    height: u32,
    /// Source pixel format
    source_format: RdpPixelFormat,
}

impl GraphicsEncoder {
    /// Create a new graphics encoder
    pub fn new(capture: Arc<Mutex<dyn ScreenCaptureProvider>>, config: GraphicsConfig) -> Self {
        let (width, height) = capture.lock().dimensions();
        let format = capture.lock().pixel_format();
        let stride = capture.lock().stride();

        let frame_size = (height * stride) as usize;

        Self {
            capture,
            converter: PixelConverter::new(format, RdpPixelFormat::Bgra8888),
            compressor: RleCompressor::new(),
            config,
            prev_frame: alloc::vec![0u8; frame_size],
            curr_frame: alloc::vec![0u8; frame_size],
            width,
            height,
            source_format: format,
        }
    }

    /// Capture current frame and detect dirty regions
    pub fn capture_frame(&mut self) -> RdpResult<Vec<DirtyRegion>> {
        // Swap buffers
        core::mem::swap(&mut self.prev_frame, &mut self.curr_frame);

        // Capture new frame
        self.capture.lock().capture_full(&mut self.curr_frame)?;

        // Detect dirty regions
        let regions = self.detect_dirty_blocks();

        Ok(regions)
    }

    /// Detect dirty 32x32 blocks by comparing frames
    fn detect_dirty_blocks(&self) -> Vec<DirtyRegion> {
        let mut dirty = Vec::new();
        let block_size = self.config.block_size;
        let bpp = self.source_format.bytes_per_pixel();
        let stride = self.capture.lock().stride();

        let blocks_x = (self.width + block_size - 1) / block_size;
        let blocks_y = (self.height + block_size - 1) / block_size;

        for by in 0..blocks_y {
            for bx in 0..blocks_x {
                let x = bx * block_size;
                let y = by * block_size;
                let w = (self.width - x).min(block_size);
                let h = (self.height - y).min(block_size);

                if self.block_is_dirty(x, y, w, h, stride, bpp) {
                    dirty.push(DirtyRegion::new(x, y, w, h));
                }
            }
        }

        // Merge adjacent dirty regions for efficiency
        self.merge_dirty_regions(dirty)
    }

    /// Check if a block differs between frames
    fn block_is_dirty(&self, x: u32, y: u32, w: u32, h: u32, stride: u32, bpp: u32) -> bool {
        for row in 0..h {
            let offset = ((y + row) * stride + x * bpp) as usize;
            let len = (w * bpp) as usize;

            if offset + len > self.prev_frame.len() || offset + len > self.curr_frame.len() {
                return true;
            }

            if self.prev_frame[offset..offset + len] != self.curr_frame[offset..offset + len] {
                return true;
            }
        }
        false
    }

    /// Merge adjacent dirty regions into larger rectangles
    fn merge_dirty_regions(&self, regions: Vec<DirtyRegion>) -> Vec<DirtyRegion> {
        if regions.len() <= 1 {
            return regions;
        }

        // Simple horizontal merge for adjacent blocks
        let mut merged = Vec::with_capacity(regions.len());
        let mut current: Option<DirtyRegion> = None;

        for region in regions {
            if let Some(ref mut curr) = current {
                // Check if regions are horizontally adjacent and same height
                if curr.y == region.y
                    && curr.height == region.height
                    && curr.x + curr.width == region.x
                {
                    // Merge
                    curr.width += region.width;
                } else {
                    // Push current and start new
                    merged.push(*curr);
                    current = Some(region);
                }
            } else {
                current = Some(region);
            }
        }

        if let Some(curr) = current {
            merged.push(curr);
        }

        // Limit number of regions
        if merged.len() > MAX_RECTS_PER_UPDATE {
            // Combine into full-screen update
            vec![DirtyRegion::new(0, 0, self.width, self.height)]
        } else {
            merged
        }
    }

    /// Encode dirty regions as bitmap update
    pub fn encode_bitmap_update(&mut self, regions: &[DirtyRegion]) -> RdpResult<BitmapUpdate> {
        let mut rectangles = Vec::with_capacity(regions.len());
        let stride = self.capture.lock().stride();
        let bpp = self.source_format.bytes_per_pixel();

        for region in regions {
            let rect_data = self.encode_region(region, stride, bpp)?;
            rectangles.push(rect_data);
        }

        Ok(BitmapUpdate { rectangles })
    }

    /// Encode a single region as bitmap rectangle
    fn encode_region(
        &mut self,
        region: &DirtyRegion,
        stride: u32,
        source_bpp: u32,
    ) -> RdpResult<BitmapRectangle> {
        let target_bpp = self.config.target_bpp as u32;
        let target_bytes = target_bpp / 8;

        // Extract and convert region data
        let mut converted =
            Vec::with_capacity((region.width * region.height * target_bytes) as usize);

        for row in 0..region.height {
            let src_offset = ((region.y + row) * stride + region.x * source_bpp) as usize;

            for col in 0..region.width {
                let pixel_offset = src_offset + (col * source_bpp) as usize;
                let pixel = &self.curr_frame[pixel_offset..pixel_offset + source_bpp as usize];

                // Convert to target format (BGRA8888)
                let bgra = self.converter.convert_pixel(pixel);
                converted.extend_from_slice(&bgra[..target_bytes as usize]);
            }
        }

        // Optionally compress
        let (data, flags) = if self.config.compression_enabled {
            let compressed = self.compressor.compress(
                &converted,
                region.width,
                region.height,
                target_bpp as u16,
            );
            if compressed.len() < converted.len() {
                (
                    compressed,
                    BitmapRectangle::BITMAP_COMPRESSION
                        | BitmapRectangle::NO_BITMAP_COMPRESSION_HDR,
                )
            } else {
                (converted, BitmapRectangle::NO_BITMAP_COMPRESSION_HDR)
            }
        } else {
            (converted, BitmapRectangle::NO_BITMAP_COMPRESSION_HDR)
        };

        Ok(BitmapRectangle {
            dest_left: region.x as u16,
            dest_top: region.y as u16,
            dest_right: (region.x + region.width - 1) as u16,
            dest_bottom: (region.y + region.height - 1) as u16,
            width: region.width as u16,
            height: region.height as u16,
            bpp: self.config.target_bpp,
            flags,
            data,
        })
    }

    /// Encode dirty regions as fast-path bitmap update
    pub fn encode_fast_path_update(
        &mut self,
        regions: &[DirtyRegion],
    ) -> RdpResult<FastPathBitmapUpdate> {
        let mut rectangles = Vec::with_capacity(regions.len());
        let stride = self.capture.lock().stride();
        let bpp = self.source_format.bytes_per_pixel();

        for region in regions {
            let rect = self.encode_region(region, stride, bpp)?;
            rectangles.push(FastPathBitmapRect {
                dest_left: rect.dest_left,
                dest_top: rect.dest_top,
                dest_right: rect.dest_right,
                dest_bottom: rect.dest_bottom,
                width: rect.width,
                height: rect.height,
                bpp: rect.bpp,
                flags: rect.flags,
                data: rect.data,
            });
        }

        Ok(FastPathBitmapUpdate { rectangles })
    }

    /// Get current screen dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Force full screen refresh
    pub fn force_full_refresh(&mut self) {
        // Clear previous frame to force all blocks dirty
        self.prev_frame.fill(0);
    }
}

/// Create a screen capture provider from the system framebuffer
pub fn create_capture_provider() -> Option<Arc<Mutex<dyn ScreenCaptureProvider>>> {
    let fb = fb::framebuffer()?;
    Some(Arc::new(Mutex::new(FramebufferCaptureProvider::new(fb))))
}
