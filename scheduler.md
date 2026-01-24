Here’s a **full scheduler spec** you can implement that matches Linux’s mental model (task_struct + per-cpu rq + classes + preemption points), without hand-wavy “minimalist” stuff.

---

## 0. Goals and non-goals

**Goals**

* Schedules **tasks (threads)**, not “processes” ... processes are resource containers.
* Supports **SMP**, **preemption**, **sleep/wakeup**, **CPU affinity**, and **multiple scheduling classes**.
* Deterministic invariants ... no double-enqueue, no switching in unsafe contexts, no queue corruption.
* Clear **entry points** into scheduling ... block, wake, tick, yield, preempt.

**Non-goals**

* Copying Linux internals 1:1. This is a clean spec that implements the same core design.

---

## 1. Core concepts and invariants

### 1.1 Task object (schedulable entity)

A task is a kernel object representing a runnable thread (user or kernel thread).

**Hard invariants**

* A task is in exactly one of these states regarding run queues:

  * `is_current == true` on exactly one cpu
  * on exactly one cpu runqueue
  * not runnable (sleeping, stopped, dead) and on no runqueue
* A task is never enqueued twice.
* A task never appears on two cpus at once.
* All runqueue mutations are serialized by that cpu’s runqueue lock (and interrupt rules).

### 1.2 Per-cpu runqueue (rq)

Each cpu has an `rq` holding:

* pointer to current task
* runnable counts
* per-class queues
* accounting clocks
* resched flags

### 1.3 Scheduling classes (like Linux)

Pick order is strict:

1. deadline (optional)
2. realtime
3. fair
4. idle

Each class provides a vtable:

* `enqueue_task(rq, p, flags)`
* `dequeue_task(rq, p, flags)`
* `pick_next_task(rq, prev, flags)`
* `task_tick(rq, p)`
* `wakeup_preempt(rq, p, sync)`
* `yield_task(rq)`
* optional `balance()` hooks

---

## 2. Task state machine

### 2.1 Task run states

Use at least:

* `TASK_RUNNING` (means runnable ... can be current or queued)
* `TASK_INTERRUPTIBLE` (sleeping, wakeable by events)
* `TASK_UNINTERRUPTIBLE` (sleeping, not interrupted by signals)
* `TASK_STOPPED`
* `TASK_DEAD`

### 2.2 Transition rules

* **block**: running ... set sleep state ... dequeue (if queued) ... call schedule
* **wakeup**: set `TASK_RUNNING` ... enqueue on target cpu rq ... maybe preempt
* **exit**: mark dead ... remove from all scheduler structures ... schedule

---

## 3. Scheduler entry points (how it is accessed)

These are the only legitimate ways to enter scheduling.

### 3.1 Voluntary switch

* `schedule()` called when current blocks or yields

### 3.2 Preemptive switch

Triggered by setting `need_resched` and later honoring it at safe points:

* timer tick and timeslice / vruntime checks
* wakeup of higher priority task
* return-from-interrupt
* return-to-user
* kernel preemption point (if enabled)

**Rule**: do not context switch directly inside hard irq context ... set flags, switch on irq exit.

---

## 4. Locking and context rules (this is where crashes usually come from)

### 4.1 Per-cpu runqueue locking

* Each rq has a spinlock `rq_lock`.
* Any enqueue/dequeue/pick/migrate touching rq structures must hold `rq_lock`.

### 4.2 Interrupt rules

When you acquire `rq_lock` on a cpu, you must also prevent local irq reentry that could call tick or wake paths:

* `rq_lock_irqsave(rq, flags)` style ... disable local irqs while locked.

### 4.3 Preemption disable regions

You must not schedule while:

* holding `rq_lock`
* holding any non-preemptible locks if your kernel requires it
* in an atomic section
* in hard irq context
* on an incomplete stack frame transition

Have counters:

* `preempt_count`
* `in_irq`
  and make `schedule()` assert they are safe.

---

## 5. Core data structures

### 5.1 task_struct fields (scheduler-relevant)

Minimum for full-blown:

* identity:

  * `pid`, `tgid`
* state:

  * `volatile task_state`
  * `on_rq` (bool)
  * `on_cpu` (bool)
* cpu placement:

  * `cpu` (current cpu if on_cpu)
  * `last_cpu`
  * `cpus_allowed` mask
* class and policy:

  * `sched_class *class`
  * `policy` (fair, rr, fifo, deadline)
  * `static_prio`, `rt_prio`, `nice`, `weight`
* preemption:

  * `need_resched` (flag)
  * `preempt_count` (counter)
* accounting:

  * `exec_start` (timestamp when last started running)
  * `sum_exec_runtime`
* fair:

  * `vruntime`
  * rb_node or heap index
* rt:

  * list node for its prio queue
  * `time_slice` for rr
* deadline (if you do it):

  * `dl_runtime`, `dl_deadline`, `dl_period`
  * `dl_bw` accounting fields

### 5.2 rq fields

* `cpu_id`
* `curr` pointer
* `idle` task pointer
* `nr_running` counts
* `clock` and `clock_task`
* `need_resched` flag
* class runqueues:

  * `cfs_rq` (rb tree root, min pointer, totals)
  * `rt_rq` (bitmap + prio arrays + counts)
  * `dl_rq` (tree/heap)
* `rq_lock`

---

## 6. Scheduling class specs

### 6.1 realtime class (what you called RTTQ)

This is Linux’s `rt_rq` idea ... per-cpu realtime task queue.

**Data structure**

* `rt_array`:

  * `queue[0..MAX_RT_PRIO-1]` ... list of tasks per priority
  * `bitmap` ... bit set if queue[i] non-empty
* `rt_nr_running`

**Pick next**

* highest priority set bit in bitmap
* choose head of that list

**FIFO**

* no timeslice expiration
* runs until block, yield, or preempted by higher priority

**RR**

* has `time_slice`
* tick decrements slice
* on 0 ... move to tail of same-priority queue and reset slice

**Preemption rules**

* RT always preempts fair.
* Higher-prio RT preempts lower-prio RT immediately at preemption points.

### 6.2 fair class (CFS-like)

**Data structure**

* per-cpu rb tree keyed by `vruntime` (or a binary min-heap if you prefer)
* `min_vruntime`

**Weighting**

* Map nice ... weight (table or formula)
* Runtime charging:

  * `delta_exec = now - p.exec_start`
  * `p.vruntime += delta_exec * (NICE_0_WEIGHT / p.weight)`

**Pick next**

* leftmost rb node (smallest vruntime)

**Preemption**

* On wakeup, if `p.vruntime` is sufficiently smaller than current’s vruntime, set resched.
* On tick, if current ran longer than latency target relative to peers, set resched.

**Time parameters**

* `sched_latency` (target period to run everyone once)
* `min_granularity` (don’t preempt too fast)
* `wakeup_granularity` (how much advantage a waking task needs)

Implement these as constants first ... tune later.

### 6.3 deadline class (if you truly mean full blown)

If you include it, do EDF + CBS style constraints.

**Task params**

* runtime (budget) `dl_runtime`
* period `dl_period`
* relative deadline `dl_deadline`

**Queue**

* ordered by earliest absolute deadline (tree or heap)

**Rules**

* A deadline task is runnable only if it has remaining budget in current period.
* Replenish budget each period.
* Enforce admission control ... total utilization per cpu must be <= limit (like 0.95).

If you skip deadline initially, do RT + fair first ... deadline is a major complexity spike.

---

## 7. The schedule() spec

### 7.1 schedule() contract

* Called with current task in a safe context (not in hard irq, not holding forbidden locks).
* Must:

  * update accounting for current
  * choose next task using class order
  * context switch if next != current
  * return in context of next task

### 7.2 Pseudocode

```c
void schedule(void)
{
    struct rq *rq = this_rq();
    unsigned long flags;
    struct task_struct *prev;
    struct task_struct *next;

    rq_lock_irqsave(rq, flags);

    prev = rq->curr;

    if (!can_schedule_now(prev)) {
        rq_unlock_irqrestore(rq, flags);
        return;
    }

    update_curr_accounting(rq, prev);

    if (prev->state != TASK_RUNNING && prev->on_rq) {
        dequeue_task(rq, prev, 0);
    }

    next = pick_next_task(rq, prev);

    clear_need_resched(prev);
    rq->curr = next;
    next->on_cpu = true;
    prev->on_cpu = false;

    context_switch(prev, next);

    rq_unlock_irqrestore(rq, flags);
}
```

### 7.3 pick_next_task rules

* If there are runnable deadline tasks ... pick deadline next
* Else if runnable rt ... pick rt next
* Else if runnable fair ... pick fair next
* Else idle

---

## 8. Tick handling and preemption

### 8.1 Timer tick entry

Tick runs in irq context ... do not switch directly.

**tick handler does**

* update rq clocks
* call `curr->class->task_tick(rq, curr)`
* if `TIF_NEED_RESCHED` set, ensure it is honored on irq exit

### 8.2 Pseudocode

```c
void scheduler_tick(void)
{
    struct rq *rq = this_rq();
    struct task_struct *p = rq->curr;

    rq->clock = read_clock();
    update_curr_accounting(rq, p);

    p->class->task_tick(rq, p);

    if (rq->need_resched) {
        set_tif_need_resched(p);
    }
}
```

### 8.3 Where the switch actually happens

* on irq exit path:

  * if `tif_need_resched` and `preempt_count == 0` ... call `schedule()`

---

## 9. Sleep and wakeup

### 9.1 Blocking

**block path must**

* set task state away from RUNNING
* call `schedule()`

Do not leave the task on a runqueue when sleeping.

### 9.2 Wakeup

Wakeup is the hardest part on SMP.

**wakeup flow**

1. choose a target cpu ... respect affinity
2. lock that cpu rq
3. set `TASK_RUNNING`
4. enqueue on that rq
5. decide if it should preempt current on that cpu
6. if preempt ... set that cpu’s resched flag (IPI if remote)

### 9.3 Target cpu selection (spec)

Order of heuristics:

* if `sync_wakeup` and waker cpu allowed and not overloaded ... prefer waker cpu (cache locality)
* else if last_cpu allowed and lightly loaded ... prefer last_cpu
* else choose least loaded allowed cpu (simple metric)
* if cpu is idle ... choose it

### 9.4 Preempt decision

* If waking task is higher class ... preempt
* If same class:

  * rt ... higher prio preempts
  * fair ... compare vruntime gap against wakeup_granularity

---

## 10. SMP migration and load balancing

### 10.1 Ownership rule

A task “belongs” to one rq at a time.

To migrate:

* dequeue under source rq lock
* change p->cpu
* enqueue under destination rq lock

Avoid deadlocks:

* always lock rq in cpu id order when holding two locks

### 10.2 Balancing triggers

* periodic balance (every N ticks)
* idle cpu pulls work
* wakeup can push task to a better cpu

### 10.3 Load metric

Use something simple first, then improve:

* fair load ... sum of weights of runnable fair tasks
* rt load ... count runnable rt tasks weighted by priority
* total ... combined metric

---

## 11. Context switch mechanics

### 11.1 Must-do steps

* save prev cpu regs
* switch kernel stack pointer
* switch address space if needed (user tasks)
* restore next regs
* return into next

### 11.2 Accounting hooks

* `prev.exec_start` updated when it begins running
* on switch out ... charge delta runtime
* on switch in ... set new exec_start

---

## 12. Flags and fast paths

### 12.1 need_resched plumbing

Per-task flag `tif_need_resched` plus per-rq `need_resched`.

**Set it when**

* tick says slice expired or fairness demands
* wakeup says preempt

**Clear it when**

* schedule chooses next and commits

### 12.2 Fast path goals

* waking a task should not do heavy work under lock
* pick_next should be O(1) for rt, O(log n) for fair rb tree

---

## 13. Correctness assertions (put these everywhere)

These catch the “crashes all the time” issues immediately.

* `assert(!p->on_rq || p->state == TASK_RUNNING)`
* `assert(!(p->on_cpu && p->on_rq))` if you keep current off rq
* `assert(p->cpu == rq->cpu_id)` when enqueuing
* `assert(rq_lock_held(rq))` in enqueue/dequeue
* `assert(!in_irq() || !doing_context_switch())`
* `assert(preempt_count == 0)` at schedule entry

Also add runqueue consistency checks in debug builds:

* count nodes in rb tree equals `cfs_nr_running`
* bitmap bits match non-empty rt queues

---

## 14. Implementation checklist (full-blown order that actually works)

1. Single cpu ... fair + idle ... sleep/wake ... no preemption
2. Add tick preemption for fair (granularity rules)
3. Add rt class (fifo + rr)
4. Add SMP with per-cpu rq locks and remote wake IPI resched
5. Add load balancing (idle pull + periodic)
6. Add deadline only after the above is stable

---

If you want this to stop blowing up, the two biggest “spec compliance” points are:

* **never switch in hard irq** ... only set resched and switch on irq exit
* **rq_lock_irqsave around every queue mutation** ... tick and wakeups will otherwise corrupt your structures

Linux scheduling and “foreground vs background apps” are mostly **two different layers**.

The **kernel scheduler** does not inherently know “this is a foreground app”. It only knows **tasks** and a set of **attributes** that user space (or a policy daemon) sets. Foreground/background is implemented by **assigning different scheduling parameters** to the tasks that belong to each app.

## How Linux relates foreground vs background to scheduling

### What the kernel actually schedules

* Linux schedules **threads (tasks)**.
* Each task has:

  * **class/policy**: fair (CFS), realtime (FIFO/RR), deadline, idle
  * **priority/weight** (nice weight for CFS, fixed prio for RT)
  * **CPU affinity** (which CPUs it may run on)
  * optional **group membership** (cgroups) that changes how CPU time is divided

### How “foreground” is expressed in Linux terms

Foreground/background maps to one or more of these knobs:

1. **CFS weight (nice)**

   * Foreground ... higher weight (lower nice) ... gets more CPU share and tends to get scheduled sooner.
   * Background ... lower weight (higher nice).

2. **Latency vs throughput behavior**

   * Interactive behavior comes from CFS heuristics plus your chosen `sched_latency`, `min_granularity`, `wakeup_granularity`.
   * Foreground tasks tend to wake frequently ... scheduler tries to keep wakeup latency low.
   * Background tasks usually run longer bursts ... scheduler treats them more like throughput work.

3. **cgroups CPU controller**

   * Foreground and background are often placed into different **cgroups**.
   * You then control:

     * relative share (cpu.weight)
     * hard caps (cpu.max)
     * optional throttling behavior
   * This is the most common “real” mechanism for app-level policy.

4. **cpusets / affinity**

   * Foreground ... allowed on faster cores (or more cores).
   * Background ... restricted to a subset of CPUs (or little cores).
   * Especially common on mobile.

5. **utilization clamping (uclamp)**

   * Foreground ... higher minimum clamp ... encourages higher CPU frequency and more aggressive scheduling.
   * Background ... lower maximum clamp ... limits boost.

6. **RT or deadline (rare for general apps)**

   * Used for specific audio, video, compositor threads ... not “the whole app”.

So “foreground vs background” is really a **policy decision** that changes those parameters.

## What you should build in your Rust scheduler to support it

If you want Linux-like behavior that supports foreground/background cleanly, implement these features in-kernel:

### 1. Scheduling classes (must-have)

* deadline (optional at first)
* realtime (FIFO/RR) with fixed priorities
* fair (CFS-like) with weights and vruntime
* idle

### 2. Group scheduling (cgroup-like) if you want “apps” to matter

This is the big one.

* Represent an “app” as a **sched_group**.
* Put each task in exactly one group.
* Your fair scheduler becomes hierarchical:

  * per-cpu rq
  * per-group fair rq
  * per-task fair rq inside the group
* Distribute CPU time by group weight, then by task weight inside group.

This is how you make “foreground app gets more CPU than background app” without hacking per-task priorities everywhere.

### 3. Policy interface

You need a clean API so user space (or a system service) can say:

* “this app is foreground now”
* “this app is background now”

That API should map to:

* group weight changes
* group caps
* task nice changes (optional)
* affinity/cpuset changes (optional)
* uclamp min/max changes (optional)

## Concrete mapping you can use

A simple, effective mapping:

* **Foreground group**

  * higher group weight
  * no CPU cap
  * broader cpuset
  * optional higher uclamp_min

* **Background group**

  * lower group weight
  * optional CPU cap (so it can’t steal cores)
  * narrower cpuset
  * optional lower uclamp_max

Your scheduler stays “dumb” in the good way ... it just enforces weights, caps, and affinities.

## Bottom line

* The scheduler does not track “foreground/background” as a concept.
* Foreground/background is implemented by changing **weights, caps, affinity, and clamps**, ideally via **group scheduling**.
* If you want this to scale to real systems, implement **cgroup-like hierarchical fair scheduling** plus RT on top.

