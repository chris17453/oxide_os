# Current Task: Fix ping crash (NX page fault after exec)

## Problem Summary

When `ping localhost` is run, the system crashes with an NX page fault (error code 0x15).

### Root Cause Identified

When `exec()` replaces a process's address space:
1. `exec()` creates a new PML4 and loads the ELF into it
2. `exec()` updates `Process.address_space` with the new PML4
3. **BUG**: The scheduler's `Task` still has the old PML4 from `fork()`
4. When the scheduler context-switches to the task, it uses the stale PML4
5. The stale PML4 has COW-marked pages with NX bit set incorrectly
6. CPU tries to execute from a page marked non-executable → crash

### Debug Evidence

```
[EXEC] PID 2 exec("/bin/servicemgr")
[EXEC] Switching to PML4=0x9c000   <-- exec sets new PML4
...
[PF] fault_addr=0x401266 error=0x15 rip=0x401266
actual_cr3=0x74000                 <-- scheduler used OLD PML4!

[DEBUG] Page table walk for 0x401266:
  PML4[0] = 0x0000000000075027 (present=true, nx=false)
  PDPT[0] = 0x0000000000076027 (present=true, nx=false)
  PD[2] = 0x0000000000077027 (present=true, nx=false)
  PT[1] = 0x8000000000009265 (present=true, nx=true)  <-- NX set!
```

## Fix Required

### 1. Add scheduler function to update task after exec

In `crates/sched/sched/src/core.rs`, add:

```rust
/// Update task's execution info after exec()
///
/// Called by exec() to update the scheduler task with new address space,
/// entry point, and stack pointer.
pub fn update_task_exec_info(
    pid: Pid,
    pml4: PhysAddr,
    entry_point: u64,
    user_stack_top: u64,
) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.pml4_phys = Some(pml4);
                task.context.rip = entry_point;
                task.context.rsp = user_stack_top;
                // Reset other registers for fresh exec
                task.context.rax = 0;
                task.context.rbx = 0;
                task.context.rcx = 0;
                task.context.rdx = 0;
                task.context.rsi = 0;
                task.context.rdi = 0;
                task.context.rbp = 0;
                task.context.r8 = 0;
                task.context.r9 = 0;
                task.context.r10 = 0;
                task.context.r11 = 0;
                task.context.r12 = 0;
                task.context.r13 = 0;
                task.context.r14 = 0;
                task.context.r15 = 0;
                true
            } else {
                false
            }
        });

        if found == Some(true) {
            break;
        }
    }
}
```

### 2. Call it from exec.rs

In `crates/proc/proc/src/exec.rs`, after replacing the address space:

```rust
// Update scheduler task with new exec info
sched::update_task_exec_info(pid, new_pml4_phys, entry_point, user_stack_top);
```

### 3. Export function from sched crate

Ensure `update_task_exec_info` is exported in `crates/sched/sched/src/lib.rs`.

## Files to Modify

1. `crates/sched/sched/src/core.rs` - Add `update_task_exec_info` function
2. `crates/sched/sched/src/lib.rs` - Export the new function
3. `crates/proc/proc/src/exec.rs` - Call the function after exec

## Previous Fixes Applied (Already Done)

1. **EFER.NXE enabled** in `arch-x86_64/src/syscall.rs`
2. **Fork doesn't propagate NX to intermediate entries** in `proc/src/fork.rs`
3. **Page table debug dumping** in `kernel/src/fault.rs`

## Secondary Issue (Lower Priority)

`service list` shows services as "stopped" but `ps` shows them running.
This is a separate bug to investigate after the ping crash is fixed.
