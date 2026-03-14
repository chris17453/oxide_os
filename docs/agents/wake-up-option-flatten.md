# Rule: wake_up() must flatten Option<Option<T>> from with_rq()

## Summary

`with_rq(cpu, closure)` returns `Option<R>` where the outer Option indicates
whether the RQ lock was acquired / RQ exists. If the closure returns `Option<T>`,
the full return type is `Option<Option<T>>`.

**Checking `.is_none()` on the outer layer does NOT detect "task not found".**

- `Some(Some(val))` = lock acquired, task found
- `Some(None)` = lock acquired, task NOT found
- `None` = RQ uninitialized / lock failed

`Some(None).is_none()` is **FALSE** — code guarded by `if result.is_none()`
silently skips the fallback path.

## The Bug

In `wake_up()` (core.rs), tier 1 returns `Option<Option<u32>>`. The check
`if task_cpu.is_none()` only entered tiers 2/3 when the RQ was uninitialized.
When the task was on a different CPU (result = `Some(None)`), tiers 2 and 3
were silently skipped. The parent process was never woken from waitpid,
causing the shell to hang permanently after every command.

## The Fix

```rust
// WRONG: only catches uninitialized RQ, not "task not found"
if task_cpu.is_none() { ... }

// CORRECT: collapses both layers
let task_found = task_cpu.flatten();
if task_found.is_none() { ... }
```

## Prevention

Any code using `with_rq()` where the closure returns `Option<T>` must use
`.flatten()` or pattern matching (`match result { Some(Some(v)) => ..., _ => ... }`)
before branching on the result.

The ISR-safe `try_wake_up()` already does this correctly with
`match result { Some(true) => ..., Some(false) => ..., None => ... }`.

-- CrashBloom: the subtlest type-system landmine in the whole scheduler
