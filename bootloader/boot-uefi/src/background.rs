//! Boot Background — BMP decoder + scale-to-cover renderer.
//!
//! — NeonVale: one embedded image, decoded at boot, scaled to fill whatever
//! resolution the firmware hands us. No filesystem access, no external deps,
//! just include_bytes! and math. The BMP format is 1990s simple — a 54-byte
//! header followed by bottom-up rows of BGR pixels. Perfect for bare metal.

use crate::efi::EfiBltPixel;

/// The embedded boot background image (24-bit BMP, ~4.6MB)
/// — NeonVale: baked into the binary at compile time. No file I/O needed.
static BG_DATA: &[u8] = include_bytes!("../assets/boot-bg.bmp");

/// BMP file header offsets (Windows BITMAPINFOHEADER format)
const BMP_WIDTH_OFFSET: usize = 18;
const BMP_HEIGHT_OFFSET: usize = 22;
const BMP_BPP_OFFSET: usize = 28;
const BMP_DATA_OFFSET: usize = 10;

/// Read a little-endian u32 from a byte slice
#[inline]
fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Read a little-endian i32 from a byte slice (BMP height can be negative)
#[inline]
fn read_i32(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Decode the BMP header and return (width, height, bpp, pixel_data_offset, top_down).
/// Returns None if the BMP is invalid or unsupported.
fn decode_header() -> Option<(usize, usize, usize, usize, bool)> {
    let data = BG_DATA;
    if data.len() < 54 { return None; }
    // — NeonVale: BMP magic: 'BM'
    if data[0] != b'B' || data[1] != b'M' { return None; }

    let pixel_offset = read_u32(data, BMP_DATA_OFFSET) as usize;
    let width = read_u32(data, BMP_WIDTH_OFFSET) as usize;
    let raw_height = read_i32(data, BMP_HEIGHT_OFFSET);
    let bpp = read_u32(data, BMP_BPP_OFFSET) as usize;

    // — NeonVale: we only handle 24-bit BMPs. 32-bit would work too but
    // ImageMagick gives us 24-bit by default.
    if bpp != 24 && bpp != 32 { return None; }

    // — NeonVale: negative height = top-down row order (rare but valid)
    let (height, top_down) = if raw_height < 0 {
        ((-raw_height) as usize, true)
    } else {
        (raw_height as usize, false)
    };

    if width == 0 || height == 0 { return None; }
    if pixel_offset >= data.len() { return None; }

    Some((width, height, bpp, pixel_offset, top_down))
}

/// Sample a pixel from the source BMP at (src_x, src_y).
/// Returns an EfiBltPixel (BGRX format — matches UEFI exactly).
#[inline]
fn sample_pixel(
    src_x: usize,
    src_y: usize,
    img_w: usize,
    img_h: usize,
    bpp: usize,
    pixel_offset: usize,
    top_down: bool,
) -> EfiBltPixel {
    let data = BG_DATA;
    let bytes_per_pixel = bpp / 8;
    // — NeonVale: BMP rows are padded to 4-byte boundaries
    let row_stride = ((img_w * bytes_per_pixel + 3) / 4) * 4;

    // — NeonVale: BMP is bottom-up by default. Row 0 in the file is the
    // bottom of the image. Flip the Y coordinate unless top_down.
    let file_row = if top_down { src_y } else { img_h - 1 - src_y };

    let pixel_start = pixel_offset + file_row * row_stride + src_x * bytes_per_pixel;
    if pixel_start + bytes_per_pixel > data.len() {
        return EfiBltPixel::new(0, 0, 0);
    }

    // — NeonVale: BMP stores BGR (not RGB). EfiBltPixel is also BGR. Match made in heaven.
    let b = data[pixel_start];
    let g = data[pixel_start + 1];
    let r = data[pixel_start + 2];
    EfiBltPixel { blue: b, green: g, red: r, reserved: 0 }
}

/// Draw the boot background scaled to COVER the screen.
///
/// Scale-to-cover: the image is scaled uniformly so it completely fills the
/// screen. If aspect ratios differ, the image is cropped (centered) on the
/// axis that overflows. No letterboxing, no stretching, no black bars.
///
/// — NeonVale: nearest-neighbor scaling because we're in pre-boot firmware
/// and bilinear filtering is a luxury we don't need for a 1536x1024 source.
pub fn draw_background(screen_w: usize, screen_h: usize) {
    let (img_w, img_h, bpp, pixel_offset, top_down) = match decode_header() {
        Some(info) => info,
        None => return, // — NeonVale: invalid BMP, skip silently
    };

    // — NeonVale: scale-to-cover math. Pick the larger scale factor so the
    // image fills both dimensions. The other dimension overflows and gets cropped.
    // Using fixed-point (×65536) to avoid floating point in UEFI context.
    let scale_x_fp = (img_w << 16) / screen_w; // source pixels per screen pixel (fixed-point)
    let scale_y_fp = (img_h << 16) / screen_h;
    let scale_fp = if scale_x_fp < scale_y_fp { scale_x_fp } else { scale_y_fp };

    // — NeonVale: how many source pixels the screen covers at this scale
    let covered_w = (screen_w * scale_fp) >> 16;
    let covered_h = (screen_h * scale_fp) >> 16;

    // — NeonVale: center crop — offset into the source image
    let crop_x = if covered_w < img_w { (img_w - covered_w) / 2 } else { 0 };
    let crop_y = if covered_h < img_h { (img_h - covered_h) / 2 } else { 0 };

    // — NeonVale: render to screen buffer, one pixel at a time.
    // With the RAM screen buffer this is just array writes — no firmware calls.
    // For 1024x768 that's ~786K pixels, each is an array index + byte read.
    for dst_y in 0..screen_h {
        let src_y = crop_y + ((dst_y * scale_fp) >> 16);
        let src_y = src_y.min(img_h - 1);

        for dst_x in 0..screen_w {
            let src_x = crop_x + ((dst_x * scale_fp) >> 16);
            let src_x = src_x.min(img_w - 1);

            let pixel = sample_pixel(src_x, src_y, img_w, img_h, bpp, pixel_offset, top_down);
            crate::screen::fill_rect(dst_x, dst_y, 1, 1, pixel);
        }
    }
}
