# P1 Priority Implementation Plan
**Date:** 2026-02-02
**Status:** Ready to Start
**Decision:** Skip SMAP (P0), focus on functionality (P1)

---

## Overview

After completing thread creation (P0), we're moving directly to P1 priorities to maximize functionality and value delivery. SMAP security hardening is deferred as it's not blocking development.

---

## P1 Items (Priority Order)

### 1. ext4 Timestamps & Metadata ⭐ HIGHEST PRIORITY
**Complexity:** Low
**Estimated Effort:** 1-2 weeks
**Impact:** File attributes work correctly
**Why First:** Quick win, high visibility, low risk

**Current Problem:**
- All files show timestamp 0 (Jan 1, 1970)
- All files show owner root:root
- No extended attributes

**Implementation:**
- ✅ ext4 driver already exists
- Need to read/write inode timestamps
- Need to read/write uid/gid
- Need xattr support

**Files to Modify:**
- `kernel/vfs/ext4/src/inode.rs` - Add timestamp reading
- `kernel/vfs/ext4/src/metadata.rs` - Add uid/gid support
- `kernel/syscall/syscall/src/vfs.rs` - Wire up stat() properly

**Success Criteria:**
- `ls -l` shows correct timestamps
- `ls -l` shows correct ownership
- `touch` updates modification time
- `chown`/`chmod` work

---

### 2. SMP/Multi-Core Support 🚀 MAJOR PERFORMANCE
**Complexity:** Very High
**Estimated Effort:** 6-8 weeks
**Impact:** 2-16x performance improvement
**Why Second:** Biggest performance gain, enables parallel workloads

**Current Problem:**
- Only CPU 0 boots
- APs timeout during boot
- Scheduler only uses one core

**Implementation Steps:**
1. **Fix AP Boot** (2 weeks)
   - Fix ACPI MADT parsing
   - Fix trampoline code
   - Get APs to start

2. **Per-CPU Data** (1-2 weeks)
   - GS_BASE for per-CPU data
   - Per-CPU scheduler queues
   - Per-CPU idle tasks

3. **Load Balancing** (2-3 weeks)
   - Migration between CPUs
   - Load balancing algorithm
   - CPU affinity support

4. **IPI Implementation** (1 week)
   - TLB shootdown
   - Reschedule IPI
   - Function call IPI

5. **Testing** (1-2 weeks)
   - Stress testing
   - Race condition hunting
   - Performance validation

**Success Criteria:**
- All CPUs boot and idle
- Tasks distributed across cores
- `htop` shows all CPUs
- Parallel builds work
- No race conditions under stress

---

### 3. TCP Non-Loopback Networking 🌐 CONNECTIVITY
**Complexity:** High
**Estimated Effort:** 3-4 weeks
**Impact:** External network access
**Why Third:** Network connectivity for real applications

**Current Problem:**
- TCP only works on loopback
- VirtIO-net incomplete
- Cannot connect to external hosts

**Implementation Steps:**
1. **VirtIO-net Driver** (1-2 weeks)
   - Complete driver implementation
   - TX/RX queue management
   - Interrupt handling

2. **TCP Stack Fixes** (1 week)
   - Out-of-order packet handling
   - Window scaling
   - SACK support

3. **ARP/DHCP** (1 week)
   - ARP resolution
   - DHCP client
   - Routing table

4. **Testing** (1 week)
   - SSH to external machines
   - HTTP requests
   - File transfers

**Success Criteria:**
- Can ping external hosts
- Can SSH to remote machines
- Can fetch HTTP content
- TCP throughput reasonable

---

## Recommended Order

### Option A: Quick Wins First (Recommended)
1. **ext4 Metadata** (1-2 weeks) - Quick win, high visibility
2. **TCP Networking** (3-4 weeks) - Enables real applications
3. **SMP Support** (6-8 weeks) - Major performance, complex

**Rationale:** Get quick wins and functionality before tackling complex SMP

**Timeline:** 10-14 weeks total

### Option B: Performance First
1. **SMP Support** (6-8 weeks) - Biggest impact
2. **ext4 Metadata** (1-2 weeks) - Quick follow-up
3. **TCP Networking** (3-4 weeks) - Connectivity

**Rationale:** Maximize performance early, complex work first

**Timeline:** 10-14 weeks total

### Option C: Parallel Development (If 2+ Engineers)
- **Engineer 1:** SMP Support (6-8 weeks)
- **Engineer 2:** ext4 Metadata (1-2 weeks) → TCP Networking (3-4 weeks)

**Rationale:** Maximize throughput with parallel work

**Timeline:** 6-8 weeks total (with 2 engineers)

---

## Decision: Start with ext4 Metadata

**Why:**
- ✅ Low complexity, low risk
- ✅ High visibility (users see file dates)
- ✅ Quick win (1-2 weeks)
- ✅ Builds confidence and momentum
- ✅ Can be done by single engineer
- ✅ ext4 driver already works, just missing metadata

**Next After ext4:**
- TCP Networking (3-4 weeks) - More functionality
- Then SMP (6-8 weeks) - Performance boost

---

## Success Metrics

### After ext4 Metadata Complete
- ✅ Files have correct timestamps
- ✅ `ls -l` shows owners
- ✅ `chown` and `chmod` work
- ✅ Better POSIX compliance

### After TCP Networking Complete
- ✅ Can access external services
- ✅ SSH to remote machines
- ✅ Download files via HTTP
- ✅ Run networked applications

### After SMP Complete
- ✅ All CPU cores utilized
- ✅ 2-16x performance improvement
- ✅ Parallel builds fast
- ✅ Better responsiveness under load

---

## Risk Assessment

### ext4 Metadata - LOW RISK
- Well-documented format
- Driver already working
- Just reading/writing fields

### TCP Networking - MEDIUM RISK
- Complex protocol interactions
- Need thorough testing
- Potential race conditions

### SMP - HIGH RISK
- Very complex
- Hardware-dependent
- Race conditions hard to debug
- Needs JTAG/hardware debugger

---

## Immediate Next Steps

1. ✅ Update progress tracker (DONE)
2. ✅ Create P1 plan (this document)
3. 🔄 Start ext4 metadata implementation
   - Audit current ext4 inode reading
   - Add timestamp field reading
   - Add uid/gid field reading
   - Wire up to stat() syscall
4. Create ext4 test suite
5. Implement and test

---

## Timeline Estimate

**Conservative (1 engineer):**
- ext4 Metadata: 2 weeks
- TCP Networking: 4 weeks
- SMP Support: 8 weeks
- **Total:** 14 weeks (~3.5 months)

**Realistic (1 engineer):**
- ext4 Metadata: 1.5 weeks
- TCP Networking: 3 weeks
- SMP Support: 6 weeks
- **Total:** 10.5 weeks (~2.5 months)

**Optimistic (2 engineers, parallel):**
- ext4 Metadata: 1 week (Engineer 2)
- TCP Networking: 3 weeks (Engineer 2)
- SMP Support: 6 weeks (Engineer 1)
- **Total:** 6 weeks (overlapped)

---

## Deliverables

### Phase 1: ext4 Metadata (Week 1-2)
- Updated ext4 driver with metadata support
- Working stat() with timestamps/ownership
- Working chown/chmod/touch
- Test suite

### Phase 2: TCP Networking (Week 3-6)
- VirtIO-net driver complete
- External TCP connections work
- Can SSH to remote hosts
- Test suite

### Phase 3: SMP Support (Week 7-14)
- All CPUs boot and run
- Load balancing working
- IPI infrastructure
- Stress test passing

---

## Current Status

✅ Thread creation complete (P0)
🔄 Moving to P1 priorities
⏭️ Starting with ext4 metadata (highest priority, lowest risk)

**Next Action:** Begin ext4 metadata implementation

---

**References:**
- Main plan: `docs/IMPLEMENTATION_PLAN.md`
- Progress: `docs/PROGRESS_TRACKER.md`
- Thread impl: `docs/THREAD_IMPLEMENTATION.md`
