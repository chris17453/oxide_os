# ISR Context Must Never Allocate Heap Memory

## Rule

Code reachable from interrupt service routines (ISRs) — timer ticks, keyboard
handlers, any interrupt context — MUST NEVER call `alloc`, `realloc`, or any
operation that can trigger `HEAP_ALLOCATOR.lock()`.

## Why

`HEAP_ALLOCATOR` is a `spin::Mutex`. If the interrupted task holds this lock
(mid-malloc), and the ISR tries to acquire it on the same CPU:

```
CPU0: Task A → malloc() → HEAP_ALLOCATOR.lock() [HELD]
CPU0: ← Timer ISR fires
CPU0: ISR → scheduler_tick → enqueue_task → BinaryHeap::push
       → Vec::grow_one → realloc → HEAP_ALLOCATOR.lock() → DEADLOCK
```

Permanent deadlock. Interrupts disabled. System frozen. No recovery possible.

## What Allocates (Surprising Sources)

| Operation | Allocates? | Safe in ISR? |
|-----------|-----------|--------------|
| `BinaryHeap::push()` | Yes (Vec::grow_one) | NO |
| `VecDeque::push_back()` | Yes (ring buffer grow) | NO |
| `Vec::push()` | Yes (grow_one) | NO |
| `BTreeMap::insert()` | Yes (new node) | NO |
| `BTreeMap::get()` / `get_mut()` | No (traversal only) | Yes |
| `Box::new()` | Yes | NO |
| `String::push_str()` | Yes (Vec grow) | NO |
| Fixed-size array operations | No | Yes |
| `AtomicU64::fetch_add()` | No | Yes |

## The Fix: Fixed-Capacity Data Structures

All scheduler run queues use pre-allocated fixed-size arrays:

- **CFS**: `[CfsEntry; 256]` min-heap with manual sift operations (`fair.rs`)
- **RT**: `[[Pid; 8]; 99]` per-priority FIFO arrays (`rt.rs`)

Linux solves this with intrusive data structures (rb-tree nodes embedded in
task_struct, no separate allocation). Our fixed-capacity arrays achieve the
same ISR safety with simpler code.

## Audit Checklist

When adding code to any ISR-reachable path:

1. Can this path be reached from `scheduler_tick_ex()`? → No alloc
2. Can this path be reached from `pick_next_task()` (called after tick)? → No alloc
3. Can this path be reached from keyboard/serial ISR handlers? → No alloc
4. Does this code call any `alloc::collections` method that might grow? → No alloc
5. Does this code call `Box::new()` or format strings? → No alloc

## Files

- `kernel/sched/sched/src/fair.rs` — Fixed-capacity CFS min-heap
- `kernel/sched/sched/src/rt.rs` — Fixed-capacity RT per-priority arrays
- `kernel/sched/sched/src/runqueue.rs` — RunQueue (BTreeMap ops are read-only in ISR path)
