//! DOS service emulation (INT 21h)

use alloc::string::String;
use alloc::vec::Vec;
use crate::{V86Context, V86Error};

/// DOS version to report
pub const DOS_VERSION_MAJOR: u8 = 5;
pub const DOS_VERSION_MINOR: u8 = 0;

/// DOS file handle
#[derive(Debug, Clone)]
pub struct DosFileHandle {
    /// Handle number
    pub handle: u16,
    /// File path
    pub path: String,
    /// Current position
    pub position: u32,
    /// File size
    pub size: u32,
    /// Open mode
    pub mode: u8,
}

/// DOS file system callback
pub trait DosFileSystem {
    /// Create file
    fn create(&mut self, path: &str) -> Result<u16, DosError>;
    /// Open file
    fn open(&mut self, path: &str, mode: u8) -> Result<u16, DosError>;
    /// Close file
    fn close(&mut self, handle: u16) -> Result<(), DosError>;
    /// Read file
    fn read(&mut self, handle: u16, buffer: &mut [u8]) -> Result<usize, DosError>;
    /// Write file
    fn write(&mut self, handle: u16, buffer: &[u8]) -> Result<usize, DosError>;
    /// Seek file
    fn seek(&mut self, handle: u16, offset: i32, whence: u8) -> Result<u32, DosError>;
    /// Delete file
    fn delete(&mut self, path: &str) -> Result<(), DosError>;
    /// Get/set file attributes
    fn get_attr(&self, path: &str) -> Result<u8, DosError>;
    fn set_attr(&mut self, path: &str, attr: u8) -> Result<(), DosError>;
}

/// DOS error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DosError {
    /// Invalid function
    InvalidFunction = 0x01,
    /// File not found
    FileNotFound = 0x02,
    /// Path not found
    PathNotFound = 0x03,
    /// Too many open files
    TooManyOpenFiles = 0x04,
    /// Access denied
    AccessDenied = 0x05,
    /// Invalid handle
    InvalidHandle = 0x06,
    /// Insufficient memory
    InsufficientMemory = 0x08,
    /// Invalid drive
    InvalidDrive = 0x0F,
    /// No more files
    NoMoreFiles = 0x12,
    /// Write protect error
    WriteProtect = 0x13,
}

/// DOS services handler
pub struct DosServices {
    /// Standard file handles
    handles: Vec<Option<DosFileHandle>>,
    /// Current directory
    current_dir: String,
    /// Current drive (0 = A:, 1 = B:, 2 = C:, etc.)
    current_drive: u8,
    /// DTA (Disk Transfer Area) address
    dta_segment: u16,
    dta_offset: u16,
    /// PSP segment
    psp_segment: u16,
    /// Console output callback
    console_out: Option<fn(char)>,
    /// Console input callback
    console_in: Option<fn() -> Option<char>>,
}

impl DosServices {
    /// Create new DOS services
    pub fn new() -> Self {
        let mut handles = Vec::with_capacity(20);

        // Standard handles
        handles.push(Some(DosFileHandle {
            handle: 0,
            path: String::from("STDIN"),
            position: 0,
            size: 0,
            mode: 0,
        }));
        handles.push(Some(DosFileHandle {
            handle: 1,
            path: String::from("STDOUT"),
            position: 0,
            size: 0,
            mode: 1,
        }));
        handles.push(Some(DosFileHandle {
            handle: 2,
            path: String::from("STDERR"),
            position: 0,
            size: 0,
            mode: 1,
        }));
        handles.push(Some(DosFileHandle {
            handle: 3,
            path: String::from("STDAUX"),
            position: 0,
            size: 0,
            mode: 2,
        }));
        handles.push(Some(DosFileHandle {
            handle: 4,
            path: String::from("STDPRN"),
            position: 0,
            size: 0,
            mode: 1,
        }));

        DosServices {
            handles,
            current_dir: String::from("\\"),
            current_drive: 2, // C:
            dta_segment: 0,
            dta_offset: 0x80,
            psp_segment: 0,
            console_out: None,
            console_in: None,
        }
    }

    /// Set console callbacks
    pub fn set_console(&mut self, out: fn(char), inp: fn() -> Option<char>) {
        self.console_out = Some(out);
        self.console_in = Some(inp);
    }

    /// Set PSP segment
    pub fn set_psp(&mut self, segment: u16) {
        self.psp_segment = segment;
        self.dta_segment = segment;
    }

    /// Handle INT 21h
    pub fn handle_int21(&mut self, ctx: &mut V86Context) -> Result<bool, V86Error> {
        let ah = ctx.regs.ah();

        match ah {
            // Terminate program
            0x00 => {
                ctx.exit_code = Some(0);
                Ok(true)
            }

            // Read character with echo
            0x01 => {
                if let Some(inp) = self.console_in {
                    if let Some(ch) = inp() {
                        ctx.regs.set_al(ch as u8);
                        if let Some(out) = self.console_out {
                            out(ch);
                        }
                    }
                }
                Ok(false)
            }

            // Write character
            0x02 => {
                let ch = ctx.regs.dl() as char;
                if let Some(out) = self.console_out {
                    out(ch);
                }
                Ok(false)
            }

            // Read character (no echo)
            0x08 => {
                if let Some(inp) = self.console_in {
                    if let Some(ch) = inp() {
                        ctx.regs.set_al(ch as u8);
                    }
                }
                Ok(false)
            }

            // Write string ($-terminated)
            0x09 => {
                let addr = ctx.linear_addr(ctx.segments.ds, ctx.regs.dx());
                let string = ctx.memory.read_dos_string(addr, 256)?;
                if let Some(out) = self.console_out {
                    for ch in string.chars() {
                        out(ch);
                    }
                }
                Ok(false)
            }

            // Buffered input
            0x0A => {
                let addr = ctx.linear_addr(ctx.segments.ds, ctx.regs.dx());
                let max_len = ctx.memory.read_u8(addr)?;
                let mut count = 0u8;

                if let Some(inp) = self.console_in {
                    for _ in 0..max_len {
                        if let Some(ch) = inp() {
                            if ch == '\r' || ch == '\n' {
                                break;
                            }
                            ctx.memory.write_u8(addr + 2 + count as u32, ch as u8)?;
                            count += 1;
                            if let Some(out) = self.console_out {
                                out(ch);
                            }
                        } else {
                            break;
                        }
                    }
                }

                ctx.memory.write_u8(addr + 1, count)?;
                Ok(false)
            }

            // Set interrupt vector
            0x25 => {
                let int_num = ctx.regs.al();
                ctx.memory.set_int_vector(int_num, ctx.segments.ds, ctx.regs.dx())?;
                Ok(false)
            }

            // Get DOS version
            0x30 => {
                ctx.regs.set_al(DOS_VERSION_MAJOR);
                ctx.regs.set_ah(DOS_VERSION_MINOR);
                ctx.regs.ebx = 0; // OEM serial
                ctx.regs.ecx = 0; // 24-bit user number
                Ok(false)
            }

            // Get interrupt vector
            0x35 => {
                let int_num = ctx.regs.al();
                let (seg, off) = ctx.memory.get_int_vector(int_num)?;
                ctx.segments.es = seg;
                ctx.regs.ebx = off as u32;
                Ok(false)
            }

            // Get current drive
            0x19 => {
                ctx.regs.set_al(self.current_drive);
                Ok(false)
            }

            // Set current drive
            0x0E => {
                self.current_drive = ctx.regs.dl();
                ctx.regs.set_al(26); // 26 drives available
                Ok(false)
            }

            // Set DTA address
            0x1A => {
                self.dta_segment = ctx.segments.ds;
                self.dta_offset = ctx.regs.dx();
                Ok(false)
            }

            // Get DTA address
            0x2F => {
                ctx.segments.es = self.dta_segment;
                ctx.regs.ebx = self.dta_offset as u32;
                Ok(false)
            }

            // Create file
            0x3C => {
                // File operations would go through DosFileSystem trait
                ctx.regs.set_ax(5); // Return handle 5
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Open file
            0x3D => {
                ctx.regs.set_ax(5);
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Close file
            0x3E => {
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Read file
            0x3F => {
                let handle = ctx.regs.bx();
                let count = ctx.regs.cx();

                if handle == 0 {
                    // STDIN
                    let mut bytes_read = 0u16;
                    let addr = ctx.linear_addr(ctx.segments.ds, ctx.regs.dx());

                    if let Some(inp) = self.console_in {
                        for i in 0..count {
                            if let Some(ch) = inp() {
                                ctx.memory.write_u8(addr + i as u32, ch as u8)?;
                                bytes_read += 1;
                                if ch == '\r' || ch == '\n' {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }

                    ctx.regs.set_ax(bytes_read);
                } else {
                    ctx.regs.set_ax(0);
                }
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Write file
            0x40 => {
                let handle = ctx.regs.bx();
                let count = ctx.regs.cx();
                let addr = ctx.linear_addr(ctx.segments.ds, ctx.regs.dx());

                if handle == 1 || handle == 2 {
                    // STDOUT or STDERR
                    if let Some(out) = self.console_out {
                        for i in 0..count {
                            let byte = ctx.memory.read_u8(addr + i as u32)?;
                            out(byte as char);
                        }
                    }
                    ctx.regs.set_ax(count);
                } else {
                    ctx.regs.set_ax(count);
                }
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Delete file
            0x41 => {
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Seek file
            0x42 => {
                // AL = origin (0=start, 1=current, 2=end)
                ctx.regs.edx = 0; // new position (high word)
                ctx.regs.set_ax(0); // new position (low word)
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Get/Set file attributes
            0x43 => {
                let op = ctx.regs.al();
                if op == 0 {
                    // Get attributes
                    ctx.regs.ecx = 0x20; // Archive attribute
                }
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Get PSP address
            0x51 | 0x62 => {
                ctx.regs.ebx = self.psp_segment as u32;
                Ok(false)
            }

            // Exit with return code
            0x4C => {
                ctx.exit_code = Some(ctx.regs.al());
                Ok(true)
            }

            // Get free disk space
            0x36 => {
                ctx.regs.set_ax(0xFFFF); // sectors per cluster
                ctx.regs.ebx = 0xFFFF; // available clusters
                ctx.regs.ecx = 512; // bytes per sector
                ctx.regs.edx = 0xFFFF; // total clusters
                Ok(false)
            }

            // Get/Set country info
            0x38 => {
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Get current directory
            0x47 => {
                let addr = ctx.linear_addr(ctx.segments.ds, ctx.regs.si() as u16);
                let dir_bytes = self.current_dir.as_bytes();
                ctx.memory.write_bytes(addr, dir_bytes)?;
                ctx.memory.write_u8(addr + dir_bytes.len() as u32, 0)?;
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Allocate memory
            0x48 => {
                // Would allocate from DOS memory
                ctx.regs.set_ax(0x1000); // Return segment
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Free memory
            0x49 => {
                ctx.regs.set_carry(false);
                Ok(false)
            }

            // Resize memory block
            0x4A => {
                ctx.regs.set_carry(false);
                Ok(false)
            }

            _ => {
                // Unknown function - set carry flag
                ctx.regs.set_carry(true);
                Ok(false)
            }
        }
    }
}

impl Default for DosServices {
    fn default() -> Self {
        Self::new()
    }
}

/// Load COM file into memory
pub fn load_com(ctx: &mut V86Context, data: &[u8], load_segment: u16) -> Result<(), V86Error> {
    if data.len() > 0xFFF0 {
        return Err(V86Error::InvalidExecutable);
    }

    // Load at segment:0100
    let load_addr = ((load_segment as u32) << 4) + 0x100;
    ctx.memory.write_bytes(load_addr, data)?;

    // Set up segments
    ctx.segments.set_all(load_segment);

    // Set up registers
    ctx.regs.set_ip(0x100);
    ctx.regs.set_sp(0xFFFE);

    // Set PSP segment
    ctx.psp_segment = load_segment;

    Ok(())
}

/// Load EXE file into memory
pub fn load_exe(ctx: &mut V86Context, data: &[u8], load_segment: u16) -> Result<(), V86Error> {
    if data.len() < 28 {
        return Err(V86Error::InvalidExecutable);
    }

    // Check MZ signature
    if data[0] != b'M' || data[1] != b'Z' {
        return Err(V86Error::InvalidExecutable);
    }

    // Parse EXE header
    let last_page_size = u16::from_le_bytes([data[2], data[3]]);
    let pages = u16::from_le_bytes([data[4], data[5]]);
    let reloc_count = u16::from_le_bytes([data[6], data[7]]);
    let header_paragraphs = u16::from_le_bytes([data[8], data[9]]);
    let _min_extra = u16::from_le_bytes([data[10], data[11]]);
    let _max_extra = u16::from_le_bytes([data[12], data[13]]);
    let init_ss = u16::from_le_bytes([data[14], data[15]]);
    let init_sp = u16::from_le_bytes([data[16], data[17]]);
    let _checksum = u16::from_le_bytes([data[18], data[19]]);
    let init_ip = u16::from_le_bytes([data[20], data[21]]);
    let init_cs = u16::from_le_bytes([data[22], data[23]]);
    let reloc_offset = u16::from_le_bytes([data[24], data[25]]) as usize;

    // Calculate sizes
    let header_size = (header_paragraphs as usize) * 16;
    let exe_size = if last_page_size == 0 {
        (pages as usize) * 512
    } else {
        ((pages as usize) - 1) * 512 + last_page_size as usize
    };
    let image_size = exe_size - header_size;

    if header_size + image_size > data.len() {
        return Err(V86Error::InvalidExecutable);
    }

    // Load image at load_segment
    let image_data = &data[header_size..header_size + image_size];
    let load_addr = (load_segment as u32) << 4;
    ctx.memory.write_bytes(load_addr, image_data)?;

    // Apply relocations
    for i in 0..reloc_count as usize {
        let reloc_entry = reloc_offset + i * 4;
        if reloc_entry + 4 > data.len() {
            break;
        }

        let offset = u16::from_le_bytes([data[reloc_entry], data[reloc_entry + 1]]);
        let segment = u16::from_le_bytes([data[reloc_entry + 2], data[reloc_entry + 3]]);

        let reloc_addr = load_addr + ((segment as u32) << 4) + offset as u32;
        let current_val = ctx.memory.read_u16(reloc_addr)?;
        ctx.memory.write_u16(reloc_addr, current_val.wrapping_add(load_segment))?;
    }

    // Set up segments
    ctx.segments.cs = load_segment.wrapping_add(init_cs);
    ctx.segments.ss = load_segment.wrapping_add(init_ss);
    ctx.segments.ds = load_segment;
    ctx.segments.es = load_segment;

    // Set up registers
    ctx.regs.set_ip(init_ip);
    ctx.regs.set_sp(init_sp);

    // Set PSP segment
    ctx.psp_segment = load_segment;

    Ok(())
}
