# Phase 3: User Mode + Syscalls

**Stage:** 1 - Foundation
**Status:** Not Started
**Dependencies:** Phase 2 (Scheduler)

---

## Goal

Run user processes in unprivileged mode with syscall interface.

---

## Deliverables

| Item | Status |
|------|--------|
| User address space creation | [ ] |
| Static ELF loader | [ ] |
| Ring 0 → Ring 3 transition | [ ] |
| Syscall entry mechanism | [ ] |
| sys_exit | [ ] |
| sys_write | [ ] |
| sys_read | [ ] |

---

## Architecture Status

| Arch | UserSpace | ELF | Transition | Syscall | Done |
|------|-----------|-----|------------|---------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
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

- [ ] User process runs in Ring 3 (all arches)
- [ ] Syscall traps to kernel correctly
- [ ] sys_exit terminates process
- [ ] sys_write outputs to serial/console
- [ ] sys_read reads from console (basic)
- [ ] User access to kernel memory faults
- [ ] Works on all 8 architectures

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

*(Add implementation notes as work progresses)*

---

*Phase 3 of EFFLUX Implementation*
