# Project: oxide-os

Custom x86_64 OS written in Rust with CFS scheduler, VFS, SMP, signals, COW fork/exec, and userspace. Memory subsystem fully hardened (March 2026): buddy allocator corruption resilience, RAII frame guards, COW TOCTOU fix, ISR deadlock prevention, TLB shootdown correctness, arch-agnostic mm crates. PML4 exec race fixed. QEMU MCP tool monitorCommand race fixed. Full login flow verified working end-to-end.

**Tech Stack:** QEMU, Rust, UEFI bootloader (custom bindings), os_log ISR-safe output, x86_64 assembly

**Key Patterns:**

- COW try_claim_exclusive atomic ops
- RAII FrameGuard for OOM cleanup
- cyberpunk persona comments
- debug-feature gating
- os_log::write_*_raw for arch-agnostic serial
- per-CPU data structures
- try_lock in exception/ISR context

**Active Goals:**

- QEMU MCP server needs restart to activate monitorCommand fix
- boot manager and UEFI custom bindings
- perf-security audit remediation (memory hardening COMPLETE)

**Stats:** 93 sessions, 23 thoughts, 30 rules

## Rules

- 🔴 ❌ 📌 [Never add Copilot co-authored-by trailer to commits](../rules/never-add-copilot-co-authored-by-trailer-to-commits.md) (×1)
- 🔴 ❌ 📌 [Timer ISR (scheduler_tick) must NEVER call functions that allocate from the heap](../rules/timer-isr-scheduler-tick-must-never-call-functions-that-allocate-from-the-heap.md) (×1)
- 🔴 ✅ 📌 [Manual sysretq/iretq paths MUST disable interrupts (cli) before touching RSP or swapgs](../rules/manual-sysretq-iretq-paths-must-disable-interrupts-cli-before-touching-rsp-or-sw.md) (×1)
- 🔴 ✅ [iret frame SS must be derived from CS — never default independently](../rules/iret-frame-ss-must-be-derived-from-cs-never-default-independently.md) (×1)
- 🔴 ✅ [Every AP must have a proper idle Task in its run queue BTreeMap](../rules/every-ap-must-have-a-proper-idle-task-in-its-run-queue-btreemap.md) (×1)
- 🔴 ✅ [Task context MUST be set before add_task — never enqueue with uninitialized context](../rules/task-context-must-be-set-before-add-task-never-enqueue-with-uninitialized-contex.md) (×1)
- 🔴 ❌ [NEVER use UTF-8 multi-byte characters in bootloader rendered strings](../rules/never-use-utf-8-multi-byte-characters-in-bootloader-rendered-strings.md) (×1)
- 🔴 ✅ [Buddy allocator: always validate canary before dereferencing any FreeBlock pointer](../rules/buddy-allocator-always-validate-canary-before-dereferencing-any-freeblock-pointe.md) (×1)
- 🔴 ✅ [Fork/exec frame allocation MUST use RAII guards for cleanup on OOM](../rules/fork-exec-frame-allocation-must-use-raii-guards-for-cleanup-on-oom.md) (×1)
- 🔴 ✅ [Page fault handler locks MUST use try_lock — never blocking lock in exception context](../rules/page-fault-handler-locks-must-use-try-lock-never-blocking-lock-in-exception-cont.md) (×1)
- 🔴 ❌ [COW tracker operations MUST be atomic — never split ref_count check from remove/decrement](../rules/cow-tracker-operations-must-be-atomic-never-split-ref-count-check-from-remove-de.md) (×1)
- 🔴 ✅ [exec MUST fail if get_task_meta returns None — never continue to enter_usermode with orphaned address space](../rules/exec-must-fail-if-get-task-meta-returns-none-never-continue-to-enter-usermode-wi.md) (×1)
- 🔴 ✅ [sys_read/sys_write MUST use kernel-stack buffer — never pass user-space pointers through VFS stack](../rules/sys-read-sys-write-must-use-kernel-stack-buffer-never-pass-user-space-pointers-t.md) (×1)
- 🔴 ✅ [exec must update scheduler metadata BEFORE replacing address space in ProcessMeta](../rules/exec-must-update-scheduler-metadata-before-replacing-address-space-in-processmet.md) (×1)
- 🔴 ❌ [NEVER do full-screen MMIO framebuffer blit in ISR context](../rules/never-do-full-screen-mmio-framebuffer-blit-in-isr-context.md) (×1)
- 🔴 ✅ [Kernel preemption model: hybrid Linux-style — never preempt without kpo, emergency timeout at 500 ticks](../rules/kernel-preemption-model-hybrid-linux-style-never-preempt-without-kpo-emergency-t.md) (×1)
- 🔴 ✅ [Use KernelMutex (not spin::Mutex) for locks reachable from timer ISR — heap, VFS, block I/O](../rules/use-kernelmutex-not-spin-mutex-for-locks-reachable-from-timer-isr-heap-vfs-block.md) (×1)
- 🔴 ✅ [Fork must mark ALL present user pages as COW — not just writable ones](../rules/fork-must-mark-all-present-user-pages-as-cow-not-just-writable-ones.md) (×1)
- 🔴 ✅ [VmAreaList::insert() must silently accept zero-size VMAs (start >= end)](../rules/vmarealist-insert-must-silently-accept-zero-size-vmas-start-end.md) (×1)
- 🔴 ✅ [Scheduler RunQueue uses flat slot array (not BTreeMap) — O(1) task lookup via PID_TO_SLOT global](../rules/scheduler-runqueue-uses-flat-slot-array-not-btreemap-o-1-task-lookup-via-pid-to.md) (×1)
- 🔴 ✅ [clock_gettime MUST use TSC for sub-tick precision — 100Hz timer gives 10ms granularity which fails sequential-call tests](../rules/clock-gettime-must-use-tsc-for-sub-tick-precision-100hz-timer-gives-10ms-granula.md) (×1)
- 🔴 ✅ [Buddy allocator: mark_free in pagedb BEFORE free_to_zone — never after](../rules/buddy-allocator-mark-free-in-pagedb-before-free-to-zone-never-after.md) (×1)
- 🔴 ✅ [Fork must use parent-runs-first and distribute children across CPUs](../rules/fork-must-use-parent-runs-first-and-distribute-children-across-cpus.md) (×1)
- 🔴 ✅ [remove_task MUST clear on_rq=false before returning Task — work stealing depends on it](../rules/remove-task-must-clear-on-rq-false-before-returning-task-work-stealing-depends-o.md) (×1)
- 🔴 ✅ [Reset ALL DMA-capable devices BEFORE buddy allocator init — OVMF leaves them running](../rules/reset-all-dma-capable-devices-before-buddy-allocator-init-ovmf-leaves-them-runni.md) (×1)
- 🔴 ✅ [Kernel page faults should kill the offending task, not panic the CPU — Linux oops model](../rules/kernel-page-faults-should-kill-the-offending-task-not-panic-the-cpu-linux-oops-m.md) (×1)
- 🟡 ✅ [Test rule: always run make build before testing](../rules/test-rule-always-run-make-build-before-testing.md) (×2)
- 🟡 ✅ [uefi-rs 0.32 Char16 uses Into<u16> not to_u16() — use (*ch).into() for UTF-16 conversion](../rules/uefi-rs-0-32-char16-uses-into-u16-not-to-u16-use-ch-into-for-utf-16-conversion.md) (×1)
- 🟡 ❌ [mm-core and mm-paging MUST NOT depend on arch_x86_64 for serial output — use os_log](../rules/mm-core-and-mm-paging-must-not-depend-on-arch-x86-64-for-serial-output-use-os-lo.md) (×1)
- 🟡 ✅ [setup-std-source.sh: use python3 for precise multi-line string replacements, never sed for owned.rs/raw.rs](../rules/setup-std-source-sh-use-python3-for-precise-multi-line-string-replacements-never.md) (×1)

## Thoughts

- [observation] 📌 [Complete compositor and scrollbar architecture analysis for Win95-style scrollbar widget](../thoughts/complete-compositor-and-scrollbar-architecture-analysis-for-win95-style-scrollba.md)
- [observation] [aarch64 port analysis: ~25-30% complete, 6+ x86 timebombs in non-arch code, QEMU virt is best test path](../thoughts/aarch64-port-analysis-25-30-complete-6-x86-timebombs-in-non-arch-code-qemu-virt.md)
- [observation] 📌 [Complete VT/Terminal/Framebuffer architecture research for compositor implementation](../thoughts/complete-vt-terminal-framebuffer-architecture-research-for-compositor-implementa.md)
- [decision] 📌 [Tiling VT Compositor architecture decision for OXIDE OS graphics](../thoughts/tiling-vt-compositor-architecture-decision-for-oxide-os-graphics.md)
- [observation] [Comprehensive analysis of double-free sources in UserAddressSpace::Drop](../thoughts/comprehensive-analysis-of-double-free-sources-in-useraddressspace-drop.md)
- [decision] [Implemented mm-pagedb crate: Linux-style page frame database (struct page)](../thoughts/implemented-mm-pagedb-crate-linux-style-page-frame-database-struct-page.md)
- [observation] 📌 [OXIDE OS memory management subsystem architecture - complete overview](../thoughts/oxide-os-memory-management-subsystem-architecture-complete-overview.md)
- [observation] [Plan 6 Part 1 (raw UEFI FFI) is ALREADY COMPLETE -- uefi crate fully replaced](../thoughts/plan-6-part-1-raw-uefi-ffi-is-already-complete-uefi-crate-fully-replaced.md)
- [decision] [VMA subsystem (mm-vma) implemented and boot-verified — Build 75](../thoughts/vma-subsystem-mm-vma-implemented-and-boot-verified-build-75.md)
- [observation] [Page 0x46ebe0 mapped read-only without COW: fork.rs doesn't check VMA-intended permissions](../thoughts/page-0x46ebe0-mapped-read-only-without-cow-fork-rs-doesn-t-check-vma-intended-pe.md)
- [observation] 📌 [oxide-std heap allocation architecture: lock-free mmap-backed bump allocator](../thoughts/oxide-std-heap-allocation-architecture-lock-free-mmap-backed-bump-allocator.md)
- [observation] [Memory safety audit March 2026: ALL 11+8 issues FIXED across buddy, COW, fork, exec, munmap, arch coupling](../thoughts/memory-safety-audit-march-2026-all-11-8-issues-fixed-across-buddy-cow-fork-exec.md)
- [observation] [Buddy allocator corruption at 0x1ff13000 was intermittent — caused by UEFI firmware memory map variation between boots, not a buddy allocator bug](../thoughts/buddy-allocator-corruption-at-0x1ff13000-was-intermittent-caused-by-uefi-firmwar.md)
- [observation] [Post-init memory writes and AP boot code analysis](../thoughts/post-init-memory-writes-and-ap-boot-code-analysis.md)
- [observation] [Buddy allocator split code and canary handling - potential stale free list issue](../thoughts/buddy-allocator-split-code-and-canary-handling-potential-stale-free-list-issue.md)
- [observation] [Buddy allocator corruption investigation - bootloader memory map + exit_boot_services flow](../thoughts/buddy-allocator-corruption-investigation-bootloader-memory-map-exit-boot-service.md)
- [decision] [Successfully replaced uefi-rs crate with custom UEFI bindings in bootloader](../thoughts/successfully-replaced-uefi-rs-crate-with-custom-uefi-bindings-in-bootloader.md)
- [decision] [OXIDE Boot Manager implementation completed — 8 new modules in bootloader, boot protocol extension, kernel cmdline parser](../thoughts/oxide-boot-manager-implementation-completed-8-new-modules-in-bootloader-boot-pro.md)
- [observation] 📌 [x86-64 interrupt frame.rsp contains KERNEL RSP for kernel-mode interrupts — NOT user RSP](../thoughts/x86-64-interrupt-frame-rsp-contains-kernel-rsp-for-kernel-mode-interrupts-not-us.md)
- [idea] 📌 [RSP-to-RIP clobber: Fix strategy and affected code locations](../thoughts/rsp-to-rip-clobber-fix-strategy-and-affected-code-locations.md)
- [observation] 📌 [Complete code flow diagram: timer ISR → scheduler_tick → context switch → iret frame building](../thoughts/complete-code-flow-diagram-timer-isr-scheduler-tick-context-switch-iret-frame-bu.md)
- [observation] 📌 [RSP-to-RIP clobber root cause analysis — register flow in context switch](../thoughts/rsp-to-rip-clobber-root-cause-analysis-register-flow-in-context-switch.md)
- [observation] [OXIDE OS is a custom x86_64 operating system written in Rust](../thoughts/oxide-os-is-a-custom-x86-64-operating-system-written-in-rust.md)

## Error Patterns

- [Buddy allocator free list corruption — alloc_contiguous fail](../errors/buddy-allocator-free-list-corruption-alloc-contiguous-fails-for-gpu-framebuffer.md)
- [Only 2-3 of 6 VTs work randomly on each boot. Tasks assigned](../errors/only-2-3-of-6-vts-work-randomly-on-each-boot-tasks-assigned-to-certain-cpus-neve.md)
- [Only 2 of 6 VT gettys get exec'd. Init forks all 6 PIDs succ](../errors/only-2-of-6-vt-gettys-get-exec-d-init-forks-all-6-pids-successfully-confirmed-by.md)
- [VT switch from OSK deadlocks the system. CPU#0 spins forever](../errors/vt-switch-from-osk-deadlocks-the-system-cpu-0-spins-forever-in-terminal-vt-termi.md)
- [Page fault at 0x66 with RIP in .rodata after fork+exec of se](../errors/page-fault-at-0x66-with-rip-in-rodata-after-fork-exec-of-servicemgr-ac-flag-set.md)
- [System hangs during pivot_root syscall — timer ISR deadlocks](../errors/system-hangs-during-pivot-root-syscall-timer-isr-deadlocks-on-heap-lock-schedule.md)
- [Pagedb array sized by buddy total_bytes instead of max physi](../errors/pagedb-array-sized-by-buddy-total-bytes-instead-of-max-physical-address-frames-a.md)
- [PML4 canary check false-positive on idle task (PID 0, cr3=0)](../errors/pml4-canary-check-false-positive-on-idle-task-pid-0-cr3-0-killed-idle-loop-froze.md)
- [oxide-test used Linux syscall numbers (READ=0, WRITE=1, CLOS](../errors/oxide-test-used-linux-syscall-numbers-read-0-write-1-close-3-instead-of-oxide-nu.md)
- [oxide-test SIGSEGV on first test — write fault at 0x46ebe0 k](../errors/oxide-test-sigsegv-on-first-test-write-fault-at-0x46ebe0-killed-pid-9-immediatel.md)
- [Full-screen MMIO framebuffer blit in ISR (timer tick) contex](../errors/full-screen-mmio-framebuffer-blit-in-isr-timer-tick-context-causes-768ms-interru.md)
- [PML4 corruption race in kernel_exec — scheduler could contex](../errors/pml4-corruption-race-in-kernel-exec-scheduler-could-context-switch-to-task-with.md)
- [QEMU MCP monitorCommand() race condition — commands silently](../errors/qemu-mcp-monitorcommand-race-condition-commands-silently-never-sent.md)
- [sys_read_vfs and sys_write_vfs passed user-space buffer poin](../errors/sys-read-vfs-and-sys-write-vfs-passed-user-space-buffer-pointers-through-the-ent.md)
- [kernel_exec leaks entire new address space if sched::get_tas](../errors/kernel-exec-leaks-entire-new-address-space-if-sched-get-task-meta-current-pid-re.md)
- [TLS region at 0x7000_0000_0000 collides with MMAP_BASE_DEFAU](../errors/tls-region-at-0x7000-0000-0000-collides-with-mmap-base-default-at-the-same-addre.md)
- [UserAddressSpace::Drop only freed PT structure frames from a](../errors/useraddressspace-drop-only-freed-pt-structure-frames-from-allocated-frames-missi.md)
- [clear_child_tid, parent_tid_addr, child_tid_addr written as ](../errors/clear-child-tid-parent-tid-addr-child-tid-addr-written-as-raw-pointers-without-u.md)
- [sys_munmap and sys_brk shrink path leaked every unmapped phy](../errors/sys-munmap-and-sys-brk-shrink-path-leaked-every-unmapped-physical-frame-unmap-us.md)
- [Bootloader fails to load kernel with 'Kernel file not found'](../errors/bootloader-fails-to-load-kernel-with-kernel-file-not-found-33mb-debug-kernel-exc.md)
- [Boot menu displays ????? characters and scrunched layout — U](../errors/boot-menu-displays-characters-and-scrunched-layout-utf-8-multi-byte-strings-in-c.md)
- [Duplicate lang item error when building bootloader with -Zbu](../errors/duplicate-lang-item-error-when-building-bootloader-with-zbuild-std-core-but-fb-c.md)
- [BSP idle task (PID 0) had rsp=0 in initial TaskContext. When](../errors/bsp-idle-task-pid-0-had-rsp-0-in-initial-taskcontext-when-scheduler-first-switch.md)
- [GPF (#GP 0x18) at iretq in timer_interrupt handler. RIP=iret](../errors/gpf-gp-0x18-at-iretq-in-timer-interrupt-handler-rip-iretq-error-code-0x18-gdt-in.md)
- [Test error: triple fault on boot when GDT not loaded on AP c](../errors/test-error-triple-fault-on-boot-when-gdt-not-loaded-on-ap-cores.md)

## Sessions

| Date | Agent | Goal | Status |
|------|-------|------|--------|
| 2026-03-14 | claude-code/claude-haiku-4-5-20251001 | [Investigate invalid opcode crash at RIP 0x43c7f0 in userspace binary - check ELF loading, segment mapping, and signal handling](../sessions/investigate-invalid-opcode-crash-at-rip-0x43c7f0-in-userspace-binary-check-elf-l.md) | completed |
| 2026-03-13 | claude-code/claude-opus-4-6 | [Fix multi-VT terminal spawning: only 1 VT gets a working shell. Debug and fix the full pipeline: init fork, getty exec, VT switch deadlock.](../sessions/fix-multi-vt-terminal-spawning-only-1-vt-gets-a-working-shell-debug-and-fix-the.md) | active |
| 2026-03-13 | claude-code/claude-opus-4-6 | [Fix fork sysretq timer interrupt race — diagnose and fix page fault crash after fork+exec of servicemgr](../sessions/fix-fork-sysretq-timer-interrupt-race-diagnose-and-fix-page-fault-crash-after-fo.md) | completed |
| 2026-03-13 | claude-code/claude-opus-4-6 | [Debug kernel page fault at address 0x66 (null pointer + offset) after fork+exec of servicemgr](../sessions/debug-kernel-page-fault-at-address-0x66-null-pointer-offset-after-fork-exec-of-s.md) | active |
| 2026-03-08 | claude-code/claude-haiku-4-5-20251001 | [Understand compositor architecture and scrollbar implementation for Win95-style scrollbar widget development](../sessions/understand-compositor-architecture-and-scrollbar-implementation-for-win95-style.md) | active |
| 2026-03-08 | copilot/gpt-5.1-codex-max | [Investigate runqueue corruption root cause triggering scheduler hang](../sessions/investigate-runqueue-corruption-root-cause-triggering-scheduler-hang.md) | active |
| 2026-03-08 | copilot/gpt-5.1-codex-max | [Attach to running QEMU with GDB to debug hang](../sessions/attach-to-running-qemu-with-gdb-to-debug-hang.md) | active |
| 2026-03-08 | claude-code/claude-haiku-4-5-20251001 | [Debug why login prompt never shows on VT after boot - investigate VT initialization, console routing, and getty spawning](../sessions/debug-why-login-prompt-never-shows-on-vt-after-boot-investigate-vt-initializatio.md) | completed |
| 2026-03-08 | claude-code/claude-opus-4-6 | [Debug why login prompt never shows on VT - user reports no shell appears after boot](../sessions/debug-why-login-prompt-never-shows-on-vt-user-reports-no-shell-appears-after-boo.md) | active |
| 2026-03-08 | claude-code/claude-opus-4-6 | [Debug why login prompt never appears after boot - VT/getty/login pipeline broken](../sessions/debug-why-login-prompt-never-appears-after-boot-vt-getty-login-pipeline-broken.md) | active |
| 2026-03-07 | copilot/gpt-5.1-codex-max | [Investigate boot log anomalies (buddy free-list corruption, GOT mapping errors)](../sessions/investigate-boot-log-anomalies-buddy-free-list-corruption-got-mapping-errors.md) | active |
| 2026-03-07 | copilot/claude-opus-4.6 | [Create comprehensive architectural layout document of the OXIDE OS kernel](../sessions/create-comprehensive-architectural-layout-document-of-the-oxide-os-kernel.md) | active |
| 2026-03-07 | copilot/claude-opus-4.6 | [Analyze what it would take to port OXIDE OS to Raspberry Pi (aarch64)](../sessions/analyze-what-it-would-take-to-port-oxide-os-to-raspberry-pi-aarch64.md) | active |
| 2026-03-07 | claude-code/claude-opus-4-6 | [Build system redesign - proper dependency chains, debug/release profiles, no stale binaries](../sessions/build-system-redesign-proper-dependency-chains-debug-release-profiles-no-stale-b.md) | active |
| 2026-03-07 | claude-code/claude-opus-4-6 | [Diagnose and fix lock/login failures in esh after recent arch refactoring, syscall alignment, and vterm rebuilds](../sessions/diagnose-and-fix-lock-login-failures-in-esh-after-recent-arch-refactoring-syscal.md) | active |
| 2026-03-06 | claude-code/claude-opus-4-6 | [Research VT, terminal, framebuffer, and compositor architecture for understanding how to implement compositing features](../sessions/research-vt-terminal-framebuffer-and-compositor-architecture-for-understanding-h.md) | active |
| 2026-03-06 | copilot/claude-opus-4.6 | [Comprehensive performance analysis of OXIDE OS - identify bottlenecks, hot paths, and optimization opportunities](../sessions/comprehensive-performance-analysis-of-oxide-os-identify-bottlenecks-hot-paths-an.md) | active |
| 2026-03-06 | claude-code/claude-opus-4-6 | [Explore package manager and plan building vim and python for OXIDE OS](../sessions/explore-package-manager-and-plan-building-vim-and-python-for-oxide-os.md) | active |
| 2026-03-05 | claude-code/claude-opus-4-6 | [Fix root cause of BUDDY-DUP-ALLOC: coalescing merges with allocated buddies](../sessions/fix-root-cause-of-buddy-dup-alloc-coalescing-merges-with-allocated-buddies.md) | active |
| 2026-03-05 | claude-code/claude-opus-4-6 | [Fix root cause of hundreds of BUDDY-DUP-ALLOC stale free-list entries causing OOM crashes](../sessions/fix-root-cause-of-hundreds-of-buddy-dup-alloc-stale-free-list-entries-causing-oo.md) | active |
