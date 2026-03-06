<!-- OXIDE Claude instructions: concise, retrieval-first -->
# Claude Agent Playbook

**Mission:** Ship correct, production-quality OXIDE OS changes. Follow every guardrail; user requests apply unless they conflict with these rules.

## Core Workflow (in order)
1. **Orient:** Read `AGENTS.md` and this file. List repo root to map key dirs.
2. **Active phase:** If present, open latest `docs/plan/PHASE_*.md` or `THIS.md`/roadmaps to know goals and exit criteria.
3. **Explore then retrieve:** Skim relevant code and local docs before editing. Prefer retrieval over model memory.
4. **Plan:** Update the Todo tool; keep tasks small; one commit per feature.
5. **Implement surgically:** Minimal diffs; preserve behavior unless intentionally changing it.
6. **Validate:** Run existing tests/linters that cover the change; capture failing logs if any.
7. **Document:** Update related docs when behavior or process changes.
8. **Build:** Always update the make run for the curent build if something canges, we will never use any other command than make build.
9. **Code**: Always use cyberpunk comments, have different persona's for different comments. 

## PERSONAS FOR DEVELOPMENT
- **Full reference:** `docs/agents/personas.md` — emotional states, tone tables, taglines
- Pick the right persona for the subsystem. Sign every comment with `— <Name>:`
- Comments must be **snarky, sarcastic, gritty** — like engineers who've debugged one too many triple-faults at 3 AM
- Match the persona's emotional state to context (critical bug = haunted, routine fix = resigned, clever hack = vindicated)

### Quick Lookup
| Domain | Personas |
|--------|----------|
| Kernel/Core | GraveShift, BlackLatch, SableWire, TorqueJax, WireSaint, ShadePacket, NeonRoot |
| Toolchain | Hexline, PulseForge |
| Security | ColdCipher, EmberLock, ZeroTrace, GhostPatch, VeilAudit |
| Test/QA | CrashBloom, FuzzStatic, StaticRiot, DeadLoop, CanaryHex |
| Runtime | IronGhost, ThreadRogue, ByteRiot |
| UI/Graphics | NeonVale, GlassSignal, EchoFrame, InputShade, SoftGlyph |
| Ops/Ecosystem | PatchBay, OverTheAir, StackTrace, NightDoc, RustViper |


## Retrieval Index (check these first)
- **Active plan:** `docs/plan/kernel-stubs-remediation.md` (P0-P3 stub/TODO remediation — 16 kernel subsystem stubs, 44 arch port stubs).
- **Completed plan:** `docs/plan/perf-security-audit-plan.md` (P0-P3 remediation plan from March 2026 audit — ~100 findings across scheduler, syscalls, VFS, memory, drivers, boot/SMP — ALL COMPLETE).
- Repo guides: `AGENTS.md` (repo rules), `THIS.md` (current plan/phase), `FIXME.md` (gaps), `manifesto.md`.
- Docs: `docs/DRIVES.md` (boot/filesystem flow), `docs/DEBUGGING.md` (debug features - always enabled), `docs/AUTONOMOUS-DEBUGGING.md` (GDB automation for autonomous debugging - crash capture, boot checks, programmatic control).
- Toolchain: `toolchain/README.md`, `toolchain/QUICKSTART.md`, `toolchain/SUMMARY.md`, `toolchain/INTEGRATION.md`.
- Components: `userspace/coreutils/TEST_PLAN.md`, `userspace/coreutils/UTILITIES.md`, `userspace/shell/BUILTINS.md`, `apps/gwbasic/README.md`.
- Personas: `docs/agents/personas.md` (full persona definitions, emotional states, tone rules for code comments).
- Agent rules: `docs/agents/syscall-register-clobber.md` (userspace asm clobbers), `docs/agents/syscall-register-restore.md` (kernel syscall exit register restore), `docs/agents/irq-handler-drain.md` (level-triggered IRQ handlers must drain data ports), `docs/agents/smp-timer-safety.md` (SMP timer ISR: atomic ticks, BSP-only terminal/dump), `docs/agents/syscall-return-resched.md` (must check need_resched before sysretq), `docs/agents/isr-lock-safety.md` (ISR must use try_lock/try_with_rq — deadlock prevention), `docs/agents/serial-saturation-safety.md` (serial writes must have bounded spin — never unbounded loop on UART THRE), `docs/agents/vt-poll-drain.md` (poll_read_ready MUST drain VT ring buffer before checking line discipline), `docs/agents/stdout-flush-requirement.md` (terminal control sequences require fflush_stdout() — buffering breaks interactivity), `docs/agents/vte-parser-reset.md` (always reset VTE parser state on timeout/error before returning -1 from wgetch()), `docs/agents/net-control-syscall.md` (NET_CONTROL syscall 310 for userspace DHCP triggering), `docs/agents/uart-bounded-spin.md` (UART TX loops MUST have iteration limit — drop byte rather than hang system), `docs/agents/terminal-dirty-marking.md` (mark only affected rows dirty — cursor/SGR don't need mark_all_dirty), `docs/agents/apic-timer-calibration.md` (PIT calibration must reset state — OUT may be HIGH from BIOS), `docs/agents/dns-resolution-rules.md` (DNS resolution order: cache → /etc/hosts → DNS servers; hostctl must protect localhost entries), `docs/agents/terminal-scrollback-selection.md` (scrollback compositing + selection reverse-video rendering rules), `docs/agents/blocking-wait-cpu-accounting.md` (HLT-looping tasks must NOT be charged CPU time — use kernel_preempt_ok flag), `docs/agents/scheduler-meta-fast-path.md` (scheduler queries MUST try this_cpu() first — never loop all CPUs for current task metadata), `docs/agents/stdout-serial-separation.md` (stdout → terminal ONLY — never write user I/O to serial port), `docs/agents/synchronous-render-on-write.md` (terminal::write() MUST render to framebuffer before releasing lock — the Linux way), `docs/agents/performance-monitoring.md` (comprehensive perf counters for ISRs, scheduler, serial health — Linux perf_events style), `docs/agents/qemu-cpu-smap-requirement.md` (QEMU MUST use -cpu qemu64,+smap,+smep — STAC instruction requires SMAP support or crashes with invalid opcode), `docs/agents/buddy-allocator-doubly-linked.md` (buddy allocator MUST use doubly-linked free lists with magic canary — O(1) removal prevents infinite loops and detects corruption), `docs/agents/memory-canary-strategy.md` (ALL memory structures need magic canaries for corruption detection — comprehensive strategy for buddy, slab, heap, process structures), `docs/agents/kbd-architecture.md` (keyboard drivers MUST use shared input::kbd module for console conversion — no per-driver duplication of modifier tracking, Ctrl codes, or ANSI sequences), `docs/agents/dma-physical-frame-allocator.md` (DMA buffers MUST use mm().alloc_contiguous() — kernel heap addresses give bogus ~128TB physical addresses via naive virt_to_phys), `docs/agents/unconditional-serial-trace-danger.md` (NEVER add unconditional serial traces to hot paths — debug-paging/debug-buddy/debug-perf excluded from debug-all because they saturate 115200 baud), `docs/agents/arch-serial-abstraction.md` (serial output MUST use arch::serial_writer() — NEVER inline asm or raw port I/O outside arch package), `docs/agents/uefi-gop-framebuffer.md` (UEFI GOP framebuffer is the display — VirtIO-GPU SET_SCANOUT doesn't take effect in QEMU/OVMF, never replace the GOP fb), `docs/agents/cfs-min-vruntime-rule.md` (CFS update_min_vruntime must exclude running task — use only tree minimum, never min(tree, curr) or tasks get permanently starved), `docs/agents/uefi-gop-virtio-gpu-conflict.md` (NEVER send VirtIO-GPU SET_SCANOUT if GOP framebuffer exists — steals display at low RAM where OVMF uses VirtIO-GPU for GOP), `docs/agents/exec-signal-reset.md` (exec MUST reset caught signal handlers to SIG_DFL — old handler addresses point into dead address space), `docs/agents/cfs-starvation-fixes.md` (three-layer CFS starvation defense: TICK_NS vruntime floor, always-charge-vruntime in scheduler_tick, kpo grace period for long kernel ops), `docs/agents/signal-delivery-rules.md` (signal delivery path, ISR try_lock requirements, exec handler reset, signal frame layout), `docs/agents/select-pselect-hlt-requirement.md` (select/pselect6 MUST use HLT+kpo, never spin_loop — burns 100% CPU in ring 0), `docs/agents/servicemgr-sleep-pattern.md` (daemon loops MUST use poll/nanosleep for sleep — never sched_yield loops as sleep fallback), `docs/agents/procfs-try-lock-rule.md` (procfs generate_content MUST use try_lock for ProcessMeta + sys_getdents MUST enable kpo during readdir loops), `docs/agents/flock-advisory-locks.md` (BSD flock advisory file locking — per-Arc<File> owner_id, FlockRegistry, blocking HLT+kpo, auto-release on Drop), `docs/agents/procfs-nonblocking-scheduler-queries.md` (procfs MUST use try_get_task_meta/try_get_task_state/try_get_task_ppid — never blocking with_rq from diagnostic contexts), `docs/agents/terminal-lock-preemption.md` (terminal::write MUST disable preemption while holding TERMINAL lock — ISR callers MUST use try_lock, VT switch uses try_write on ACTIVE_VT), `docs/agents/isr-no-heap-allocation.md` (ISR-reachable code MUST NEVER allocate heap memory — scheduler run queues use fixed-capacity arrays, not BinaryHeap/VecDeque), `docs/agents/task-context-before-enqueue.md` (task context MUST be set before add_task — never enqueue with uninitialized context; layered defense: set-before-enqueue + safe Default + is_schedulable() validation), `docs/agents/smap-kernel-buffer-rule.md` (sys_read/sys_write MUST use kernel-stack buffer — never pass user-space pointers through VFS stack; nested STAC/CLAC in subsystems clobbers AC flag).
- Code roots: `kernel/` (entry + all subsystems), `bootloader/`, `userspace/`, `tools/`, `scripts/`, `external/`.
- Curses: `docs/agents/curses-init-pattern.md`
- Kernel preemption in syscalls: `docs/agents/write-syscall-kernel-preempt.md` (sys_write MUST enable kernel preemption — spinning on TERMINAL.lock() without it = permanent deadlock) (ncurses apps MUST call cbreak()+noecho() after initscr() — canonical mode prevents getch from detecting keys).
- Signal delivery: `docs/agents/signal-delivery-blocking-reads.md` (every kernel blocking loop MUST check for pending actionable signals and return -EINTR — without this, Ctrl+C is silently swallowed).
- SMP syscall safety: `docs/agents/percpu-syscall-data.md` (SYSCALL_USER_CONTEXT and CPU_DATA MUST be per-CPU arrays — single globals cause SMP register clobbering; every AP must call syscall::init() + init_kernel_stack()).
- Preemption model: `docs/agents/preempt-count-model.md` (Linux-style per-CPU preempt_count counter replaces kpo boolean — KernelMutex auto-disables preemption on lock, scheduler checks preemptable() in timer ISR).
- Fork COW: `docs/agents/fork-cow-all-pages.md` (fork MUST mark ALL present user pages as COW — not just writable ones; without VMAs, can't distinguish RO code from temporarily-RO data segments).

## Guardrails (must follow)
- **NEVER REVERT FILES OR COMMITS.** Never run `git checkout` on files, `git reset`, `git revert`, or any command that undoes work. Never restore files to a previous state. If debugging, comment out code or add feature flags — do NOT undo changes. This is an ABSOLUTE rule with ZERO exceptions.
- **No stubs or TODO fallbacks.** Implement fully or state concrete blockers/needs.
- **Debug policy:** Never delete debug output; gate via `debug-*` features (`kernel/src/debug.rs`, `kernel/Cargo.toml`). No raw serial writes—use `debug_*!` macros; `debug_sched_unsafe!` only in ISR contexts.
- **Feature completeness:** Don't drop or simplify requested features because they're hard. If blocked, be explicit.
- **Structured code:** Prefer structs/enums/bitflags; document magic constants with source.
- **Architecture scope:** Target x86_64; design portable, implement x86_64 now.
- **Quality bar:** Production-ready code; consider edge cases, security, performance.
- **Workflow hygiene:** Small commits per feature (imperative subject); keep worktree clean; never add AI attribution; don't revert user changes.

## Default Behaviors
- **Retrieval-led reasoning:** Read local docs/specs and code before coding.
- **Minimal change set:** Touch the fewest lines needed; keep working behavior unless deliberately changing it.
- **Testing:** Run `make build`/`make build-full`/`make test` or targeted `cargo test -p <crate>` when relevant. Share outputs for failures; don’t add new tools.
- **Unsafe code:** Document why it is safe; keep `unsafe` blocks narrow.
- **Comments:** Add only when code is not self-explanatory.

## If Blocked
- Stop and ask with specific blockers and required info. Don’t proceed with guesses or scope cuts.

## Quick Commands
- Build: `make build`, `make build-full`, `make userspace[-pkg PKG=...]`.
- Run: `make run` (debug-all enabled by default in Makefile).
- Tests: `make test`; targeted `cargo test -p <crate>`.
- Debug: See `docs/DEBUGGING.md` (debug-all is default, just run `make run`).

## Change Checklist (before finishing)
- [ ] Read relevant phase/spec/plan docs
- [ ] Minimal diff; no removed debug hooks or stubbed logic
- [ ] Tests/linters run as appropriate; failures noted with logs
- [ ] Docs updated if behavior/process changed
- [ ] Commit message ready (imperative, one feature)

## Knowledge Management — Memgram MCP Server
- **Primary memory store:** Use the `mcp__memory__*` memgram tools for ALL persistent knowledge across sessions. This is the source of truth for what works, what doesn't, and why.

### Session Lifecycle (MANDATORY)
- **On every session start:** Call `mcp__memory__start_session` with project `oxide-os`, then `mcp__memory__get_resume_context` to reload prior state (last session, pinned thoughts, active rules, project summary, last snapshot).
- **On every session end:** Call `mcp__memory__end_session` with a summary, outcome, decisions made, files modified, and any unresolved items.

### What to Store and When
- **Positive rules (DO this):** When something works — a pattern, approach, workaround — immediately call `mcp__memory__add_rule` with `type: "do"`. Example: "CFS must exclude running task from min_vruntime".
- **Negative rules (DON'T do this):** When something breaks or causes bugs — immediately call `mcp__memory__add_rule` with `type: "dont"`. Example: "NEVER allocate heap memory in ISR context".
- **Severity:** Use `critical` for invariants that cause crashes/deadlocks/data loss, `preference` for soft guidelines, `context_dependent` for situational rules.
- **Bug patterns:** When fixing a bug, call `mcp__memory__add_error_pattern` with the error description, root cause, and fix. Link it to a prevention rule if you create one.
- **Observations & decisions:** Use `mcp__memory__add_thought` for architecture observations, design decisions, ideas, or general notes worth remembering.
- **Reinforce rules:** When you encounter another case that confirms an existing rule, call `mcp__memory__reinforce_rule` to bump its confidence.
- **Search before creating:** Always `mcp__memory__search` first to avoid duplicates — use `mcp__memory__update_thought` or reinforcement instead of creating new entries.

### When Discovering New Rules (full flow)
1. Call `mcp__memory__search` to check if it already exists.
2. Call `mcp__memory__add_rule` (positive or negative) with severity, keywords, and project.
3. Create a `docs/agents/<rule-name>.md` file with the detailed explanation.
4. Update this file's Retrieval Index with a pointer to the new doc.
5. Call `mcp__memory__link_items` to connect related rules/errors/thoughts.

### Compaction-Phase Memory Protocol
When context compression (compaction) occurs, you MUST:
1. **Save snapshot FIRST:** Call `mcp__memory__save_snapshot` with current_goal, progress_summary, blockers, next_steps, active_files, and key_decisions — this preserves your exact state.
2. **Dump unsaved knowledge:** Call `mcp__memory__add_rule` / `mcp__memory__add_thought` / `mcp__memory__add_error_pattern` for any new insights discovered in this segment that haven't been persisted yet. Both positive ("this works") and negative ("this breaks everything") rules.
3. **After compaction resumes:** Call `mcp__memory__get_resume_context` to reload state. Verify the snapshot matches where you left off.
4. **Cross-validate:** When inserting a new rule, search for related items and check for contradictions. Archive stale items with `mcp__memory__archive_item`.

### Quick Reference — Memgram Tools
| Action | Tool |
|--------|------|
| Start session | `start_session(agent_type, model, project, goal)` |
| End session | `end_session(session_id, summary, outcome, ...)` |
| Save before compaction | `save_snapshot(session_id, current_goal, progress, ...)` |
| Resume after compaction | `get_resume_context(project)` |
| Store a positive rule | `add_rule(summary, type:"do", severity, content, project, keywords)` |
| Store a negative rule | `add_rule(summary, type:"dont", severity, content, project, keywords)` |
| Log a bug pattern | `add_error_pattern(error_description, cause, fix, project, keywords)` |
| Store observation/note | `add_thought(summary, type, content, project, keywords)` |
| Search everything | `search(query, type_filter?, project?, limit?)` |
| Get active rules | `get_rules(project, severity?, keywords?)` |
| Reinforce a rule | `reinforce_rule(rule_id, note)` |
| Link items | `link_items(from_id, from_type, to_id, to_type, link_type)` |
| Pin important item | `pin_item(item_id, pinned:true)` |
| Archive stale item | `archive_item(item_id)` |
| Update project summary | `update_project_summary(project, summary, tech_stack, ...)` |
