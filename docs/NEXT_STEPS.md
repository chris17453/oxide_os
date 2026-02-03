# OXIDE OS - Next Steps Quick Reference
**Date:** 2026-02-03  
**For:** Development team starting work on production readiness  
**Read First:** `docs/SYSTEM_ANALYSIS_2026.md` for complete context

---

## TL;DR: What to Work On Next

### Priority 1: Validate Recent Network Fix 🚨
**Owner:** ShadePacket (Network Engineer)  
**Time:** 1-2 days  
**Why Critical:** We just added tcpip::poll() - need to confirm external networking works

```bash
# Test procedure:
make build-full run

# In OXIDE terminal:
$ ping 8.8.8.8
$ ssh user@some-external-host

# Expected: Should work now
# If not: Debug VirtIO-net driver + TCP/IP stack
```

**Success Criteria:**
- ✅ Can ping external hosts
- ✅ Can SSH to/from OXIDE
- ✅ TCP connections stable

**If Fails:** This is P1 blocker - debug immediately

---

### Priority 2: SMP/Multi-Core Support 🎯
**Owner:** GraveShift + NeonRoot (Kernel Engineers)  
**Time:** 6-8 weeks  
**Impact:** 2-16x performance improvement

**Current Problem:**
- Only CPU 0 boots and runs
- APs timeout during boot sequence  
- Massive performance loss on modern hardware

**Implementation Steps:**

#### Week 1-2: AP Boot Fix
```rust
// kernel/smp/smp/src/lib.rs
// kernel/arch/arch-x86_64/src/smp.rs

1. Fix ACPI MADT parsing
   - Parse processor entries correctly
   - Extract LAPIC IDs

2. Fix trampoline code
   - Real-mode entry point
   - Protected mode transition
   - Long mode jump

3. SIPI sequence
   - Send INIT IPI
   - Wait 10ms
   - Send SIPI x2
   - Poll for AP response
```

#### Week 3-4: Per-CPU Data
```rust
// kernel/arch/arch-x86_64/src/percpu.rs (new)

1. GS_BASE for per-CPU storage
   - Set up on each CPU
   - Store CPU ID, scheduler queue

2. Per-CPU scheduler queues
   - Separate run queue per CPU
   - Per-CPU idle task

3. Per-CPU interrupt stacks
```

#### Week 5-6: Load Balancing
```rust
// kernel/sched/sched/src/balance.rs (new)

1. Task migration
   - Move tasks between CPUs
   - Affinity masks

2. Load balancing algorithm
   - Check imbalance periodically
   - Steal tasks from busy CPUs

3. CPU affinity
   - sched_setaffinity syscall
   - Respect pinned tasks
```

#### Week 7-8: IPI & Testing
```rust
// kernel/arch/arch-x86_64/src/ipi.rs (new)

1. IPI infrastructure
   - Send IPI to specific CPU
   - Send IPI to all CPUs

2. TLB shootdown
   - Invalidate TLB on all CPUs
   - Wait for acknowledgment

3. Stress testing
   - Boot 16 CPUs
   - Run parallel workloads
   - Hunt race conditions
```

**Success Criteria:**
- ✅ All CPUs boot and idle
- ✅ Tasks distributed across cores
- ✅ `htop` shows all CPUs
- ✅ Parallel builds work (test with `make -j16`)
- ✅ No race conditions under stress

**Files to Modify:**
- `kernel/smp/smp/src/lib.rs`
- `kernel/arch/arch-x86_64/src/smp.rs`
- `kernel/sched/sched/src/lib.rs`
- `kernel/arch/arch-x86_64/src/interrupt.rs`
- Create `kernel/arch/arch-x86_64/src/percpu.rs`
- Create `kernel/arch/arch-x86_64/src/ipi.rs`
- Create `kernel/sched/sched/src/balance.rs`

**References:**
- `docs/P1_PRIORITIES.md` Section 2
- `docs/IMPLEMENTATION_PLAN.md` P1 Item
- Linux kernel: `arch/x86/kernel/smpboot.c`
- OSDev Wiki: https://wiki.osdev.org/SMP

---

### Priority 3: SMAP Fix 🔒
**Owner:** BlackLatch (Security Engineer)  
**Time:** 2-3 weeks  
**Impact:** Close security vulnerability

**Current Problem:**
- SMAP disabled due to AC flag timing bug
- Kernel can accidentally access user memory
- Security risk

**Implementation:**

#### Week 1: AC Flag Management
```rust
// kernel/arch/arch-x86_64/src/interrupt.rs

1. Save/restore AC flag in interrupt handlers
   - RFLAGS.AC must be preserved
   - Save on entry, restore on exit

2. Fix syscall handler
   - Ensure AC flag correct during syscall

3. Fix page fault handler
   - Don't corrupt RFLAGS
```

#### Week 2: Explicit User Access
```rust
// kernel/mm/mm-core/src/user_access.rs (new)

1. stac() / clac() wrappers
   - Set AC flag (allow user access)
   - Clear AC flag (disallow user access)

2. Wrap all user memory access
   - copy_from_user()
   - copy_to_user()
   - All syscall handlers

3. Atomic sections
   - No interrupts during user access
   - Or handle AC flag in interrupt
```

#### Week 3: Testing & Validation
```bash
# Enable SMAP
# kernel/arch/arch-x86_64/src/cpu.rs
set_smap(true);

# Run stress tests
make build-full run

# Should NOT see:
# "SMAP violation" page faults

# Run for 24 hours
# All tests must pass
```

**Success Criteria:**
- ✅ SMAP enabled with no violations
- ✅ All syscalls work correctly
- ✅ Stress test passes 24 hours
- ✅ No performance regression

**Files to Modify:**
- `kernel/arch/arch-x86_64/src/interrupt.rs`
- `kernel/arch/arch-x86_64/src/syscall.rs`
- `kernel/mm/mm-core/src/user_access.rs` (create)
- All syscall handlers in `kernel/syscall/syscall/src/`

**References:**
- `docs/IMPLEMENTATION_PLAN.md` P0 Item 2
- Intel SDM Volume 3A, Section 4.6
- Linux kernel: `arch/x86/include/asm/smap.h`

---

## Phase 1 Completion Checklist

**Target:** 3-4 months to VM production readiness

- [ ] **Week 1-2:** Validate networking fix (Priority 1)
  - [ ] Test external connectivity
  - [ ] Fix any bugs found
  - [ ] Document configuration

- [ ] **Week 3-10:** Implement SMP support (Priority 2)
  - [ ] AP boot working
  - [ ] Per-CPU data structures
  - [ ] Load balancing
  - [ ] IPI infrastructure
  - [ ] Testing complete

- [ ] **Week 11-13:** SMAP fix (Priority 3)
  - [ ] AC flag management
  - [ ] User access wrappers
  - [ ] Validation testing

- [ ] **Week 14-16:** Polish & release
  - [ ] Update documentation
  - [ ] Performance benchmarks
  - [ ] Deployment guides
  - [ ] Release announcement

**Deliverable:** OXIDE OS 1.0 - Production-ready for VM deployment

---

## Quick Command Reference

### Building & Testing
```bash
# Full build
make build-full

# Run in QEMU
make run

# Run with all debug
make run-debug-all

# Run automated test
make test
```

### Cross-Compilation
```bash
# Set up toolchain
export PATH=$PWD/toolchain/bin:$PATH

# Compile C program
oxide-cc -o myapp myapp.c

# Deploy to OXIDE
cp myapp target/initramfs/bin/
make initramfs run
```

### Development Workflow
```bash
# 1. Make changes to kernel
vim kernel/smp/smp/src/lib.rs

# 2. Build kernel only
make kernel

# 3. Test (boots with new kernel)
make run

# 4. If userspace changes
make userspace

# 5. Full rebuild if needed
make build-full
```

### Debug Features
```bash
# Enable specific debug output
make run KERNEL_FEATURES=debug-sched     # Scheduler
make run KERNEL_FEATURES=debug-fork      # Fork/exec
make run KERNEL_FEATURES=debug-lock      # Locks
make run KERNEL_FEATURES=debug-all       # Everything

# Check serial output
cat target/serial.log
```

---

## Resource & Tool Requirements

### Hardware for Testing
- **Minimum:** x86_64 machine with 4GB RAM, QEMU
- **Recommended:** x86_64 machine with 16GB RAM, 8+ cores
- **For Phase 2:** Bare-metal test machines with SATA/NVMe/USB

### Software Dependencies
```bash
# Ubuntu/Debian
sudo apt install qemu-system-x86_64 edk2-ovmf clang lld

# Fedora/RHEL
sudo dnf install qemu-system-x86 edk2-ovmf clang lld
```

### Development Tools
- **Editor:** Any (vim, VSCode with rust-analyzer)
- **Debugger:** gdb with QEMU remote debugging
- **Version Control:** git
- **Rust:** Nightly (via rust-toolchain.toml)

---

## Team Organization (Recommended)

### Phase 1 Team (3-4 months)
- **2 Kernel Engineers**
  - Engineer 1: SMP implementation (6-8 weeks full time)
  - Engineer 2: Networking validation (1-2 weeks) → SMAP fix (2-3 weeks) → SMP support
- **0.5 Documentation Engineer**
  - Keep docs updated
  - Write deployment guides
  - Test procedures

### Communication
- Daily standups (async OK)
- Weekly progress reviews
- Slack/Discord for quick questions
- GitHub issues for tracking
- Pull requests for code review

### Milestones
1. **Week 4:** Networking validated ✅
2. **Week 10:** SMP complete, all CPUs running ✅
3. **Week 13:** SMAP enabled, security hardened ✅
4. **Week 16:** Release 1.0, production-ready ✅

---

## When You're Stuck

### Problem: Can't get APs to boot
**Debug Steps:**
1. Check QEMU CPU count: `-smp 4`
2. Check ACPI MADT parsing output
3. Check trampoline code address (below 1MB)
4. Add debug prints in AP entry point
5. Check LAPIC initialization
6. Verify SIPI timing (10ms delays)

**Resources:**
- OSDev SMP wiki: https://wiki.osdev.org/SMP
- Intel SDM Volume 3A, Chapter 8
- Linux code: `arch/x86/kernel/smpboot.c`

### Problem: Race conditions with SMP
**Debug Steps:**
1. Use `debug-lock` feature to trace lock usage
2. Add assertions for lock ordering
3. Check for missing locks around shared data
4. Use `-smp 1` to isolate SMP issues
5. Add debug prints with CPU ID

**Tools:**
- ThreadSanitizer (if we port it)
- Manual code review
- Stress testing

### Problem: SMAP violations
**Debug Steps:**
1. Check which syscall caused violation (RIP in crash)
2. Look for missing stac()/clac() wrappers
3. Check interrupt handler RFLAGS handling
4. Verify user memory access is wrapped

**Tools:**
- Page fault handler logs
- RIP → source mapping
- `addr2line` for addresses

---

## Success Metrics

### Phase 1 Complete When:
- ✅ All CPUs boot and run tasks
- ✅ Can ping/SSH to external hosts
- ✅ SMAP enabled with no violations
- ✅ System stable for 24+ hours under load
- ✅ Performance: 8-core system uses all cores
- ✅ Documentation complete

### How to Measure:
```bash
# 1. Boot test
make test   # Must pass

# 2. CPU test
$ cat /proc/cpuinfo | grep processor
processor : 0
processor : 1
processor : 2
processor : 3
# ... (all CPUs listed)

# 3. Network test
$ ping -c 10 8.8.8.8
10 packets transmitted, 10 received, 0% loss

# 4. Stress test
$ while true; do ls -lR / > /dev/null 2>&1; done &
# Run on all CPUs, watch htop, leave for 24h

# 5. Security test
# Boot with SMAP enabled
# No page faults with "SMAP violation" message
```

---

## Questions & Support

### Technical Questions
- **SMP/Kernel:** GraveShift (kernel architect)
- **Security:** BlackLatch (security engineer)
- **Networking:** ShadePacket (network engineer)
- **Build System:** PulseForge (build engineer)

### Documentation
- **Main Analysis:** `docs/SYSTEM_ANALYSIS_2026.md`
- **Implementation Plan:** `docs/IMPLEMENTATION_PLAN.md`
- **Progress Tracking:** `docs/PROGRESS_TRACKER.md`
- **P1 Priorities:** `docs/P1_PRIORITIES.md`
- **Build Guide:** `AGENTS.md`

### Community
- GitHub Issues: https://github.com/chris17453/oxide_os/issues
- Pull Requests: https://github.com/chris17453/oxide_os/pulls

---

## Final Notes

### Remember the Guardrails
- **No stubs or TODOs** - Implement fully or state blockers
- **Debug policy** - Never delete debug output, gate via features
- **Cyberpunk comments** - Use personas (GraveShift, BlackLatch, etc.)
- **Small commits** - One feature per commit
- **Test everything** - Build, boot, validate before committing

### The Big Picture
We're 65% there. The foundation is solid. With focused effort on:
1. SMP (6-8 weeks)
2. Networking (1-2 weeks)
3. Security (2-3 weeks)

We'll have a production-ready OS in 3-4 months. This is achievable.

**Let's ship it.** 🚀

---

**Last Updated:** 2026-02-03  
**Next Review:** After Phase 1 Milestone 1 (Week 4)
