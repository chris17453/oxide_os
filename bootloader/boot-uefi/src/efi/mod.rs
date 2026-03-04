//! Custom UEFI bindings — raw #[repr(C)] structs matching UEFI 2.10 spec.
//! Zero heap. Zero third-party deps. Every byte explicit.
//!
//! — SableWire: we own the metal now — no more hidden allocators, no more mystery state

pub mod types;
pub mod guid;
pub mod system_table;
pub mod boot_services;
pub mod runtime;
pub mod text;
pub mod gop;
pub mod fs;
pub mod mem;
pub mod scratch;
pub mod fmt;

// Re-export commonly used types for convenience
pub use types::*;
pub use guid::*;
pub use system_table::*;
pub use boot_services::{
    EfiBootServices,
    ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, EFI_LOADER_CODE,
    EFI_BOOT_SERVICES_CODE, EFI_BOOT_SERVICES_DATA,
    EFI_CONVENTIONAL_MEMORY, EFI_ACPI_RECLAIM_MEMORY, EFI_ACPI_MEMORY_NVS,
    EFI_RESERVED_MEMORY_TYPE, BY_PROTOCOL,
    EFI_OPEN_PROTOCOL_BY_HANDLE_PROTOCOL,
    EFI_OPEN_PROTOCOL_GET_PROTOCOL,
    EFI_OPEN_PROTOCOL_EXCLUSIVE,
};
pub use runtime::*;
pub use text::*;
pub use gop::*;
pub use fs::*;
pub use mem::*;
pub use fmt::FmtBuf;

use core::ptr;

/// Global system table pointer — set once in init(), read everywhere else.
/// — SableWire: the one mutable global we actually need
static mut SYSTEM_TABLE: *mut EfiSystemTable = ptr::null_mut();

/// Global image handle — needed for exit_boot_services
static mut IMAGE_HANDLE: EfiHandle = ptr::null_mut();

/// Initialize the EFI subsystem. Must be called first thing in efi_main.
/// — SableWire: plug in the umbilical cord — everything else flows from this
pub unsafe fn init(handle: EfiHandle, st: *mut EfiSystemTable) {
    unsafe {
        IMAGE_HANDLE = handle;
        SYSTEM_TABLE = st;

        // Initialize scratch arena
        let bs = (*st).boot_services;
        if !scratch::ScratchArena::init(bs) {
            // If scratch init fails, we can still function for a bit without temp allocations
            // but file loading will fail. Print a warning if we can.
            let con_out = (*st).con_out;
            if !con_out.is_null() {
                let msg: [u16; 30] = encode_ucs2_const(b"[WARN] Scratch arena init fail");
                ((*con_out).output_string)(con_out, msg.as_ptr());
            }
        }
    }
}

/// Get the system table pointer. Returns None if not initialized.
/// — SableWire: reach into the firmware — returns None if the bridge is burned
#[inline]
pub fn system_table() -> Option<&'static mut EfiSystemTable> {
    unsafe {
        if SYSTEM_TABLE.is_null() {
            None
        } else {
            Some(&mut *SYSTEM_TABLE)
        }
    }
}

/// Get the boot services pointer. Returns None if not initialized.
#[inline]
pub fn boot_services() -> Option<&'static mut EfiBootServices> {
    unsafe {
        let st = system_table()?;
        if st.boot_services.is_null() {
            None
        } else {
            Some(&mut *st.boot_services)
        }
    }
}

/// Get the image handle
#[inline]
pub fn image_handle() -> EfiHandle {
    unsafe { IMAGE_HANDLE }
}

/// Get a protocol interface by GUID using LocateProtocol
/// — SableWire: the universal protocol finder — GUID in, interface out
pub fn locate_protocol<T>(guid: &EfiGuid) -> Option<*mut T> {
    let bs = boot_services()?;
    let mut interface: *mut core::ffi::c_void = ptr::null_mut();
    let status = unsafe {
        (bs.locate_protocol)(
            guid as *const EfiGuid,
            ptr::null_mut(),
            &mut interface,
        )
    };
    if efi_error(status) || interface.is_null() {
        None
    } else {
        Some(interface as *mut T)
    }
}

/// Get a protocol interface from a specific handle using HandleProtocol
pub fn handle_protocol<T>(handle: EfiHandle, guid: &EfiGuid) -> Option<*mut T> {
    let bs = boot_services()?;
    let mut interface: *mut core::ffi::c_void = ptr::null_mut();
    let status = unsafe {
        (bs.handle_protocol)(
            handle,
            guid as *const EfiGuid,
            &mut interface,
        )
    };
    if efi_error(status) || interface.is_null() {
        None
    } else {
        Some(interface as *mut T)
    }
}

/// Find a handle that supports a given protocol
/// — SableWire: protocol handle discovery — the UEFI equivalent of service lookup
pub fn locate_handle_for_protocol(guid: &EfiGuid) -> Option<EfiHandle> {
    let bs = boot_services()?;
    let mut buf_size: usize = 0;
    let mut handle: EfiHandle = ptr::null_mut();

    // First call to get required buffer size
    unsafe {
        let status = (bs.locate_handle)(
            BY_PROTOCOL,
            guid as *const EfiGuid,
            ptr::null_mut(),
            &mut buf_size,
            ptr::null_mut(),
        );
        // Should return BUFFER_TOO_SMALL with the required size
        if buf_size == 0 {
            return None;
        }
    }

    // For simplicity, we only need the first handle
    // Allocate space for one handle on the stack
    let mut handles = [ptr::null_mut(); 16];
    let mut actual_size = core::mem::size_of::<EfiHandle>() * 16;
    if actual_size < buf_size {
        actual_size = buf_size;
    }

    let status = unsafe {
        (bs.locate_handle)(
            BY_PROTOCOL,
            guid as *const EfiGuid,
            ptr::null_mut(),
            &mut actual_size,
            handles.as_mut_ptr(),
        )
    };

    if efi_error(status) {
        None
    } else {
        Some(handles[0])
    }
}

/// Stall (sleep) for the given number of microseconds
/// — SableWire: firmware-level busy wait — as accurate as the firmware cares to be
pub fn stall(microseconds: usize) {
    if let Some(bs) = boot_services() {
        unsafe { (bs.stall)(microseconds) };
    }
}

/// Allocate physical pages from UEFI boot services
/// — SableWire: the raw page allocator — LOADER_DATA type so the kernel knows we claimed them
pub fn allocate_pages(count: usize) -> Option<u64> {
    let bs = boot_services()?;
    let mut phys_addr: EfiPhysicalAddress = 0;
    let status = unsafe {
        (bs.allocate_pages)(
            ALLOCATE_ANY_PAGES,
            EFI_LOADER_DATA,
            count,
            &mut phys_addr,
        )
    };
    if efi_error(status) {
        None
    } else {
        Some(phys_addr)
    }
}

/// Print a UCS-2 string to the console output
/// — NeonVale: the lowest-level print — raw UTF-16 to firmware console
pub fn print_ucs2(s: &[u16]) {
    unsafe {
        let st = match system_table() {
            Some(st) => st,
            None => return,
        };
        let con_out = st.con_out;
        if !con_out.is_null() {
            ((*con_out).output_string)(con_out, s.as_ptr());
        }
    }
}

/// Print an ASCII string to the console output (converts to UCS-2 inline)
/// — NeonVale: the convenience wrapper — ASCII in, UCS-2 out, firmware happy
pub fn print_ascii(s: &str) {
    // Convert ASCII to UCS-2 in chunks to avoid giant stack buffers
    let bytes = s.as_bytes();
    let mut buf = [0u16; 128];
    let mut i = 0;

    for &b in bytes {
        buf[i] = b as u16;
        i += 1;
        if i >= 127 {
            buf[i] = 0; // null terminate
            print_ucs2(&buf[..=i]);
            i = 0;
        }
    }

    if i > 0 {
        buf[i] = 0;
        print_ucs2(&buf[..=i]);
    }
}

/// Print an ASCII string followed by \r\n
pub fn println_ascii(s: &str) {
    print_ascii(s);
    print_ucs2(&[b'\r' as u16, b'\n' as u16, 0]);
}

/// Clear the console screen
pub fn clear_screen() {
    unsafe {
        let st = match system_table() {
            Some(st) => st,
            None => return,
        };
        let con_out = st.con_out;
        if !con_out.is_null() {
            ((*con_out).clear_screen)(con_out);
        }
    }
}

/// Read a key from SimpleTextInput (non-blocking).
/// Returns Some(key) if a key is available, None otherwise.
/// — InputShade: the non-blocking key poll — returns NOT_READY when there's nothing to say
pub fn read_key() -> Option<EfiInputKey> {
    unsafe {
        let st = system_table()?;
        let con_in = st.con_in;
        if con_in.is_null() {
            return None;
        }
        let mut key = EfiInputKey::default();
        let status = ((*con_in).read_key_stroke)(con_in, &mut key);
        if efi_error(status) {
            None
        } else {
            Some(key)
        }
    }
}

/// Convert an ASCII byte slice to a null-terminated UCS-2 buffer on the stack.
/// Returns the buffer and its length (including null terminator).
/// — SableWire: ASCII → UTF-16 — because FAT32 is ASCII anyway
pub fn ascii_to_ucs2(ascii: &[u8], out: &mut [u16]) -> usize {
    let max = out.len().saturating_sub(1); // leave room for null
    let len = ascii.len().min(max);
    for i in 0..len {
        out[i] = ascii[i] as u16;
    }
    out[len] = 0; // null terminate
    len + 1 // return total length including null
}

/// Compile-time ASCII to UCS-2 conversion for short constant strings
/// Returns a fixed-size [u16; N] buffer, null-terminated
/// — SableWire: const fn magic for path literals
pub const fn encode_ucs2_const<const N: usize>(ascii: &[u8]) -> [u16; N] {
    let mut buf = [0u16; N];
    let len = if ascii.len() < N { ascii.len() } else { N - 1 };
    let mut i = 0;
    while i < len {
        buf[i] = ascii[i] as u16;
        i += 1;
    }
    // buf[len] is already 0 from initialization
    buf
}

/// Get the UEFI firmware vendor string as ASCII (best effort)
pub fn firmware_vendor_ascii(buf: &mut [u8]) -> usize {
    unsafe {
        let st = match system_table() {
            Some(st) => st,
            None => return 0,
        };
        let vendor = st.firmware_vendor;
        if vendor.is_null() {
            return 0;
        }
        let mut i = 0;
        loop {
            let ch = *vendor.add(i);
            if ch == 0 || i >= buf.len() - 1 {
                break;
            }
            buf[i] = if ch < 128 { ch as u8 } else { b'?' };
            i += 1;
        }
        i
    }
}

/// Get the firmware revision
pub fn firmware_revision() -> u32 {
    unsafe {
        match system_table() {
            Some(st) => st.firmware_revision,
            None => 0,
        }
    }
}

/// Get UEFI revision (major, minor)
pub fn uefi_revision() -> (u16, u16) {
    unsafe {
        match system_table() {
            Some(st) => {
                let rev = st.hdr.revision;
                ((rev >> 16) as u16, (rev & 0xFFFF) as u16)
            }
            None => (0, 0),
        }
    }
}

/// Get the number of configuration table entries
pub fn config_table_count() -> usize {
    unsafe {
        match system_table() {
            Some(st) => st.number_of_table_entries,
            None => 0,
        }
    }
}

/// Get a configuration table entry by index
pub fn config_table_entry(index: usize) -> Option<&'static EfiConfigurationTable> {
    unsafe {
        let st = system_table()?;
        if index >= st.number_of_table_entries || st.configuration_table.is_null() {
            return None;
        }
        Some(&*st.configuration_table.add(index))
    }
}

/// Exit boot services — the point of no return
/// Implements the spec-required retry loop: if the map key is stale,
/// re-get the memory map and try again.
/// — SableWire: burning the bridge — after this, the firmware is dead to us
pub unsafe fn exit_boot_services(
    mmap_buf: &mut [u8],
    out_map_key: &mut usize,
    out_desc_size: &mut usize,
    out_desc_count: &mut usize,
) -> bool {
    unsafe {
        let bs = match boot_services() {
            Some(bs) => bs as *mut EfiBootServices,
            None => return false,
        };

        let handle = IMAGE_HANDLE;

        // Try up to 3 times — spec says map key can go stale between get and exit
        for _ in 0..3 {
            let mut map_size = mmap_buf.len();
            let mut map_key: usize = 0;
            let mut desc_size: usize = 0;
            let mut desc_version: u32 = 0;

            let status = ((*bs).get_memory_map)(
                &mut map_size,
                mmap_buf.as_mut_ptr() as *mut EfiMemoryDescriptor,
                &mut map_key,
                &mut desc_size,
                &mut desc_version,
            );

            if efi_error(status) {
                continue;
            }

            *out_map_key = map_key;
            *out_desc_size = desc_size;
            *out_desc_count = map_size / desc_size;

            let status = ((*bs).exit_boot_services)(handle, map_key);
            if !efi_error(status) {
                // Success — null out the system table boot services pointer
                // to prevent any further calls
                SYSTEM_TABLE = ptr::null_mut();
                return true;
            }
            // Map key was stale — loop and retry
        }

        false
    }
}
