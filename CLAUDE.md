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
- Docs: `docs/DRIVES.md` (boot/filesystem flow), `docs/DEBUGGING.md` (debug features - always enabled).
- Toolchain: `toolchain/README.md`, `toolchain/QUICKSTART.md`, `toolchain/SUMMARY.md`, `toolchain/INTEGRATION.md`.
- Components: `userspace/coreutils/TEST_PLAN.md`, `userspace/coreutils/UTILITIES.md`, `userspace/shell/BUILTINS.md`, `apps/gwbasic/README.md`.
- Personas: `docs/agents/personas.md` (full persona definitions, emotional states, tone rules for code comments).
- Agent rules: `docs/agents/syscall-register-clobber.md` (userspace asm clobbers), `docs/agents/syscall-register-restore.md` (kernel syscall exit register restore), `docs/agents/irq-handler-drain.md` (level-triggered IRQ handlers must drain data ports), `docs/agents/smp-timer-safety.md` (SMP timer ISR: atomic ticks, BSP-only terminal/dump), `docs/agents/syscall-return-resched.md` (must check need_resched before sysretq), `docs/agents/isr-lock-safety.md` (ISR must use try_lock/try_with_rq — deadlock prevention), `docs/agents/serial-saturation-safety.md` (serial writes must have bounded spin — never unbounded loop on UART THRE), `docs/agents/vt-poll-drain.md` (poll_read_ready MUST drain VT ring buffer before checking line discipline), `docs/agents/stdout-flush-requirement.md` (terminal control sequences require fflush_stdout() — buffering breaks interactivity), `docs/agents/vte-parser-reset.md` (always reset VTE parser state on timeout/error before returning -1 from wgetch()), `docs/agents/net-control-syscall.md` (NET_CONTROL syscall 310 for userspace DHCP triggering).
- Code roots: `kernel/` (entry + all subsystems), `bootloader/`, `userspace/`, `tools/`, `scripts/`, `external/`.

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
