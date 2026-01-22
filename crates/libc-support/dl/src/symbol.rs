//! Symbol table management

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Symbol binding type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolBinding {
    Local,
    Global,
    Weak,
}

/// Symbol type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolType {
    NoType,
    Object,
    Function,
    Section,
    File,
    Common,
    Tls,
}

/// Symbol information
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// Symbol name
    pub name: String,
    /// Offset from base address
    pub offset: usize,
    /// Symbol size
    pub size: usize,
    /// Symbol binding
    pub binding: SymbolBinding,
    /// Symbol type
    pub sym_type: SymbolType,
    /// Section index
    pub section: u16,
}

impl SymbolInfo {
    /// Create new symbol info
    pub fn new(
        name: String,
        offset: usize,
        size: usize,
        binding: SymbolBinding,
        sym_type: SymbolType,
        section: u16,
    ) -> Self {
        SymbolInfo {
            name,
            offset,
            size,
            binding,
            sym_type,
            section,
        }
    }

    /// Is this a defined symbol (not undefined)
    pub fn is_defined(&self) -> bool {
        self.section != 0 // SHN_UNDEF
    }

    /// Is this a global or weak symbol
    pub fn is_exported(&self) -> bool {
        matches!(self.binding, SymbolBinding::Global | SymbolBinding::Weak)
    }
}

/// Symbol table
pub struct SymbolTable {
    /// Symbols by name
    by_name: BTreeMap<String, SymbolInfo>,
    /// Symbols sorted by offset (for nearest symbol lookup)
    by_offset: Vec<(usize, String)>,
}

impl SymbolTable {
    /// Create empty symbol table
    pub fn new() -> Self {
        SymbolTable {
            by_name: BTreeMap::new(),
            by_offset: Vec::new(),
        }
    }

    /// Add a symbol
    pub fn add(&mut self, info: SymbolInfo) {
        let offset = info.offset;
        let name = info.name.clone();
        self.by_name.insert(info.name.clone(), info);
        self.by_offset.push((offset, name));
    }

    /// Rebuild offset index
    pub fn rebuild_offset_index(&mut self) {
        self.by_offset.sort_by_key(|(offset, _)| *offset);
    }

    /// Find symbol by name
    pub fn find(&self, name: &str) -> Option<&SymbolInfo> {
        self.by_name.get(name)
    }

    /// Find nearest symbol to an offset
    pub fn find_nearest(&self, offset: usize) -> Option<&SymbolInfo> {
        // Binary search for nearest symbol <= offset
        let idx = self.by_offset.partition_point(|(o, _)| *o <= offset);
        if idx == 0 {
            return None;
        }
        let (_, name) = &self.by_offset[idx - 1];
        self.by_name.get(name)
    }

    /// Iterate over all symbols
    pub fn iter(&self) -> impl Iterator<Item = &SymbolInfo> {
        self.by_name.values()
    }

    /// Get number of symbols
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Get all undefined symbols
    pub fn undefined(&self) -> impl Iterator<Item = &SymbolInfo> {
        self.by_name.values().filter(|s| !s.is_defined())
    }

    /// Get all exported symbols
    pub fn exported(&self) -> impl Iterator<Item = &SymbolInfo> {
        self.by_name
            .values()
            .filter(|s| s.is_exported() && s.is_defined())
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Global symbol resolution
pub struct SymbolResolver {
    /// Tables to search, in order
    tables: Vec<(usize, SymbolTable)>, // (base_addr, table)
}

impl SymbolResolver {
    /// Create new resolver
    pub fn new() -> Self {
        SymbolResolver { tables: Vec::new() }
    }

    /// Add a symbol table to the search
    pub fn add_table(&mut self, base_addr: usize, table: SymbolTable) {
        self.tables.push((base_addr, table));
    }

    /// Resolve a symbol name
    pub fn resolve(&self, name: &str) -> Option<usize> {
        // First pass: look for global symbols
        for (base, table) in &self.tables {
            if let Some(info) = table.find(name) {
                if info.binding == SymbolBinding::Global && info.is_defined() {
                    return Some(*base + info.offset);
                }
            }
        }

        // Second pass: look for weak symbols
        for (base, table) in &self.tables {
            if let Some(info) = table.find(name) {
                if info.binding == SymbolBinding::Weak && info.is_defined() {
                    return Some(*base + info.offset);
                }
            }
        }

        None
    }
}

impl Default for SymbolResolver {
    fn default() -> Self {
        Self::new()
    }
}
