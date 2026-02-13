# Lock-Free Buffers MUST Live Outside Mutexes

— GraveShift: The lock-free buffer that was locked. Peak engineering.

## The Rule

**Lock-free data structures MUST be directly accessible without acquiring ANY mutex.** If you put a lock-free ring buffer inside a `Mutex<T>`, you've just made it a locked ring buffer with extra steps.

## The Bug

```rust
pub struct VtManager {
    vts: [Mutex<VtState>; NUM_VTS],  // VtState contains input_buffer
}

struct VtState {
    input_buffer: LockFreeRing,  // Lock-free... behind a Mutex. LOL.
    tty: Arc<Tty>,
}

// push_input() — called from ISR:
if let Some(vt) = self.vts[active].try_lock() {  // ← THE PROBLEM
    vt.input_buffer.push(ch);  // ← Never reached when lock is held
}
```

### What Happened

1. Shell calls `read()` on `/dev/tty1` → acquires VT mutex → waits for input
2. User types → keyboard IRQ fires → `push_input()` called
3. `try_lock()` returns `None` (read() holds the lock)
4. Keystroke silently dropped — the input that `read()` is WAITING FOR
5. User sees "typing registers but does nothing"

The read() function was holding the lock while waiting for input that could only arrive through a path that needed the same lock. A circular dependency — not a deadlock in the technical sense, but a deadlock of intent.

## The Fix

```rust
pub struct VtManager {
    vts: [Mutex<VtState>; NUM_VTS],
    input_rings: [LockFreeRing; NUM_VTS],  // OUTSIDE the mutex
}

// push_input() — no lock needed:
self.input_rings[active].push(ch);  // Direct atomic push

// read() — reference ring without lock:
let ring = &self.input_rings[vt_num];
while let Some(ch) = ring.pop() { ... }
```

## The Principle

| Pattern | Correct? | Why |
|---------|----------|-----|
| `Mutex<Struct { LockFreeRing }>` | NO | Lock gates the lock-free thing |
| `LockFreeRing` as sibling field | YES | Directly accessible |
| `unsafe { &*ptr }` to escape lock | FRAGILE | Works but why bother |
| Lock-free struct behind RwLock | NO | Same problem, different lock |

## Init Ordering (Companion Fix)

VT manager must be initialized BEFORE PS/2 keyboard IRQ is enabled:
1. `vt::init()` — creates VtManager, sets VT_MANAGER_PTR
2. `arch::init_ps2_keyboard()` — enables PS/2 controller, IRQs can fire
3. `set_keyboard_callback()` — connects IRQ 1 to PS/2 handler
4. `set_console_callback()` — connects kbd → vt::push_input_global

If `vt::init()` comes after PS/2 init, keystrokes during the gap hit
`push_input_global()` → `get_manager()` → `None` → silently dropped.
This race was exposed when serial trace cleanup made boot faster.

## Prevention

- If a field uses atomic operations for thread safety, it doesn't belong inside a Mutex
- ISR push paths must have ZERO lock acquisitions between IRQ entry and data delivery
- `try_lock()` in an ISR input path = silent data loss. Always.
- read()/poll() must never hold a lock that the data producer needs
- VT manager init must precede any input subsystem init that could generate keystrokes

---

— GraveShift: A lock-free buffer behind a lock is just a buffer with abandonment issues.
