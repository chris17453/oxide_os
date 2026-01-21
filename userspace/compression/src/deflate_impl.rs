//! DEFLATE compression implementation
//!
//! Implements RFC 1951 DEFLATE compressed data format.
//! Uses fixed Huffman codes for simplicity while maintaining compatibility.

use alloc::vec::Vec;
use crate::{CompressionError, Result};

/// Bit writer for packing bits into bytes
struct BitWriter {
    output: Vec<u8>,
    bit_buffer: u32,
    bits_in_buffer: u8,
}

impl BitWriter {
    fn new() -> Self {
        BitWriter {
            output: Vec::new(),
            bit_buffer: 0,
            bits_in_buffer: 0,
        }
    }

    /// Write bits (LSB first)
    fn write_bits(&mut self, value: u16, num_bits: u8) {
        self.bit_buffer |= (value as u32) << self.bits_in_buffer;
        self.bits_in_buffer += num_bits;

        while self.bits_in_buffer >= 8 {
            self.output.push(self.bit_buffer as u8);
            self.bit_buffer >>= 8;
            self.bits_in_buffer -= 8;
        }
    }

    /// Flush remaining bits
    fn flush(&mut self) {
        if self.bits_in_buffer > 0 {
            self.output.push(self.bit_buffer as u8);
            self.bit_buffer = 0;
            self.bits_in_buffer = 0;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        self.flush();
        self.output
    }
}

/// Bit reader for unpacking bits from bytes
struct BitReader<'a> {
    input: &'a [u8],
    byte_pos: usize,
    bit_buffer: u32,
    bits_in_buffer: u8,
}

impl<'a> BitReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        BitReader {
            input,
            byte_pos: 0,
            bit_buffer: 0,
            bits_in_buffer: 0,
        }
    }

    /// Read bits (LSB first)
    fn read_bits(&mut self, num_bits: u8) -> Result<u16> {
        while self.bits_in_buffer < num_bits {
            if self.byte_pos >= self.input.len() {
                return Err(CompressionError::InvalidData);
            }
            self.bit_buffer |= (self.input[self.byte_pos] as u32) << self.bits_in_buffer;
            self.byte_pos += 1;
            self.bits_in_buffer += 8;
        }

        let value = (self.bit_buffer & ((1 << num_bits) - 1)) as u16;
        self.bit_buffer >>= num_bits;
        self.bits_in_buffer -= num_bits;

        Ok(value)
    }

    /// Align to byte boundary
    fn align_to_byte(&mut self) {
        let skip_bits = self.bits_in_buffer % 8;
        if skip_bits > 0 {
            self.bit_buffer >>= skip_bits;
            self.bits_in_buffer -= skip_bits;
        }
    }

    fn read_byte(&mut self) -> Result<u8> {
        if self.byte_pos >= self.input.len() {
            return Err(CompressionError::InvalidData);
        }
        let byte = self.input[self.byte_pos];
        self.byte_pos += 1;
        Ok(byte)
    }
}

/// Length codes for DEFLATE (base_length, extra_bits, code)
const LENGTH_CODES: [(u16, u8, u16); 29] = [
    (3, 0, 257), (4, 0, 258), (5, 0, 259), (6, 0, 260), (7, 0, 261),
    (8, 0, 262), (9, 0, 263), (10, 0, 264), (11, 1, 265), (13, 1, 266),
    (15, 1, 267), (17, 1, 268), (19, 2, 269), (23, 2, 270), (27, 2, 271),
    (31, 2, 272), (35, 3, 273), (43, 3, 274), (51, 3, 275), (59, 3, 276),
    (67, 4, 277), (83, 4, 278), (99, 4, 279), (115, 4, 280), (131, 5, 281),
    (163, 5, 282), (195, 5, 283), (227, 5, 284), (258, 0, 285),
];

/// Distance codes for DEFLATE
const DISTANCE_CODES: [(u16, u8); 30] = [
    (1, 0), (2, 0), (3, 0), (4, 0), (5, 1), (7, 1), (9, 2), (13, 2),
    (17, 3), (25, 3), (33, 4), (49, 4), (65, 5), (97, 5), (129, 6), (193, 6),
    (257, 7), (385, 7), (513, 8), (769, 8), (1025, 9), (1537, 9),
    (2049, 10), (3073, 10), (4097, 11), (6145, 11), (8193, 12), (12289, 12),
    (16385, 13), (24577, 13),
];

/// Find length code for a given length
/// Returns (code, extra_bits, extra_value)
fn get_length_code(length: u16) -> (u16, u8, u16) {
    for &(base, extra_bits, code) in &LENGTH_CODES {
        if length < base + (1 << extra_bits) {
            return (code, extra_bits, length - base);
        }
    }
    (285, 0, 0) // 258
}

/// Find distance code for a given distance
fn get_distance_code(distance: u16) -> (u8, u8, u16) {
    for (code, &(base, extra_bits)) in DISTANCE_CODES.iter().enumerate() {
        if distance < base + (1 << extra_bits) {
            return (code as u8, extra_bits, distance - base);
        }
    }
    (29, 13, distance - 24577) // Max distance
}

/// Fixed Huffman code for literals/lengths
fn write_fixed_literal(writer: &mut BitWriter, value: u16) {
    if value <= 143 {
        // 0-143: 8 bits, 00110000 - 10111111
        let code = 0b00110000 + value;
        writer.write_bits(code, 8);
    } else if value <= 255 {
        // 144-255: 9 bits, 110010000 - 111111111
        let code = 0b110010000 + (value - 144);
        writer.write_bits(code, 9);
    } else if value <= 279 {
        // 256-279: 7 bits, 0000000 - 0010111
        let code = value - 256;
        writer.write_bits(code, 7);
    } else {
        // 280-287: 8 bits, 11000000 - 11000111
        let code = 0b11000000 + (value - 280);
        writer.write_bits(code, 8);
    }
}

/// Compress using DEFLATE with fixed Huffman codes
pub fn compress_deflate(input: &[u8], level: u8) -> Result<Vec<u8>> {
    if level == 0 {
        // Uncompressed block
        return compress_uncompressed(input);
    }

    let mut writer = BitWriter::new();

    // Write block header: BFINAL=1, BTYPE=01 (fixed Huffman)
    writer.write_bits(0b101, 3);

    // Simple LZ77 compression
    let mut pos = 0;
    let window_size = 32768.min(input.len());

    while pos < input.len() {
        let mut best_length = 0;
        let mut best_distance = 0;

        // Look for matches in the sliding window
        if pos >= 3 {
            let search_start = pos.saturating_sub(window_size);
            let max_length = (input.len() - pos).min(258);

            for start in search_start..pos {
                let mut length = 0;
                while length < max_length && input[start + length] == input[pos + length] {
                    length += 1;
                }

                if length >= 3 && length > best_length {
                    best_length = length;
                    best_distance = pos - start;
                }
            }
        }

        if best_length >= 3 {
            // Emit length/distance pair
            let (length_code, length_extra_bits, length_extra) = get_length_code(best_length as u16);
            write_fixed_literal(&mut writer, length_code);
            if length_extra_bits > 0 {
                writer.write_bits(length_extra, length_extra_bits);
            }

            let (distance_code, distance_extra_bits, distance_extra) = get_distance_code(best_distance as u16);
            writer.write_bits(distance_code as u16, 5); // Fixed distance code is 5 bits
            if distance_extra_bits > 0 {
                writer.write_bits(distance_extra, distance_extra_bits);
            }

            pos += best_length;
        } else {
            // Emit literal
            write_fixed_literal(&mut writer, input[pos] as u16);
            pos += 1;
        }
    }

    // Write end-of-block symbol (256)
    write_fixed_literal(&mut writer, 256);

    Ok(writer.finish())
}

/// Compress as uncompressed DEFLATE block
fn compress_uncompressed(input: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len() + 5);

    // Block header: BFINAL=1, BTYPE=00 (uncompressed)
    output.push(0x01);

    // Length and complement
    let len = input.len() as u16;
    output.extend_from_slice(&len.to_le_bytes());
    output.extend_from_slice(&(!len).to_le_bytes());

    // Data
    output.extend_from_slice(input);

    Ok(output)
}

/// Decode fixed Huffman code
fn read_fixed_literal(reader: &mut BitReader) -> Result<u16> {
    // Read bits one at a time and determine code length
    let mut code = 0u16;

    // Try 7-bit codes (256-279)
    for _ in 0..7 {
        let bit = reader.read_bits(1)?;
        code = (code << 1) | bit;
    }

    if code <= 0b0010111 {
        return Ok(256 + code);
    }

    // Try 8-bit codes (0-143, 280-287)
    let bit = reader.read_bits(1)?;
    code = (code << 1) | bit;

    if code >= 0b00110000 && code <= 0b10111111 {
        return Ok(code - 0b00110000);
    }
    if code >= 0b11000000 && code <= 0b11000111 {
        return Ok(280 + (code - 0b11000000));
    }

    // Must be 9-bit code (144-255)
    let bit = reader.read_bits(1)?;
    code = (code << 1) | bit;

    if code >= 0b110010000 && code <= 0b111111111 {
        return Ok(144 + (code - 0b110010000));
    }

    Err(CompressionError::InvalidData)
}

/// Decompress DEFLATE data
pub fn decompress_deflate(input: &[u8]) -> Result<Vec<u8>> {
    let mut reader = BitReader::new(input);
    let mut output = Vec::new();

    loop {
        // Read block header
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;

        match btype {
            0 => {
                // Uncompressed block
                reader.align_to_byte();

                let len = reader.read_byte()? as u16 | ((reader.read_byte()? as u16) << 8);
                let nlen = reader.read_byte()? as u16 | ((reader.read_byte()? as u16) << 8);

                if len != !nlen {
                    return Err(CompressionError::InvalidData);
                }

                for _ in 0..len {
                    output.push(reader.read_byte()?);
                }
            }
            1 => {
                // Fixed Huffman codes
                loop {
                    let symbol = read_fixed_literal(&mut reader)?;

                    if symbol < 256 {
                        // Literal
                        output.push(symbol as u8);
                    } else if symbol == 256 {
                        // End of block
                        break;
                    } else {
                        // Length/distance pair
                        let length_idx = (symbol - 257) as usize;
                        if length_idx >= LENGTH_CODES.len() {
                            return Err(CompressionError::InvalidData);
                        }

                        let (base_length, extra_bits, _) = LENGTH_CODES[length_idx];
                        let extra = if extra_bits > 0 {
                            reader.read_bits(extra_bits)?
                        } else {
                            0
                        };
                        let length = base_length + extra;

                        // Read distance
                        let distance_code = reader.read_bits(5)? as usize;
                        if distance_code >= DISTANCE_CODES.len() {
                            return Err(CompressionError::InvalidData);
                        }

                        let (base_distance, distance_extra_bits) = DISTANCE_CODES[distance_code];
                        let distance_extra = if distance_extra_bits > 0 {
                            reader.read_bits(distance_extra_bits)?
                        } else {
                            0
                        };
                        let distance = base_distance + distance_extra;

                        // Copy from history
                        if distance as usize > output.len() {
                            return Err(CompressionError::InvalidData);
                        }

                        let start = output.len() - distance as usize;
                        for i in 0..length {
                            let byte = output[start + i as usize];
                            output.push(byte);
                        }
                    }
                }
            }
            2 => {
                // Dynamic Huffman codes - not implemented yet
                return Err(CompressionError::UnsupportedFormat);
            }
            _ => {
                return Err(CompressionError::InvalidData);
            }
        }

        if bfinal != 0 {
            break;
        }
    }

    Ok(output)
}
