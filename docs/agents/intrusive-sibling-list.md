# Agent Rule: Intrusive Sibling List for Task Children

## Rule
Task children MUST be stored as an intrusive doubly-linked sibling list using
PID fields (first_child, sibling_next, sibling_prev), NOT Vec<Pid>.

## Why
Vec<Pid> heap-allocates inside scheduler RQ locks on fork/exit/waitpid.
The heap lock under the RQ lock = nested lock deadlock. With servicemgr
polling 6 children at 100Hz, that's 600 heap alloc/free pairs per second.

## Data Structure
```rust
pub first_child: Pid,     // PID_NONE = no children
pub sibling_next: Pid,    // PID_NONE = last sibling
pub sibling_prev: Pid,    // PID_NONE = first sibling
```

## Operations
- **Add child**: O(1) prepend — new child becomes first_child
- **Remove child**: O(1) unlink — update prev.next and next.prev
- **Iterate**: Walk sibling_next chain from first_child (capped at 256)
- **Zero heap allocation** for any operation

## Cross-CPU Safety
Siblings may be on different CPUs. Each mutation uses separate
`with_task_on_any_cpu` calls (one per task touched). PID values (not pointers)
are stored, so no dangling references even if tasks migrate between steps.

## How to Apply
Never add Vec<T> fields to Task that are modified under RQ locks.
Use intrusive lists with PID-based linkage for zero-alloc O(1) operations.
