//! Binary Format Detection and Registration for EFFLUX OS
//!
//! Provides binfmt_misc-like functionality for automatic interpreter selection.

#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use bitflags::bitflags;
use spin::RwLock;

bitflags! {
    /// Binary format flags
    #[derive(Debug, Clone, Copy)]
    pub struct BinfmtFlags: u32 {
        /// Preserve argv[0] (use script name, not interpreter)
        const PRESERVE_ARGV0 = 0x01;
        /// Open binary file and pass fd to interpreter
        const OPEN_BINARY = 0x02;
        /// Use credentials from binary, not interpreter
        const CREDENTIALS = 0x04;
        /// Fix interpreter path at registration time
        const FIX_BINARY = 0x08;
    }
}

/// Binary format entry
#[derive(Debug, Clone)]
pub struct BinfmtEntry {
    /// Entry name
    pub name: String,
    /// Magic bytes to match at offset
    pub magic: Option<Vec<u8>>,
    /// Mask for magic bytes (optional)
    pub mask: Option<Vec<u8>>,
    /// Offset in file for magic
    pub offset: usize,
    /// File extension to match (without dot)
    pub extension: Option<String>,
    /// Path to interpreter
    pub interpreter: String,
    /// Flags
    pub flags: BinfmtFlags,
    /// Enabled
    pub enabled: bool,
}

impl BinfmtEntry {
    /// Create new entry with magic bytes
    pub fn magic(name: &str, magic: &[u8], interpreter: &str) -> Self {
        BinfmtEntry {
            name: String::from(name),
            magic: Some(magic.to_vec()),
            mask: None,
            offset: 0,
            extension: None,
            interpreter: String::from(interpreter),
            flags: BinfmtFlags::empty(),
            enabled: true,
        }
    }

    /// Create new entry with extension
    pub fn extension(name: &str, ext: &str, interpreter: &str) -> Self {
        BinfmtEntry {
            name: String::from(name),
            magic: None,
            mask: None,
            offset: 0,
            extension: Some(String::from(ext)),
            interpreter: String::from(interpreter),
            flags: BinfmtFlags::empty(),
            enabled: true,
        }
    }

    /// Set magic mask
    pub fn with_mask(mut self, mask: &[u8]) -> Self {
        self.mask = Some(mask.to_vec());
        self
    }

    /// Set magic offset
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// Set flags
    pub fn with_flags(mut self, flags: BinfmtFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Check if file matches this entry
    pub fn matches(&self, filename: &str, header: &[u8]) -> bool {
        if !self.enabled {
            return false;
        }

        // Check extension
        if let Some(ref ext) = self.extension {
            if let Some(dot_pos) = filename.rfind('.') {
                let file_ext = &filename[dot_pos + 1..];
                if file_ext.eq_ignore_ascii_case(ext) {
                    return true;
                }
            }
        }

        // Check magic
        if let Some(ref magic) = self.magic {
            if header.len() < self.offset + magic.len() {
                return false;
            }

            let file_magic = &header[self.offset..self.offset + magic.len()];

            if let Some(ref mask) = self.mask {
                // Apply mask and compare
                for i in 0..magic.len() {
                    let masked_file = if i < mask.len() {
                        file_magic[i] & mask[i]
                    } else {
                        file_magic[i]
                    };
                    let masked_magic = if i < mask.len() {
                        magic[i] & mask[i]
                    } else {
                        magic[i]
                    };
                    if masked_file != masked_magic {
                        return false;
                    }
                }
                return true;
            } else {
                // Direct comparison
                return file_magic == magic.as_slice();
            }
        }

        false
    }
}

/// Binary format registry
pub struct BinfmtRegistry {
    /// Registered formats
    entries: RwLock<BTreeMap<String, BinfmtEntry>>,
}

impl BinfmtRegistry {
    /// Create new registry
    pub fn new() -> Self {
        BinfmtRegistry {
            entries: RwLock::new(BTreeMap::new()),
        }
    }

    /// Create registry with default formats
    pub fn with_defaults() -> Self {
        let registry = Self::new();

        // DOS executables
        registry.register(
            BinfmtEntry::magic("dos-com", &[], "/usr/bin/dosbox")
                .with_flags(BinfmtFlags::PRESERVE_ARGV0)
        );
        registry.register(
            BinfmtEntry::extension("dos-com-ext", "com", "/usr/bin/dosbox")
                .with_flags(BinfmtFlags::PRESERVE_ARGV0)
        );
        registry.register(
            BinfmtEntry::magic("dos-exe", b"MZ", "/usr/bin/dosbox")
                .with_flags(BinfmtFlags::PRESERVE_ARGV0)
        );

        // Python scripts
        registry.register(
            BinfmtEntry::extension("python", "py", "/usr/bin/python-sandbox")
        );
        registry.register(
            BinfmtEntry::magic("python-shebang", b"#!/usr/bin/python", "/usr/bin/python-sandbox")
        );

        // Shell scripts
        registry.register(
            BinfmtEntry::magic("bash", b"#!/bin/bash", "/bin/bash")
        );
        registry.register(
            BinfmtEntry::magic("sh", b"#!/bin/sh", "/bin/sh")
        );

        // Java bytecode
        registry.register(
            BinfmtEntry::magic("java", &[0xCA, 0xFE, 0xBA, 0xBE], "/usr/bin/java")
                .with_flags(BinfmtFlags::PRESERVE_ARGV0)
        );
        registry.register(
            BinfmtEntry::extension("jar", "jar", "/usr/bin/java")
                .with_flags(BinfmtFlags::PRESERVE_ARGV0)
        );

        // WebAssembly
        registry.register(
            BinfmtEntry::magic("wasm", &[0x00, 0x61, 0x73, 0x6D], "/usr/bin/wasmtime")
        );

        registry
    }

    /// Register new format
    pub fn register(&self, entry: BinfmtEntry) {
        self.entries.write().insert(entry.name.clone(), entry);
    }

    /// Unregister format
    pub fn unregister(&self, name: &str) -> Option<BinfmtEntry> {
        self.entries.write().remove(name)
    }

    /// Enable format
    pub fn enable(&self, name: &str) -> bool {
        if let Some(entry) = self.entries.write().get_mut(name) {
            entry.enabled = true;
            true
        } else {
            false
        }
    }

    /// Disable format
    pub fn disable(&self, name: &str) -> bool {
        if let Some(entry) = self.entries.write().get_mut(name) {
            entry.enabled = false;
            true
        } else {
            false
        }
    }

    /// Find interpreter for file
    pub fn find_interpreter(&self, filename: &str, header: &[u8]) -> Option<BinfmtEntry> {
        let entries = self.entries.read();

        for entry in entries.values() {
            if entry.matches(filename, header) {
                return Some(entry.clone());
            }
        }

        None
    }

    /// List all registered formats
    pub fn list(&self) -> Vec<BinfmtEntry> {
        self.entries.read().values().cloned().collect()
    }

    /// Get format by name
    pub fn get(&self, name: &str) -> Option<BinfmtEntry> {
        self.entries.read().get(name).cloned()
    }
}

impl Default for BinfmtRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Detect file type from header
pub fn detect_file_type(header: &[u8]) -> FileType {
    if header.len() < 4 {
        return FileType::Unknown;
    }

    // ELF
    if header.len() >= 4 && &header[0..4] == b"\x7FELF" {
        if header.len() >= 5 {
            return match header[4] {
                1 => FileType::Elf32,
                2 => FileType::Elf64,
                _ => FileType::Unknown,
            };
        }
    }

    // DOS MZ executable
    if header.len() >= 2 && &header[0..2] == b"MZ" {
        // Check for PE header (Windows)
        if header.len() >= 64 {
            let pe_offset = u32::from_le_bytes([
                header[60], header[61], header[62], header[63]
            ]) as usize;
            if header.len() > pe_offset + 4 {
                if &header[pe_offset..pe_offset + 4] == b"PE\x00\x00" {
                    return FileType::Pe;
                }
            }
        }
        return FileType::DosMz;
    }

    // DOS COM (no header, usually starts with specific opcodes)
    // This is heuristic-based

    // Script shebangs
    if header.len() >= 2 && &header[0..2] == b"#!" {
        return FileType::Script;
    }

    // Java class file
    if header.len() >= 4 && &header[0..4] == &[0xCA, 0xFE, 0xBA, 0xBE] {
        return FileType::JavaClass;
    }

    // WebAssembly
    if header.len() >= 4 && &header[0..4] == &[0x00, 0x61, 0x73, 0x6D] {
        return FileType::Wasm;
    }

    // Mach-O (macOS/iOS)
    if header.len() >= 4 {
        match &header[0..4] {
            &[0xFE, 0xED, 0xFA, 0xCE] => return FileType::MachO32,
            &[0xFE, 0xED, 0xFA, 0xCF] => return FileType::MachO64,
            &[0xCE, 0xFA, 0xED, 0xFE] => return FileType::MachO32, // little-endian
            &[0xCF, 0xFA, 0xED, 0xFE] => return FileType::MachO64, // little-endian
            &[0xCA, 0xFE, 0xBA, 0xBE] => return FileType::JavaClass, // or universal binary
            _ => {}
        }
    }

    FileType::Unknown
}

/// Detected file type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// 32-bit ELF
    Elf32,
    /// 64-bit ELF
    Elf64,
    /// DOS MZ executable
    DosMz,
    /// DOS COM executable
    DosCom,
    /// Windows PE executable
    Pe,
    /// Script (shebang)
    Script,
    /// Java class file
    JavaClass,
    /// WebAssembly module
    Wasm,
    /// 32-bit Mach-O
    MachO32,
    /// 64-bit Mach-O
    MachO64,
    /// Unknown format
    Unknown,
}

impl FileType {
    /// Check if this is a native executable for current platform
    pub fn is_native(&self) -> bool {
        matches!(self, FileType::Elf64) // For x86_64
    }

    /// Check if this needs an interpreter
    pub fn needs_interpreter(&self) -> bool {
        !self.is_native() && *self != FileType::Unknown
    }

    /// Get suggested interpreter
    pub fn suggested_interpreter(&self) -> Option<&'static str> {
        match self {
            FileType::DosMz | FileType::DosCom => Some("/usr/bin/dosbox"),
            FileType::Script => None, // Interpreter is in shebang
            FileType::JavaClass => Some("/usr/bin/java"),
            FileType::Wasm => Some("/usr/bin/wasmtime"),
            FileType::Elf32 => Some("/lib/ld-linux.so.2"),
            _ => None,
        }
    }
}

/// Syscall translation layer types
pub mod syscall_compat {
    use alloc::collections::BTreeMap;

    /// Linux syscall numbers (x86_64)
    pub mod linux_x86_64 {
        pub const SYS_READ: u64 = 0;
        pub const SYS_WRITE: u64 = 1;
        pub const SYS_OPEN: u64 = 2;
        pub const SYS_CLOSE: u64 = 3;
        pub const SYS_STAT: u64 = 4;
        pub const SYS_FSTAT: u64 = 5;
        pub const SYS_LSTAT: u64 = 6;
        pub const SYS_POLL: u64 = 7;
        pub const SYS_LSEEK: u64 = 8;
        pub const SYS_MMAP: u64 = 9;
        pub const SYS_MPROTECT: u64 = 10;
        pub const SYS_MUNMAP: u64 = 11;
        pub const SYS_BRK: u64 = 12;
        pub const SYS_EXIT: u64 = 60;
        pub const SYS_EXIT_GROUP: u64 = 231;
    }

    /// Syscall translator
    pub struct SyscallTranslator {
        /// Syscall mapping
        map: BTreeMap<u64, u64>,
    }

    impl SyscallTranslator {
        /// Create new translator
        pub fn new() -> Self {
            let mut map = BTreeMap::new();

            // Map Linux syscalls to EFFLUX syscalls
            // These would be actual EFFLUX syscall numbers
            map.insert(linux_x86_64::SYS_READ, 0);
            map.insert(linux_x86_64::SYS_WRITE, 1);
            map.insert(linux_x86_64::SYS_OPEN, 2);
            map.insert(linux_x86_64::SYS_CLOSE, 3);
            map.insert(linux_x86_64::SYS_EXIT, 60);
            map.insert(linux_x86_64::SYS_EXIT_GROUP, 60);

            SyscallTranslator { map }
        }

        /// Translate syscall number
        pub fn translate(&self, linux_num: u64) -> Option<u64> {
            self.map.get(&linux_num).copied()
        }

        /// Register custom mapping
        pub fn register(&mut self, linux_num: u64, num: u64) {
            self.map.insert(linux_num, num);
        }
    }

    impl Default for SyscallTranslator {
        fn default() -> Self {
            Self::new()
        }
    }
}
