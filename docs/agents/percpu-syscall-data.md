# Per-CPU Syscall Data — SMP Safety Rule

## Rule

Every CPU MUST have its own `SyscallCpuData` and `SyscallUserContext`. These are
accessed via the `KERNEL_GS_BASE` MSR + `swapgs` at syscall entry. A single global
for either data structure is a **fatal SMP bug**: two CPUs entering syscalls
simultaneously clobber each other's saved registers.

## Architecture

```
CPU 0:  KERNEL_GS_BASE → &CPU_DATA_ARRAY[0]  →  user_ctx_ptr → &SYSCALL_USER_CONTEXTS[0]
CPU 1:  KERNEL_GS_BASE → &CPU_DATA_ARRAY[1]  →  user_ctx_ptr → &SYSCALL_USER_CONTEXTS[1]
CPU 2:  KERNEL_GS_BASE → &CPU_DATA_ARRAY[2]  →  user_ctx_ptr → &SYSCALL_USER_CONTEXTS[2]
CPU 3:  KERNEL_GS_BASE → &CPU_DATA_ARRAY[3]  →  user_ctx_ptr → &SYSCALL_USER_CONTEXTS[3]
```

### SyscallCpuData Layout (GS-relative offsets)

| Offset | Field          | Purpose                                    |
|--------|----------------|--------------------------------------------|
| 0      | kernel_rsp     | Kernel stack pointer for syscall entry      |
| 8      | scratch_rsp    | Save user RSP before stack switch           |
| 16     | scratch_rax    | Save syscall number (RAX)                   |
| 24     | scratch_r12    | Save user R12                               |
| 32     | scratch_rcx    | Reserved (unused)                           |
| 40     | cpu_id         | Logical CPU index                           |
| 48     | user_ctx_ptr   | Pointer to this CPU's SyscallUserContext    |

### GS State Machine

```
User mode:   GS.base = user value    KERNEL_GS_BASE = per-CPU data ptr
  ↓ syscall
  swapgs:    GS.base = per-CPU data  KERNEL_GS_BASE = user value
  ↓ handler runs (gs:[N] = per-CPU access)
  ↓ sysret
  swapgs:    GS.base = user value    KERNEL_GS_BASE = per-CPU data ptr
```

ISR entry: conditional `swapgs` if CS & 3 != 0 (from user mode).
During kernel execution (syscall handler, ISR from kernel), GS is always kernel.

## Initialization Requirements

Each CPU MUST call these before handling syscalls:

1. **`syscall::init()`** — Sets EFER.SCE, STAR, LSTAR, SFMASK MSRs (per-CPU!)
2. **`syscall::init_kernel_stack(cpu_id, kernel_rsp)`** — Sets BOTH GS_BASE AND
   KERNEL_GS_BASE to per-CPU data, initializes cpu_id and user_ctx_ptr fields

**CRITICAL: Both GS_BASE (0xC0000101) and KERNEL_GS_BASE (0xC0000102) must be set.**
Setting only KERNEL_GS_BASE leaves GS_BASE = 0 (boot default). Then:
- Timer ISR from kernel mode does NOT swapgs (CS & 3 == 0)
- `set_kernel_stack`'s `mov gs:[0], reg` writes to address 0
- ISR `iretq` to user mode DOES swapgs, putting 0 into KERNEL_GS_BASE
- Next syscall's swapgs loads 0 → gs:[0] reads address 0 → crash

Setting both to per-CPU makes all swapgs operations no-ops (swapping identical values).
User code uses FS for TLS, not GS, so GS_BASE = per-CPU in user mode is harmless.

BSP: Called in `init.rs` during kernel boot.
APs: Called in `smp_init.rs:ap_init_callback()` before timer start.

## set_kernel_stack

Uses `mov gs:[0], reg` — naturally per-CPU via GS segment. Called during context
switch to update the kernel stack for the newly-scheduled task.

## get_user_context / get_user_context_mut

Read `gs:[48]` to get the per-CPU user context pointer. Only valid in kernel
context (after swapgs).

## What Happens Without This

- **No init_kernel_stack on AP**: `swapgs` loads KERNEL_GS_BASE=0 → `mov rsp, gs:[0]`
  reads address 0 → RSP becomes garbage → crash on next push
- **Single global SYSCALL_USER_CONTEXT**: CPU A saves registers, gets preempted,
  CPU B enters syscall and overwrites the global, CPU A returns with CPU B's
  RIP/RSP → process executes wrong code at wrong stack
- **No syscall::init on AP**: LSTAR=0 → `syscall` jumps to address 0 → #PF or
  execution of random memory; EFER.SCE=0 → `syscall` causes #UD

## Files

- `kernel/arch/arch-x86_64/src/syscall.rs` — Per-CPU arrays, asm, init functions
- `kernel/src/smp_init.rs` — AP initialization with syscall::init + init_kernel_stack
- `kernel/src/init.rs` — BSP initialization
