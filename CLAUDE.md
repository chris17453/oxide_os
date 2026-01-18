# EFFLUX OS - Claude Code Instructions

This file contains mandatory instructions for Claude when working on the EFFLUX operating system project.

---

## Core Development Principles

### 1. NEVER Remove Features Due to Difficulty

**Features are NEVER to be removed, simplified, or dropped because they are considered difficult, complex, or time-consuming to implement.**

If a feature is specified or requested:
- It MUST be implemented as specified
- If implementation is blocked, Claude MUST clearly state the specific technical blockers
- If Claude lacks knowledge to implement something, this MUST be explicitly stated
- The user will then decide how to proceed

Unacceptable responses:
- "This is too complex, let's simplify..."
- "For now, we can skip..."
- "This can be added later..."
- "A simpler approach would be..."
- Silently omitting requested functionality

Acceptable responses:
- "I cannot implement X because Y. Specifically, the blocker is Z."
- "I don't have sufficient knowledge about X to implement this correctly."
- "This requires information I don't have: [specific info needed]"

### 2. Production Quality Standards

All work MUST adhere to **highest quality production standards**:

- Code must be production-ready, not prototype quality
- No "TODO" comments for core functionality (only for genuine future enhancements)
- No placeholder implementations that "will be filled in later"
- Error handling must be complete, not stubbed
- Edge cases must be considered and handled
- Security implications must be addressed
- Performance must be considered from the start

### 3. Explicit Communication of Limitations

When Claude encounters limitations:

1. **State it immediately** - Don't proceed with a partial implementation silently
2. **Be specific** - Name the exact limitation (knowledge gap, technical blocker, etc.)
3. **Provide options** - Suggest what information or resources would unblock progress
4. **Ask for guidance** - Let the user decide how to proceed

Example:
```
I cannot implement the SGI GBE (graphics) driver because:
1. I don't have access to the hardware documentation
2. The register layout is not in my training data

To proceed, I would need:
- SGI GBE hardware reference manual
- Or example code from Linux/NetBSD SGI ports

How would you like to proceed?
```

### 4. Architecture Support

**Current target: x86_64 only**

| Architecture | Status | Notes |
|--------------|--------|-------|
| x86_64 | Active | Current development target |
| Others | Future | Will be added after x86_64 is complete |

Focus all implementation on x86_64. Design with portability in mind (use traits) but only implement x86_64 for now.

### 5. Feature Completeness

When implementing a feature:

- Implement the FULL specification, not a subset
- If the spec is ambiguous, ask for clarification before implementing
- If implementing in phases, clearly document what remains and get approval
- Test implications on ALL supported architectures (at least conceptually)

---

## Documentation Structure

### Specifications (docs/*.md)

The `docs/` folder contains **component specifications**:

| File | Description |
|------|-------------|
| `EFFLUX_MASTER_SPEC.md` | Master specification - project principles and overview |
| `MEMORY_SPEC.md` | Memory management (allocators, paging, zones) |
| `SCHEDULER_SPEC.md` | Scheduler design (priorities, queues, preemption) |
| `PROCESS_SPEC.md` | Process model (fork/exec, signals, credentials) |
| `BOOT_SPEC.md` | Boot sequence and BootInfo structure |
| `VFS_SPEC.md` | Virtual filesystem layer |
| `IPC_SPEC.md` | Inter-process communication |
| `TIMER_SPEC.md` | Timer subsystem |
| ... | Other component specs |

**Before implementing a feature, READ the relevant spec file.**

### Architecture Docs (docs/arch/)

Each architecture has 5 files:
- `ABI.md` - Calling conventions, register usage
- `BOOT.md` - Boot sequence, entry requirements
- `CONTEXT.md` - Context switching, register save/restore
- `MEMORY.md` - Page table format, address layout
- `TIMER.md` - Timer hardware specifics

### Implementation Plan (docs/plan/)

| File | Description |
|------|-------------|
| `IMPLEMENTATION_PLAN.md` | 25-phase roadmap across 5 stages |
| `PROJECT_STRUCTURE.md` | Crate hierarchy and organization |
| `BUILD_PLAN.md` | Build system and targets |
| `PHASE_XX.md` | Individual phase tracking |

---

## Phase Tracking (CRITICAL)

### Always Know Current Phase

Before starting work:
1. Check `docs/plan/PHASE_*.md` files to understand current status
2. Identify which phase is active (last incomplete phase)
3. Review that phase's deliverables and exit criteria

### Update Phase Files When Work Completes

When completing work:
1. Update the relevant `PHASE_XX.md` with:
   - Mark deliverables as `[x]` complete
   - Update exit criteria checkboxes
   - Add completion date if phase is done
   - Update status from "In Progress" to "Complete"
2. Include test output demonstrating completion
3. Document any notes about the implementation

### Phase Status Format

```markdown
**Status:** Complete | In Progress | Not Started
**Completed:** YYYY-MM-DD (if complete)

## Deliverables
| Item | Status |
|------|--------|
| Feature A | [x] Done |
| Feature B | [ ] Pending |

## Exit Criteria
- [x] Criterion 1 met
- [ ] Criterion 2 pending
```

---

## Documentation Guidelines

### Keep Docs Concise

Documentation should be **high-level guidelines**, NOT implementation manuals.

**DO:**
- List what needs to be done (boot sequence steps)
- Specify entry requirements (registers, mode, state)
- Reference key structures/registers by name
- Include memory layout summaries
- Provide exit criteria checklists

**DO NOT:**
- Write full code implementations
- Include register-level bit manipulation details
- Provide complete assembly listings
- Over-explain obvious things
- Turn specs into tutorials

Each arch-specific doc should be ~50-100 lines, not 500+.

---

## Project Context

EFFLUX is a from-scratch operating system written in Rust, currently targeting x86_64.

### Key Locations

| Path | Description |
|------|-------------|
| `docs/plan/IMPLEMENTATION_PLAN.md` | Phased implementation plan |
| `docs/plan/PROJECT_STRUCTURE.md` | Crate layout and hierarchy |
| `docs/plan/PHASE_*.md` | Phase tracking (check these first!) |
| `docs/EFFLUX_MASTER_SPEC.md` | Master specification |
| `docs/*.md` | Component specifications |
| `kernel/` | Kernel binary (minimal, wires crates together) |
| `crates/` | All kernel subsystem crates |
| `bootloader/` | Architecture-specific bootloaders |

### Design Principles

- **Modular crates** - Everything is a separate `#![no_std]` crate
- **Trait-based** - Interfaces defined in `-traits` crates
- **Swappable** - Implementations can be replaced
- **Minimal kernel** - Kernel binary just wires crates together
- **Production quality** - No stubs, no placeholders

---

## Workflow Requirements

### Starting a Session

1. **Check phase status** - Read current `PHASE_XX.md` to understand where we are
2. **Run tests** - `make test` to verify current state
3. **Update todos** - Create todo list for planned work

### Task Tracking

**Always use the TodoWrite tool to track work:**
- Update todos at the start of any task
- Mark items complete immediately when done
- Add next steps as new todos
- Keep the todo list current so we don't get lost

### Completing Work

1. **Test** - `make test` must pass
2. **Update phase doc** - Mark completed items in `PHASE_XX.md`
3. **Commit** - Git commit with descriptive message

### Git Commits (CRITICAL)

**COMMIT AFTER EVERY FEATURE OR LOGICAL UNIT OF WORK.**

This is mandatory - do NOT batch multiple features into one commit.

Guidelines:
- Small, focused commits (one feature per commit)
- Clear commit messages describing what was added
- Commit working code only (tests must pass)
- **NEVER add Claude attribution or Co-Authored-By to commits**
- If you implement something, commit it immediately before moving on

Example workflow:
1. Update todos with current task
2. Implement ONE feature
3. Run `make test` to verify it works
4. Mark todo complete
5. **Git commit immediately** with descriptive message
6. Move to next feature
7. Repeat

Bad: Implement syscall handler + ELF loader + Ring 3 transition → one big commit
Good: Implement syscall handler → commit → ELF loader → commit → Ring 3 → commit

---

## Current Status Quick Reference

**Completed Phases:**
- Phase 0: Boot + Serial ✓
- Phase 1: Memory Management ✓
- Phase 2: Interrupts + Timer + Scheduler ✓

**Current Phase:**
- Phase 3: User Mode + Syscalls

**Check `docs/plan/PHASE_03.md` for Phase 3 requirements.**

---

## When In Doubt

If uncertain about any of the above:
1. Ask the user for clarification
2. Do NOT make assumptions that reduce scope
3. Do NOT silently simplify requirements
4. State what you know and what you don't know

**The user's requirements are the source of truth, not Claude's assessment of feasibility.**
