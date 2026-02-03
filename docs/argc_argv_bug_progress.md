# argc/argv Bug Investigation Progress

**Date:** 2026-02-02
**Issue:** Programs receive argc=0, cannot see command-line arguments
**Status:** ROOT CAUSE IDENTIFIED - Ready to fix

---

## Problem Summary

When running programs like `argtest bob bob obo` or `ping 8.8.8.8`, they show:
- argc = 0
- argv ptr shows a valid address (e.g., 0x00007ffffffeffb0)
- Programs show help/usage instead of running with arguments

---

## Investigation Timeline

### What We Tried

1. **Initial Theory:** Kernel wasn't writing argc to stack correctly
   - Modified exec.rs to clear registers (rdi=0, rsi=0, rdx=0)
   - This was CORRECT but didn't fix the issue

2. **Debug Feature Issue:**
   - Enabling debug features in Makefile caused kernel hang during SMP init
   - All 4 CPUs printing simultaneously overwhelmed the system
   - **Solution:** Disabled all debug features (`RUN_KERNEL_FEATURES ?=`)

3. **Stack Pointer Tests:**
   - Wrote test patterns (0xDEADBEEF, 0xAAAAAAAA, etc.) to multiple stack locations
   - None showed up - argc always remained 0
   - Calculated actual RSP from argv_ptr: **0x00007FFFFFFEFFA8**

4. **Deep Dive into exec() Flow:**
   - Traced through exec.rs stack setup - CORRECT
   - Checked update_task_exec_info - seemed OK
   - Found the context gets copied correctly
   - But something was overwriting or ignoring the correct RSP value

---

## ROOT CAUSE DISCOVERED ✓

**File:** `kernel/src/process.rs`
**Lines:** 1296-1302 in kernel_exec()

After exec(), when returning to userspace via sysretq, the code does:

```asm
"mov rsp, r10",              // Line 1297 - Set RSP
"mov rdi, r12",              // Line 1300 - Set rdi = argc  ← BUG!
"mov rsi, r13",              // Line 1301 - Set rsi = argv  ← BUG!
"mov rdx, r14",              // Line 1302 - Set rdx = envp  ← BUG!
```

### The Bug

**Problem:** The code is putting argc/argv/envp into registers (rdi/rsi/rdx) for program startup.

**Why This Is Wrong:**
- System V ABI for **function calls**: argc/argv in rdi/rsi (CORRECT for calling functions)
- System V ABI for **program startup**: argc/argv on **STACK** at [rsp+0], [rsp+8] (DIFFERENT!)
- When a program starts (not a function call), there are NO arguments in registers
- The _start code expects: `[rsp+0] = argc`, `[rsp+8] = argv[0]`, etc.

**What Actually Happens:**
1. Kernel writes argc correctly to stack at calculated address
2. Kernel sets RSP to some value (r10)
3. Kernel ALSO sets rdi/rsi/rdx to argc/argv/envp (wrong!)
4. Program's _start reads `mov eax, [rsp]` expecting argc
5. But RSP is pointing to wrong location, reads 0

### Why RSP Is Wrong

The value loaded into r10 (which becomes RSP) is likely coming from the wrong source:
- Should be: The calculated `final_rsp` from exec.rs (points to where argc was written)
- Actually is: Possibly `task.user_stack_top` or some other default value

---

## The Fix (Not Yet Applied)

### Part 1: Fix Register Setup

**Location:** kernel/src/process.rs, lines 1298-1302

**Change from:**
```asm
"mov rdi, r12",  // argc
"mov rsi, r13",  // argv
"mov rdx, r14",  // envp
```

**Change to:**
```asm
"xor rdi, rdi",  // Clear rdi (no argc in register!)
"xor rsi, rsi",  // Clear rsi (no argv in register!)
"xor rdx, rdx",  // Clear rdx (no envp in register!)
```

### Part 2: Verify RSP Source

**Location:** kernel/src/process.rs, lines before 1297

Need to verify that r10 is loaded with the CORRECT RSP value:
- Should be: `ctx.rsp` from exec_result (which is `final_rsp` from exec.rs)
- This is the address where argc was actually written

**Find where r10 is loaded** (around line 1260-1280) and ensure it uses `ctx.rsp`, not `task.user_stack_top`

---

## Files Modified (Not Yet Fixed)

### Already Changed (Preparation)
1. **kernel/proc/proc/src/exec.rs**
   - Lines 475-490: Cleared registers in ProcessContext (rdi=0, rsi=0, rdx=0)
   - Lines 438-447: Added diagnostic test patterns (can be removed after fix)
   - This was CORRECT preparation but not the actual bug

2. **Makefile**
   - Line 25: Disabled debug features to prevent SMP hang
   - `RUN_KERNEL_FEATURES ?=` (empty)

### Needs to be Fixed
1. **kernel/src/process.rs**
   - Lines 1298-1302: Remove argc/argv/envp register loading
   - Lines ~1260-1280: Verify r10 is loaded from correct RSP source

---

## Next Steps (Tomorrow)

### 1. Find where r10/r12/r13/r14 are loaded
```bash
grep -B 30 "mov rsp, r10" /home/nd/repos/Projects/oxide_os/kernel/src/process.rs
```
Look for where these registers get their values

### 2. Apply the fix
- Change lines 1300-1302 to clear rdi/rsi/rdx instead of setting them
- Verify r10 comes from ctx.rsp (the correct calculated stack pointer)

### 3. Clean up diagnostic code
Remove the test pattern code from exec.rs lines 438-447:
```rust
// GraveShift: EXTREME DIAGNOSTIC - Fill entire region with argc
for offset in -64..128 {
    // ... can be removed
}
```

Replace with the original single write:
```rust
write_to_user_stack(&new_address_space, ptr, &argc_bytes)?;
```

### 4. Test
```bash
make run
# Login as root
argtest bob bob obo
# Should show: argc = 4
ping 8.8.8.8
# Should actually ping, not show usage
```

### 5. Verify with other programs
- ls -la /tmp
- echo hello world
- cat /etc/fstab
- gwbasic (with script argument)

---

## Key Learnings

1. **System V ABI has TWO different conventions:**
   - Function calls: args in rdi/rsi/rdx/rcx/r8/r9
   - Program startup: args on stack only, registers cleared

2. **Debug features cause SMP hang:**
   - Multiple CPUs printing simultaneously = system hang
   - Keep debug features disabled unless specifically needed
   - The debug system has recursion protection but still problematic with SMP

3. **Stack setup in exec.rs was always correct:**
   - The bug wasn't in the stack calculation
   - The bug was in the usermode entry code AFTER exec
   - argc was written correctly, but RSP wasn't pointing to it OR registers were overriding the stack values

4. **The register initialization in exec.rs (clearing rdi/rsi/rdx) was correct:**
   - We fixed that in ProcessContext
   - But kernel_exec has its own inline assembly that overrides it!

---

## Related Documentation

- System V ABI: [RSP+0] = argc, [RSP+8] = argv[0], etc.
- kernel/proc/proc/src/exec.rs: Stack layout comments (lines 328-340)
- userspace/libs/libc/src/arch/x86_64/start.rs: How _start reads argc/argv
- docs/PROGRESS_TRACKER.md: Overall project status

---

## Quick Reference

**Test command:**
```bash
argtest bob bob obo
```

**Expected output:**
```
=== ARGTEST ===
argc = 4
argv ptr = 0x00007ffffffeffa8
argv[0] = 0x... -> "argtest"
argv[1] = 0x... -> "bob"
argv[2] = 0x... -> "bob"
argv[3] = 0x... -> "obo"
===============
```

**Current (broken) output:**
```
=== ARGTEST ===
argc = 0
argv ptr = 0x00007ffffffeffb0
===============
```

---

**Status:** Ready to fix - just need to modify kernel/src/process.rs lines 1298-1302 and verify r10 source.
