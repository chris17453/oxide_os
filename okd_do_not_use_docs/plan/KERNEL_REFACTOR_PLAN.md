# Kernel Main.rs Refactoring Plan

## Current State
- `kernel/src/main.rs` is 2499 lines
- Contains all kernel functionality in a single file
- Hard to navigate, maintain, and debug

## Target Structure

```
kernel/src/
├── main.rs           # Thin wrapper: module declarations, panic handler, entry call
├── globals.rs        # Global statics and allocators
├── init.rs           # Boot initialization (kernel_main contents)
├── process.rs        # Process callbacks (fork, exec, wait, exit)
├── scheduler.rs      # Preemptive scheduler
├── console.rs        # Console I/O and keyboard
├── memory.rs         # Frame allocator wrapper
├── fault.rs          # Page fault handler
├── smp_init.rs       # AP initialization callback
└── debug.rs          # (existing) Debug macros
```

## Module Breakdown

### 1. `globals.rs` (~50 lines)
Global statics moved from main.rs:
- `HEAP_ALLOCATOR` and `HEAP_STORAGE`
- `FRAME_ALLOCATOR`
- `KERNEL_PML4`
- `USER_EXITED`, `USER_EXIT_STATUS`
- `READY_QUEUE`
- `PARENT_CONTEXT`, `CHILD_DONE`
- `ParentContext` struct

### 2. `init.rs` (~700 lines)
Boot initialization:
- `kernel_main()` - the entry point
- VFS setup
- Network initialization
- Initramfs loading
- Init process creation
- User mode entry

### 3. `process.rs` (~600 lines)
Process management callbacks:
- `kernel_fork()`
- `kernel_wait()`
- `kernel_exec()`
- `user_exit()`
- `run_child_process()`
- Context restoration code

### 4. `scheduler.rs` (~150 lines)
Preemptive scheduler:
- `InterruptFrame` struct
- `scheduler_tick()`
- Ready queue management

### 5. `console.rs` (~200 lines)
Console I/O:
- `console_write()`
- `console_read()`
- `terminal_tick()`
- Keyboard modifier state
- `process_scancode()`
- `scancode_to_ascii()`

### 6. `memory.rs` (~100 lines)
Memory utilities:
- `FrameAllocatorWrapper`
- `get_memory_stats()`
- Framebuffer device callbacks

### 7. `fault.rs` (~80 lines)
Exception handling:
- `page_fault_handler()`

### 8. `smp_init.rs` (~30 lines)
SMP callbacks:
- `ap_init_callback()`

### 9. `main.rs` (new, ~50 lines)
Entry point and glue:
- `#![no_std]`, `#![no_main]`
- `extern crate alloc`
- Module declarations
- `serial_writer()` helper
- `#[panic_handler]`
- Call to `init::kernel_main()`

## Implementation Order

1. Create `globals.rs` with statics
2. Create `memory.rs` with allocator wrapper
3. Create `console.rs` with I/O functions
4. Create `fault.rs` with page fault handler
5. Create `smp_init.rs` with AP callback
6. Create `scheduler.rs` with scheduler
7. Create `process.rs` with process callbacks
8. Create `init.rs` with kernel_main
9. Reduce `main.rs` to thin wrapper
10. Verify build compiles
11. Test in QEMU

## Key Considerations

- All modules need access to globals via `crate::globals`
- Some functions need to be `pub` for cross-module access
- Import paths need careful management
- The syscall context callbacks need visibility
- Serial writer needs to be accessible everywhere

## Exit Criteria

- [ ] All modules created with correct code
- [ ] `make build` succeeds
- [ ] No functionality changes
- [ ] `make test` or QEMU boot works
