//! # Terminfo Binary Format Parser
//!
//! Parses compiled terminfo database files.
//! Format specification from ncurses terminfo(5) man page.
//!
//! -- WireSaint: Binary format parser, efficient terminal database loading

use alloc::string::String;
use alloc::vec::Vec;
use crate::TerminalEntry;

/// Terminfo file header
#[repr(C)]
struct TerminfoHeader {
    magic: u16,              // 0x011A (legacy) or 0x021E (extended)
    names_size: u16,         // Size of terminal names section
    bool_count: u16,         // Number of boolean capabilities
    num_count: u16,          // Number of numeric capabilities
    str_count: u16,          // Number of string capabilities
    str_table_size: u16,     // Size of string table
}

/// Parse a terminfo binary file
///
/// Format:
/// ```text
/// [Header]
/// [Terminal Names] (null-terminated, | separated)
/// [Boolean Flags] (1 byte each)
/// [Padding if needed for alignment]
/// [Numbers] (2 bytes each, little-endian)
/// [String Offsets] (2 bytes each into string table)
/// [String Table] (null-terminated strings)
/// [Extended Section] (optional, for extended capabilities)
/// ```
pub fn parse_terminfo_binary(data: &[u8]) -> Result<TerminalEntry, &'static str> {
    if data.len() < 12 {
        return Err("File too small");
    }
    
    // Parse header
    let magic = u16::from_le_bytes([data[0], data[1]]);
    if magic != 0x011A && magic != 0x021E {
        return Err("Invalid terminfo magic number");
    }
    
    let names_size = u16::from_le_bytes([data[2], data[3]]) as usize;
    let bool_count = u16::from_le_bytes([data[4], data[5]]) as usize;
    let num_count = u16::from_le_bytes([data[6], data[7]]) as usize;
    let str_count = u16::from_le_bytes([data[8], data[9]]) as usize;
    let str_table_size = u16::from_le_bytes([data[10], data[11]]) as usize;
    
    let mut offset = 12;
    
    // Parse terminal names
    if offset + names_size > data.len() {
        return Err("Invalid names section size");
    }
    
    let names_bytes = &data[offset..offset + names_size];
    let names_str = core::str::from_utf8(names_bytes)
        .map_err(|_| "Invalid UTF-8 in names")?;
    
    let names: Vec<&str> = names_str.trim_end_matches('\0').split('|').collect();
    if names.is_empty() {
        return Err("No terminal name");
    }
    
    let mut entry = TerminalEntry::new(names[0]);
    for alias in &names[1..] {
        entry.aliases.push(alias.to_string());
    }
    
    offset += names_size;
    
    // Parse boolean capabilities
    if offset + bool_count > data.len() {
        return Err("Invalid boolean section");
    }
    
    for i in 0..bool_count {
        let value = data[offset + i];
        if value != 0 && i < BOOL_CAP_NAMES.len() {
            entry.set_flag(BOOL_CAP_NAMES[i], true);
        }
    }
    
    offset += bool_count;
    
    // Align to even boundary for numbers
    if offset % 2 != 0 {
        offset += 1;
    }
    
    // Parse numeric capabilities
    if offset + num_count * 2 > data.len() {
        return Err("Invalid numeric section");
    }
    
    for i in 0..num_count {
        let num_offset = offset + i * 2;
        let value = i16::from_le_bytes([data[num_offset], data[num_offset + 1]]);
        
        // -1 means not present, -2 means canceled
        if value >= 0 && i < NUM_CAP_NAMES.len() {
            entry.set_number(NUM_CAP_NAMES[i], value as i32);
        }
    }
    
    offset += num_count * 2;
    
    // Parse string capability offsets
    if offset + str_count * 2 > data.len() {
        return Err("Invalid string section");
    }
    
    let str_table_offset = offset + str_count * 2;
    
    if str_table_offset + str_table_size > data.len() {
        return Err("Invalid string table size");
    }
    
    let str_table = &data[str_table_offset..str_table_offset + str_table_size];
    
    for i in 0..str_count {
        let str_offset_pos = offset + i * 2;
        let str_offset = u16::from_le_bytes([
            data[str_offset_pos],
            data[str_offset_pos + 1],
        ]) as usize;
        
        // -1 (0xFFFF) means not present, -2 (0xFFFE) means canceled
        if str_offset < str_table_size && i < STR_CAP_NAMES.len() {
            // Find null terminator
            let str_start = str_offset;
            let mut str_end = str_start;
            while str_end < str_table.len() && str_table[str_end] != 0 {
                str_end += 1;
            }
            
            if let Ok(cap_str) = core::str::from_utf8(&str_table[str_start..str_end]) {
                entry.set_string(STR_CAP_NAMES[i], cap_str);
            }
        }
    }
    
    Ok(entry)
}

/// Standard boolean capability names (in terminfo order)
const BOOL_CAP_NAMES: &[&str] = &[
    "bw", "am", "xsb", "xhp", "xenl", "eo", "gn", "hc", "km", "hs",
    "in", "da", "db", "mir", "msgr", "os", "eslok", "xt", "hz", "ul",
    "xon", "nxon", "mc5i", "chts", "nrrmc", "npc", "ndscr", "ccc", "bce", "hls",
    "xhpa", "crxm", "daisy", "xvpa", "sam", "cpix", "lpix", "OTcr", "OTcu", "OTbs",
];

/// Standard numeric capability names (in terminfo order)
const NUM_CAP_NAMES: &[&str] = &[
    "cols", "it", "lines", "lm", "xmc", "pb", "vt", "wsl", "nlab", "lh",
    "lw", "ma", "wnum", "colors", "pairs", "ncv", "bufsz", "spinv", "spinh", "maddr",
    "mjump", "mcs", "mls", "npins", "orc", "orl", "orhi", "orvi", "cps", "widcs",
];

/// Standard string capability names (in terminfo order)
const STR_CAP_NAMES: &[&str] = &[
    "cbt", "bel", "cr", "csr", "tbc", "clear", "el", "ed", "hpa", "cmdch",
    "cup", "cud1", "home", "civis", "cub1", "mrcup", "cnorm", "cuf1", "ll", "cuu1",
    "cvvis", "dch1", "dl1", "dsl", "hd", "smacs", "blink", "bold", "smcup", "smdc",
    "dim", "smir", "invis", "prot", "rev", "smso", "smul", "ech", "rmacs", "sgr0",
    "rmcup", "rmdc", "rmir", "rmso", "rmul", "flash", "ff", "fsl", "is1", "is2",
    "is3", "if", "ich1", "il1", "ip", "kbs", "ktbc", "kclr", "kctab", "kdch1",
    "kdl1", "kcud1", "krmir", "kel", "ked", "kf0", "kf1", "kf10", "kf2", "kf3",
    "kf4", "kf5", "kf6", "kf7", "kf8", "kf9", "khome", "kich1", "kcub1", "kll",
    "knp", "kpp", "kcuf1", "kind", "kri", "khts", "kcuu1", "rmkx", "smkx", "lf0",
    "lf1", "lf10", "lf2", "lf3", "lf4", "lf5", "lf6", "lf7", "lf8", "lf9",
    "rmm", "smm", "nel", "pad", "dch", "dl", "cud", "ich", "indn", "il",
    "cub", "cuf", "rin", "cuu", "pfkey", "pfloc", "pfx", "mc0", "mc4", "mc5",
    "rep", "rs1", "rs2", "rs3", "rf", "rc", "vpa", "sc", "ind", "ri",
    "sgr", "hts", "wind", "ht", "tsl", "uc", "hu", "iprog", "ka1", "ka3",
    "kb2", "kc1", "kc3", "mc5p", "rmp", "acsc", "pln", "kcbt", "smxon", "rmxon",
    "smam", "rmam", "xonc", "xoffc", "enacs", "smln", "rmln", "kbeg", "kcan", "kclo",
    "kcmd", "kcpy", "kcrt", "kend", "kent", "kext", "kfnd", "khlp", "kmrk", "kmsg",
    "kmov", "knxt", "kopn", "kopt", "kprv", "kprt", "krdo", "kref", "krfr", "krpl",
    "krst", "kres", "ksav", "kspd", "kund", "kBEG", "kCAN", "kCMD", "kCPY", "kCRT",
    "kDC", "kDL", "kslt", "kEND", "kEOL", "kEXT", "kFND", "kHLP", "kHOM", "kIC",
    "kLFT", "kMSG", "kMOV", "kNXT", "kOPT", "kPRV", "kPRT", "kRDO", "kRPL", "kRIT",
    "kRES", "kSAV", "kSPD", "kUND", "rfi", "kf11", "kf12", "kf13", "kf14", "kf15",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_size() {
        assert_eq!(core::mem::size_of::<TerminfoHeader>(), 12);
    }

    #[test]
    fn test_cap_name_arrays() {
        assert!(!BOOL_CAP_NAMES.is_empty());
        assert!(!NUM_CAP_NAMES.is_empty());
        assert!(!STR_CAP_NAMES.is_empty());
    }
}
