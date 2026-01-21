# OXIDE OS - Deferred Work & TODOs

**Generated:** 2026-01-21
**Status:** Active work items

This document tracks all incomplete work, stubs, and TODOs found in the codebase that need to be implemented for production quality.

---

## HIGH PRIORITY - Core Functionality Blockers

### 1. OxideFS Filesystem - COMPLETELY NON-FUNCTIONAL ⚠️
**Location:** `crates/fs/oxidefs/src/lib.rs:415-520`
**Status:** CRITICAL - Entire VnodeOps implementation stubbed

All filesystem operations return errors:
- [ ] `lookup()` - Always returns NotFound (line 416)
- [ ] `create()` - Returns NotSupported (line 430)
- [ ] `read()` - Returns 0 bytes (line 445)
- [ ] `write()` - Returns 0 bytes (line 459)
- [ ] `readdir()` - Returns None (line 469)
- [ ] `mkdir()` - Returns NotSupported (line 483)
- [ ] `rmdir()` - Returns NotSupported (line 497)
- [ ] `unlink()` - Returns NotSupported (line 510)

**Impact:** OxideFS cannot be used as a working filesystem. This is a core kernel component.

---

### 2. DEFLATE Compression Algorithm Missing
**Location:** `userspace/compression/src/deflate.rs`
**Status:** CRITICAL - Only uncompressed blocks work

- [ ] Line 55: Implement actual DEFLATE compression (LZ77 + Huffman)
- [ ] Line 91: Implement full DEFLATE decompression

**Current Limitations:**
- Only handles uncompressed DEFLATE blocks (level 0)
- Actual compression returns `NotImplemented` error
- Actual decompression returns `NotImplemented` error

**Impact:**
- `gzip` command non-functional
- `gunzip` command non-functional
- Cannot compress/decompress real files

---

### 3. File-backed mmap Not Implemented
**Location:** `crates/syscall/syscall/src/memory.rs:62`
**Status:** HIGH - Only anonymous mappings work

- [ ] Implement file-backed mmap support
- [ ] Connect to VFS layer
- [ ] Handle MAP_SHARED vs MAP_PRIVATE
- [ ] Implement page fault handler for file-backed pages

**Impact:** Cannot memory-map files for efficient I/O

---

### 4. DNS Resolution Missing
**Location:** Multiple files
**Status:** HIGH - Hostname resolution doesn't work

- [ ] `userspace/libc/src/dns.rs:370` - `gethostbyname()` returns None
- [ ] Implement UDP-based DNS client
- [ ] Parse DNS responses
- [ ] Add resolver configuration (/etc/resolv.conf)

**Impact:**
- `ping`, `wget`, all networking tools require IP addresses
- Cannot resolve hostnames

---

## MEDIUM PRIORITY - Feature Limitations

### 5. Network Checksums Not Computed
**Status:** MEDIUM - Packets may be dropped by receivers

- [ ] `crates/net/tcpip/src/udp.rs:85` - UDP checksum placeholder
- [ ] `crates/net/tcpip/src/icmp.rs:138` - ICMP checksum placeholder
- [ ] `crates/net/tcpip/src/ip.rs:203` - IP header checksum placeholder

**Impact:** Network packets may be rejected by strict receivers

---

### 6. Per-CPU Data Access Wrong
**Location:** `crates/smp/smp/src/cpu.rs:95`
**Status:** MEDIUM - Not using proper per-CPU mechanism

- [ ] Read from per-CPU data via GS segment (not BSP variable)

**Impact:** SMP performance and correctness issues

---

### 7. Initramfs Limited File Types
**Location:** `crates/vfs/initramfs/src/lib.rs:339`
**Status:** MEDIUM

- [ ] Handle symlinks
- [ ] Handle block devices
- [ ] Handle character devices

**Impact:** Cannot package full filesystem in initramfs

---

### 8. Command-Line Argument Passing
**Location:** Multiple userspace/coreutils binaries
**Status:** MEDIUM - Several commands non-functional

- [ ] `mkdir` - line 12
- [ ] `echo` - line 12
- [ ] `rm` - line 11
- [ ] `kill` - line 11

**Impact:** Basic shell commands don't accept arguments

---

### 9. chown Syscall Not Implemented
**Location:** `userspace/coreutils/src/bin/chown.rs:64,82`
**Status:** MEDIUM

- [ ] Implement kernel handler for chown syscall
- [ ] Update inode ownership in VFS

**Impact:** Cannot change file ownership

---

### 10. Block Device Read Operations
**Status:** MEDIUM - Stub implementations

- [ ] `crates/drivers/block/nvme/src/lib.rs:340,444` - NVMe read stub
- [ ] `crates/drivers/block/ahci/src/lib.rs:428` - AHCI read stub
- [ ] `crates/drivers/block/virtio-blk/src/lib.rs:296` - VirtIO read stub

**Impact:** Cannot actually read from block devices

---

### 11. Job Control in Shell
**Location:** `userspace/shell/src/main.rs:1163,1166,1175`
**Status:** MEDIUM

- [ ] Implement `jobs` command
- [ ] Implement `fg` command
- [ ] Implement `bg` command
- [ ] Implement `getopts` builtin

**Impact:** Limited shell functionality, no background job management

---

### 12. GPT Partition Names
**Location:** `crates/block/gpt/src/lib.rs:334`
**Status:** LOW

- [ ] Fix lifetime issue to use `entry.name_string()` properly

**Impact:** Cannot display partition names, only UUIDs

---

## LOW PRIORITY - Future Enhancements

### 13. GWBASIC Date/Time Functions
**Location:** `apps/gwbasic/src/oxide_main.rs:133`
**Status:** LOW

- [ ] Implement proper RTC reading via syscall
- [ ] Return actual date/time instead of placeholder 2025-01-01

---

### 14. GWBASIC Cursor Position
**Location:** `apps/gwbasic/src/oxide_main.rs:152,159`
**Status:** LOW

- [ ] Add cursor position tracking to terminal
- [ ] Expose via API to userspace

---

### 15. GWBASIC Graphics Mode
**Location:** `apps/gwbasic/src/oxide_main.rs:166`
**Status:** LOW - Text mode only for now

---

### 16. Timer Syscall Variants
**Location:** `crates/syscall/syscall/src/lib.rs:1074,1116`
**Status:** LOW

- [ ] Implement `ITIMER_VIRTUAL` (user time only)
- [ ] Implement `ITIMER_PROF` (user + system time)

**Note:** `ITIMER_REAL` already works

---

### 17. IPI (Inter-Processor Interrupt) Platform Implementation
**Location:** `crates/smp/smp/src/ipi.rs:60-96`
**Status:** LOW - Architecture-specific work

- [ ] x86_64: Implement via APIC
- [ ] Send IPI to specific CPU
- [ ] Send broadcast IPI
- [ ] Send self IPI

---

### 18. Dynamic Linking
**Location:** `userspace/libc/src/dlfcn.rs:82,104`
**Status:** FUTURE - Not planned for current phase

- [ ] `dlopen()` implementation
- [ ] `dlsym()` implementation
- [ ] `dlclose()` implementation

**Note:** Static linking is intentional for current phase

---

### 19. AI/Embedding Features
**Status:** FUTURE - Experimental features

- [ ] `crates/ai/embed/src/model.rs:86` - Real transformer model
- [ ] `userspace/search/src/main.rs:108` - Search indexd communication
- [ ] `crates/ai/embed/src/extract.rs:277` - EXIF parser

---

### 20. Virtualization
**Status:** FUTURE - Experimental

- [ ] `crates/hypervisor/vmx/src/vmcs.rs:423` - VM exit handler
- [ ] VirtIO console placeholder handling

---

### 21. Security Features
**Status:** FUTURE

- [ ] `crates/security/x509/src/lib.rs:225` - Real X.509 certificates
- [ ] `crates/security/trust/src/store.rs:197` - Trust store implementation

---

## ACCEPTABLE STUBS (Not Issues)

These are intentional and don't need fixing:

- ✓ `apps/gwbasic/src/platform/stub_platform.rs` - Library-only build support
- ✓ `crates/media/automount/src/mount.rs:61` - StubMountExecutor for no_std
- ✓ `crates/compat/python-sandbox/` - Future feature placeholder
- ✓ `apps/gwbasic_linux_version/` - Test/compatibility layer
- ✓ `kernel/src/main.rs:68` - Temporary heap (acceptable for early boot)
- ✓ `docs/BOOT_SPEC.md:360` - Documentation example with todo!()
- ✓ All "temporary file/directory" references - Legitimate uses

---

## Work Priority Order

Based on impact and phase requirements:

1. **OxideFS** - Critical for filesystem functionality
2. **DEFLATE** - Critical for compression tools
3. **File-backed mmap** - Important for performance
4. **DNS resolution** - Important for networking
5. **Network checksums** - Correctness issue
6. **Argument passing** - Usability issue
7. **chown syscall** - Permission management
8. **Block device reads** - Storage functionality
9. Everything else can wait for future phases

---

## Notes

- This list was generated by searching for: TODO, FIXME, HACK, XXX, unimplemented!, todo!, stub, placeholder, "not implemented", "not yet implemented"
- Items marked HIGH/CRITICAL violate CLAUDE.md production quality standards
- All HIGH/CRITICAL items should be fixed before declaring the current phase complete
