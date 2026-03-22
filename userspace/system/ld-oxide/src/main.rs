//! OXIDE Dynamic Linker/Loader (ld-oxide.so.1)
//!
//! — IronGhost: fully self-contained, no libc dependency.
//! Supports recursive DT_NEEDED loading — if libreadline.so needs libc.so,
//! ld-oxide loads both. Breadth-first loading ensures correct symbol scope.

#![no_std]
#![no_main]

mod elf;
mod reloc;
mod symbol;
mod link_map;
mod sys;

#[cfg(target_arch = "x86_64")]
mod arch_x86_64;

const LIB_PATHS: &[&[u8]] = &[b"/usr/lib/", b"/lib/"];
const MAX_LIBS: usize = 32;
/// — IronGhost: track loaded library names to prevent double-loading
const MAX_LOADED_NAMES: usize = 32;
const MAX_NAME_LEN: usize = 64;

mod at {
    pub const AT_NULL: u64 = 0;
    pub const AT_PHDR: u64 = 3;
    pub const AT_PHENT: u64 = 4;
    pub const AT_PHNUM: u64 = 5;
    pub const AT_PAGESZ: u64 = 6;
    pub const AT_BASE: u64 = 7;
    pub const AT_ENTRY: u64 = 9;
}

struct AuxVec { phdr: u64, phent: u64, phnum: u64, pagesz: u64, base: u64, entry: u64 }
impl AuxVec { fn new() -> Self { Self { phdr:0, phent:0, phnum:0, pagesz:4096, base:0, entry:0 } } }

unsafe fn parse_auxv(mut ptr: *const u64) -> AuxVec {
    let mut aux = AuxVec::new();
    loop {
        let t = unsafe { *ptr }; let v = unsafe { *ptr.add(1) }; ptr = ptr.add(2);
        match t { at::AT_NULL => break, at::AT_PHDR => aux.phdr = v, at::AT_PHENT => aux.phent = v,
            at::AT_PHNUM => aux.phnum = v, at::AT_PAGESZ => aux.pagesz = v,
            at::AT_BASE => aux.base = v, at::AT_ENTRY => aux.entry = v, _ => {} }
    }
    aux
}

struct LoadedLib { base: u64, dyn_info: elf::DynamicInfo, initialized: bool }

/// — IronGhost: dedup tracker — prevents loading the same .so twice
struct LoadedNames {
    names: [[u8; MAX_NAME_LEN]; MAX_LOADED_NAMES],
    lens: [usize; MAX_LOADED_NAMES],
    count: usize,
}

impl LoadedNames {
    const fn new() -> Self {
        Self { names: [[0; MAX_NAME_LEN]; MAX_LOADED_NAMES], lens: [0; MAX_LOADED_NAMES], count: 0 }
    }
    fn contains(&self, name: &[u8]) -> bool {
        for i in 0..self.count {
            if self.lens[i] == name.len() && self.names[i][..name.len()] == *name { return true; }
        }
        false
    }
    fn add(&mut self, name: &[u8]) {
        if self.count >= MAX_LOADED_NAMES || name.len() >= MAX_NAME_LEN { return; }
        self.names[self.count][..name.len()].copy_from_slice(name);
        self.lens[self.count] = name.len();
        self.count += 1;
    }
}

fn read_file(path: &[u8]) -> (*mut u8, usize) {
    let fd = sys::sys_open(path, sys::O_RDONLY, 0);
    if fd < 0 { return (core::ptr::null_mut(), 0); }
    let mut hdr = [0u8; 64];
    let n = sys::sys_read(fd, &mut hdr);
    if n < 64 || hdr[0..4] != [0x7f, b'E', b'L', b'F'] { sys::sys_close(fd); return (core::ptr::null_mut(), 0); }
    let e_phoff = u64::from_le_bytes(hdr[32..40].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(hdr[54..56].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(hdr[56..58].try_into().unwrap()) as usize;
    let ph_size = e_phentsize * e_phnum;
    let total_hdr = e_phoff + ph_size;
    let hdr_pages = (total_hdr + 4095) / 4096;
    let hdr_map = sys::sys_mmap(0, (hdr_pages * 4096) as u64, sys::PROT_READ | sys::PROT_WRITE, sys::MAP_PRIVATE | sys::MAP_ANONYMOUS, -1, 0);
    if (hdr_map as i64) < 0 { sys::sys_close(fd); return (core::ptr::null_mut(), 0); }
    let hdr_slice = unsafe { core::slice::from_raw_parts_mut(hdr_map as *mut u8, total_hdr) };
    hdr_slice[..64].copy_from_slice(&hdr);
    if total_hdr > 64 { let rest = &mut hdr_slice[64..]; let mut rp = 0; while rp < rest.len() { let n = sys::sys_read(fd, &mut rest[rp..]); if n <= 0 { break; } rp += n as usize; } }
    let mut max_extent = total_hdr;
    let ph_data = &hdr_slice[e_phoff..e_phoff + ph_size];
    for i in 0..e_phnum { let o = i * e_phentsize; if o + 56 > ph_data.len() { break; }
        let pt = u32::from_le_bytes(ph_data[o..o+4].try_into().unwrap());
        let po = u64::from_le_bytes(ph_data[o+8..o+16].try_into().unwrap()) as usize;
        let pf = u64::from_le_bytes(ph_data[o+32..o+40].try_into().unwrap()) as usize;
        if (pt == 1 || pt == 2 || pt == 7) && pf > 0 { let e = po + pf; if e > max_extent { max_extent = e; } }
    }
    let file_pages = (max_extent + 4095) / 4096;
    let file_map = sys::sys_mmap(0, (file_pages * 4096) as u64, sys::PROT_READ | sys::PROT_WRITE, sys::MAP_PRIVATE | sys::MAP_ANONYMOUS, -1, 0);
    if (file_map as i64) < 0 { sys::sys_close(fd); return (core::ptr::null_mut(), 0); }
    let file_slice = unsafe { core::slice::from_raw_parts_mut(file_map as *mut u8, max_extent) };
    let cl = core::cmp::min(total_hdr, max_extent);
    file_slice[..cl].copy_from_slice(&hdr_slice[..cl]);
    if max_extent > total_hdr { let rest = &mut file_slice[total_hdr..]; let mut rp = 0; while rp < rest.len() { let n = sys::sys_read(fd, &mut rest[rp..]); if n <= 0 { break; } rp += n as usize; } }
    sys::sys_close(fd);
    (file_map as *mut u8, max_extent)
}

fn load_library_segments(elf_data: &[u8], base_addr: u64) -> Option<u64> {
    if elf_data.len() < 64 || elf_data[0..4] != [0x7f, b'E', b'L', b'F'] { return None; }
    let e_phoff = u64::from_le_bytes(elf_data[32..40].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(elf_data[54..56].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(elf_data[56..58].try_into().unwrap()) as usize;
    let mut min_vaddr: u64 = u64::MAX; let mut max_vaddr: u64 = 0;
    for i in 0..e_phnum { let off = e_phoff + i * e_phentsize; if off + 56 > elf_data.len() { break; }
        let pt = u32::from_le_bytes(elf_data[off..off+4].try_into().unwrap()); if pt != 1 { continue; }
        let pv = u64::from_le_bytes(elf_data[off+16..off+24].try_into().unwrap());
        // — IronGhost: p_memsz is at offset 40, NOT 32 (that's p_filesz).
        // Using p_filesz here was the bug — BSS (mem > file) wasn't included
        // in the mapped range, causing SIGSEGV on first write to BSS globals.
        let pm = u64::from_le_bytes(elf_data[off+40..off+48].try_into().unwrap());
        let s = pv & !0xFFF; let e = (pv + pm + 0xFFF) & !0xFFF;
        if s < min_vaddr { min_vaddr = s; } if e > max_vaddr { max_vaddr = e; }
    }
    if min_vaddr == u64::MAX { return None; }
    let total_size = max_vaddr - min_vaddr;
    let base_offset = base_addr - min_vaddr;
    // — IronGhost: use NON-fixed mmap first to get pages, then remap with MAP_FIXED.
    // Actually no — just do the mmap without MAP_FIXED and accept the address the kernel gives us.
    // The problem with MAP_FIXED is that the kernel removes VMAs in the target range,
    // and anonymous private creates demand-paged VMAs. The page fault handler might
    // not find the VMA later if something goes wrong.
    //
    // Instead: use MAP_FIXED but NOT anonymous — we'll just do a simple mmap and
    // manually zero the pages. The key insight: the bug might be that the kernel's
    // fault handler can't find the VMA created by the previous mmap because the
    // next library's MAP_FIXED mmap destroyed it (if ranges overlap due to rounding).
    let map = sys::sys_mmap(base_addr, total_size, sys::PROT_READ | sys::PROT_WRITE | sys::PROT_EXEC,
        sys::MAP_PRIVATE | sys::MAP_ANONYMOUS | sys::MAP_FIXED, -1, 0);
    if map == 0 || (map as i64) < 0 { return None; }
    for i in 0..e_phnum { let off = e_phoff + i * e_phentsize; if off + 56 > elf_data.len() { break; }
        let pt = u32::from_le_bytes(elf_data[off..off+4].try_into().unwrap()); if pt != 1 { continue; }
        let po = u64::from_le_bytes(elf_data[off+8..off+16].try_into().unwrap()) as usize;
        let pv = u64::from_le_bytes(elf_data[off+16..off+24].try_into().unwrap());
        let pf = u64::from_le_bytes(elf_data[off+32..off+40].try_into().unwrap()) as usize;
        if pf > 0 && po + pf <= elf_data.len() {
            unsafe { core::ptr::copy_nonoverlapping(elf_data[po..].as_ptr(), (pv + base_offset) as *mut u8, pf); }
        }
    }
    Some(base_offset)
}

fn find_and_read_library(name: &[u8]) -> (*mut u8, usize) {
    let mut path_buf = [0u8; 256];
    for &dir in LIB_PATHS {
        if dir.len() + name.len() >= 255 { continue; }
        path_buf[..dir.len()].copy_from_slice(dir);
        path_buf[dir.len()..dir.len() + name.len()].copy_from_slice(name);
        let total = dir.len() + name.len();
        let (ptr, len) = read_file(&path_buf[..total]);
        if !ptr.is_null() { return (ptr, len); }
    }
    (core::ptr::null_mut(), 0)
}

/// — IronGhost: load a single library by name. Returns true if loaded (or already loaded).
/// Adds the library's DT_NEEDED entries to the pending queue for recursive loading.
fn load_one_library(
    name: &[u8],
    libs: &mut [Option<LoadedLib>; MAX_LIBS],
    lib_count: &mut usize,
    next_lib_base: &mut u64,
    loaded_names: &mut LoadedNames,
    pending: &mut [[u8; MAX_NAME_LEN]; MAX_LIBS],
    pending_lens: &mut [usize; MAX_LIBS],
    pending_count: &mut usize,
) -> bool {
    // — IronGhost: dedup — skip if already loaded
    if loaded_names.contains(name) { return true; }

    sys::write_str("  -> ");
    sys::write_bytes(name);

    let (data_ptr, data_len) = find_and_read_library(name);
    if data_ptr.is_null() { sys::write_str(" [NOT FOUND]\n"); return false; }

    let elf_data = unsafe { core::slice::from_raw_parts(data_ptr, data_len) };
    let base_offset = match load_library_segments(elf_data, *next_lib_base) {
        Some(b) => b,
        None => { sys::write_str(" [LOAD FAILED]\n"); return false; }
    };

    // Find PT_DYNAMIC
    let e_phoff = u64::from_le_bytes(elf_data[32..40].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(elf_data[54..56].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(elf_data[56..58].try_into().unwrap()) as usize;
    let mut lib_dyn_addr: u64 = 0;
    let mut lib_min_vaddr: u64 = u64::MAX;
    let mut lib_max_vaddr: u64 = 0;
    for j in 0..e_phnum { let off = e_phoff + j * e_phentsize; if off + 56 > elf_data.len() { break; }
        let pt = u32::from_le_bytes(elf_data[off..off+4].try_into().unwrap());
        let pv = u64::from_le_bytes(elf_data[off+16..off+24].try_into().unwrap());
        let pm = u64::from_le_bytes(elf_data[off+40..off+48].try_into().unwrap()); // p_memsz, NOT p_filesz
        if pt == 2 { lib_dyn_addr = pv + base_offset; }
        if pt == 1 { let s = pv & !0xFFF; let e = ((pv + pm) + 0xFFF) & !0xFFF;
            if s < lib_min_vaddr { lib_min_vaddr = s; } if e > lib_max_vaddr { lib_max_vaddr = e; } }
    }

    if lib_dyn_addr != 0 {
        let mut lib_dyn_info = unsafe { elf::parse_dynamic(lib_dyn_addr) };
        lib_dyn_info.relocate(base_offset);

        // — IronGhost: queue this library's own DT_NEEDED entries for recursive loading
        if lib_dyn_info.needed_count > 0 && lib_dyn_info.strtab != 0 {
            for i in 0..lib_dyn_info.needed_count {
                let dep_name = unsafe { elf::strtab_get(lib_dyn_info.strtab, lib_dyn_info.needed[i]) };
                if !loaded_names.contains(dep_name) && *pending_count < MAX_LIBS && dep_name.len() < MAX_NAME_LEN {
                    pending[*pending_count][..dep_name.len()].copy_from_slice(dep_name);
                    pending_lens[*pending_count] = dep_name.len();
                    *pending_count += 1;
                }
            }
        }

        if *lib_count < MAX_LIBS {
            libs[*lib_count] = Some(LoadedLib { base: base_offset, dyn_info: lib_dyn_info, initialized: false });
            *lib_count += 1;
        }
    }

    loaded_names.add(name);

    let lib_size = if lib_max_vaddr > lib_min_vaddr { lib_max_vaddr - lib_min_vaddr } else { 0x20_0000 };
    *next_lib_base += (lib_size + 0xFFF) & !0xFFF;
    *next_lib_base = (*next_lib_base + 0x1F_FFFF) & !0x1F_FFFF;

    sys::write_str(" [OK]\n");
    true
}

/// — ThreadRogue: look up a symbol in a single DynamicInfo (hash-based).
/// Returns absolute address or 0.
fn lookup_in_dyninfo(name: &[u8], di: &elf::DynamicInfo, base: u64) -> u64 {
    if di.symtab == 0 || di.strtab == 0 { return 0; }

    let sym = if di.gnu_hash != 0 {
        unsafe { symbol::lookup_gnu_hash(name, di.gnu_hash, di.symtab, di.strtab, di.syment) }
    } else { None };

    let sym = sym.or_else(|| {
        if di.hash != 0 {
            unsafe { symbol::lookup_elf_hash(name, di.hash, di.symtab, di.strtab, di.syment) }
        } else { None }
    });

    if let Some(s) = sym {
        if s.st_shndx != 0 && s.st_value != 0 { return s.st_value + base; }
    }
    0
}

/// — ThreadRogue: global symbol resolution — search main exe first, then all loaded libs.
/// Main exe has priority (like Linux: executable scope searched before libraries).
/// WEAK symbols are resolved like GLOBAL — first definition in scope order wins.
/// This matches Linux behavior: executable scope > library DT_NEEDED order.
static mut MAIN_DYN_INFO: Option<elf::DynamicInfo> = None;

fn resolve_symbol_global(name: &[u8], libs: &[Option<LoadedLib>; MAX_LIBS], lib_count: usize) -> u64 {
    // — ThreadRogue: search main executable first (base=0, loaded at linked address)
    let main_di_ref = unsafe { &*(&raw const MAIN_DYN_INFO) };
    if let Some(main_di) = main_di_ref {
        let addr = lookup_in_dyninfo(name, main_di, 0);
        if addr != 0 { return addr; }
    }

    // — ThreadRogue: then search all loaded shared libraries
    for i in 0..lib_count {
        if let Some(ref lib) = libs[i] {
            let addr = lookup_in_dyninfo(name, &lib.dyn_info, lib.base);
            if addr != 0 { return addr; }
        }
    }
    0
}

fn apply_relocations_with_resolution(
    base: u64, rela_addr: u64, rela_size: u64, rela_ent: u64,
    symtab: u64, syment: u64, strtab: u64,
    libs: &[Option<LoadedLib>; MAX_LIBS], lib_count: usize,
) {
    if rela_addr == 0 || rela_size == 0 { return; }
    let count = rela_size / rela_ent;
    for i in 0..count {
        let rela = unsafe { &*((rela_addr + i * rela_ent) as *const elf::Elf64Rela) };
        let sym_value: u64 = if rela.r_sym() != 0 && symtab != 0 && strtab != 0 {
            let sym = unsafe { symbol::get_sym_by_index(symtab, syment, rela.r_sym()) };
            let name = unsafe { symbol::get_sym_name(strtab, sym) };
            if sym.st_shndx != 0 && sym.st_value != 0 { sym.st_value + base }
            else { resolve_symbol_global(name, libs, lib_count) }
        } else { 0 };
        unsafe { reloc::apply_relocation(base, rela, sym_value); }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dl_main(sp: *const u64) -> ! {
    let argc = unsafe { *sp } as usize;
    let mut ptr = unsafe { sp.add(1) };
    ptr = unsafe { ptr.add(argc + 1) };
    while unsafe { *ptr } != 0 { ptr = unsafe { ptr.add(1) }; }
    ptr = unsafe { ptr.add(1) };
    let aux = unsafe { parse_auxv(ptr) };

    sys::write_str("[ld-oxide] dynamic linker started\n");

    let mut libs: [Option<LoadedLib>; MAX_LIBS] = [const { None }; MAX_LIBS];
    let mut lib_count: usize = 0;
    let mut next_lib_base: u64 = 0x3000_0000;
    let mut loaded_names = LoadedNames::new();

    // — IronGhost: pending queue for breadth-first recursive DT_NEEDED loading
    let mut pending: [[u8; MAX_NAME_LEN]; MAX_LIBS] = [[0; MAX_NAME_LEN]; MAX_LIBS];
    let mut pending_lens: [usize; MAX_LIBS] = [0; MAX_LIBS];
    let mut pending_count: usize = 0;

    if aux.phdr != 0 {
        let phdr_base = aux.phdr as *const elf::Elf64Phdr;
        let dynamic_addr = elf::find_dynamic(phdr_base, aux.phnum as usize);

        if let Some(dyn_addr) = dynamic_addr {
            let dyn_info = unsafe { elf::parse_dynamic(dyn_addr) };

            // — ThreadRogue: store main exe's .dynamic for symbol resolution.
            // Main exe symbols are searched first (like Linux's executable scope).
            unsafe { *(&raw mut MAIN_DYN_INFO) = Some(dyn_info.clone()); }

            // ============================================================
            // PHASE 1: Seed the loading queue with main exe's DT_NEEDED
            // ============================================================
            if dyn_info.needed_count > 0 && dyn_info.strtab != 0 {
                for i in 0..dyn_info.needed_count {
                    let name = unsafe { elf::strtab_get(dyn_info.strtab, dyn_info.needed[i]) };
                    if name.len() < MAX_NAME_LEN && pending_count < MAX_LIBS {
                        pending[pending_count][..name.len()].copy_from_slice(name);
                        pending_lens[pending_count] = name.len();
                        pending_count += 1;
                    }
                }
            }

            // ============================================================
            // PHASE 2: Load libraries breadth-first (recursive DT_NEEDED)
            // — IronGhost: process the queue. Each loaded lib may add more
            // entries to the queue via its own DT_NEEDED. Loop until empty.
            // ============================================================
            let mut processed = 0usize;
            if pending_count > 0 {
                sys::write_str("[ld-oxide] loading shared libraries\n");
            }
            while processed < pending_count {
                // — SableWire: copy name to local buffer to avoid borrow conflict
                // (load_one_library mutably borrows pending to add recursive deps)
                let mut name_buf = [0u8; MAX_NAME_LEN];
                let name_len = pending_lens[processed];
                name_buf[..name_len].copy_from_slice(&pending[processed][..name_len]);

                load_one_library(
                    &name_buf[..name_len], &mut libs, &mut lib_count, &mut next_lib_base,
                    &mut loaded_names, &mut pending, &mut pending_lens, &mut pending_count,
                );
                processed += 1;
            }

            // ============================================================
            // PHASE 3: Apply relocations with full symbol resolution
            // ============================================================
            for i in 0..lib_count {
                if let Some(ref lib) = libs[i] {
                    let di = &lib.dyn_info;
                    apply_relocations_with_resolution(lib.base, di.rela, di.relasz, di.relaent,
                        di.symtab, di.syment, di.strtab, &libs, lib_count);
                    apply_relocations_with_resolution(lib.base, di.jmprel, di.pltrelsz, 24,
                        di.symtab, di.syment, di.strtab, &libs, lib_count);
                }
            }

            // ============================================================
            // PHASE 2.5: Set up PLT lazy binding (GOT[1]/GOT[2])
            // — WireSaint: for each library with a PLTGOT, set up the lazy
            // resolver. GOT[0] = .dynamic addr, GOT[1] = lib index cookie,
            // GOT[2] = dl_runtime_resolve trampoline. When a PLT stub is
            // called for the first time, it pushes its reloc index and jumps
            // to GOT[2], which resolves the symbol and patches the GOT entry.
            //
            // Since we already did eager binding (all JUMP_SLOT resolved above),
            // this is redundant but establishes the infrastructure for future
            // lazy mode. If a GOT entry was somehow missed, the resolver catches it.
            // ============================================================
            // — WireSaint: set up GOT[1]/GOT[2] for each library with a PLTGOT.
            // Even with eager binding, this provides the fallback trampoline.
            #[cfg(target_arch = "x86_64")]
            let resolver_addr = arch_x86_64::_dl_runtime_resolve as u64;
            for i in 0..lib_count {
                if let Some(ref lib) = libs[i] {
                    if lib.dyn_info.pltgot != 0 {
                        unsafe { reloc::setup_got_plt(lib.dyn_info.pltgot, lib.base, resolver_addr); }
                    }
                }
            }

            // Main exe relocations
            if dyn_info.rela != 0 && dyn_info.relasz > 0 {
                apply_relocations_with_resolution(0, dyn_info.rela, dyn_info.relasz, dyn_info.relaent,
                    dyn_info.symtab, dyn_info.syment, dyn_info.strtab, &libs, lib_count);
            }
            if dyn_info.jmprel != 0 && dyn_info.pltrelsz > 0 {
                apply_relocations_with_resolution(0, dyn_info.jmprel, dyn_info.pltrelsz, 24,
                    dyn_info.symtab, dyn_info.syment, dyn_info.strtab, &libs, lib_count);
            }

            // ============================================================
            // PHASE 4: Run constructors (dependency order = load order)
            // ============================================================
            for i in 0..lib_count {
                if let Some(ref mut lib) = libs[i] {
                    if lib.initialized { continue; }
                    let di = &lib.dyn_info;
                    if di.init != 0 { let f: extern "C" fn() = unsafe { core::mem::transmute(di.init) }; f(); }
                    if di.init_array != 0 && di.init_arraysz > 0 {
                        let count = di.init_arraysz / 8;
                        for j in 0..count {
                            let addr = unsafe { *((di.init_array + j * 8) as *const u64) };
                            if addr != 0 { let f: extern "C" fn() = unsafe { core::mem::transmute(addr) }; f(); }
                        }
                    }
                    lib.initialized = true;
                }
            }
        }
    }

    // ============================================================
    // PHASE 5: Jump to main executable
    // ============================================================
    if aux.entry != 0 {
        sys::write_str("[ld-oxide] jumping to entry ");
        sys::write_hex(aux.entry);
        sys::write_str("\n");
        // — SableWire: arch-specific jump — lives in arch_x86_64.rs, not here.
        #[cfg(target_arch = "x86_64")]
        unsafe { arch_x86_64::jump_to_entry(sp, aux.entry); }
    }
    sys::write_str("[ld-oxide] FATAL: no AT_ENTRY\n");
    sys::exit(127);
}

// — SableWire: _start lives in arch_x86_64.rs (or arch_aarch64.rs for ARM).
// No arch-specific asm in this file.

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! { sys::write_str("[ld-oxide] PANIC\n"); sys::exit(1); }
