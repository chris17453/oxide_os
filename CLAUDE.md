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
- Repo guides: `AGENTS.md` (repo rules), `THIS.md` (current plan/phase), `FIXME.md` (gaps), `manifesto.md`.
- Docs: `docs/DRIVES.md` (boot/filesystem flow), `docs/DEBUGGING.md` (debug features - always enabled), `docs/AUTONOMOUS-DEBUGGING.md` (GDB automation for autonomous debugging - crash capture, boot checks, programmatic control).
- Toolchain: `toolchain/README.md`, `toolchain/QUICKSTART.md`, `toolchain/SUMMARY.md`, `toolchain/INTEGRATION.md`.
- Components: `userspace/coreutils/TEST_PLAN.md`, `userspace/coreutils/UTILITIES.md`, `userspace/shell/BUILTINS.md`, `apps/gwbasic/README.md`.
- Personas: `docs/agents/personas.md` (full persona definitions, emotional states, tone rules for code comments).
- Agent rules: `docs/agents/syscall-register-clobber.md` (userspace asm clobbers), `docs/agents/syscall-register-restore.md` (kernel syscall exit register restore), `docs/agents/irq-handler-drain.md` (level-triggered IRQ handlers must drain data ports), `docs/agents/smp-timer-safety.md` (SMP timer ISR: atomic ticks, BSP-only terminal/dump), `docs/agents/syscall-return-resched.md` (must check need_resched before sysretq), `docs/agents/isr-lock-safety.md` (ISR must use try_lock/try_with_rq — deadlock prevention), `docs/agents/serial-saturation-safety.md` (serial writes must have bounded spin — never unbounded loop on UART THRE), `docs/agents/vt-poll-drain.md` (poll_read_ready MUST drain VT ring buffer before checking line discipline), `docs/agents/stdout-flush-requirement.md` (terminal control sequences require fflush_stdout() — buffering breaks interactivity), `docs/agents/vte-parser-reset.md` (always reset VTE parser state on timeout/error before returning -1 from wgetch()), `docs/agents/net-control-syscall.md` (NET_CONTROL syscall 310 for userspace DHCP triggering), `docs/agents/uart-bounded-spin.md` (UART TX loops MUST have iteration limit — drop byte rather than hang system), `docs/agents/terminal-dirty-marking.md` (mark only affected rows dirty — cursor/SGR don't need mark_all_dirty), `docs/agents/apic-timer-calibration.md` (PIT calibration must reset state — OUT may be HIGH from BIOS), `docs/agents/dns-resolution-rules.md` (DNS resolution order: cache → /etc/hosts → DNS servers; hostctl must protect localhost entries), `docs/agents/terminal-scrollback-selection.md` (scrollback compositing + selection reverse-video rendering rules), `docs/agents/blocking-wait-cpu-accounting.md` (HLT-looping tasks must NOT be charged CPU time — use kernel_preempt_ok flag), `docs/agents/scheduler-meta-fast-path.md` (scheduler queries MUST try this_cpu() first — never loop all CPUs for current task metadata), `docs/agents/stdout-serial-separation.md` (stdout → terminal ONLY — never write user I/O to serial port), `docs/agents/synchronous-render-on-write.md` (terminal::write() MUST render to framebuffer before releasing lock — the Linux way), `docs/agents/performance-monitoring.md` (comprehensive perf counters for ISRs, scheduler, serial health — Linux perf_events style), `docs/agents/qemu-cpu-smap-requirement.md` (QEMU MUST use -cpu qemu64,+smap,+smep — STAC instruction requires SMAP support or crashes with invalid opcode), `docs/agents/buddy-allocator-doubly-linked.md` (buddy allocator MUST use doubly-linked free lists with magic canary — O(1) removal prevents infinite loops and detects corruption), `docs/agents/memory-canary-strategy.md` (ALL memory structures need magic canaries for corruption detection — comprehensive strategy for buddy, slab, heap, process structures), `docs/agents/kbd-architecture.md` (keyboard drivers MUST use shared input::kbd module for console conversion — no per-driver duplication of modifier tracking, Ctrl codes, or ANSI sequences), `docs/agents/dma-physical-frame-allocator.md` (DMA buffers MUST use mm().alloc_contiguous() — kernel heap addresses give bogus ~128TB physical addresses via naive virt_to_phys), `docs/agents/unconditional-serial-trace-danger.md` (NEVER add unconditional serial traces to hot paths — debug-paging/debug-buddy/debug-perf excluded from debug-all because they saturate 115200 baud), `docs/agents/arch-serial-abstraction.md` (serial output MUST use arch::serial_writer() — NEVER inline asm or raw port I/O outside arch package), `docs/agents/uefi-gop-framebuffer.md` (UEFI GOP framebuffer is the display — VirtIO-GPU SET_SCANOUT doesn't take effect in QEMU/OVMF, never replace the GOP fb), `docs/agents/cfs-min-vruntime-rule.md` (CFS update_min_vruntime must exclude running task — use only tree minimum, never min(tree, curr) or tasks get permanently starved), `docs/agents/uefi-gop-virtio-gpu-conflict.md` (NEVER send VirtIO-GPU SET_SCANOUT if GOP framebuffer exists — steals display at low RAM where OVMF uses VirtIO-GPU for GOP).
- Code roots: `kernel/` (entry + all subsystems), `bootloader/`, `userspace/`, `tools/`, `scripts/`, `external/`.
- Curses: `docs/agents/curses-init-pattern.md` (ncurses apps MUST call cbreak()+noecho() after initscr() — canonical mode prevents getch from detecting keys).

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

## DIRECTION
- When you discover new rules about theoperating system, please create a docs/agents/instruction.md text file containint the exact rules you've found and how touse them then update the Claude.md file with an intex pointer to that file...
- this is how we grow our knowledge
