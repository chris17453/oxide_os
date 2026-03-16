# Memgram Export

| Item | Count |
|------|-------|
| Sessions | 93 |
| Thoughts | 24 |
| Rules | 30 |
| Error Patterns | 25 |
| Groups | 1 |
| Links | 2 |
| Projects | 1 |

## Rules Overview

| Severity | Type | Summary | Reinforced | Project |
|----------|------|---------|------------|---------|
| critical | dont | 📌 [Never add Copilot co-authored-by trailer to commits](rules/never-add-copilot-co-authored-by-trailer-to-commits.md) | ×1 | oxide-os |
| critical | dont | 📌 [Timer ISR (scheduler_tick) must NEVER call functions that allocate from the heap](rules/timer-isr-scheduler-tick-must-never-call-functions-that-allocate-from-the-heap.md) | ×1 | oxide-os |
| critical | do | 📌 [Manual sysretq/iretq paths MUST disable interrupts (cli) before touching RSP or swapgs](rules/manual-sysretq-iretq-paths-must-disable-interrupts-cli-before-touching-rsp-or-sw.md) | ×1 | oxide-os |
| critical | do | [iret frame SS must be derived from CS — never default independently](rules/iret-frame-ss-must-be-derived-from-cs-never-default-independently.md) | ×1 | oxide-os |
| critical | do | [Every AP must have a proper idle Task in its run queue BTreeMap](rules/every-ap-must-have-a-proper-idle-task-in-its-run-queue-btreemap.md) | ×1 | oxide-os |
| critical | do | [Task context MUST be set before add_task — never enqueue with uninitialized context](rules/task-context-must-be-set-before-add-task-never-enqueue-with-uninitialized-contex.md) | ×1 | oxide-os |
| critical | dont | [NEVER use UTF-8 multi-byte characters in bootloader rendered strings](rules/never-use-utf-8-multi-byte-characters-in-bootloader-rendered-strings.md) | ×1 | oxide-os |
| critical | do | [Buddy allocator: always validate canary before dereferencing any FreeBlock pointer](rules/buddy-allocator-always-validate-canary-before-dereferencing-any-freeblock-pointe.md) | ×1 | oxide-os |
| critical | do | [Fork/exec frame allocation MUST use RAII guards for cleanup on OOM](rules/fork-exec-frame-allocation-must-use-raii-guards-for-cleanup-on-oom.md) | ×1 | oxide-os |
| critical | do | [Page fault handler locks MUST use try_lock — never blocking lock in exception context](rules/page-fault-handler-locks-must-use-try-lock-never-blocking-lock-in-exception-cont.md) | ×1 | oxide-os |
| critical | dont | [COW tracker operations MUST be atomic — never split ref_count check from remove/decrement](rules/cow-tracker-operations-must-be-atomic-never-split-ref-count-check-from-remove-de.md) | ×1 | oxide-os |
| critical | do | [exec MUST fail if get_task_meta returns None — never continue to enter_usermode with orphaned address space](rules/exec-must-fail-if-get-task-meta-returns-none-never-continue-to-enter-usermode-wi.md) | ×1 | oxide-os |
| critical | do | [sys_read/sys_write MUST use kernel-stack buffer — never pass user-space pointers through VFS stack](rules/sys-read-sys-write-must-use-kernel-stack-buffer-never-pass-user-space-pointers-t.md) | ×1 | oxide-os |
| critical | do | [exec must update scheduler metadata BEFORE replacing address space in ProcessMeta](rules/exec-must-update-scheduler-metadata-before-replacing-address-space-in-processmet.md) | ×1 | oxide-os |
| critical | dont | [NEVER do full-screen MMIO framebuffer blit in ISR context](rules/never-do-full-screen-mmio-framebuffer-blit-in-isr-context.md) | ×1 | oxide-os |
| critical | do | [Kernel preemption model: hybrid Linux-style — never preempt without kpo, emergency timeout at 500 ticks](rules/kernel-preemption-model-hybrid-linux-style-never-preempt-without-kpo-emergency-t.md) | ×1 | oxide-os |
| critical | do | [Use KernelMutex (not spin::Mutex) for locks reachable from timer ISR — heap, VFS, block I/O](rules/use-kernelmutex-not-spin-mutex-for-locks-reachable-from-timer-isr-heap-vfs-block.md) | ×1 | oxide-os |
| critical | do | [Fork must mark ALL present user pages as COW — not just writable ones](rules/fork-must-mark-all-present-user-pages-as-cow-not-just-writable-ones.md) | ×1 | oxide-os |
| critical | do | [VmAreaList::insert() must silently accept zero-size VMAs (start >= end)](rules/vmarealist-insert-must-silently-accept-zero-size-vmas-start-end.md) | ×1 | oxide-os |
| critical | do | [Scheduler RunQueue uses flat slot array (not BTreeMap) — O(1) task lookup via PID_TO_SLOT global](rules/scheduler-runqueue-uses-flat-slot-array-not-btreemap-o-1-task-lookup-via-pid-to.md) | ×1 | oxide-os |
| critical | do | [clock_gettime MUST use TSC for sub-tick precision — 100Hz timer gives 10ms granularity which fails sequential-call tests](rules/clock-gettime-must-use-tsc-for-sub-tick-precision-100hz-timer-gives-10ms-granula.md) | ×1 | oxide-os |
| critical | do | [Buddy allocator: mark_free in pagedb BEFORE free_to_zone — never after](rules/buddy-allocator-mark-free-in-pagedb-before-free-to-zone-never-after.md) | ×1 | oxide-os |
| critical | do | [Fork must use parent-runs-first and distribute children across CPUs](rules/fork-must-use-parent-runs-first-and-distribute-children-across-cpus.md) | ×1 | oxide-os |
| critical | do | [remove_task MUST clear on_rq=false before returning Task — work stealing depends on it](rules/remove-task-must-clear-on-rq-false-before-returning-task-work-stealing-depends-o.md) | ×1 | oxide-os |
| critical | do | [Reset ALL DMA-capable devices BEFORE buddy allocator init — OVMF leaves them running](rules/reset-all-dma-capable-devices-before-buddy-allocator-init-ovmf-leaves-them-runni.md) | ×1 | oxide-os |
| critical | do | [Kernel page faults should kill the offending task, not panic the CPU — Linux oops model](rules/kernel-page-faults-should-kill-the-offending-task-not-panic-the-cpu-linux-oops-m.md) | ×1 | oxide-os |
| preference | do | [Test rule: always run make build before testing](rules/test-rule-always-run-make-build-before-testing.md) | ×2 | oxide-os |
| preference | do | [uefi-rs 0.32 Char16 uses Into<u16> not to_u16() — use (*ch).into() for UTF-16 conversion](rules/uefi-rs-0-32-char16-uses-into-u16-not-to-u16-use-ch-into-for-utf-16-conversion.md) | ×1 | oxide-os |
| preference | dont | [mm-core and mm-paging MUST NOT depend on arch_x86_64 for serial output — use os_log](rules/mm-core-and-mm-paging-must-not-depend-on-arch-x86-64-for-serial-output-use-os-lo.md) | ×1 | oxide-os |
| preference | do | [setup-std-source.sh: use python3 for precise multi-line string replacements, never sed for owned.rs/raw.rs](rules/setup-std-source-sh-use-python3-for-precise-multi-line-string-replacements-never.md) | ×1 | oxide-os |

## Recent Sessions

| Date | Agent | Model | Project | Goal | Status |
|------|-------|-------|---------|------|--------|
| 2026-03-14 | claude-code | claude-haiku-4-5-20251001 | oxide-os | [Investigate invalid opcode crash at RIP 0x43c7f0 in userspace binary - check ELF loading, segment mapping, and signal handling](sessions/investigate-invalid-opcode-crash-at-rip-0x43c7f0-in-userspace-binary-check-elf-l.md) | completed |
| 2026-03-13 | claude-code | claude-opus-4-6 | oxide-os | [Fix multi-VT terminal spawning: only 1 VT gets a working shell. Debug and fix the full pipeline: init fork, getty exec, VT switch deadlock.](sessions/fix-multi-vt-terminal-spawning-only-1-vt-gets-a-working-shell-debug-and-fix-the.md) | active |
| 2026-03-13 | claude-code | claude-opus-4-6 | oxide-os | [Fix fork sysretq timer interrupt race — diagnose and fix page fault crash after fork+exec of servicemgr](sessions/fix-fork-sysretq-timer-interrupt-race-diagnose-and-fix-page-fault-crash-after-fo.md) | completed |
| 2026-03-13 | claude-code | claude-opus-4-6 | oxide-os | [Debug kernel page fault at address 0x66 (null pointer + offset) after fork+exec of servicemgr](sessions/debug-kernel-page-fault-at-address-0x66-null-pointer-offset-after-fork-exec-of-s.md) | active |
| 2026-03-08 | claude-code | claude-haiku-4-5-20251001 | oxide-os | [Understand compositor architecture and scrollbar implementation for Win95-style scrollbar widget development](sessions/understand-compositor-architecture-and-scrollbar-implementation-for-win95-style.md) | active |
| 2026-03-08 | copilot | gpt-5.1-codex-max | oxide-os | [Investigate runqueue corruption root cause triggering scheduler hang](sessions/investigate-runqueue-corruption-root-cause-triggering-scheduler-hang.md) | active |
| 2026-03-08 | copilot | gpt-5.1-codex-max | oxide-os | [Attach to running QEMU with GDB to debug hang](sessions/attach-to-running-qemu-with-gdb-to-debug-hang.md) | active |
| 2026-03-08 | claude-code | claude-haiku-4-5-20251001 | oxide-os | [Debug why login prompt never shows on VT after boot - investigate VT initialization, console routing, and getty spawning](sessions/debug-why-login-prompt-never-shows-on-vt-after-boot-investigate-vt-initializatio.md) | completed |
| 2026-03-08 | claude-code | claude-opus-4-6 | oxide-os | [Debug why login prompt never shows on VT - user reports no shell appears after boot](sessions/debug-why-login-prompt-never-shows-on-vt-user-reports-no-shell-appears-after-boo.md) | active |
| 2026-03-08 | claude-code | claude-opus-4-6 | oxide-os | [Debug why login prompt never appears after boot - VT/getty/login pipeline broken](sessions/debug-why-login-prompt-never-appears-after-boot-vt-getty-login-pipeline-broken.md) | active |
| 2026-03-07 | copilot | gpt-5.1-codex-max | oxide-os | [Investigate boot log anomalies (buddy free-list corruption, GOT mapping errors)](sessions/investigate-boot-log-anomalies-buddy-free-list-corruption-got-mapping-errors.md) | active |
| 2026-03-07 | copilot | claude-opus-4.6 | oxide-os | [Create comprehensive architectural layout document of the OXIDE OS kernel](sessions/create-comprehensive-architectural-layout-document-of-the-oxide-os-kernel.md) | active |
| 2026-03-07 | copilot | claude-opus-4.6 | oxide-os | [Analyze what it would take to port OXIDE OS to Raspberry Pi (aarch64)](sessions/analyze-what-it-would-take-to-port-oxide-os-to-raspberry-pi-aarch64.md) | active |
| 2026-03-07 | claude-code | claude-opus-4-6 | oxide-os | [Build system redesign - proper dependency chains, debug/release profiles, no stale binaries](sessions/build-system-redesign-proper-dependency-chains-debug-release-profiles-no-stale-b.md) | active |
| 2026-03-07 | claude-code | claude-opus-4-6 | oxide-os | [Diagnose and fix lock/login failures in esh after recent arch refactoring, syscall alignment, and vterm rebuilds](sessions/diagnose-and-fix-lock-login-failures-in-esh-after-recent-arch-refactoring-syscal.md) | active |
| 2026-03-06 | claude-code | claude-opus-4-6 | oxide-os | [Research VT, terminal, framebuffer, and compositor architecture for understanding how to implement compositing features](sessions/research-vt-terminal-framebuffer-and-compositor-architecture-for-understanding-h.md) | active |
| 2026-03-06 | copilot | claude-opus-4.6 | oxide-os | [Comprehensive performance analysis of OXIDE OS - identify bottlenecks, hot paths, and optimization opportunities](sessions/comprehensive-performance-analysis-of-oxide-os-identify-bottlenecks-hot-paths-an.md) | active |
| 2026-03-06 | claude-code | claude-opus-4-6 | oxide-os | [Explore package manager and plan building vim and python for OXIDE OS](sessions/explore-package-manager-and-plan-building-vim-and-python-for-oxide-os.md) | active |
| 2026-03-05 | claude-code | claude-opus-4-6 | oxide-os | [Fix root cause of BUDDY-DUP-ALLOC: coalescing merges with allocated buddies](sessions/fix-root-cause-of-buddy-dup-alloc-coalescing-merges-with-allocated-buddies.md) | active |
| 2026-03-05 | claude-code | claude-opus-4-6 | oxide-os | [Fix root cause of hundreds of BUDDY-DUP-ALLOC stale free-list entries causing OOM crashes](sessions/fix-root-cause-of-hundreds-of-buddy-dup-alloc-stale-free-list-entries-causing-oo.md) | active |

## Projects

- [oxide-os](projects/oxide-os.md) — Custom x86_64 OS written in Rust with CFS scheduler, VFS, SMP, signals, COW fork
