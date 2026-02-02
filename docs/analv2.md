# OXIDE Kernel Gap Analysis v2
**Date:** 2026-02-02
**Status:** Implementation assessment for production readiness

## Executive Summary

OXIDE has a substantial kernel implementation (~120 crates) with working core functionality including:
- Process management (fork, exec, exit, wait) ✅
- Preemptive scheduler with CFS ✅
- Virtual filesystem with tmpfs, devfs, procfs ✅
- TTY/PTY subsystem with virtual terminals ✅
- Basic networking (TCP/IP, loopback, sockets) ✅
- Memory management with COW fork ✅
- Signal handling framework ✅

However, several subsystems have gaps, stubs, or incomplete flows that need attention.

---

## 1. Critical Gaps (Blocking Issues)

### 1.1 SMAP Disabled
**Location:** `kernel/src/init.rs`
**Status:** ❌ DISABLED
**Impact:** Security vulnerability - kernel can access user memory without explicit permission

```
[INIT] SMAP supported but DISABLED (needs fix - complex timing issue)
```

**Problem:** AC flag (Alignment Check) gets cleared between STAC and actual user memory access. This is a timing/interrupt issue where an interrupt handler might clear AC.

**Fix Required:**
- Audit all interrupt handlers for AC flag preservation
- Ensure STAC/CLAC pairs are atomic with respect to the memory access
- Consider disabling interrupts around critical SMAP sections

### 1.2 SMP/Multi-Core Not Working
**Location:** `kernel/src/init.rs`, `kernel/smp/`
**Status:** ❌ AP Boot Fails
**Impact:** Single-core only, cannot utilize multi-core CPUs

```
[SMP] Failed to boot CPU 1: AP boot timeout
```

**Problems:**
1. AP (Application Processor) boot trampoline exists but times out
2. No proper ACPI MADT enumeration for CPU discovery
3. IPI (Inter-Processor Interrupt) for TLB shootdown not implemented

**Fix Required:**
- Debug AP boot trampoline (`kernel/arch/arch-x86_64/asm/ap_boot.S`)
- Implement proper ACPI parsing for CPU topology
- Implement IPI sending for cross-CPU notifications
- Add per-CPU run queues (scheduler already supports this)

### 1.3 Thread Creation (clone) Incomplete
**Location:** `kernel/syscall/syscall/src/lib.rs`
**Status:** ⚠️ Partial
**Impact:** Cannot create threads, only processes

**Current State:**
- `clone()` without `CLONE_VM` falls back to `fork()` ✅
- `clone()` with `CLONE_VM` (threads) returns `ENOSYS` ❌

**Fix Required:**
- Implement shared address space for threads
- Handle `CLONE_THREAD`, `CLONE_SIGHAND`, `CLONE_FILES` flags
- Implement TLS (Thread Local Storage) setup via `CLONE_SETTLS`
- Wire up `set_tid_address` for thread exit notification

---

## 2. Subsystem Gaps

### 2.1 Filesystem

#### ext4 - Partial Implementation
**Location:** `kernel/fs/ext4/`
**Status:** ⚠️ ~5200 lines, needs work

**Missing:**
- Real timestamps (uses `0` for now)
- Real UID/GID (uses `0` for now)
- Journal recovery (journal parsing exists but recovery incomplete)
- Extended attributes
- ACLs

#### Block Device I/O
**Location:** `kernel/drivers/block/`
**Status:** ⚠️ Stubs for real hardware

- AHCI: Returns zeros (stub)
- NVMe: Stub implementation
- VirtIO-blk: Working ✅

**Fix Required:**
- Implement real AHCI driver for SATA disks
- Implement NVMe driver for NVMe SSDs

### 2.2 Networking

#### TCP/IP Stack
**Location:** `kernel/net/tcpip/`
**Status:** ⚠️ ~3500 lines, basic functionality

**Working:**
- Loopback device ✅
- Basic TCP connections (loopback) ✅
- UDP ✅
- ARP ✅
- ICMP ✅
- Connection tracking ✅
- Packet filtering ✅

**Missing/Incomplete:**
- TCP congestion control (basic only)
- TCP fast retransmit
- Out-of-order packet handling
- Non-loopback socket connect/send (marked as "stub for now")
- IPv6

#### VirtIO-net
**Location:** `kernel/drivers/net/virtio-net/`
**Status:** ⚠️ Driver exists but not fully integrated

### 2.3 Memory Management

#### mmap
**Location:** `kernel/libc-support/mmap/`, `kernel/syscall/syscall/src/memory.rs`
**Status:** ✅ Working

**Working:**
- Anonymous mappings
- File-backed mappings (basic)
- MAP_PRIVATE, MAP_SHARED
- mprotect, munmap, mremap

#### Demand Paging
**Location:** `kernel/mm/mm-paging/src/demand.rs`
**Status:** ⚠️ Framework exists

**Missing:**
- Swap to disk
- Page reclamation under memory pressure
- Working set estimation

### 2.4 Signals

**Location:** `kernel/signal/signal/`
**Status:** ⚠️ ~1000 lines, framework complete

**Working:**
- Signal delivery framework
- sigaction, sigprocmask
- Standard signal handlers (SIGKILL, SIGSTOP, etc.)
- Signal frame setup

**Missing/Issues:**
- Real-time signals
- Signal queueing for RT signals
- Proper SIGSEGV/SIGBUS delivery for invalid memory access
- sigaltstack not fully tested

### 2.5 Time

**Location:** `kernel/syscall/syscall/src/time.rs`
**Status:** ⚠️ Basic

**Working:**
- gettimeofday
- clock_gettime (CLOCK_REALTIME, CLOCK_MONOTONIC)
- nanosleep
- alarm (basic)

**Missing:**
- Per-process/thread CPU time tracking (`TODO` in code)
- ITIMER_VIRTUAL, ITIMER_PROF (returns ENOSYS)
- High-resolution timers
- CLOCK_PROCESS_CPUTIME_ID, CLOCK_THREAD_CPUTIME_ID

### 2.6 Process Priority

**Location:** `kernel/syscall/syscall/src/lib.rs`
**Status:** ⚠️ Partial

**Working:**
- nice() for current process
- getpriority/setpriority for PRIO_PROCESS

**Missing:**
- PRIO_PGRP (process group priority) - returns ENOSYS
- PRIO_USER (user priority) - returns ENOSYS

---

## 3. Architecture Portability

### 3.1 x86_64
**Status:** ✅ Primary target, fully implemented

### 3.2 AArch64 (ARM64)
**Location:** `kernel/arch/arch-aarch64/`
**Status:** ❌ Stubs only

```rust
// These are stubs that panic if called
```

**Missing:**
- Exception vector table setup
- SVC (syscall) handler
- Serial/UART driver
- Timer
- Interrupt controller (GIC)

### 3.3 MIPS64
**Location:** `kernel/arch/arch-mips64/`
**Status:** ❌ Stubs only

**Missing:**
- Exception handler setup
- Syscall handler
- Serial driver
- Timer
- SGI-specific hardware support

---

## 4. Driver Gaps

### 4.1 USB
**Location:** `kernel/drivers/usb/xhci/`
**Status:** ⚠️ ~750 lines, framework exists

**Missing:**
- Device enumeration
- USB HID (keyboard/mouse via USB)
- USB mass storage

### 4.2 Audio
**Location:** `kernel/drivers/audio/virtio-snd/`, `kernel/audio/audio/`
**Status:** ⚠️ Framework only

### 4.3 GPU
**Location:** `kernel/drivers/gpu/virtio-gpu/`
**Status:** ⚠️ Framework only
- No actual GPU rendering
- Framebuffer only

---

## 5. Syscall Coverage

**Total Syscall Numbers Defined:** 168
**Syscall Dispatch Entries:** 296 (includes aliases)
**Returning ENOSYS:** ~24 cases

### Syscalls Returning ENOSYS (Not Implemented)

| Syscall | Reason |
|---------|--------|
| QUERY_MODULE | Deprecated |
| ITIMER_VIRTUAL/PROF | Timer types not implemented |
| PRIO_PGRP | Process group priority |
| PRIO_USER | User-based priority |
| INIT_MODULE | Module loading incomplete |
| DELETE_MODULE | Module unloading incomplete |
| Various *at variants | Some directory-relative ops |

---

## 6. Security Gaps

### 6.1 SMAP/SMEP
- SMEP: ✅ Enabled (prevents executing user pages in kernel)
- SMAP: ❌ Disabled (see section 1.1)

### 6.2 Capabilities
**Status:** ❌ Not implemented
- No capability-based security
- Root (UID 0) has all permissions

### 6.3 Seccomp
**Location:** `kernel/container/seccomp/`
**Status:** ⚠️ Framework exists but not integrated

### 6.4 Namespaces
**Location:** `kernel/container/namespace/`
**Status:** ⚠️ Framework exists but not integrated

---

## 7. Recommended Fix Priority

### P0 - Critical (Stability/Security)
1. **SMAP fix** - Security vulnerability
2. **Zombie dequeue** - ✅ FIXED in this session
3. **Thread creation** - Many programs need threads

### P1 - High (Core Functionality)
4. **SMP/AP boot** - Utilize multi-core
5. **ext4 timestamps/uid** - Proper file metadata
6. **TCP non-loopback** - Real networking

### P2 - Medium (Feature Completeness)
7. **Per-process CPU time** - Required for profiling
8. **USB device support** - Hardware compatibility
9. **AHCI/NVMe drivers** - Real disk support

### P3 - Low (Nice to Have)
10. **AArch64 port** - ARM server support
11. **Audio drivers** - Multimedia
12. **GPU acceleration** - Graphics

---

## 8. Testing Gaps

### Missing Test Coverage
- No automated tests for signal delivery
- No stress tests for scheduler
- No memory pressure tests
- No network throughput tests
- No filesystem corruption tests

### Recommended Tests to Add
1. Signal delivery under load
2. Fork bomb handling
3. OOM killer behavior
4. TCP connection storm
5. ext4 journal recovery

---

## 9. Documentation Gaps

### Missing Documentation
- Syscall ABI specification
- Driver development guide
- Memory layout diagram
- Boot sequence documentation
- Debugging guide

---

## Appendix: Code Metrics

| Subsystem | Lines of Code | Status |
|-----------|---------------|--------|
| Scheduler | ~2500 | ✅ Working |
| Syscall dispatch | ~3500 | ✅ Working |
| TCP/IP | ~3500 | ⚠️ Partial |
| ext4 | ~5200 | ⚠️ Partial |
| Signal | ~1000 | ⚠️ Partial |
| VFS | ~2000 | ✅ Working |
| Memory Manager | ~1500 | ✅ Working |
| x86_64 arch | ~3000 | ✅ Working |
| AArch64 arch | ~200 | ❌ Stubs |
| MIPS64 arch | ~200 | ❌ Stubs |

**Total kernel crates:** 120
**Total TODOs in kernel:** 37
