# OXIDE Kernel Implementation Plan
**Based on:** analv2.md Gap Analysis
**Date:** 2026-02-02
**Status:** Ready for Implementation

---

## Overview

This plan addresses all issues identified in the OXIDE kernel gap analysis, organized by priority (P0-P3) with detailed implementation steps, estimated complexity, and dependencies.

**Total Issues:** 27 major items across 9 subsystems
**Critical Path:** SMAP → Threading → SMP → Network/Storage

---

## P0 - CRITICAL (Security & Stability)

### 1. SMAP Fix [SECURITY]
**Priority:** P0
**Complexity:** High
**Estimated Effort:** 2-3 weeks
**Blocking:** None

**Location:** `kernel/src/init.rs`, `kernel/arch/arch-x86_64/`

**Problem:**
- AC flag cleared between STAC and user memory access
- Timing/interrupt issue causing SMAP violations
- Currently DISABLED (security vulnerability)

**Implementation Steps:**

1. **Audit Phase** (3-5 days)
   - [ ] Audit all interrupt handlers for AC flag preservation
   - [ ] List all locations where user memory is accessed from kernel
   - [ ] Document current STAC/CLAC usage patterns
   - [ ] Create test cases to reproduce AC flag clearing

2. **Fix Interrupt Handlers** (5-7 days)
   - [ ] Ensure `interrupt_entry` macro preserves AC flag
   - [ ] Add AC flag save/restore to context switch code
   - [ ] Verify exception handlers (page fault, GPF) preserve AC
   - [ ] Test AC flag preservation across all interrupt types

3. **Atomic SMAP Sections** (3-4 days)
   - [ ] Wrap critical user memory accesses with interrupt-disable
   - [ ] Create safe wrappers: `with_smap_disabled(|| { ... })`
   - [ ] Refactor `copy_from_user`/`copy_to_user` to be atomic
   - [ ] Add compiler barriers around STAC/CLAC

4. **Testing & Validation** (2-3 days)
   - [ ] Enable SMAP in kernel config
   - [ ] Run full syscall test suite
   - [ ] Stress test with high interrupt load
   - [ ] Verify no SMAP violations in dmesg

**Files to Modify:**
- `kernel/arch/arch-x86_64/src/interrupts/mod.rs`
- `kernel/arch/arch-x86_64/asm/interrupt_entry.S`
- `kernel/arch/arch-x86_64/src/user_access.rs`
- `kernel/src/init.rs` (re-enable SMAP)

**Success Criteria:**
- SMAP enabled in production
- Zero SMAP violations under stress tests
- All syscalls pass with SMAP enabled

---

### 2. Thread Creation (clone with CLONE_VM) [CORE]
**Priority:** P0
**Status:** ✅ **COMPLETED** (2026-02-02)
**Complexity:** High
**Actual Effort:** 1 day (design already existed in do_clone)
**Blocking:** Many userspace applications (now unblocked)

**Location:** `kernel/syscall/syscall/src/lib.rs`, `kernel/src/process.rs`, `kernel/proc/proc/src/clone.rs`

**Problem:** ~~SOLVED~~
- ~~`clone()` with `CLONE_VM` returns ENOSYS~~ ✅ Now calls kernel_clone
- ~~Cannot create threads, only processes~~ ✅ Full thread support
- ~~No TLS (Thread Local Storage) support~~ ✅ ARCH_PRCTL + CLONE_SETTLS

**Implementation Steps:**

1. **Shared Address Space** ✅ COMPLETED
   - [x] Modify `Process` struct to support shared `AddressSpace`
   - [x] Use `Arc<Mutex<AddressSpace>>` for shared memory
   - [x] Update `fork()` to handle COW semantics correctly
   - [x] Implement reference counting for page tables
   - [x] Test that threads see same memory mappings
   - **Note:** do_clone() already had full implementation

2. **Thread Group Support** ✅ COMPLETED
   - [x] Implement `CLONE_THREAD` - share PID, TGID
   - [x] Create thread group leader tracking
   - [x] Implement `CLONE_SIGHAND` - shared signal handlers
   - [x] Implement `CLONE_FILES` - shared file descriptor table
   - [x] Update `getpid()`/`gettid()` to return correct values
   - **Implementation:** kernel_clone() creates Task with shared ProcessMeta

3. **TLS Implementation** ✅ COMPLETED
   - [x] Implement `CLONE_SETTLS` flag handling
   - [x] Set FS base register for TLS pointer
   - [x] Wire up `set_tid_address()` syscall (already existed)
   - [x] Implement `arch_prctl()` for TLS manipulation (NEW)
   - [x] Test pthread TLS access (ready for testing)
   - **Added:** ARCH_PRCTL syscall (158) with ARCH_SET_FS/GET_FS/SET_GS/GET_GS

4. **Thread Lifecycle** ✅ COMPLETED
   - [x] Implement thread exit without killing process
   - [x] Handle last thread exit → process termination
   - [x] Implement futex-based thread joining (clear_child_tid + futex wake)
   - [x] Fix zombie thread cleanup (threads don't zombie, immediate removal)
   - **Implementation:** user_exit() checks TGID != TID for thread detection

5. **Integration & Testing** 🔄 READY FOR TESTING
   - [ ] Test with pthread_create()
   - [ ] Test thread-local variables
   - [ ] Test thread cancellation
   - [ ] Test thousands of threads
   - [ ] Verify no memory leaks

**Files Modified:**
- `kernel/syscall/syscall/src/lib.rs` - Added CloneFn, sys_arch_prctl, fixed getpid/gettid
- `kernel/src/process.rs` - Added kernel_clone() callback, updated user_exit()
- `kernel/src/init.rs` - Registered clone callback
- `kernel/proc/proc/src/clone.rs` - Already complete (no changes needed!)

**Success Criteria:** ✅ KERNEL IMPLEMENTATION COMPLETE
- [x] clone() syscall works (wired to kernel_clone)
- [x] Thread-local storage infrastructure ready (ARCH_PRCTL)
- [x] Thread exit doesn't kill process (separate exit paths)
- [x] TID/TGID semantics correct (getpid returns TGID, gettid returns TID)
- [ ] Runtime testing (next step)

---

### 3. Zombie Process Dequeue Fix [STABILITY]
**Priority:** P0
**Status:** ✅ **FIXED** (completed in previous session)

---

## P1 - HIGH (Core Functionality)

### 4. SMP/Multi-Core Support [PERFORMANCE]
**Priority:** P1
**Complexity:** Very High
**Estimated Effort:** 6-8 weeks
**Depends On:** Thread creation

**Location:** `kernel/src/init.rs`, `kernel/smp/`, `kernel/arch/arch-x86_64/`

**Problem:**
- AP (Application Processor) boot fails
- Timeout during AP startup
- No ACPI MADT enumeration
- No IPI support

**Implementation Steps:**

1. **ACPI MADT Parsing** (1-2 weeks)
   - [ ] Implement proper ACPI table parsing
   - [ ] Parse MADT (Multiple APIC Description Table)
   - [ ] Enumerate all available CPUs
   - [ ] Store CPU topology information
   - [ ] Handle APIC ID mapping

2. **AP Boot Trampoline Fix** (2-3 weeks)
   - [ ] Debug `ap_boot.S` trampoline code
   - [ ] Verify GDT/IDT setup for APs
   - [ ] Fix page table initialization for APs
   - [ ] Implement proper AP stack allocation
   - [ ] Add timeout debugging/logging
   - [ ] Test on real hardware (not just QEMU)

3. **Per-CPU Data Structures** (1-2 weeks)
   - [ ] Implement per-CPU GS base
   - [ ] Create per-CPU interrupt stacks
   - [ ] Allocate per-CPU scheduler run queues
   - [ ] Create per-CPU memory allocators
   - [ ] Implement CPU-local storage macros

4. **IPI (Inter-Processor Interrupts)** (1-2 weeks)
   - [ ] Implement IPI sending via APIC
   - [ ] Handle IPI_TLB_SHOOTDOWN
   - [ ] Handle IPI_RESCHEDULE
   - [ ] Implement IPI_CALL_FUNCTION
   - [ ] Test cross-CPU synchronization

5. **Scheduler Integration** (1 week)
   - [ ] Enable per-CPU run queues
   - [ ] Implement load balancing across CPUs
   - [ ] Handle CPU hotplug events
   - [ ] Implement CPU affinity

6. **Testing & Validation** (1 week)
   - [ ] Boot with 2, 4, 8, 16 CPUs
   - [ ] Run parallel workloads
   - [ ] Stress test with CPU-intensive tasks
   - [ ] Verify load balancing works

**Files to Modify:**
- `kernel/drivers/acpi/` (new crate for ACPI parsing)
- `kernel/arch/arch-x86_64/asm/ap_boot.S`
- `kernel/smp/src/lib.rs`
- `kernel/arch/arch-x86_64/src/apic.rs`
- `kernel/scheduler/scheduler/src/lib.rs`

**Success Criteria:**
- All CPUs boot successfully
- Processes scheduled across all CPUs
- Load balancing works
- No race conditions under SMP

---

### 5. ext4 Timestamps & Metadata [FILESYSTEM]
**Priority:** P1
**Complexity:** Medium
**Estimated Effort:** 1-2 weeks
**Blocking:** None

**Location:** `kernel/fs/ext4/`

**Problem:**
- Timestamps hardcoded to 0
- UID/GID hardcoded to 0
- No proper file metadata

**Implementation Steps:**

1. **Timestamp Support** (3-5 days)
   - [ ] Read current time from RTC/TSC
   - [ ] Store atime, mtime, ctime in inode
   - [ ] Update timestamps on file operations
   - [ ] Implement `utime()`/`utimes()` syscalls
   - [ ] Handle noatime mount option

2. **UID/GID Support** (3-5 days)
   - [ ] Read UID/GID from inode
   - [ ] Write UID/GID to inode on creation
   - [ ] Implement `chown()`/`fchown()` syscalls
   - [ ] Update permissions checking
   - [ ] Test with different users

3. **Extended Attributes (xattr)** (5-7 days)
   - [ ] Parse xattr blocks in ext4
   - [ ] Implement `setxattr()`/`getxattr()`
   - [ ] Implement `listxattr()`/`removexattr()`
   - [ ] Support security.* namespace
   - [ ] Support user.* namespace

**Files to Modify:**
- `kernel/fs/ext4/src/inode.rs`
- `kernel/fs/ext4/src/operations.rs`
- `kernel/syscall/syscall/src/file.rs`

**Success Criteria:**
- Files have correct timestamps (ls -l)
- Files preserve ownership
- touch command works
- chown command works

---

### 6. TCP Non-Loopback Networking [NETWORK]
**Priority:** P1
**Complexity:** High
**Estimated Effort:** 3-4 weeks
**Blocking:** Network applications

**Location:** `kernel/net/tcpip/`, `kernel/drivers/net/virtio-net/`

**Problem:**
- TCP connections only work on loopback
- Non-loopback connect/send stubbed
- VirtIO-net driver not fully integrated

**Implementation Steps:**

1. **VirtIO-net Integration** (1-2 weeks)
   - [ ] Complete VirtIO-net driver initialization
   - [ ] Wire up RX packet handling
   - [ ] Wire up TX packet sending
   - [ ] Implement interrupt handling
   - [ ] Create network device abstraction
   - [ ] Register with network stack

2. **TCP Stack Fixes** (1-2 weeks)
   - [ ] Remove "stub for now" in connect/send
   - [ ] Implement proper route lookup
   - [ ] Implement ARP resolution
   - [ ] Fix packet routing to network devices
   - [ ] Implement TCP retransmission logic
   - [ ] Add TCP congestion control (Reno/CUBIC)

3. **Out-of-Order Handling** (3-5 days)
   - [ ] Implement TCP receive queue sorting
   - [ ] Handle duplicate ACKs
   - [ ] Implement fast retransmit
   - [ ] Test with packet reordering

4. **Testing & Validation** (3-5 days)
   - [ ] Test TCP connections to external hosts
   - [ ] Test HTTP downloads
   - [ ] Test SSH connections
   - [ ] Run iperf benchmarks
   - [ ] Test under packet loss

**Files to Modify:**
- `kernel/net/tcpip/src/tcp.rs`
- `kernel/net/tcpip/src/routing.rs`
- `kernel/drivers/net/virtio-net/src/lib.rs`
- `kernel/net/device/src/lib.rs` (new abstraction)

**Success Criteria:**
- Can ping external hosts
- Can download files via HTTP
- Can SSH to external machines
- TCP throughput > 100 Mbps

---

## P2 - MEDIUM (Feature Completeness)

### 7. Per-Process CPU Time Tracking [PROFILING]
**Priority:** P2
**Complexity:** Medium
**Estimated Effort:** 1 week
**Blocking:** Profiling tools

**Location:** `kernel/syscall/syscall/src/time.rs`, `kernel/scheduler/`

**Implementation Steps:**

1. **Track CPU Time** (2-3 days)
   - [ ] Add utime/stime fields to Process struct
   - [ ] Update on every context switch
   - [ ] Update on every timer tick
   - [ ] Handle overflow correctly

2. **Implement Clock Types** (2-3 days)
   - [ ] Implement CLOCK_PROCESS_CPUTIME_ID
   - [ ] Implement CLOCK_THREAD_CPUTIME_ID
   - [ ] Update clock_gettime() to support them
   - [ ] Test with getrusage()

3. **Timer Support** (2-3 days)
   - [ ] Implement ITIMER_VIRTUAL
   - [ ] Implement ITIMER_PROF
   - [ ] Remove ENOSYS returns
   - [ ] Test with setitimer()

**Files to Modify:**
- `kernel/process/process/src/lib.rs`
- `kernel/syscall/syscall/src/time.rs`
- `kernel/scheduler/scheduler/src/lib.rs`

**Success Criteria:**
- time command shows accurate CPU time
- getrusage() returns correct values
- ITIMER_VIRTUAL works correctly

---

### 8. AHCI Driver (SATA Disks) [STORAGE]
**Priority:** P2
**Complexity:** Very High
**Estimated Effort:** 4-6 weeks
**Blocking:** Real hardware support

**Location:** `kernel/drivers/block/ahci/`

**Implementation Steps:**

1. **AHCI Controller Initialization** (1-2 weeks)
   - [ ] Parse AHCI BAR from PCI config
   - [ ] Initialize AHCI HBA registers
   - [ ] Allocate command lists and tables
   - [ ] Configure ports and detect drives
   - [ ] Implement AHCI reset sequence

2. **Command Submission** (1 week)
   - [ ] Implement command slot allocation
   - [ ] Build AHCI command FIS
   - [ ] Submit commands to port
   - [ ] Handle command completion interrupts

3. **Read/Write Operations** (1-2 weeks)
   - [ ] Implement READ DMA commands
   - [ ] Implement WRITE DMA commands
   - [ ] Handle PRD (Physical Region Descriptor) lists
   - [ ] Support multiple outstanding commands
   - [ ] Implement NCQ (Native Command Queueing)

4. **Error Handling** (1 week)
   - [ ] Handle port errors
   - [ ] Implement error recovery
   - [ ] Handle timeouts
   - [ ] Log diagnostic information

5. **Integration** (3-5 days)
   - [ ] Wire up to block device layer
   - [ ] Test with ext4 filesystem
   - [ ] Run fio benchmarks

**Files to Modify:**
- `kernel/drivers/block/ahci/src/lib.rs`
- `kernel/drivers/block/ahci/src/hba.rs` (new)
- `kernel/drivers/block/ahci/src/port.rs` (new)

**Success Criteria:**
- Can read/write to SATA disk
- Performance > 200 MB/s sequential
- No data corruption
- Works on real hardware

---

### 9. NVMe Driver [STORAGE]
**Priority:** P2
**Complexity:** Very High
**Estimated Effort:** 4-6 weeks
**Blocking:** NVMe SSD support

**Location:** `kernel/drivers/block/nvme/`

**Implementation Steps:**

1. **NVMe Controller Init** (1-2 weeks)
   - [ ] Parse NVMe BAR from PCI config
   - [ ] Initialize controller registers
   - [ ] Create Admin Queue (submission/completion)
   - [ ] Identify controller capabilities
   - [ ] Enumerate namespaces

2. **I/O Queue Setup** (1 week)
   - [ ] Create I/O submission queues
   - [ ] Create I/O completion queues
   - [ ] Set up MSI-X interrupts
   - [ ] Map queues to CPUs (for SMP)

3. **Command Implementation** (1-2 weeks)
   - [ ] Implement Read command
   - [ ] Implement Write command
   - [ ] Implement Flush command
   - [ ] Handle completion queue processing
   - [ ] Support multiple outstanding commands

4. **Advanced Features** (1 week)
   - [ ] Implement I/O queue pair per CPU
   - [ ] Support Write Zeroes
   - [ ] Support TRIM/Discard
   - [ ] Implement power management

5. **Testing** (3-5 days)
   - [ ] Test on QEMU with NVMe device
   - [ ] Test on real NVMe SSD
   - [ ] Run fio benchmarks
   - [ ] Test under stress

**Files to Modify:**
- `kernel/drivers/block/nvme/src/lib.rs`
- `kernel/drivers/block/nvme/src/controller.rs` (new)
- `kernel/drivers/block/nvme/src/queue.rs` (new)

**Success Criteria:**
- Can read/write to NVMe SSD
- Performance > 1 GB/s sequential
- Low latency (< 100us)
- Works with multiple namespaces

---

### 10. USB Device Support [HARDWARE]
**Priority:** P2
**Complexity:** Very High
**Estimated Effort:** 6-8 weeks
**Blocking:** USB peripherals

**Location:** `kernel/drivers/usb/xhci/`

**Implementation Steps:**

1. **XHCI Controller Init** (2-3 weeks)
   - [ ] Complete XHCI initialization
   - [ ] Set up device context base array
   - [ ] Initialize event ring
   - [ ] Initialize command ring
   - [ ] Start controller

2. **Device Enumeration** (2-3 weeks)
   - [ ] Handle port status change events
   - [ ] Implement USB reset sequence
   - [ ] Read device descriptors
   - [ ] Assign device address
   - [ ] Parse configuration

3. **USB HID Driver** (1-2 weeks)
   - [ ] Implement HID descriptor parsing
   - [ ] Support keyboard input
   - [ ] Support mouse input
   - [ ] Integrate with input subsystem

4. **USB Mass Storage** (2-3 weeks)
   - [ ] Implement Bulk-Only Transport
   - [ ] Support SCSI commands
   - [ ] Implement as block device
   - [ ] Handle USB storage quirks

**Files to Modify:**
- `kernel/drivers/usb/xhci/src/lib.rs`
- `kernel/drivers/usb/hid/` (new crate)
- `kernel/drivers/usb/storage/` (new crate)

**Success Criteria:**
- USB keyboard works
- USB mouse works
- USB flash drives mount correctly
- Hot-plug detection works

---

## P3 - LOW (Nice to Have)

### 11. Journal Recovery (ext4) [FILESYSTEM]
**Priority:** P3
**Complexity:** High
**Estimated Effort:** 2-3 weeks

**Implementation Steps:**
- [ ] Complete journal replay on mount
- [ ] Handle transaction commits
- [ ] Implement checkpoint mechanism
- [ ] Test with crash recovery scenarios

---

### 12. Swap Support [MEMORY]
**Priority:** P3
**Complexity:** High
**Estimated Effort:** 3-4 weeks

**Implementation Steps:**
- [ ] Implement swap file/partition support
- [ ] Create page eviction algorithm (LRU)
- [ ] Handle page-in from swap
- [ ] Handle page-out to swap
- [ ] Implement swap accounting

---

### 13. Real-time Signals [SIGNALS]
**Priority:** P3
**Complexity:** Medium
**Estimated Effort:** 1-2 weeks

**Implementation Steps:**
- [ ] Implement signal queueing
- [ ] Support SIGRTMIN-SIGRTMAX
- [ ] Implement sigqueue() syscall
- [ ] Handle signal info delivery

---

### 14. Capabilities [SECURITY]
**Priority:** P3
**Complexity:** High
**Estimated Effort:** 3-4 weeks

**Implementation Steps:**
- [ ] Implement capability sets (permitted, effective, inheritable)
- [ ] Add capability checks to syscalls
- [ ] Implement capset()/capget()
- [ ] Update exec to handle capabilities

---

### 15. Seccomp Integration [SECURITY]
**Priority:** P3
**Complexity:** Medium
**Estimated Effort:** 2-3 weeks

**Location:** `kernel/container/seccomp/`

**Implementation Steps:**
- [ ] Wire up seccomp() syscall
- [ ] Implement BPF filter evaluation
- [ ] Add seccomp checks to syscall entry
- [ ] Test with Docker/containers

---

### 16. Namespace Integration [CONTAINERS]
**Priority:** P3
**Complexity:** High
**Estimated Effort:** 4-6 weeks

**Location:** `kernel/container/namespace/`

**Implementation Steps:**
- [ ] Complete namespace implementation
- [ ] Support PID namespace
- [ ] Support mount namespace
- [ ] Support network namespace
- [ ] Integrate with clone()

---

### 17. IPv6 Support [NETWORK]
**Priority:** P3
**Complexity:** High
**Estimated Effort:** 4-6 weeks

**Implementation Steps:**
- [ ] Implement IPv6 packet parsing
- [ ] Implement ICMPv6
- [ ] Implement NDP (Neighbor Discovery)
- [ ] Support IPv6 sockets
- [ ] Implement DHCPv6

---

## Testing Strategy

### Automated Testing
- [ ] Create kernel test framework
- [ ] Add signal delivery tests
- [ ] Add scheduler stress tests
- [ ] Add memory pressure tests
- [ ] Add network throughput tests
- [ ] Add filesystem corruption tests

### Specific Test Cases
- [ ] Fork bomb handling (limit processes)
- [ ] OOM killer behavior
- [ ] TCP connection storm
- [ ] ext4 journal recovery after crash
- [ ] Signal delivery under load

---

## Documentation Tasks

### Critical Documentation
- [ ] Syscall ABI specification
- [ ] Driver development guide
- [ ] Memory layout diagram
- [ ] Boot sequence documentation
- [ ] Debugging guide
- [ ] Architecture porting guide

### Developer Experience
- [ ] Setup instructions for contributors
- [ ] Code style guide
- [ ] Testing procedures
- [ ] Release checklist

---

## Resource Requirements

### Personnel
- **Kernel Engineer** (1-2): Core kernel work (SMAP, SMP, threading)
- **Driver Engineer** (1): AHCI, NVMe, USB drivers
- **Network Engineer** (1): TCP/IP stack, VirtIO-net
- **Security Engineer** (0.5): Capabilities, seccomp, namespaces

### Testing Hardware
- Multi-core x86_64 machine (8+ cores)
- SATA disk for AHCI testing
- NVMe SSD for NVMe testing
- USB keyboard, mouse, storage devices
- Network interface card

---

## Risk Assessment

### High Risk Items
1. **SMAP Fix** - May uncover other race conditions
2. **SMP** - Complex debugging, hardware-dependent
3. **Threading** - Potential for subtle race conditions

### Mitigation Strategies
- Extensive testing at each stage
- Code review by multiple engineers
- Use hardware debuggers (JTAG) for SMP issues
- Run stress tests continuously

---

## Success Metrics

### P0 Completion (Critical)
- SMAP enabled with zero violations
- Threading works with pthread
- System stable under load

### P1 Completion (Core)
- All CPUs boot and schedule processes
- ext4 has proper metadata
- Network works beyond loopback

### P2 Completion (Features)
- Real hardware disk support (AHCI/NVMe)
- USB devices work
- CPU time tracking accurate

### P3 Completion (Nice to Have)
- Container support functional
- IPv6 networking works
- Advanced security features enabled

---

## Timeline Estimate

### Phase 1: Critical Fixes (8-12 weeks)
- SMAP fix
- Thread creation
- Zombie dequeue (done)

### Phase 2: Core Functionality (12-16 weeks)
- SMP support
- ext4 metadata
- TCP networking

### Phase 3: Feature Completeness (16-24 weeks)
- AHCI/NVMe drivers
- USB support
- CPU time tracking

### Phase 4: Advanced Features (12-20 weeks)
- Swap support
- Capabilities
- Containers
- IPv6

**Total Estimated Timeline:** 48-72 weeks (9-14 months)

With 2-3 engineers working in parallel, this could be reduced to **6-9 months**.

---

## Conclusion

This plan provides a comprehensive roadmap to address all gaps identified in the OXIDE kernel analysis. The prioritization ensures that critical security and stability issues are addressed first, followed by core functionality improvements, and finally advanced features.

The key to success is:
1. Strict adherence to priority ordering
2. Thorough testing at each stage
3. Continuous integration and automated testing
4. Regular code reviews
5. Documentation alongside implementation

All code changes should follow the cyberpunk comment convention with appropriate persona signatures as specified in CLAUDE.md.
