# Rule: Fork sysretq MUST have CLI before touching RSP

## Summary

Any inline asm block that does manual `sysretq` or `iretq` to return to
userspace **MUST** begin with `cli` to disable interrupts before modifying
RSP, loading CR3, or executing `swapgs`.

## Why

The timer interrupt (IRQ0) does **not** use an IST (Interrupt Stack Table)
entry. For same-privilege interrupts (ring 0 → ring 0), the CPU pushes the
interrupt frame onto the **current RSP**. If a timer fires after `pop rsp`
switches RSP to a user-space stack address but before `sysretq` transitions
to ring 3, the interrupt frame is pushed onto user memory — corrupting the
scheduler's saved context.

### Crash signature

- Page fault at a low address (e.g. `0x66`) or RIP landing in `.rodata`
- AC flag SET in RFLAGS (still inside STAC/CLAC syscall window)
- Happens intermittently after `fork()` — timing-dependent race

### The race window (before fix)

```
fork handler inline asm:
    mov cr3, <child_cr3>    ; switch address space
    pop rsp                 ; ← RSP now points to user stack
    ─── TIMER FIRES HERE ───  ; ISR pushes frame onto user memory!
    swapgs
    sysretq                 ; never reached or reached with trashed state
```

### The fix

```asm
    cli                     ; — SableWire: seal the interrupt window
    mov cr3, <child_cr3>
    pop rsp
    swapgs
    sysretq                 ; sysretq restores IF from R11 atomically
```

`sysretq` loads RFLAGS from R11 (which has IF=1), so interrupts are
re-enabled **atomically** on the ring transition — no window exists.

## Scope

- `kernel/src/process.rs` — fork handler's manual sysretq to child
- `kernel/arch/arch-x86_64/src/syscall.rs` — normal syscall return (already
  has `cli` at line 348, safe)
- Any future code that manually does `sysretq` or `iretq`

## Verification

After applying the fix, the kernel boots cleanly through all fork+exec
calls (servicemgr, gettys, login, esh) with zero page faults or panics.
Build 619 confirmed working.
