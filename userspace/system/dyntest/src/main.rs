//! Dynamic linking test program
//!
//! — CrashBloom: the canary in the dynamic linking coal mine. This binary has a
//! PT_INTERP segment pointing to /lib/ld-oxide.so.1. When the kernel execs it:
//! 1. Kernel loads this binary's LOAD segments
//! 2. Kernel detects PT_INTERP, reads ld-oxide.so.1 from VFS
//! 3. Kernel loads ld-oxide.so.1 into the address space
//! 4. Kernel sets entry point to ld-oxide's entry
//! 5. ld-oxide starts, parses aux vector, finds AT_ENTRY
//! 6. ld-oxide jumps to THIS binary's _start
//! 7. We print "DYNTEST OK" and exit
//!
//! If you see "DYNTEST OK" on the terminal, the full PT_INTERP path works.

#![no_std]
#![no_main]

use libc::*;

/// PT_INTERP path — tells the kernel to load ld-oxide.so.1 as our interpreter
/// — CrashBloom: this goes into the .interp section which the linker script
/// maps as a PT_INTERP program header.
#[unsafe(link_section = ".interp")]
#[used]
static INTERP: [u8; 19] = *b"/lib/ld-oxide.so.1\0";

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // — CrashBloom: if we get here, the full chain worked:
    // kernel → ld-oxide → this binary's _start → libc _start → main()
    let msg = b"[DYNTEST] OK - dynamic linker handoff successful!\n";
    syscall::sys_write(1, msg);

    // Also write to serial for QEMU capture
    let serial_msg = b"DYNTEST_PASS\n";
    syscall::sys_write(1, serial_msg);

    0
}
