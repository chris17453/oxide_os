# Oxide OS: The Story So Far

## Snapshot
Oxide OS (briefly called EFFLUX before the name was swapped in a 15-minute decision) is a work in progress: a mostly working kernel that is not yet performant, but largely Linux-compatible at the syscall/libc surface while keeping its own ABI signatures. It’s written entirely in Rust with architecture-specific crates: x86_64 is the active target, and an in-progress arm64 crate will mature after the x86_64 path solidifies. Commits cluster around **Chris Watkins** (including the early “Ablative Personality” address), plus AI copilots **copilot-swe-agent**, **Claude**, and **Codex**. There are no git tags yet; milestones are encoded in the “Phase N” commits and the later refactor waves.

## Why this exists
- Prompt: “Can I build an OS?” Answer: yes. Next: “How far can I push it?” The goal is personal ownership—an OS that’s 100% yours to shape and to experiment with at a depth most only dream about.

## How it was built
- Nights-and-weekends, vibe-coded with thousands of builds; a window was always open, often overnight.
- Heavy prompting and orchestration: you set intent, guidelines, and workflow, while AI copilots (Copilot, Claude, Codex) and automation did the typing. Branches and bad AI ideas were routinely discarded; you don’t claim to have handwritten the code—you directed the work.

## Coding with AI
- You still pride yourself on being a strong coder, but the workflow shifted: higher-level guidance replaces hand-typing, shipping faster with intention over keystrokes.
- Guardrails: code sweeps, security checks, unit tests, and code-smell reviews keep safety in a ~200k-line Rust codebase.
- Beliefs are codified into instructions so validation can be automated; you review outcomes at an abstracted layer while the tools execute.

## Perspective
- This snapshot will age fast; the OS changes daily and isn’t meant for others to use—it’s a demonstration of what’s possible.
- If this is doable in spare hours, imagine what a focused, organized team could build with the same tools.
- Already an entire kernel and working OS with 100+ built-in tools, even if it never ships to users.

## Status & architecture
- Kernel: Rust-only, Linux syscall numbers honored with Oxide-specific ABI signatures; mostly working but not yet performance-tuned.
- Toolchain: custom targets enabling C builds against the provided libc bindings.
- Architectures: x86_64 primary; arm64 crate in progress and queued behind x86_64 stabilization.
- Supply chain: can cross-compile Fedora SRPMs into Oxide binaries to quickly seed tools/apps, while preferring to author first-party replacements over time.

## Origin (early January 2026)
- 2026-01-03: Ablative Personality lays down the EFFLUX project skeleton, build tooling, and completes Phase 0–1 with memory management crates and integration.
- Mid-January: Rapid phase completions: preemptive multitasking (Phase 2), user address spaces and ELF loading (Phase 3), process model (Phase 4), and a Phase 5 VFS with initramfs + procfs. The groundwork brings kernel↔user transitions, syscalls, and filesystem basics online.

## Handoff and sprint (mid-January 2026)
- 2026-01-18: Chris Watkins takes over and lands Phase 6–10: TTY/PTY, signals, libc + userland, SMP, and loadable kernel modules. Phase 11 storage follows, marking the completion of the planned subsystem ladder.
- The phase ladder establishes the north star: each commit closes a capability gap (I/O, scheduling, memory safety, SMP, modules) while keeping the boot path stable.

## Hardening and drivers (February 2026)
- Driver-core refactor: virtio drivers converted to a common `PciDriver` trait, runtime driver registration via linker sections, and a shared keyboard module (Feb 8). A virtio-core crate and module symbol exports enable dynamic driver loading.
- Stability/perf pushes: PML4 canary fixes, OOM killer, SMP work stealing, VMA subsystem, KernelMutex preemption model, exec/fork hardening, and integration test suites (Feb 4). Serial output is removed in favor of console/stderr, reducing debug-induced stalls.
- User experience and reliability: scheduler fixes (“OK NO SCHEDULER BROKENESS”), alt-screen/display fixes, SMAP ioctl GPF fix, virtio-GPU scanout safety, and autologin for getty.

## Platform & tooling uplift (late Feb – early Mar 2026)
- Package/build pipeline overhaul replaces external builds with `oxdnf` (Mar 6), and a new std target lands (Mar 1) to unify userspace/toolchain expectations.
- Architecture abstraction finalization and feature-driven arch selection close portability gaps (Mar 6).

## Performance & UX polish (early March 2026)
- Performance autopsy doc enumerates bottlenecks with a remediation plan; TSC nanosecond clock and syscall renumbering align with Linux ABI; kernel stubs are remediated.
- Terminal/graphics: per-VT terminal emulators with a compositor, tiling VT plan, Win95-style scrollbars, and row-batched MMIO writes to keep UI smooth. On-screen virtual keyboard (Alt+K) and 24-unit test suite validate input paths.
- Misc polish: profiling hooks for the terminal write pipeline, GWBasic auto-flush for progressive graphics, and screenshots documenting the login experience.

## People
- **Chris Watkins** founder and lead across all phases, from Phase 0–11 through drivers, perf hardening, and UX polish.

## AI Agents
- **Claude**  - the workhorse and hight 95% creater of all code
- **Codex** - for when al the others get stuck
- **CoPilot** - the thinker in difficult situations, and master of gap analysis

## Themes to highlight in an article
1) **Phase ladder delivery**: Clear capability milestones (0–11) gave structure and allowed aggressive vertical integration.
2) **Driver architecture reset**: Moving to shared `virtio-core` + `PciDriver` trait unlocked modularity and hotplug handling.
3) **Safety and preemption model**: Canary checks, VMA subsystem, preemption model, and OOM handling turned a demo kernel into a resilient system.
4) **Performance + UX convergence**: Compositor/terminal pipeline, profiling, and UI niceties arrived alongside perf fixes—usability mattered.
5) **Tooling and supply chain**: `oxdnf` package pipeline and new std target reduced friction and made builds reproducible.

## What’s next 
- Make it more performantive
- Stabalize the feature list
- install it on a Physical Machine
- Capture screenshots/demos for the compositor, on-screen keyboard, and perf improvements to show the human impact of the engineering work.
