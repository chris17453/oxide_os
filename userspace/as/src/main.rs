//! EFFLUX Assembler (as)
//!
//! A minimal x86_64 assembler for EFFLUX OS.
//! Supports AT&T syntax and generates ELF64 relocatable object files.

#![no_std]
#![no_main]

use libc::*;

/// Maximum source file size (64KB)
const MAX_SOURCE: usize = 65536;

/// Maximum symbols
const MAX_SYMBOLS: usize = 256;

/// Maximum relocations
const MAX_RELOCS: usize = 512;

/// Maximum output size (64KB)
const MAX_OUTPUT: usize = 65536;

/// Symbol entry
#[derive(Clone, Copy)]
struct Symbol {
    name: [u8; 64],
    value: u64,
    section: u8,      // 0=undefined, 1=text, 2=data, 3=bss
    binding: u8,      // 0=local, 1=global
    defined: bool,
}

impl Symbol {
    const fn new() -> Self {
        Symbol {
            name: [0u8; 64],
            value: 0,
            section: 0,
            binding: 0,
            defined: false,
        }
    }
}

/// Relocation entry
#[derive(Clone, Copy)]
struct Reloc {
    offset: u64,
    sym_idx: usize,
    rtype: u32,       // R_X86_64_* type
    addend: i64,
    section: u8,      // Which section the relocation is in
}

impl Reloc {
    const fn new() -> Self {
        Reloc {
            offset: 0,
            sym_idx: 0,
            rtype: 0,
            addend: 0,
            section: 0,
        }
    }
}

/// Assembler state
struct Assembler {
    /// Source buffer
    source: [u8; MAX_SOURCE],
    source_len: usize,

    /// Text section (code)
    text: [u8; MAX_OUTPUT],
    text_len: usize,

    /// Data section
    data: [u8; MAX_OUTPUT],
    data_len: usize,

    /// BSS section size (no content)
    bss_len: usize,

    /// Symbol table
    symbols: [Symbol; MAX_SYMBOLS],
    num_symbols: usize,

    /// Relocations
    relocs: [Reloc; MAX_RELOCS],
    num_relocs: usize,

    /// Current section: 1=text, 2=data, 3=bss
    current_section: u8,

    /// Current line number for error reporting
    line_num: usize,

    /// Error flag
    had_error: bool,
}

impl Assembler {
    const fn new() -> Self {
        const EMPTY_SYM: Symbol = Symbol::new();
        const EMPTY_RELOC: Reloc = Reloc::new();

        Assembler {
            source: [0u8; MAX_SOURCE],
            source_len: 0,
            text: [0u8; MAX_OUTPUT],
            text_len: 0,
            data: [0u8; MAX_OUTPUT],
            data_len: 0,
            bss_len: 0,
            symbols: [EMPTY_SYM; MAX_SYMBOLS],
            num_symbols: 0,
            relocs: [EMPTY_RELOC; MAX_RELOCS],
            num_relocs: 0,
            current_section: 1, // Start in .text
            line_num: 1,
            had_error: false,
        }
    }

    /// Get current section output pointer and length
    fn current_output(&mut self) -> (*mut u8, &mut usize) {
        match self.current_section {
            1 => (self.text.as_mut_ptr(), &mut self.text_len),
            2 => (self.data.as_mut_ptr(), &mut self.data_len),
            _ => (self.data.as_mut_ptr(), &mut self.data_len),
        }
    }

    /// Get current section offset
    fn current_offset(&self) -> usize {
        match self.current_section {
            1 => self.text_len,
            2 => self.data_len,
            3 => self.bss_len,
            _ => 0,
        }
    }

    /// Emit a byte to current section
    fn emit_byte(&mut self, b: u8) {
        let (ptr, len) = self.current_output();
        if *len < MAX_OUTPUT {
            unsafe { *ptr.add(*len) = b; }
            *len += 1;
        }
    }

    /// Emit bytes to current section
    fn emit_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.emit_byte(b);
        }
    }

    /// Emit a little-endian 16-bit value
    fn emit_u16(&mut self, v: u16) {
        self.emit_byte(v as u8);
        self.emit_byte((v >> 8) as u8);
    }

    /// Emit a little-endian 32-bit value
    fn emit_u32(&mut self, v: u32) {
        self.emit_byte(v as u8);
        self.emit_byte((v >> 8) as u8);
        self.emit_byte((v >> 16) as u8);
        self.emit_byte((v >> 24) as u8);
    }

    /// Emit a little-endian 64-bit value
    fn emit_u64(&mut self, v: u64) {
        self.emit_u32(v as u32);
        self.emit_u32((v >> 32) as u32);
    }

    /// Report error
    fn error(&mut self, msg: &str) {
        eprints("as: line ");
        print_u64(self.line_num as u64);
        eprints(": ");
        eprintlns(msg);
        self.had_error = true;
    }

    /// Add or find a symbol
    fn add_symbol(&mut self, name: &[u8], value: u64, section: u8, binding: u8, defined: bool) -> usize {
        // Check if symbol already exists
        for i in 0..self.num_symbols {
            if bytes_eq_len(&self.symbols[i].name, name) {
                if defined {
                    if self.symbols[i].defined {
                        self.error("duplicate symbol definition");
                        return i;
                    }
                    self.symbols[i].value = value;
                    self.symbols[i].section = section;
                    self.symbols[i].binding = binding;
                    self.symbols[i].defined = true;
                }
                return i;
            }
        }

        // Add new symbol
        if self.num_symbols >= MAX_SYMBOLS {
            self.error("too many symbols");
            return 0;
        }

        let idx = self.num_symbols;
        copy_bytes(&mut self.symbols[idx].name, name);
        self.symbols[idx].value = value;
        self.symbols[idx].section = section;
        self.symbols[idx].binding = binding;
        self.symbols[idx].defined = defined;
        self.num_symbols += 1;
        idx
    }

    /// Add a relocation
    fn add_reloc(&mut self, sym_idx: usize, rtype: u32, addend: i64) {
        if self.num_relocs >= MAX_RELOCS {
            self.error("too many relocations");
            return;
        }

        let idx = self.num_relocs;
        self.relocs[idx].offset = self.current_offset() as u64;
        self.relocs[idx].sym_idx = sym_idx;
        self.relocs[idx].rtype = rtype;
        self.relocs[idx].addend = addend;
        self.relocs[idx].section = self.current_section;
        self.num_relocs += 1;
    }

    /// Parse the source file
    fn parse(&mut self) {
        let mut pos = 0;

        while pos < self.source_len {
            // Skip whitespace
            while pos < self.source_len && (self.source[pos] == b' ' || self.source[pos] == b'\t') {
                pos += 1;
            }

            // Skip empty lines and comments
            if pos >= self.source_len || self.source[pos] == b'\n' {
                if pos < self.source_len {
                    pos += 1;
                    self.line_num += 1;
                }
                continue;
            }

            // Skip comments
            if self.source[pos] == b'#' || self.source[pos] == b';' {
                while pos < self.source_len && self.source[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }

            // Parse line
            pos = self.parse_line(pos);
        }
    }

    /// Parse a single line
    fn parse_line(&mut self, mut pos: usize) -> usize {
        // Get first token
        let (token, new_pos) = self.get_token(pos);
        pos = new_pos;

        if token.is_empty() {
            return self.skip_to_eol(pos);
        }

        // Check for label (ends with :)
        if token[token.len() - 1] == b':' {
            // Define label
            let name = &token[..token.len() - 1];
            self.add_symbol(name, self.current_offset() as u64, self.current_section, 0, true);

            // Continue parsing rest of line
            while pos < self.source_len && (self.source[pos] == b' ' || self.source[pos] == b'\t') {
                pos += 1;
            }

            if pos >= self.source_len || self.source[pos] == b'\n' || self.source[pos] == b'#' {
                return self.skip_to_eol(pos);
            }

            let (token, new_pos) = self.get_token(pos);
            pos = new_pos;

            if token.is_empty() {
                return self.skip_to_eol(pos);
            }

            return self.parse_instruction(&token, pos);
        }

        // Check for directive
        if token[0] == b'.' {
            return self.parse_directive(&token, pos);
        }

        // Parse instruction
        self.parse_instruction(&token, pos)
    }

    /// Parse a directive
    fn parse_directive(&mut self, dir: &[u8], mut pos: usize) -> usize {
        match dir {
            b".text" => self.current_section = 1,
            b".data" => self.current_section = 2,
            b".bss" => self.current_section = 3,
            b".section" => {
                let (name, new_pos) = self.get_token(pos);
                pos = new_pos;
                if bytes_eq_len(&name, b".text") {
                    self.current_section = 1;
                } else if bytes_eq_len(&name, b".data") {
                    self.current_section = 2;
                } else if bytes_eq_len(&name, b".bss") {
                    self.current_section = 3;
                }
            }
            b".global" | b".globl" => {
                let (name, new_pos) = self.get_token(pos);
                pos = new_pos;
                // Find or add symbol, mark as global
                for i in 0..self.num_symbols {
                    if bytes_eq_len(&self.symbols[i].name, &name) {
                        self.symbols[i].binding = 1;
                        return self.skip_to_eol(pos);
                    }
                }
                // Symbol not found, add it as undefined global
                self.add_symbol(&name, 0, 0, 1, false);
            }
            b".byte" => {
                loop {
                    let (val, new_pos) = self.get_token(pos);
                    pos = new_pos;
                    if val.is_empty() { break; }
                    if let Some(n) = parse_number(&val) {
                        self.emit_byte(n as u8);
                    }
                    // Skip comma
                    while pos < self.source_len && self.source[pos] == b',' {
                        pos += 1;
                    }
                }
            }
            b".word" | b".short" => {
                loop {
                    let (val, new_pos) = self.get_token(pos);
                    pos = new_pos;
                    if val.is_empty() { break; }
                    if let Some(n) = parse_number(&val) {
                        self.emit_u16(n as u16);
                    }
                    while pos < self.source_len && self.source[pos] == b',' {
                        pos += 1;
                    }
                }
            }
            b".long" | b".int" => {
                loop {
                    let (val, new_pos) = self.get_token(pos);
                    pos = new_pos;
                    if val.is_empty() { break; }
                    if let Some(n) = parse_number(&val) {
                        self.emit_u32(n as u32);
                    }
                    while pos < self.source_len && self.source[pos] == b',' {
                        pos += 1;
                    }
                }
            }
            b".quad" => {
                loop {
                    let (val, new_pos) = self.get_token(pos);
                    pos = new_pos;
                    if val.is_empty() { break; }
                    if let Some(n) = parse_number(&val) {
                        self.emit_u64(n as u64);
                    }
                    while pos < self.source_len && self.source[pos] == b',' {
                        pos += 1;
                    }
                }
            }
            b".ascii" => {
                // Skip whitespace
                while pos < self.source_len && (self.source[pos] == b' ' || self.source[pos] == b'\t') {
                    pos += 1;
                }
                if pos < self.source_len && self.source[pos] == b'"' {
                    pos += 1;
                    while pos < self.source_len && self.source[pos] != b'"' && self.source[pos] != b'\n' {
                        if self.source[pos] == b'\\' && pos + 1 < self.source_len {
                            pos += 1;
                            match self.source[pos] {
                                b'n' => self.emit_byte(b'\n'),
                                b't' => self.emit_byte(b'\t'),
                                b'r' => self.emit_byte(b'\r'),
                                b'0' => self.emit_byte(0),
                                b'\\' => self.emit_byte(b'\\'),
                                b'"' => self.emit_byte(b'"'),
                                c => self.emit_byte(c),
                            }
                        } else {
                            self.emit_byte(self.source[pos]);
                        }
                        pos += 1;
                    }
                    if pos < self.source_len && self.source[pos] == b'"' {
                        pos += 1;
                    }
                }
            }
            b".asciz" | b".string" => {
                // Skip whitespace
                while pos < self.source_len && (self.source[pos] == b' ' || self.source[pos] == b'\t') {
                    pos += 1;
                }
                if pos < self.source_len && self.source[pos] == b'"' {
                    pos += 1;
                    while pos < self.source_len && self.source[pos] != b'"' && self.source[pos] != b'\n' {
                        if self.source[pos] == b'\\' && pos + 1 < self.source_len {
                            pos += 1;
                            match self.source[pos] {
                                b'n' => self.emit_byte(b'\n'),
                                b't' => self.emit_byte(b'\t'),
                                b'r' => self.emit_byte(b'\r'),
                                b'0' => self.emit_byte(0),
                                b'\\' => self.emit_byte(b'\\'),
                                b'"' => self.emit_byte(b'"'),
                                c => self.emit_byte(c),
                            }
                        } else {
                            self.emit_byte(self.source[pos]);
                        }
                        pos += 1;
                    }
                    if pos < self.source_len && self.source[pos] == b'"' {
                        pos += 1;
                    }
                    self.emit_byte(0); // Null terminator
                }
            }
            b".zero" | b".skip" => {
                let (val, new_pos) = self.get_token(pos);
                pos = new_pos;
                if let Some(n) = parse_number(&val) {
                    for _ in 0..n {
                        self.emit_byte(0);
                    }
                }
            }
            b".align" => {
                let (val, new_pos) = self.get_token(pos);
                pos = new_pos;
                if let Some(n) = parse_number(&val) {
                    let align = n as usize;
                    let offset = self.current_offset();
                    let padding = (align - (offset % align)) % align;
                    for _ in 0..padding {
                        self.emit_byte(0x90); // NOP for code, 0 for data
                    }
                }
            }
            _ => {
                // Unknown directive, skip
            }
        }

        self.skip_to_eol(pos)
    }

    /// Parse an instruction
    fn parse_instruction(&mut self, mnemonic: &[u8], pos: usize) -> usize {
        // Get operands
        let mut operands: [[u8; 64]; 3] = [[0u8; 64]; 3];
        let mut num_ops = 0;
        let mut cur_pos = pos;

        while num_ops < 3 {
            // Skip whitespace
            while cur_pos < self.source_len && (self.source[cur_pos] == b' ' || self.source[cur_pos] == b'\t') {
                cur_pos += 1;
            }

            if cur_pos >= self.source_len || self.source[cur_pos] == b'\n' ||
               self.source[cur_pos] == b'#' || self.source[cur_pos] == b';' {
                break;
            }

            // Get operand (until comma, whitespace, newline, or comment)
            let mut j = 0;
            while cur_pos < self.source_len && j < 63 {
                let c = self.source[cur_pos];
                if c == b',' || c == b'\n' || c == b'#' || c == b';' {
                    break;
                }
                // Handle parentheses (for memory operands)
                if c == b'(' {
                    // Copy until matching ')'
                    while cur_pos < self.source_len && j < 63 {
                        operands[num_ops][j] = self.source[cur_pos];
                        j += 1;
                        if self.source[cur_pos] == b')' {
                            cur_pos += 1;
                            break;
                        }
                        cur_pos += 1;
                    }
                    continue;
                }
                // Skip trailing whitespace in operand
                if c == b' ' || c == b'\t' {
                    // Peek ahead - if next non-whitespace is comma or EOL, stop
                    let mut peek = cur_pos;
                    while peek < self.source_len && (self.source[peek] == b' ' || self.source[peek] == b'\t') {
                        peek += 1;
                    }
                    if peek >= self.source_len || self.source[peek] == b',' ||
                       self.source[peek] == b'\n' || self.source[peek] == b'#' {
                        break;
                    }
                }
                operands[num_ops][j] = c;
                j += 1;
                cur_pos += 1;
            }

            if j > 0 {
                operands[num_ops][j] = 0;
                num_ops += 1;
            }

            // Skip comma
            while cur_pos < self.source_len && self.source[cur_pos] == b',' {
                cur_pos += 1;
            }
        }

        // Assemble the instruction
        self.assemble_insn(mnemonic, &operands, num_ops);

        self.skip_to_eol(cur_pos)
    }

    /// Assemble a single instruction
    fn assemble_insn(&mut self, mnemonic: &[u8], operands: &[[u8; 64]; 3], num_ops: usize) {
        // Convert mnemonic to lowercase for comparison
        let mut mn_lower = [0u8; 16];
        let mn_len = mnemonic.len().min(15);
        for i in 0..mn_len {
            mn_lower[i] = to_lower(mnemonic[i]);
        }
        let mn = &mn_lower[..mn_len];

        match mn {
            // No operand instructions
            b"ret" => self.emit_byte(0xC3),
            b"retq" => self.emit_byte(0xC3),
            b"nop" => self.emit_byte(0x90),
            b"hlt" => self.emit_byte(0xF4),
            b"cli" => self.emit_byte(0xFA),
            b"sti" => self.emit_byte(0xFB),
            b"cld" => self.emit_byte(0xFC),
            b"std" => self.emit_byte(0xFD),
            b"syscall" => self.emit_bytes(&[0x0F, 0x05]),
            b"leave" => self.emit_byte(0xC9),
            b"pushfq" => self.emit_byte(0x9C),
            b"popfq" => self.emit_byte(0x9D),
            b"cqo" => self.emit_bytes(&[0x48, 0x99]),
            b"cdq" => self.emit_byte(0x99),

            // Push/Pop
            b"push" | b"pushq" if num_ops == 1 => {
                self.assemble_push_pop(&operands[0], true);
            }
            b"pop" | b"popq" if num_ops == 1 => {
                self.assemble_push_pop(&operands[0], false);
            }

            // Jumps and calls
            b"jmp" if num_ops == 1 => {
                self.assemble_jmp(&operands[0], 0xEB, 0xE9);
            }
            b"call" if num_ops == 1 => {
                self.assemble_call(&operands[0]);
            }
            b"je" | b"jz" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x74, 0x84);
            }
            b"jne" | b"jnz" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x75, 0x85);
            }
            b"jl" | b"jnge" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x7C, 0x8C);
            }
            b"jle" | b"jng" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x7E, 0x8E);
            }
            b"jg" | b"jnle" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x7F, 0x8F);
            }
            b"jge" | b"jnl" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x7D, 0x8D);
            }
            b"ja" | b"jnbe" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x77, 0x87);
            }
            b"jae" | b"jnb" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x73, 0x83);
            }
            b"jb" | b"jnae" | b"jc" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x72, 0x82);
            }
            b"jbe" | b"jna" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x76, 0x86);
            }
            b"js" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x78, 0x88);
            }
            b"jns" if num_ops == 1 => {
                self.assemble_jcc(&operands[0], 0x79, 0x89);
            }

            // Two operand instructions
            b"mov" | b"movq" | b"movl" if num_ops == 2 => {
                self.assemble_mov(&operands[0], &operands[1], mn);
            }
            b"movb" if num_ops == 2 => {
                self.assemble_mov_byte(&operands[0], &operands[1]);
            }
            b"movzbl" | b"movzbq" if num_ops == 2 => {
                self.assemble_movzx(&operands[0], &operands[1], 1);
            }
            b"movzwl" | b"movzwq" if num_ops == 2 => {
                self.assemble_movzx(&operands[0], &operands[1], 2);
            }
            b"movsbl" | b"movsbq" if num_ops == 2 => {
                self.assemble_movsx(&operands[0], &operands[1], 1);
            }
            b"movswl" | b"movswq" if num_ops == 2 => {
                self.assemble_movsx(&operands[0], &operands[1], 2);
            }

            b"add" | b"addq" | b"addl" if num_ops == 2 => {
                self.assemble_alu(&operands[0], &operands[1], 0x00, 0);
            }
            b"sub" | b"subq" | b"subl" if num_ops == 2 => {
                self.assemble_alu(&operands[0], &operands[1], 0x28, 5);
            }
            b"and" | b"andq" | b"andl" if num_ops == 2 => {
                self.assemble_alu(&operands[0], &operands[1], 0x20, 4);
            }
            b"or" | b"orq" | b"orl" if num_ops == 2 => {
                self.assemble_alu(&operands[0], &operands[1], 0x08, 1);
            }
            b"xor" | b"xorq" | b"xorl" if num_ops == 2 => {
                self.assemble_alu(&operands[0], &operands[1], 0x30, 6);
            }
            b"cmp" | b"cmpq" | b"cmpl" if num_ops == 2 => {
                self.assemble_alu(&operands[0], &operands[1], 0x38, 7);
            }
            b"test" | b"testq" | b"testl" if num_ops == 2 => {
                self.assemble_test(&operands[0], &operands[1]);
            }

            b"shl" | b"shlq" | b"shll" if num_ops == 2 => {
                self.assemble_shift(&operands[0], &operands[1], 4);
            }
            b"shr" | b"shrq" | b"shrl" if num_ops == 2 => {
                self.assemble_shift(&operands[0], &operands[1], 5);
            }
            b"sar" | b"sarq" | b"sarl" if num_ops == 2 => {
                self.assemble_shift(&operands[0], &operands[1], 7);
            }

            b"lea" | b"leaq" if num_ops == 2 => {
                self.assemble_lea(&operands[0], &operands[1]);
            }

            // Single operand instructions
            b"inc" | b"incq" | b"incl" if num_ops == 1 => {
                self.assemble_unary(&operands[0], 0xFF, 0);
            }
            b"dec" | b"decq" | b"decl" if num_ops == 1 => {
                self.assemble_unary(&operands[0], 0xFF, 1);
            }
            b"neg" | b"negq" | b"negl" if num_ops == 1 => {
                self.assemble_unary(&operands[0], 0xF7, 3);
            }
            b"not" | b"notq" | b"notl" if num_ops == 1 => {
                self.assemble_unary(&operands[0], 0xF7, 2);
            }
            b"mul" | b"mulq" if num_ops == 1 => {
                self.assemble_unary(&operands[0], 0xF7, 4);
            }
            b"div" | b"divq" if num_ops == 1 => {
                self.assemble_unary(&operands[0], 0xF7, 6);
            }
            b"idiv" | b"idivq" if num_ops == 1 => {
                self.assemble_unary(&operands[0], 0xF7, 7);
            }

            b"imul" | b"imulq" if num_ops == 2 => {
                self.assemble_imul(&operands[0], &operands[1]);
            }

            b"sete" | b"setz" if num_ops == 1 => {
                self.assemble_setcc(&operands[0], 0x94);
            }
            b"setne" | b"setnz" if num_ops == 1 => {
                self.assemble_setcc(&operands[0], 0x95);
            }
            b"setl" if num_ops == 1 => {
                self.assemble_setcc(&operands[0], 0x9C);
            }
            b"setg" if num_ops == 1 => {
                self.assemble_setcc(&operands[0], 0x9F);
            }
            b"setle" if num_ops == 1 => {
                self.assemble_setcc(&operands[0], 0x9E);
            }
            b"setge" if num_ops == 1 => {
                self.assemble_setcc(&operands[0], 0x9D);
            }

            _ => {
                self.error("unknown instruction");
            }
        }
    }

    /// Assemble push/pop
    fn assemble_push_pop(&mut self, op: &[u8], is_push: bool) {
        let op = trim_bytes(op);

        if op.len() > 1 && op[0] == b'%' {
            // Register
            if let Some(reg) = parse_reg64(&op[1..]) {
                if reg >= 8 {
                    self.emit_byte(0x41); // REX.B
                }
                if is_push {
                    self.emit_byte(0x50 + (reg & 7));
                } else {
                    self.emit_byte(0x58 + (reg & 7));
                }
            } else {
                self.error("invalid register for push/pop");
            }
        } else if op[0] == b'$' {
            // Immediate (push only)
            if !is_push {
                self.error("cannot pop to immediate");
                return;
            }
            if let Some(imm) = parse_number(&op[1..]) {
                if imm >= -128 && imm <= 127 {
                    self.emit_byte(0x6A);
                    self.emit_byte(imm as u8);
                } else {
                    self.emit_byte(0x68);
                    self.emit_u32(imm as u32);
                }
            }
        } else {
            self.error("invalid operand for push/pop");
        }
    }

    /// Assemble jmp
    fn assemble_jmp(&mut self, target: &[u8], short_op: u8, near_op: u8) {
        let target = trim_bytes(target);

        if target[0] == b'*' {
            // Indirect jump
            let inner = trim_bytes(&target[1..]);
            if inner.len() > 1 && inner[0] == b'%' {
                if let Some(reg) = parse_reg64(&inner[1..]) {
                    if reg >= 8 {
                        self.emit_byte(0x41);
                    }
                    self.emit_byte(0xFF);
                    self.emit_byte(0xE0 + (reg & 7));
                    return;
                }
            }
            self.error("invalid indirect jump");
            return;
        }

        // Label reference - emit 32-bit relative jump and add relocation
        let sym_idx = self.add_symbol(target, 0, 0, 0, false);
        self.emit_byte(near_op);
        self.add_reloc(sym_idx, 2, -4); // R_X86_64_PC32
        self.emit_u32(0);
    }

    /// Assemble call
    fn assemble_call(&mut self, target: &[u8]) {
        let target = trim_bytes(target);

        if target[0] == b'*' {
            // Indirect call
            let inner = trim_bytes(&target[1..]);
            if inner.len() > 1 && inner[0] == b'%' {
                if let Some(reg) = parse_reg64(&inner[1..]) {
                    if reg >= 8 {
                        self.emit_byte(0x41);
                    }
                    self.emit_byte(0xFF);
                    self.emit_byte(0xD0 + (reg & 7));
                    return;
                }
            }
            self.error("invalid indirect call");
            return;
        }

        // Label reference
        let sym_idx = self.add_symbol(target, 0, 0, 0, false);
        self.emit_byte(0xE8);
        self.add_reloc(sym_idx, 2, -4); // R_X86_64_PC32
        self.emit_u32(0);
    }

    /// Assemble conditional jump
    fn assemble_jcc(&mut self, target: &[u8], short_op: u8, near_op: u8) {
        let target = trim_bytes(target);
        let sym_idx = self.add_symbol(target, 0, 0, 0, false);

        // Always emit near jump with relocation
        self.emit_byte(0x0F);
        self.emit_byte(near_op);
        self.add_reloc(sym_idx, 2, -4); // R_X86_64_PC32
        self.emit_u32(0);
    }

    /// Assemble mov instruction
    fn assemble_mov(&mut self, src: &[u8], dst: &[u8], mn: &[u8]) {
        let src = trim_bytes(src);
        let dst = trim_bytes(dst);
        let is_32bit = mn == b"movl";

        // Determine if we need REX.W prefix
        let need_rex_w = !is_32bit;

        if src[0] == b'$' {
            // Immediate to register/memory
            if let Some(imm) = parse_number(&src[1..]) {
                if dst[0] == b'%' {
                    // Immediate to register
                    if let Some(reg) = parse_reg64(&dst[1..]) {
                        // movabs for 64-bit immediates, regular mov for 32-bit
                        if need_rex_w && (imm > 0x7FFFFFFF || imm < -0x80000000i64) {
                            // 64-bit immediate
                            let mut rex = 0x48;
                            if reg >= 8 { rex |= 0x01; }
                            self.emit_byte(rex);
                            self.emit_byte(0xB8 + (reg & 7));
                            self.emit_u64(imm as u64);
                        } else {
                            // 32-bit immediate
                            if need_rex_w || reg >= 8 {
                                let mut rex = if need_rex_w { 0x48 } else { 0x40 };
                                if reg >= 8 { rex |= 0x01; }
                                self.emit_byte(rex);
                            }
                            self.emit_byte(0xC7);
                            self.emit_byte(0xC0 + (reg & 7));
                            self.emit_u32(imm as u32);
                        }
                    }
                } else {
                    // Immediate to memory - simplified
                    self.error("mov imm to memory not fully implemented");
                }
            } else {
                // Symbol reference
                let sym_name = &src[1..];
                let sym_idx = self.add_symbol(sym_name, 0, 0, 0, false);

                if dst[0] == b'%' {
                    if let Some(reg) = parse_reg64(&dst[1..]) {
                        // mov $symbol, %reg -> need relocation
                        let mut rex = 0x48;
                        if reg >= 8 { rex |= 0x01; }
                        self.emit_byte(rex);
                        self.emit_byte(0xB8 + (reg & 7));
                        self.add_reloc(sym_idx, 1, 0); // R_X86_64_64
                        self.emit_u64(0);
                    }
                }
            }
        } else if src[0] == b'%' && dst[0] == b'%' {
            // Register to register
            if let (Some(src_reg), Some(dst_reg)) = (parse_reg64(&src[1..]), parse_reg64(&dst[1..])) {
                let mut rex = if need_rex_w { 0x48 } else { 0x40 };
                if src_reg >= 8 { rex |= 0x04; }
                if dst_reg >= 8 { rex |= 0x01; }
                if rex != 0x40 || need_rex_w {
                    self.emit_byte(rex);
                }
                self.emit_byte(0x89);
                self.emit_byte(0xC0 + ((src_reg & 7) << 3) + (dst_reg & 7));
            }
        } else if src[0] == b'%' {
            // Register to memory
            if let Some(src_reg) = parse_reg64(&src[1..]) {
                self.assemble_reg_mem(src_reg, dst, 0x89, need_rex_w);
            }
        } else if dst[0] == b'%' {
            // Memory to register
            if let Some(dst_reg) = parse_reg64(&dst[1..]) {
                self.assemble_reg_mem(dst_reg, src, 0x8B, need_rex_w);
            }
        }
    }

    /// Assemble mov byte
    fn assemble_mov_byte(&mut self, src: &[u8], dst: &[u8]) {
        let src = trim_bytes(src);
        let dst = trim_bytes(dst);

        if src[0] == b'$' {
            if let Some(imm) = parse_number(&src[1..]) {
                if dst[0] == b'%' {
                    if let Some(reg) = parse_reg8(&dst[1..]) {
                        if reg >= 4 {
                            self.emit_byte(0x40 | ((reg >> 3) & 1));
                        }
                        self.emit_byte(0xB0 + (reg & 7));
                        self.emit_byte(imm as u8);
                    }
                }
            }
        }
    }

    /// Assemble movzx (zero extend)
    fn assemble_movzx(&mut self, src: &[u8], dst: &[u8], src_size: u8) {
        let src = trim_bytes(src);
        let dst = trim_bytes(dst);

        if dst[0] == b'%' {
            if let Some(dst_reg) = parse_reg64(&dst[1..]) {
                let mut rex = 0x48;
                if dst_reg >= 8 { rex |= 0x04; }
                self.emit_byte(rex);
                self.emit_byte(0x0F);
                self.emit_byte(if src_size == 1 { 0xB6 } else { 0xB7 });

                if src[0] == b'%' {
                    // Register source
                    let src_reg = if src_size == 1 {
                        parse_reg8(&src[1..]).unwrap_or(0)
                    } else {
                        parse_reg16(&src[1..]).unwrap_or(0)
                    };
                    self.emit_byte(0xC0 + ((dst_reg & 7) << 3) + (src_reg & 7));
                }
            }
        }
    }

    /// Assemble movsx (sign extend)
    fn assemble_movsx(&mut self, src: &[u8], dst: &[u8], src_size: u8) {
        let src = trim_bytes(src);
        let dst = trim_bytes(dst);

        if dst[0] == b'%' {
            if let Some(dst_reg) = parse_reg64(&dst[1..]) {
                let mut rex = 0x48;
                if dst_reg >= 8 { rex |= 0x04; }
                self.emit_byte(rex);
                self.emit_byte(0x0F);
                self.emit_byte(if src_size == 1 { 0xBE } else { 0xBF });

                if src[0] == b'%' {
                    let src_reg = if src_size == 1 {
                        parse_reg8(&src[1..]).unwrap_or(0)
                    } else {
                        parse_reg16(&src[1..]).unwrap_or(0)
                    };
                    self.emit_byte(0xC0 + ((dst_reg & 7) << 3) + (src_reg & 7));
                }
            }
        }
    }

    /// Assemble ALU instruction (add, sub, and, or, xor, cmp)
    fn assemble_alu(&mut self, src: &[u8], dst: &[u8], base_op: u8, imm_ext: u8) {
        let src = trim_bytes(src);
        let dst = trim_bytes(dst);

        if src[0] == b'$' {
            // Immediate
            if let Some(imm) = parse_number(&src[1..]) {
                if dst[0] == b'%' {
                    if let Some(reg) = parse_reg64(&dst[1..]) {
                        let mut rex = 0x48;
                        if reg >= 8 { rex |= 0x01; }
                        self.emit_byte(rex);

                        if imm >= -128 && imm <= 127 {
                            self.emit_byte(0x83);
                            self.emit_byte(0xC0 + (imm_ext << 3) + (reg & 7));
                            self.emit_byte(imm as u8);
                        } else {
                            self.emit_byte(0x81);
                            self.emit_byte(0xC0 + (imm_ext << 3) + (reg & 7));
                            self.emit_u32(imm as u32);
                        }
                    }
                }
            }
        } else if src[0] == b'%' && dst[0] == b'%' {
            // Register to register
            if let (Some(src_reg), Some(dst_reg)) = (parse_reg64(&src[1..]), parse_reg64(&dst[1..])) {
                let mut rex = 0x48;
                if src_reg >= 8 { rex |= 0x04; }
                if dst_reg >= 8 { rex |= 0x01; }
                self.emit_byte(rex);
                self.emit_byte(base_op + 1);
                self.emit_byte(0xC0 + ((src_reg & 7) << 3) + (dst_reg & 7));
            }
        } else if src[0] == b'%' {
            // Register to memory
            if let Some(src_reg) = parse_reg64(&src[1..]) {
                self.assemble_reg_mem(src_reg, dst, base_op + 1, true);
            }
        } else if dst[0] == b'%' {
            // Memory to register
            if let Some(dst_reg) = parse_reg64(&dst[1..]) {
                self.assemble_reg_mem(dst_reg, src, base_op + 3, true);
            }
        }
    }

    /// Assemble test instruction
    fn assemble_test(&mut self, src: &[u8], dst: &[u8]) {
        let src = trim_bytes(src);
        let dst = trim_bytes(dst);

        if src[0] == b'$' {
            if let Some(imm) = parse_number(&src[1..]) {
                if dst[0] == b'%' {
                    if let Some(reg) = parse_reg64(&dst[1..]) {
                        let mut rex = 0x48;
                        if reg >= 8 { rex |= 0x01; }
                        self.emit_byte(rex);
                        self.emit_byte(0xF7);
                        self.emit_byte(0xC0 + (reg & 7));
                        self.emit_u32(imm as u32);
                    }
                }
            }
        } else if src[0] == b'%' && dst[0] == b'%' {
            if let (Some(src_reg), Some(dst_reg)) = (parse_reg64(&src[1..]), parse_reg64(&dst[1..])) {
                let mut rex = 0x48;
                if src_reg >= 8 { rex |= 0x04; }
                if dst_reg >= 8 { rex |= 0x01; }
                self.emit_byte(rex);
                self.emit_byte(0x85);
                self.emit_byte(0xC0 + ((src_reg & 7) << 3) + (dst_reg & 7));
            }
        }
    }

    /// Assemble shift instruction
    fn assemble_shift(&mut self, src: &[u8], dst: &[u8], ext: u8) {
        let src = trim_bytes(src);
        let dst = trim_bytes(dst);

        if dst[0] == b'%' {
            if let Some(reg) = parse_reg64(&dst[1..]) {
                let mut rex = 0x48;
                if reg >= 8 { rex |= 0x01; }
                self.emit_byte(rex);

                if src[0] == b'$' {
                    if let Some(imm) = parse_number(&src[1..]) {
                        if imm == 1 {
                            self.emit_byte(0xD1);
                            self.emit_byte(0xC0 + (ext << 3) + (reg & 7));
                        } else {
                            self.emit_byte(0xC1);
                            self.emit_byte(0xC0 + (ext << 3) + (reg & 7));
                            self.emit_byte(imm as u8);
                        }
                    }
                } else if bytes_eq_len(src, b"%cl") {
                    self.emit_byte(0xD3);
                    self.emit_byte(0xC0 + (ext << 3) + (reg & 7));
                }
            }
        }
    }

    /// Assemble lea instruction
    fn assemble_lea(&mut self, src: &[u8], dst: &[u8]) {
        let dst = trim_bytes(dst);

        if dst[0] == b'%' {
            if let Some(dst_reg) = parse_reg64(&dst[1..]) {
                self.assemble_reg_mem(dst_reg, src, 0x8D, true);
            }
        }
    }

    /// Assemble unary instruction (inc, dec, neg, not, mul, div, idiv)
    fn assemble_unary(&mut self, op: &[u8], base_op: u8, ext: u8) {
        let op = trim_bytes(op);

        if op[0] == b'%' {
            if let Some(reg) = parse_reg64(&op[1..]) {
                let mut rex = 0x48;
                if reg >= 8 { rex |= 0x01; }
                self.emit_byte(rex);
                self.emit_byte(base_op);
                self.emit_byte(0xC0 + (ext << 3) + (reg & 7));
            }
        }
    }

    /// Assemble imul (two operand form)
    fn assemble_imul(&mut self, src: &[u8], dst: &[u8]) {
        let src = trim_bytes(src);
        let dst = trim_bytes(dst);

        if src[0] == b'%' && dst[0] == b'%' {
            if let (Some(src_reg), Some(dst_reg)) = (parse_reg64(&src[1..]), parse_reg64(&dst[1..])) {
                let mut rex = 0x48;
                if dst_reg >= 8 { rex |= 0x04; }
                if src_reg >= 8 { rex |= 0x01; }
                self.emit_byte(rex);
                self.emit_byte(0x0F);
                self.emit_byte(0xAF);
                self.emit_byte(0xC0 + ((dst_reg & 7) << 3) + (src_reg & 7));
            }
        }
    }

    /// Assemble setcc instruction
    fn assemble_setcc(&mut self, op: &[u8], cc: u8) {
        let op = trim_bytes(op);

        if op[0] == b'%' {
            if let Some(reg) = parse_reg8(&op[1..]) {
                if reg >= 4 {
                    self.emit_byte(0x40 | ((reg >> 3) & 1));
                }
                self.emit_byte(0x0F);
                self.emit_byte(cc);
                self.emit_byte(0xC0 + (reg & 7));
            }
        }
    }

    /// Assemble instruction with register and memory operand
    fn assemble_reg_mem(&mut self, reg: u8, mem: &[u8], opcode: u8, need_rex_w: bool) {
        let mem = trim_bytes(mem);

        // Parse memory operand: disp(base,index,scale) or (base) or symbol
        let (disp, base, index, scale) = parse_mem_operand(mem);

        // Check for symbol reference (RIP-relative)
        if base == 255 && index == 255 {
            // Symbol reference - use RIP-relative addressing
            // Find or create symbol
            let sym_name = if mem.iter().any(|&c| c == b'(') {
                // Extract symbol name before (
                let paren_pos = mem.iter().position(|&c| c == b'(').unwrap();
                &mem[..paren_pos]
            } else {
                mem
            };

            let sym_idx = self.add_symbol(sym_name, 0, 0, 0, false);

            let mut rex = if need_rex_w { 0x48 } else { 0x40 };
            if reg >= 8 { rex |= 0x04; }
            if rex != 0x40 || need_rex_w {
                self.emit_byte(rex);
            }

            self.emit_byte(opcode);
            self.emit_byte(0x05 + ((reg & 7) << 3)); // ModR/M for RIP-relative
            self.add_reloc(sym_idx, 2, -4); // R_X86_64_PC32
            self.emit_u32(disp as u32);
            return;
        }

        // Build REX prefix
        let mut rex = if need_rex_w { 0x48 } else { 0x40 };
        if reg >= 8 { rex |= 0x04; }
        if base >= 8 { rex |= 0x01; }
        if index >= 8 { rex |= 0x02; }

        if rex != 0x40 || need_rex_w {
            self.emit_byte(rex);
        }

        self.emit_byte(opcode);

        // Build ModR/M and SIB bytes
        let need_sib = index != 255 || (base & 7) == 4;

        // Determine mod (displacement size)
        let modrm_mod = if disp == 0 && (base & 7) != 5 {
            0x00 // No displacement (except for RBP/R13)
        } else if disp >= -128 && disp <= 127 {
            0x40 // 8-bit displacement
        } else {
            0x80 // 32-bit displacement
        };

        if need_sib {
            // ModR/M with SIB
            self.emit_byte(modrm_mod | ((reg & 7) << 3) | 4);

            // SIB byte
            let scale_bits = match scale {
                1 => 0,
                2 => 1,
                4 => 2,
                8 => 3,
                _ => 0,
            };
            let index_bits = if index != 255 { index & 7 } else { 4 }; // 4 = no index
            let base_bits = base & 7;
            self.emit_byte((scale_bits << 6) | (index_bits << 3) | base_bits);
        } else {
            // ModR/M without SIB
            self.emit_byte(modrm_mod | ((reg & 7) << 3) | (base & 7));
        }

        // Emit displacement
        if modrm_mod == 0x40 {
            self.emit_byte(disp as u8);
        } else if modrm_mod == 0x80 || (modrm_mod == 0x00 && (base & 7) == 5) {
            self.emit_u32(disp as u32);
        }
    }

    /// Get a token from the source
    fn get_token(&self, mut pos: usize) -> ([u8; 64], usize) {
        let mut token = [0u8; 64];
        let mut len = 0;

        // Skip whitespace
        while pos < self.source_len && (self.source[pos] == b' ' || self.source[pos] == b'\t') {
            pos += 1;
        }

        if pos >= self.source_len || self.source[pos] == b'\n' ||
           self.source[pos] == b'#' || self.source[pos] == b';' {
            return (token, pos);
        }

        // Get token
        while pos < self.source_len && len < 63 {
            let c = self.source[pos];
            if c == b' ' || c == b'\t' || c == b'\n' || c == b',' || c == b'#' || c == b';' {
                break;
            }
            token[len] = c;
            len += 1;
            pos += 1;
        }

        (token, pos)
    }

    /// Skip to end of line
    fn skip_to_eol(&mut self, mut pos: usize) -> usize {
        while pos < self.source_len && self.source[pos] != b'\n' {
            pos += 1;
        }
        if pos < self.source_len {
            pos += 1;
            self.line_num += 1;
        }
        pos
    }
}

/// Parse 64-bit register name
fn parse_reg64(name: &[u8]) -> Option<u8> {
    let name = trim_bytes(name);
    match name {
        b"rax" => Some(0),
        b"rcx" => Some(1),
        b"rdx" => Some(2),
        b"rbx" => Some(3),
        b"rsp" => Some(4),
        b"rbp" => Some(5),
        b"rsi" => Some(6),
        b"rdi" => Some(7),
        b"r8" => Some(8),
        b"r9" => Some(9),
        b"r10" => Some(10),
        b"r11" => Some(11),
        b"r12" => Some(12),
        b"r13" => Some(13),
        b"r14" => Some(14),
        b"r15" => Some(15),
        // Also accept 32-bit names for 64-bit operations
        b"eax" => Some(0),
        b"ecx" => Some(1),
        b"edx" => Some(2),
        b"ebx" => Some(3),
        b"esp" => Some(4),
        b"ebp" => Some(5),
        b"esi" => Some(6),
        b"edi" => Some(7),
        b"r8d" => Some(8),
        b"r9d" => Some(9),
        b"r10d" => Some(10),
        b"r11d" => Some(11),
        b"r12d" => Some(12),
        b"r13d" => Some(13),
        b"r14d" => Some(14),
        b"r15d" => Some(15),
        _ => None,
    }
}

/// Parse 32-bit register name
fn parse_reg32(name: &[u8]) -> Option<u8> {
    let name = trim_bytes(name);
    match name {
        b"eax" => Some(0),
        b"ecx" => Some(1),
        b"edx" => Some(2),
        b"ebx" => Some(3),
        b"esp" => Some(4),
        b"ebp" => Some(5),
        b"esi" => Some(6),
        b"edi" => Some(7),
        b"r8d" => Some(8),
        b"r9d" => Some(9),
        b"r10d" => Some(10),
        b"r11d" => Some(11),
        b"r12d" => Some(12),
        b"r13d" => Some(13),
        b"r14d" => Some(14),
        b"r15d" => Some(15),
        _ => None,
    }
}

/// Parse 16-bit register name
fn parse_reg16(name: &[u8]) -> Option<u8> {
    let name = trim_bytes(name);
    match name {
        b"ax" => Some(0),
        b"cx" => Some(1),
        b"dx" => Some(2),
        b"bx" => Some(3),
        b"sp" => Some(4),
        b"bp" => Some(5),
        b"si" => Some(6),
        b"di" => Some(7),
        _ => None,
    }
}

/// Parse 8-bit register name
fn parse_reg8(name: &[u8]) -> Option<u8> {
    let name = trim_bytes(name);
    match name {
        b"al" => Some(0),
        b"cl" => Some(1),
        b"dl" => Some(2),
        b"bl" => Some(3),
        b"spl" => Some(4),
        b"bpl" => Some(5),
        b"sil" => Some(6),
        b"dil" => Some(7),
        b"r8b" => Some(8),
        b"r9b" => Some(9),
        b"r10b" => Some(10),
        b"r11b" => Some(11),
        b"r12b" => Some(12),
        b"r13b" => Some(13),
        b"r14b" => Some(14),
        b"r15b" => Some(15),
        // Legacy names
        b"ah" => Some(4),
        b"ch" => Some(5),
        b"dh" => Some(6),
        b"bh" => Some(7),
        _ => None,
    }
}

/// Parse memory operand: returns (displacement, base_reg, index_reg, scale)
/// Returns (disp, 255, 255, 1) for symbol references
fn parse_mem_operand(mem: &[u8]) -> (i64, u8, u8, u8) {
    let mem = trim_bytes(mem);

    // Find '(' if present
    let paren_pos = mem.iter().position(|&c| c == b'(');

    // Parse displacement (before '(')
    let disp = if let Some(pos) = paren_pos {
        if pos > 0 {
            parse_number(&mem[..pos]).unwrap_or(0)
        } else {
            0
        }
    } else {
        // No parentheses - could be just a number or a symbol
        if let Some(n) = parse_number(mem) {
            return (n, 255, 255, 1);
        } else {
            // Symbol reference
            return (0, 255, 255, 1);
        }
    };

    // Parse (base,index,scale)
    if let Some(paren_start) = paren_pos {
        let paren_end = mem.iter().position(|&c| c == b')').unwrap_or(mem.len());
        let inner = &mem[paren_start + 1..paren_end];

        // Split by commas
        let mut parts: [[u8; 32]; 3] = [[0u8; 32]; 3];
        let mut num_parts = 0;
        let mut part_start = 0;

        for i in 0..inner.len() {
            if inner[i] == b',' || i == inner.len() - 1 {
                let end = if inner[i] == b',' { i } else { i + 1 };
                if num_parts < 3 {
                    let len = (end - part_start).min(31);
                    parts[num_parts][..len].copy_from_slice(&inner[part_start..part_start + len]);
                    num_parts += 1;
                }
                part_start = i + 1;
            }
        }

        let base = if num_parts > 0 && parts[0][0] == b'%' {
            parse_reg64(&parts[0][1..]).unwrap_or(255)
        } else {
            255
        };

        let index = if num_parts > 1 && parts[1][0] == b'%' {
            parse_reg64(&parts[1][1..]).unwrap_or(255)
        } else {
            255
        };

        let scale = if num_parts > 2 {
            parse_number(trim_bytes(&parts[2])).unwrap_or(1) as u8
        } else {
            1
        };

        (disp, base, index, scale)
    } else {
        (disp, 255, 255, 1)
    }
}

/// Parse a number (decimal, hex, or octal)
fn parse_number(s: &[u8]) -> Option<i64> {
    let s = trim_bytes(s);
    if s.is_empty() {
        return None;
    }

    let mut i = 0;
    let negative = if s[0] == b'-' {
        i = 1;
        true
    } else {
        false
    };

    if i >= s.len() {
        return None;
    }

    let (base, start) = if s.len() > i + 1 && s[i] == b'0' && (s[i + 1] == b'x' || s[i + 1] == b'X') {
        (16, i + 2)
    } else if s[i] == b'0' && s.len() > i + 1 {
        (8, i + 1)
    } else {
        (10, i)
    };

    let mut result: i64 = 0;
    for j in start..s.len() {
        let c = s[j];
        let digit = if c >= b'0' && c <= b'9' {
            c - b'0'
        } else if c >= b'a' && c <= b'f' {
            c - b'a' + 10
        } else if c >= b'A' && c <= b'F' {
            c - b'A' + 10
        } else {
            return None;
        };

        if digit >= base {
            return None;
        }

        result = result * base as i64 + digit as i64;
    }

    Some(if negative { -result } else { result })
}

/// Trim whitespace from byte slice
fn trim_bytes(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| c != b' ' && c != b'\t' && c != 0).unwrap_or(s.len());
    let end = s.iter().rposition(|&c| c != b' ' && c != b'\t' && c != 0).map(|p| p + 1).unwrap_or(start);
    &s[start..end]
}

/// Compare byte slices (null-terminated aware)
fn bytes_eq_len(a: &[u8], b: &[u8]) -> bool {
    let a_len = a.iter().position(|&c| c == 0).unwrap_or(a.len());
    let b_len = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    if a_len != b_len {
        return false;
    }
    for i in 0..a_len {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

/// Copy bytes
fn copy_bytes(dst: &mut [u8], src: &[u8]) {
    let len = src.iter().position(|&c| c == 0).unwrap_or(src.len()).min(dst.len() - 1);
    dst[..len].copy_from_slice(&src[..len]);
    dst[len] = 0;
}

/// Convert to lowercase
fn to_lower(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' {
        c + 32
    } else {
        c
    }
}

// ELF constants
const ELFMAG: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_REL: u16 = 1;
const EM_X86_64: u16 = 62;

const SHT_NULL: u32 = 0;
const SHT_PROGBITS: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;
const SHT_NOBITS: u32 = 8;

const SHF_WRITE: u64 = 1;
const SHF_ALLOC: u64 = 2;
const SHF_EXECINSTR: u64 = 4;
const SHF_INFO_LINK: u64 = 0x40;

const STB_LOCAL: u8 = 0;
const STB_GLOBAL: u8 = 1;
const STT_NOTYPE: u8 = 0;
const STT_SECTION: u8 = 3;

// Relocation types
const R_X86_64_64: u32 = 1;
const R_X86_64_PC32: u32 = 2;

/// ELF64 header
#[repr(C)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

/// ELF64 section header
#[repr(C)]
struct Elf64Shdr {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
}

/// ELF64 symbol
#[repr(C)]
struct Elf64Sym {
    st_name: u32,
    st_info: u8,
    st_other: u8,
    st_shndx: u16,
    st_value: u64,
    st_size: u64,
}

/// ELF64 relocation with addend
#[repr(C)]
struct Elf64Rela {
    r_offset: u64,
    r_info: u64,
    r_addend: i64,
}

impl Assembler {
    /// Write ELF object file
    fn write_elf(&self, fd: i32) {
        // Section layout:
        // 0: NULL
        // 1: .text
        // 2: .data
        // 3: .bss
        // 4: .rela.text
        // 5: .rela.data
        // 6: .symtab
        // 7: .strtab
        // 8: .shstrtab

        let ehdr_size = core::mem::size_of::<Elf64Header>();
        let shdr_size = core::mem::size_of::<Elf64Shdr>();

        // Build string tables
        let mut shstrtab = [0u8; 256];
        let mut shstrtab_len = 1; // Start with null byte

        let shname_text = shstrtab_len;
        copy_to_buf(&mut shstrtab, &mut shstrtab_len, b".text\0");
        let shname_data = shstrtab_len;
        copy_to_buf(&mut shstrtab, &mut shstrtab_len, b".data\0");
        let shname_bss = shstrtab_len;
        copy_to_buf(&mut shstrtab, &mut shstrtab_len, b".bss\0");
        let shname_relatext = shstrtab_len;
        copy_to_buf(&mut shstrtab, &mut shstrtab_len, b".rela.text\0");
        let shname_reladata = shstrtab_len;
        copy_to_buf(&mut shstrtab, &mut shstrtab_len, b".rela.data\0");
        let shname_symtab = shstrtab_len;
        copy_to_buf(&mut shstrtab, &mut shstrtab_len, b".symtab\0");
        let shname_strtab = shstrtab_len;
        copy_to_buf(&mut shstrtab, &mut shstrtab_len, b".strtab\0");
        let shname_shstrtab = shstrtab_len;
        copy_to_buf(&mut shstrtab, &mut shstrtab_len, b".shstrtab\0");

        // Build symbol string table
        let mut strtab = [0u8; 4096];
        let mut strtab_len = 1; // Start with null byte

        // Calculate section offsets
        let num_sections = 9;
        let text_off = ehdr_size + (num_sections * shdr_size);
        let data_off = text_off + self.text_len;
        let shstrtab_off = data_off + self.data_len;

        // Count relocations per section
        let mut text_relocs = 0;
        let mut data_relocs = 0;
        for i in 0..self.num_relocs {
            match self.relocs[i].section {
                1 => text_relocs += 1,
                2 => data_relocs += 1,
                _ => {}
            }
        }

        let rela_size = core::mem::size_of::<Elf64Rela>();
        let relatext_off = shstrtab_off + shstrtab_len;
        let reladata_off = relatext_off + (text_relocs * rela_size);

        // Build symbol table
        let sym_size = core::mem::size_of::<Elf64Sym>();
        let symtab_off = reladata_off + (data_relocs * rela_size);

        // Count symbols: NULL + 3 section symbols + user symbols
        let mut num_local_syms = 4; // NULL + .text + .data + .bss sections
        for i in 0..self.num_symbols {
            if self.symbols[i].binding == 0 && self.symbols[i].defined {
                num_local_syms += 1;
            }
        }

        let strtab_off = symtab_off + ((4 + self.num_symbols) * sym_size);
        let shdr_off = strtab_off + strtab_len + 4096; // Reserve space for strtab

        // Write ELF header
        let mut ehdr = [0u8; 64];
        ehdr[0..4].copy_from_slice(&ELFMAG);
        ehdr[4] = ELFCLASS64;
        ehdr[5] = ELFDATA2LSB;
        ehdr[6] = EV_CURRENT;
        write_u16(&mut ehdr[16..], ET_REL);
        write_u16(&mut ehdr[18..], EM_X86_64);
        write_u32(&mut ehdr[20..], 1); // e_version
        write_u64(&mut ehdr[40..], shdr_off as u64); // e_shoff
        write_u16(&mut ehdr[52..], ehdr_size as u16); // e_ehsize
        write_u16(&mut ehdr[58..], shdr_size as u16); // e_shentsize
        write_u16(&mut ehdr[60..], num_sections as u16); // e_shnum
        write_u16(&mut ehdr[62..], 8); // e_shstrndx

        syscall::sys_write(fd, &ehdr);

        // Write section headers
        // NULL section
        let null_shdr = [0u8; 64];
        syscall::sys_write(fd, &null_shdr);

        // .text section
        let mut text_shdr = [0u8; 64];
        write_u32(&mut text_shdr[0..], shname_text as u32);
        write_u32(&mut text_shdr[4..], SHT_PROGBITS);
        write_u64(&mut text_shdr[8..], SHF_ALLOC | SHF_EXECINSTR);
        write_u64(&mut text_shdr[24..], text_off as u64);
        write_u64(&mut text_shdr[32..], self.text_len as u64);
        write_u64(&mut text_shdr[48..], 16); // alignment
        syscall::sys_write(fd, &text_shdr);

        // .data section
        let mut data_shdr = [0u8; 64];
        write_u32(&mut data_shdr[0..], shname_data as u32);
        write_u32(&mut data_shdr[4..], SHT_PROGBITS);
        write_u64(&mut data_shdr[8..], SHF_ALLOC | SHF_WRITE);
        write_u64(&mut data_shdr[24..], data_off as u64);
        write_u64(&mut data_shdr[32..], self.data_len as u64);
        write_u64(&mut data_shdr[48..], 8);
        syscall::sys_write(fd, &data_shdr);

        // .bss section
        let mut bss_shdr = [0u8; 64];
        write_u32(&mut bss_shdr[0..], shname_bss as u32);
        write_u32(&mut bss_shdr[4..], SHT_NOBITS);
        write_u64(&mut bss_shdr[8..], SHF_ALLOC | SHF_WRITE);
        write_u64(&mut bss_shdr[32..], self.bss_len as u64);
        write_u64(&mut bss_shdr[48..], 8);
        syscall::sys_write(fd, &bss_shdr);

        // .rela.text section
        let mut relatext_shdr = [0u8; 64];
        write_u32(&mut relatext_shdr[0..], shname_relatext as u32);
        write_u32(&mut relatext_shdr[4..], SHT_RELA);
        write_u64(&mut relatext_shdr[8..], SHF_INFO_LINK);
        write_u64(&mut relatext_shdr[24..], relatext_off as u64);
        write_u64(&mut relatext_shdr[32..], (text_relocs * rela_size) as u64);
        write_u32(&mut relatext_shdr[40..], 6); // sh_link = .symtab
        write_u32(&mut relatext_shdr[44..], 1); // sh_info = .text
        write_u64(&mut relatext_shdr[48..], 8);
        write_u64(&mut relatext_shdr[56..], rela_size as u64);
        syscall::sys_write(fd, &relatext_shdr);

        // .rela.data section
        let mut reladata_shdr = [0u8; 64];
        write_u32(&mut reladata_shdr[0..], shname_reladata as u32);
        write_u32(&mut reladata_shdr[4..], SHT_RELA);
        write_u64(&mut reladata_shdr[8..], SHF_INFO_LINK);
        write_u64(&mut reladata_shdr[24..], reladata_off as u64);
        write_u64(&mut reladata_shdr[32..], (data_relocs * rela_size) as u64);
        write_u32(&mut reladata_shdr[40..], 6);
        write_u32(&mut reladata_shdr[44..], 2);
        write_u64(&mut reladata_shdr[48..], 8);
        write_u64(&mut reladata_shdr[56..], rela_size as u64);
        syscall::sys_write(fd, &reladata_shdr);

        // .symtab section
        let mut symtab_shdr = [0u8; 64];
        write_u32(&mut symtab_shdr[0..], shname_symtab as u32);
        write_u32(&mut symtab_shdr[4..], SHT_SYMTAB);
        write_u64(&mut symtab_shdr[24..], symtab_off as u64);
        write_u64(&mut symtab_shdr[32..], ((4 + self.num_symbols) * sym_size) as u64);
        write_u32(&mut symtab_shdr[40..], 7); // sh_link = .strtab
        write_u32(&mut symtab_shdr[44..], num_local_syms as u32); // sh_info = first global
        write_u64(&mut symtab_shdr[48..], 8);
        write_u64(&mut symtab_shdr[56..], sym_size as u64);
        syscall::sys_write(fd, &symtab_shdr);

        // .strtab section
        let mut strtab_shdr = [0u8; 64];
        write_u32(&mut strtab_shdr[0..], shname_strtab as u32);
        write_u32(&mut strtab_shdr[4..], SHT_STRTAB);
        write_u64(&mut strtab_shdr[24..], strtab_off as u64);
        write_u64(&mut strtab_shdr[32..], (strtab_len + 4096) as u64);
        write_u64(&mut strtab_shdr[48..], 1);
        syscall::sys_write(fd, &strtab_shdr);

        // .shstrtab section
        let mut shstrtab_shdr = [0u8; 64];
        write_u32(&mut shstrtab_shdr[0..], shname_shstrtab as u32);
        write_u32(&mut shstrtab_shdr[4..], SHT_STRTAB);
        write_u64(&mut shstrtab_shdr[24..], shstrtab_off as u64);
        write_u64(&mut shstrtab_shdr[32..], shstrtab_len as u64);
        write_u64(&mut shstrtab_shdr[48..], 1);
        syscall::sys_write(fd, &shstrtab_shdr);

        // Write .text section
        syscall::sys_write(fd, &self.text[..self.text_len]);

        // Write .data section
        syscall::sys_write(fd, &self.data[..self.data_len]);

        // Write .shstrtab
        syscall::sys_write(fd, &shstrtab[..shstrtab_len]);

        // Write .rela.text
        for i in 0..self.num_relocs {
            if self.relocs[i].section == 1 {
                let mut rela = [0u8; 24];
                write_u64(&mut rela[0..], self.relocs[i].offset);
                let sym_idx = self.relocs[i].sym_idx + 4; // Offset for section symbols
                let info = ((sym_idx as u64) << 32) | (self.relocs[i].rtype as u64);
                write_u64(&mut rela[8..], info);
                write_i64(&mut rela[16..], self.relocs[i].addend);
                syscall::sys_write(fd, &rela);
            }
        }

        // Write .rela.data
        for i in 0..self.num_relocs {
            if self.relocs[i].section == 2 {
                let mut rela = [0u8; 24];
                write_u64(&mut rela[0..], self.relocs[i].offset);
                let sym_idx = self.relocs[i].sym_idx + 4;
                let info = ((sym_idx as u64) << 32) | (self.relocs[i].rtype as u64);
                write_u64(&mut rela[8..], info);
                write_i64(&mut rela[16..], self.relocs[i].addend);
                syscall::sys_write(fd, &rela);
            }
        }

        // Write .symtab
        // NULL symbol
        let null_sym = [0u8; 24];
        syscall::sys_write(fd, &null_sym);

        // Section symbols
        for (idx, shndx) in [(1u16, 1u16), (2, 2), (3, 3)] {
            let mut sym = [0u8; 24];
            sym[4] = STT_SECTION;
            write_u16(&mut sym[6..], shndx);
            syscall::sys_write(fd, &sym);
        }

        // User symbols - locals first, then globals
        for pass in 0..2 {
            let want_global = pass == 1;
            for i in 0..self.num_symbols {
                let is_global = self.symbols[i].binding == 1;
                if is_global != want_global {
                    continue;
                }

                let name_off = strtab_len;
                let name_len = self.symbols[i].name.iter().position(|&c| c == 0).unwrap_or(0);
                strtab[strtab_len..strtab_len + name_len].copy_from_slice(&self.symbols[i].name[..name_len]);
                strtab_len += name_len + 1;

                let mut sym = [0u8; 24];
                write_u32(&mut sym[0..], name_off as u32);
                sym[4] = if is_global { (STB_GLOBAL << 4) | STT_NOTYPE } else { STT_NOTYPE };

                let shndx = if self.symbols[i].defined {
                    self.symbols[i].section as u16
                } else {
                    0 // SHN_UNDEF
                };
                write_u16(&mut sym[6..], shndx);
                write_u64(&mut sym[8..], self.symbols[i].value);
                syscall::sys_write(fd, &sym);
            }
        }

        // Write .strtab
        syscall::sys_write(fd, &strtab[..strtab_len]);

        // Pad to shdr_off if needed
        let current_pos = strtab_off + strtab_len;
        if current_pos < shdr_off {
            let padding = [0u8; 64];
            let mut remaining = shdr_off - current_pos;
            while remaining > 0 {
                let chunk = remaining.min(64);
                syscall::sys_write(fd, &padding[..chunk]);
                remaining -= chunk;
            }
        }
    }
}

/// Copy bytes to buffer
fn copy_to_buf(buf: &mut [u8], pos: &mut usize, data: &[u8]) {
    let len = data.len();
    if *pos + len <= buf.len() {
        buf[*pos..*pos + len].copy_from_slice(data);
        *pos += len;
    }
}

/// Write u16 little-endian
fn write_u16(buf: &mut [u8], v: u16) {
    buf[0] = v as u8;
    buf[1] = (v >> 8) as u8;
}

/// Write u32 little-endian
fn write_u32(buf: &mut [u8], v: u32) {
    buf[0] = v as u8;
    buf[1] = (v >> 8) as u8;
    buf[2] = (v >> 16) as u8;
    buf[3] = (v >> 24) as u8;
}

/// Write u64 little-endian
fn write_u64(buf: &mut [u8], v: u64) {
    write_u32(&mut buf[0..], v as u32);
    write_u32(&mut buf[4..], (v >> 32) as u32);
}

/// Write i64 little-endian
fn write_i64(buf: &mut [u8], v: i64) {
    write_u64(buf, v as u64);
}

/// Main entry point
static mut ASM: Assembler = Assembler::new();

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: as [-o output] input.s");
        return 1;
    }

    // Parse arguments
    let mut input_file: &[u8] = b"";
    let mut output_file: &[u8] = b"a.out";

    let mut i = 1;
    while i < argc as usize {
        let arg = get_arg(argv, i);
        if bytes_eq_len(arg, b"-o") {
            i += 1;
            if i < argc as usize {
                output_file = get_arg(argv, i);
            }
        } else if arg[0] != b'-' {
            input_file = arg;
        }
        i += 1;
    }

    if input_file.is_empty() {
        eprintlns("as: no input file");
        return 1;
    }

    let asm = unsafe { &mut *core::ptr::addr_of_mut!(ASM) };

    // Read input file
    let fd = open2(bytes_to_str(input_file), O_RDONLY);
    if fd < 0 {
        eprints("as: cannot open ");
        eprintlns(bytes_to_str(input_file));
        return 1;
    }

    let n = syscall::sys_read(fd, &mut asm.source);
    close(fd);

    if n < 0 {
        eprintlns("as: read error");
        return 1;
    }

    asm.source_len = n as usize;

    // Assemble
    asm.parse();

    if asm.had_error {
        return 1;
    }

    // Write output
    let out_fd = open(bytes_to_str(output_file), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if out_fd < 0 {
        eprints("as: cannot create ");
        eprintlns(bytes_to_str(output_file));
        return 1;
    }

    asm.write_elf(out_fd);
    close(out_fd);

    0
}

/// Get argument at index
fn get_arg(argv: *const *const u8, idx: usize) -> &'static [u8] {
    unsafe {
        let ptr = *argv.add(idx);
        if ptr.is_null() {
            return b"";
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::slice::from_raw_parts(ptr, len)
    }
}

/// Convert byte slice to str
fn bytes_to_str(s: &[u8]) -> &str {
    unsafe { core::str::from_utf8_unchecked(s) }
}
