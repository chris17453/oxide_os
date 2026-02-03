# Signal Delivery Fix Documentation

## Problem Statement
Applications in OXIDE OS were not receiving signals (SIGINT, SIGTERM, etc.) because signals were only being delivered during timer interrupts, not when returning from system calls. This meant that a process could send a signal to another process, but the receiving process wouldn't react until the next timer tick (up to 10ms delay).

## Root Cause Analysis

### Signal Delivery Architecture
OXIDE OS implements POSIX-style signal handling with:
- Signal pending queues per process (`PendingSignals`)
- Signal actions/handlers per process (`SigAction` array)
- Signal masks for blocking (`SigSet`)
- Signal frame mechanism for user-mode handlers

### The Missing Link
Signal delivery was only implemented in `kernel/src/scheduler.rs:scheduler_tick()` which runs on timer interrupts (100Hz). The syscall return path in `kernel/arch/arch-x86_64/src/syscall.rs` had NO signal checking.

**Flow Before Fix:**
```
User Process → syscall instruction → kernel handler → sysretq → User Process
                                                        ↑
                                                No signal check!
```

**Flow After Fix:**
```
User Process → syscall instruction → kernel handler → signal check → sysretq → Signal Handler or User Process
                                                        ↑
                                                Checks pending signals!
```

## Solution Implementation

### 1. Architecture Layer Changes (`kernel/arch/arch-x86_64/src/syscall.rs`)

#### Added Signal Check Hook
```rust
pub type SignalCheckFunction = fn();
static mut SIGNAL_CHECK_FUNCTION: Option<SignalCheckFunction> = None;

pub unsafe fn set_signal_check_function(func: SignalCheckFunction) {
    *addr_of_mut!(SIGNAL_CHECK_FUNCTION) = Some(func);
}
```

#### Modified Syscall Dispatcher
```rust
extern "C" fn syscall_dispatch(...) -> i64 {
    // Call syscall handler
    let result = handler(number, arg1, arg2, arg3, arg4, arg5, arg6);
    
    // 🔥 NEW: Check for pending signals before returning
    unsafe {
        if let Some(check_fn) = *addr_of!(SIGNAL_CHECK_FUNCTION) {
            check_fn();  // Calls check_signals_on_syscall_return()
        }
    }
    
    return result;
}
```

#### Enhanced Context Restoration
Modified assembly code to reload RIP/RSP/RDI from `SYSCALL_USER_CONTEXT` after the handler returns:

```asm
// After calling handler, reload potentially-modified context
lea r12, [user_ctx]
mov r13, [r12 + 0]      // Load ctx.rip
mov [rsp + 56], r13     // Store to stack (will be loaded to RCX for sysret)
mov r13, [r12 + 8]      // Load ctx.rsp
mov [rsp + 48], r13     // Store to stack
mov rdi, [r12 + 64]     // Load ctx.rdi (signal handler first arg)
```

### 2. Scheduler Layer Changes (`kernel/src/scheduler.rs`)

#### Added Signal Delivery Function
```rust
pub fn check_signals_on_syscall_return() {
    // Get current PID
    // Try to get process metadata (non-blocking)
    // Check if there are deliverable signals
    // Dequeue highest priority signal
    // Determine action (Terminate, UserHandler, Stop, Continue, Ignore)
    // Handle each case appropriately
}
```

#### Signal Handling Cases

**Terminate/CoreDump:**
- Set exit status to 128 + signal_number
- Wake parent process for wait()
- Block current task as zombie

**UserHandler:**
- Build signal frame with saved registers
- Get restorer address (sigreturn trampoline)
- Setup signal frame on user stack
- Modify `SYSCALL_USER_CONTEXT`:
  - `rip` → signal handler address
  - `rsp` → new stack pointer (below signal frame)
  - `rdi` → signal number (first argument)
- Update signal mask for handler execution

**Stop:**
- Set stop_signal field
- Block task as TASK_STOPPED
- Wake parent for WUNTRACED waitpid

**Continue:**
- Clear stop_signal field
- Wake task if stopped
- Wake parent for WCONTINUED waitpid

**Ignore:**
- Do nothing, continue execution

### 3. Kernel Initialization (`kernel/src/init.rs`)

Registered the signal check function during boot:
```rust
unsafe {
    arch::syscall::set_signal_check_function(
        crate::scheduler::check_signals_on_syscall_return
    );
}
```

## Technical Details

### Why Modify SYSCALL_USER_CONTEXT?
The syscall entry assembly saves user register state to `SYSCALL_USER_CONTEXT` at entry. When delivering a signal that requires a user handler, we need to redirect execution to that handler. By modifying this context structure, the assembly code will restore the modified values on sysret, causing the process to jump to the signal handler instead of continuing from where it was interrupted.

### SMAP Considerations
The syscall entry enables SMAP user access with STAC instruction, which remains active during the handler and signal check. This allows the signal delivery code to write the signal frame directly to the user stack.

### Lock Contention
The signal check uses `try_lock()` instead of blocking lock to avoid deadlock in nested contexts. If the lock can't be acquired immediately, the signal will be delivered on the next opportunity (next syscall or timer interrupt).

### Performance Impact
- Minimal: Signal check only runs on syscall return
- Fast path: Single branch when no signals pending
- No additional syscalls or context switches unless signal needs delivery

## Test Program

Created `userspace/tests/signal-test/` to validate signal delivery:

```rust
fn main() {
    println!("Sending SIGTERM to self");
    let pid = getpid();
    kill(pid, 15);  // SIGTERM
    println!("If you see this, BUG - signal was not delivered!");
}
```

Expected behavior: Process terminates immediately after kill() returns, before the println.

## Integration

### Build System
- Added `signal-test` to workspace in `Cargo.toml`
- Added binary to initramfs in `Makefile`
- Test program available as `/bin/signal-test` in OS

### Compatibility
- No ABI changes
- No changes to signal numbers or structures
- Fully backward compatible with existing code

## Verification Steps

1. Build: `make build-full`
2. Boot: `make run`
3. At shell prompt: `/bin/signal-test`
   - Should see "Sending SIGTERM" then immediate termination
   - Should NOT see "If you see this..." message
4. Test Ctrl+C: Run any program, press Ctrl+C
   - Process should terminate immediately
5. Test kill: Run `sleep 1000 &`, note PID, run `kill <PID>`
   - Background process should terminate

## Known Limitations

1. **Thread Signals**: Currently per-process signals only. Thread-specific signals (pthread_kill) not yet implemented.

2. **Signal Queuing**: Real-time signal queuing not fully implemented. Multiple pending signals of the same number may be coalesced.

3. **Alternate Signal Stack**: sigaltstack() syscall accepts values but doesn't use them yet.

4. **Signal Restorer**: User programs must provide signal restorer (sigreturn trampoline). This is typically done by libc.

## Future Enhancements

1. **SA_RESTART Support**: Automatically restart interrupted syscalls when SA_RESTART flag set
2. **Real-time Signals**: Proper queuing for SIGRTMIN-SIGRTMAX
3. **Thread Signals**: Per-thread signal masks and pthread_kill()
4. **Signal Stack**: Implement alternate signal stack support
5. **Core Dumps**: Generate core dump files on SIGSEGV, SIGABRT, etc.

## References

- POSIX.1-2017 Signal Concepts: https://pubs.opengroup.org/onlinepubs/9699919799/functions/V2_chap02.html#tag_15_04
- Linux signal(7): https://man7.org/linux/man-pages/man7/signal.7.html
- x86_64 syscall/sysret ABI: AMD64 Architecture Programmer's Manual, Volume 2

## Author Comments

**— GraveShift**: Signal delivery on syscall return, not just timer ticks. Signals ain't gonna deliver themselves. Direct write to user stack - page fault will catch invalid stacks. The UNIX way.

**— BlackLatch**: Process gets the axe when it needs the axe. No delays, no mercy. Clean termination path.

**— ThreadRogue**: Freeze-frame and thaw support for job control. Parent gets notified, child gets suspended. Classic UNIX job control.
