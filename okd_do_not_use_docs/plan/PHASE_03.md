# Phase 3: User Mode + Syscalls

**Stage:** 1 - Foundation
**Status:** Complete (x86_64)
**Dependencies:** Phase 2 (Scheduler)

---

## Goal

Run user processes in unprivileged mode with syscall interface.

---

## Deliverables

| Item | Status |
|------|--------|
| User address space creation | [x] |
| Static ELF loader | [x] |
| Ring 0 → Ring 3 transition | [x] |
| Syscall entry mechanism | [x] |
| sys_exit | [x] |
| sys_write | [x] |
| sys_read | [x] |

---

## Architecture Status

| Arch | UserSpace | ELF | Transition | Syscall | Done |
|------|-----------|-----|------------|---------|------|
| x86_64 | [x] | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Syscall Mechanisms

| Arch | Instruction | Number | Args |
|------|-------------|--------|------|
| x86_64 | syscall | RAX | RDI, RSI, RDX, R10, R8, R9 |
| i686 | int 0x80 / sysenter | EAX | EBX, ECX, EDX, ESI, EDI, EBP |
| aarch64 | svc #0 | X8 | X0-X5 |
| arm | svc #0 | R7 | R0-R5 |
| mips64/mips32 | syscall | v0 | a0-a3, stack |
| riscv64/riscv32 | ecall | A7 | A0-A5 |

---

## Initial Syscalls

| Number | Name | Args | Return |
|--------|------|------|--------|
| 0 | sys_exit | status | - |
| 1 | sys_write | fd, buf, len | bytes written |
| 2 | sys_read | fd, buf, len | bytes read |

---

## Key Files to Create

```
kernel/
├── arch/
│   ├── x86_64/
│   │   ├── syscall.rs          # syscall/sysret setup
│   │   └── user.rs             # Ring 3 transition
│   ├── aarch64/
│   │   ├── syscall.rs          # svc handler
│   │   └── user.rs             # EL0 transition
│   └── ... (other arches)
├── core/
│   ├── syscall/
│   │   ├── mod.rs              # Syscall dispatch
│   │   ├── table.rs            # Syscall table
│   │   └── handlers.rs         # Handler implementations
│   ├── elf/
│   │   ├── mod.rs              # ELF parser
│   │   └── loader.rs           # Load into address space
│   └── process.rs              # Basic process structure
```

---

## ELF Loading Steps

1. Parse ELF header
2. Validate (executable, correct arch)
3. Create user address space
4. For each PT_LOAD segment:
   - Allocate pages
   - Copy segment data
   - Set permissions (RWX)
5. Set up user stack
6. Jump to entry point in user mode

---

## Exit Criteria

- [x] User process runs in Ring 3 (x86_64)
- [x] Syscall traps to kernel correctly
- [x] sys_exit terminates process
- [x] sys_write outputs to serial/console
- [x] sys_read reads from console (basic)
- [x] User access to kernel memory faults
- [ ] Works on all 8 architectures (x86_64 only for now)

---

## Test Program

```c
// Minimal test: write to stdout and exit
void _start() {
    const char msg[] = "Hello from userspace!\n";
    syscall(SYS_write, 1, msg, sizeof(msg) - 1);
    syscall(SYS_exit, 0);
}
```

---

## Notes

**x86_64 Implementation Complete (2025-01-18):**

Key implementation details:
- UserAddressSpace in `proc` creates user page tables with kernel higher-half shared
- ELF loader in `elf` parses ELF64 and loads PT_LOAD segments
- Ring 3 transition via `iretq` with proper segment selectors (USER_CS=0x23, USER_DS=0x1B)
- Syscall entry via `syscall` instruction using MSR configuration (STAR, LSTAR, SFMASK, EFER.SCE)
- Kernel stack for syscalls stored in KERNEL_GS_BASE, accessed via swapgs
- TSS.RSP0 set for interrupt handling during user mode

Key fixes during implementation:
- FrameAllocator trait changed to `&self` for interior mutability
- Frame 0 protection added (never allocate NULL page)
- `enter_usermode()` function created to switch kernel stacks before page tables
- swapgs removed before iretq (KERNEL_GS_BASE must retain kernel stack)

---

*Phase 3 of OXIDE Implementation*
