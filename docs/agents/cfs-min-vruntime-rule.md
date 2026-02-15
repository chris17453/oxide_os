# CFS min_vruntime: Exclude Running Task from Tree Minimum

## Rule
When updating `min_vruntime`, use ONLY the leftmost tree entry's vruntime when the CFS tree is non-empty. The currently-running task (popped from tree, `on_rq=false`) must NOT clamp `min_vruntime` downward.

## The Bug
```rust
// WRONG — holds min_vruntime back when curr has low vruntime
let new_min = match min_from_tree {
    Some(tree_min) => tree_min.min(curr_vruntime),
    None => curr_vruntime,
};
```

When a blocking/waking task (e.g. PID 2 doing poll/nanosleep) has low vruntime and alternates between running and blocking:
1. PID 2 runs (vr=254ms), PID 3 is in tree (vr=344ms)
2. `min(344, 254) = 254` — min_vruntime stays at 254ms
3. PID 2 blocks and wakes — its adjusted vr = `max(old_vr, 254 - 6) = 282ms`
4. PID 2 **never catches up** to PID 3's 344ms — PID 3 starved permanently

## The Fix
```rust
// CORRECT — matches Linux CFS (curr->on_rq is false, excluded)
let new_min = match min_from_tree {
    Some(tree_min) => tree_min,
    None => curr_vruntime,
};
self.min_vruntime = self.min_vruntime.max(new_min);
```

## Why This Matches Linux
In Linux CFS (`kernel/sched/fair.c`), `update_min_vruntime()` only includes `curr->vruntime` when `curr->on_rq` is true. The currently-running task is dequeued from the rb-tree (`on_rq=0`), so its vruntime is excluded from the minimum calculation. We mirror this by using `curr_vruntime` only as a fallback when the tree is empty (no other runnable tasks).

## Symptoms of This Bug
- A task's vruntime runs ahead during a burst of syscalls (write/fork/open loop)
- After blocking and waking, it never gets scheduled again because other tasks' vruntimes plateau below it
- `[SCHED DUMP]` shows the starved task with `on_rq=1`, `state=0` (RUNNING), but it's never picked
- The starved task's vruntime is frozen while other tasks' vruntimes stay low

## File
`kernel/sched/sched/src/fair.rs` — `CfsRunQueue::update_min_vruntime()`

-- GraveShift: The running task was always a ghost in the tree. min() made it a poltergeist.
