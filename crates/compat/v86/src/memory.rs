//! V86 memory management

use alloc::vec;
use alloc::vec::Vec;
use crate::V86Error;

/// V86 memory (1MB address space)
#[derive(Clone)]
pub struct V86Memory {
    /// Conventional memory (0-640KB)
    conventional: Vec<u8>,
    /// UMA (640KB-1MB) - typically ROM and video memory
    uma: Vec<u8>,
    /// XMS memory (optional extended memory)
    xms: Option<Vec<u8>>,
}

impl V86Memory {
    /// Create new V86 memory
    pub fn new() -> Self {
        V86Memory {
            conventional: vec![0; 640 * 1024],  // 640KB
            uma: vec![0; 384 * 1024],           // 384KB UMA
            xms: None,
        }
    }

    /// Create with XMS memory
    pub fn with_xms(xms_size: usize) -> Self {
        let mut mem = Self::new();
        mem.xms = Some(vec![0; xms_size]);
        mem
    }

    /// Read byte from linear address
    pub fn read_u8(&self, addr: u32) -> Result<u8, V86Error> {
        let addr = addr as usize;

        if addr < 640 * 1024 {
            Ok(self.conventional[addr])
        } else if addr < 1024 * 1024 {
            let uma_offset = addr - 640 * 1024;
            Ok(self.uma[uma_offset])
        } else if let Some(ref xms) = self.xms {
            let xms_offset = addr - 1024 * 1024;
            if xms_offset < xms.len() {
                Ok(xms[xms_offset])
            } else {
                Err(V86Error::InvalidMemory)
            }
        } else {
            Err(V86Error::InvalidMemory)
        }
    }

    /// Write byte to linear address
    pub fn write_u8(&mut self, addr: u32, val: u8) -> Result<(), V86Error> {
        let addr = addr as usize;

        if addr < 640 * 1024 {
            self.conventional[addr] = val;
            Ok(())
        } else if addr < 1024 * 1024 {
            let uma_offset = addr - 640 * 1024;
            // UMA is typically read-only (ROM), but we allow writes for now
            self.uma[uma_offset] = val;
            Ok(())
        } else if let Some(ref mut xms) = self.xms {
            let xms_offset = addr - 1024 * 1024;
            if xms_offset < xms.len() {
                xms[xms_offset] = val;
                Ok(())
            } else {
                Err(V86Error::InvalidMemory)
            }
        } else {
            Err(V86Error::InvalidMemory)
        }
    }

    /// Read word (16-bit) from linear address
    pub fn read_u16(&self, addr: u32) -> Result<u16, V86Error> {
        let lo = self.read_u8(addr)?;
        let hi = self.read_u8(addr + 1)?;
        Ok(u16::from_le_bytes([lo, hi]))
    }

    /// Write word (16-bit) to linear address
    pub fn write_u16(&mut self, addr: u32, val: u16) -> Result<(), V86Error> {
        let bytes = val.to_le_bytes();
        self.write_u8(addr, bytes[0])?;
        self.write_u8(addr + 1, bytes[1])?;
        Ok(())
    }

    /// Read dword (32-bit) from linear address
    pub fn read_u32(&self, addr: u32) -> Result<u32, V86Error> {
        let b0 = self.read_u8(addr)?;
        let b1 = self.read_u8(addr + 1)?;
        let b2 = self.read_u8(addr + 2)?;
        let b3 = self.read_u8(addr + 3)?;
        Ok(u32::from_le_bytes([b0, b1, b2, b3]))
    }

    /// Write dword (32-bit) to linear address
    pub fn write_u32(&mut self, addr: u32, val: u32) -> Result<(), V86Error> {
        let bytes = val.to_le_bytes();
        for i in 0..4 {
            self.write_u8(addr + i, bytes[i as usize])?;
        }
        Ok(())
    }

    /// Read bytes from linear address
    pub fn read_bytes(&self, addr: u32, len: usize) -> Result<Vec<u8>, V86Error> {
        let mut result = Vec::with_capacity(len);
        for i in 0..len {
            result.push(self.read_u8(addr + i as u32)?);
        }
        Ok(result)
    }

    /// Write bytes to linear address
    pub fn write_bytes(&mut self, addr: u32, data: &[u8]) -> Result<(), V86Error> {
        for (i, &byte) in data.iter().enumerate() {
            self.write_u8(addr + i as u32, byte)?;
        }
        Ok(())
    }

    /// Read string from linear address (null-terminated)
    pub fn read_string(&self, addr: u32, max_len: usize) -> Result<alloc::string::String, V86Error> {
        let mut result = alloc::string::String::new();
        for i in 0..max_len {
            let byte = self.read_u8(addr + i as u32)?;
            if byte == 0 {
                break;
            }
            result.push(byte as char);
        }
        Ok(result)
    }

    /// Read DOS-style string ($-terminated)
    pub fn read_dos_string(&self, addr: u32, max_len: usize) -> Result<alloc::string::String, V86Error> {
        let mut result = alloc::string::String::new();
        for i in 0..max_len {
            let byte = self.read_u8(addr + i as u32)?;
            if byte == b'$' {
                break;
            }
            result.push(byte as char);
        }
        Ok(result)
    }

    /// Get slice of conventional memory
    pub fn conventional_slice(&self, start: usize, len: usize) -> Option<&[u8]> {
        if start + len <= self.conventional.len() {
            Some(&self.conventional[start..start + len])
        } else {
            None
        }
    }

    /// Get mutable slice of conventional memory
    pub fn conventional_slice_mut(&mut self, start: usize, len: usize) -> Option<&mut [u8]> {
        if start + len <= self.conventional.len() {
            Some(&mut self.conventional[start..start + len])
        } else {
            None
        }
    }

    /// Get conventional memory size
    pub fn conventional_size(&self) -> usize {
        self.conventional.len()
    }

    /// Get XMS memory size
    pub fn xms_size(&self) -> usize {
        self.xms.as_ref().map(|x| x.len()).unwrap_or(0)
    }

    /// Set interrupt vector
    pub fn set_int_vector(&mut self, int_num: u8, segment: u16, offset: u16) -> Result<(), V86Error> {
        let addr = (int_num as u32) * 4;
        self.write_u16(addr, offset)?;
        self.write_u16(addr + 2, segment)?;
        Ok(())
    }

    /// Get interrupt vector
    pub fn get_int_vector(&self, int_num: u8) -> Result<(u16, u16), V86Error> {
        let addr = (int_num as u32) * 4;
        let offset = self.read_u16(addr)?;
        let segment = self.read_u16(addr + 2)?;
        Ok((segment, offset))
    }
}

impl Default for V86Memory {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory region types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegion {
    /// Interrupt vector table (0x0000-0x03FF)
    Ivt,
    /// BIOS data area (0x0400-0x04FF)
    Bda,
    /// Free conventional memory (0x0500-0x9FFFF)
    Conventional,
    /// Video memory (0xA0000-0xBFFFF)
    Video,
    /// ROM area (0xC0000-0xFFFFF)
    Rom,
    /// Extended memory (above 1MB)
    Extended,
}

impl MemoryRegion {
    /// Get region for linear address
    pub fn from_addr(addr: u32) -> Self {
        match addr {
            0x00000..=0x003FF => MemoryRegion::Ivt,
            0x00400..=0x004FF => MemoryRegion::Bda,
            0x00500..=0x9FFFF => MemoryRegion::Conventional,
            0xA0000..=0xBFFFF => MemoryRegion::Video,
            0xC0000..=0xFFFFF => MemoryRegion::Rom,
            _ => MemoryRegion::Extended,
        }
    }
}

/// Segment:offset pair
#[derive(Debug, Clone, Copy, Default)]
pub struct FarPtr {
    /// Segment
    pub segment: u16,
    /// Offset
    pub offset: u16,
}

impl FarPtr {
    /// Create new far pointer
    pub fn new(segment: u16, offset: u16) -> Self {
        FarPtr { segment, offset }
    }

    /// Convert to linear address
    pub fn to_linear(&self) -> u32 {
        ((self.segment as u32) << 4) + self.offset as u32
    }

    /// Create from linear address
    pub fn from_linear(addr: u32) -> Self {
        FarPtr {
            segment: (addr >> 4) as u16,
            offset: (addr & 0xF) as u16,
        }
    }
}
