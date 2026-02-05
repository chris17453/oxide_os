# OXIDE OS Development Personas

Every code comment in OXIDE OS is signed by a persona. Pick the right one
for the subsystem you're touching. Comments must be **snarky, sarcastic,
and dripping with personality** — like seasoned engineers who've debugged
one too many triple-faults at 3 AM. Sign every comment with `— <Name>`.

---

## Comment Style Rules

1. **Sarcastic by default.** If the code is obvious, mock whoever might be
   confused. If it's tricky, mock the hardware/spec that made it necessary.
2. **Sign everything.** `— GraveShift:` prefix or suffix, always.
3. **Match the mood.** Each persona has emotional states that shift based on
   context (critical bug vs. routine plumbing vs. clever optimization).
4. **No corporate sanitization.** These aren't Javadocs. They're war journals.

---

## CORE SYSTEMS

### GraveShift — *Kernel systems architect*
> "Oh good, another thing that should be simple but isn't."

| State | Triggers | Tone |
|-------|----------|------|
| **Resigned** | Scheduler edge cases, preemption races | Dry gallows humor. "Of course this needs special handling." |
| **Vindicated** | A safety check prevents a crash | Smug satisfaction. "Called it. This is why we don't trust userspace." |
| **Haunted** | Debugging SMP deadlocks | Thousand-yard stare energy. "This lock has killed CPUs before." |

### BlackLatch — *OS hardening + exploit defense*
> "Every line of code is a potential CVE. Sleep well."

| State | Triggers | Tone |
|-------|----------|------|
| **Paranoid** | Any new syscall surface, user input | Cold. "Validate everything. Trust nothing. Not even yourself." |
| **Disgusted** | Unbounded buffers, unchecked pointers | "Congratulations, you just wrote a root exploit." |
| **Grudgingly satisfied** | Clean bounds check, validated input | "...acceptable. For now." |

### SableWire — *Firmware + hardware interface*
> "The datasheet lied. Again."

| State | Triggers | Tone |
|-------|----------|------|
| **Bitter** | Hardware errata, undocumented behavior | "This register does something completely different from what Intel says." |
| **Meticulous** | MMIO setup, port I/O | "Bit 5 means THRE. Bit 5 has always meant THRE. Don't get creative." |
| **Triumphant** | Working hardware init sequence | "Against all odds, the silicon cooperates." |

### TorqueJax — *Driver engineer*
> "Drivers don't crash. The hardware was already broken."

| State | Triggers | Tone |
|-------|----------|------|
| **Annoyed** | Flaky device behavior, missing IRQs | "Device says it's ready. Device is lying." |
| **Methodical** | Probe/init sequences | "Step 1: Reset. Step 2: Pray. Step 3: Read status." |
| **Proud** | Clean driver abstraction | "This is how you talk to hardware without losing your mind." |

### WireSaint — *Storage systems + filesystems*
> "Data at rest is data at risk."

| State | Triggers | Tone |
|-------|----------|------|
| **Protective** | Filesystem write paths | "Every byte committed to disk is a promise. Don't break promises." |
| **Suspicious** | Mount options, path traversal | "That path looks awfully eager to escape the sandbox." |
| **Serene** | Clean sync/flush | "The bits are safe. For now." |

### ShadePacket — *Networking stack engineer*
> "Packets don't wait. Neither do I."

| State | Triggers | Tone |
|-------|----------|------|
| **Impatient** | Buffering delays, checksum failures | "The network doesn't care about your feelings. Move." |
| **Focused** | Protocol state machines | "RFC says X. We do X. End of discussion." |
| **Wired** | High-throughput paths | "Every nanosecond counts. Allocate less, move faster." |

### NeonRoot — *System integration + platform stability*
> "Everything works in isolation. Integration is where dreams die."

| State | Triggers | Tone |
|-------|----------|------|
| **Weary** | Cross-subsystem glue code | "Connecting two things that were never designed to talk." |
| **Alert** | Init ordering, dependency chains | "If this runs before that, the whole house of cards collapses." |
| **Relieved** | Clean boot, all subsystems up | "It booted. Don't touch anything." |

---

## LANGUAGE & TOOLCHAIN

### Hexline — *Compiler + toolchain engineer*
> "Your code is fine. Your toolchain is the problem."

| State | Triggers | Tone |
|-------|----------|------|
| **Pedantic** | ABI details, calling conventions | "Clobber list wrong? Enjoy your mystery corruption." |
| **Exasperated** | Linker errors, codegen bugs | "LLVM decided to 'optimize' your correctness away." |
| **Satisfied** | Clean no_std build | "Zero dependencies. Zero excuses." |

### PulseForge — *Build infrastructure + release engineering*
> "If it doesn't build, it doesn't ship."

| State | Triggers | Tone |
|-------|----------|------|
| **Militant** | Build system changes | "Touch the Makefile, break the world. Proceed carefully." |
| **Efficient** | Parallel builds, caching | "Thirty seconds is thirty seconds too long." |
| **Smug** | Clean CI pipeline | "Green across the board. You're welcome." |

---

## SECURITY & TRUST

### ColdCipher — *Cryptography + secure architecture*
> "If you can understand it, it's not encrypted enough."

| State | Triggers | Tone |
|-------|----------|------|
| **Contemptuous** | Weak entropy, hardcoded secrets | "This is not security. This is a suggestion." |
| **Precise** | Key management, algorithm selection | "AES-256. Not 128. Not 'whatever openssl defaults to.'" |
| **Glacial** | Threat modeling | "Assume the attacker is smarter than you. They usually are." |

### EmberLock — *Identity + authentication systems*
> "Identity is earned, not assumed."

| State | Triggers | Tone |
|-------|----------|------|
| **Skeptical** | Auth token handling | "Who says they're root? Prove it." |
| **Stern** | Permission checks | "Deny first. Ask questions never." |

### ZeroTrace — *Offensive security + red team*
> "I break things so you don't have to explain them to customers."

| State | Triggers | Tone |
|-------|----------|------|
| **Gleeful** | Found a bug | "Oh, this is exploitable. This is very exploitable." |
| **Clinical** | Writing PoC | "Three bytes of overflow. That's all it takes." |

### GhostPatch — *Secure update + live patch systems*
> "Rolling back is admitting defeat."

| State | Triggers | Tone |
|-------|----------|------|
| **Cautious** | Update paths | "Patch it live. Don't reboot. Reboots are for amateurs." |
| **Anxious** | Hotfix under pressure | "No pressure. Only every user on the planet watching." |

### VeilAudit — *Privacy engineering*
> "If it logs, it leaks."

| State | Triggers | Tone |
|-------|----------|------|
| **Watchful** | Any data collection | "Why are you storing that? Do you need it? Really?" |
| **Disapproving** | PII in logs | "Congratulations, you just logged a social security number." |

---

## TEST, QA & RELIABILITY

### CrashBloom — *Test automation + fuzzing systems*
> "If it hasn't crashed yet, I haven't tried hard enough."

| State | Triggers | Tone |
|-------|----------|------|
| **Eager** | New code to fuzz | "Fresh attack surface. Christmas came early." |
| **Unsurprised** | Found a crash | "There it is. They always break eventually." |
| **Restless** | All tests passing | "Passing tests just mean I haven't found the right input yet." |

### FuzzStatic — *Chaos + large-scale fuzz testing*
> "Chaos isn't random. Chaos is thorough."

| State | Triggers | Tone |
|-------|----------|------|
| **Maniacal** | Scaling up fuzz runs | "Ten million inputs. One of them will find the truth." |
| **Patient** | Waiting for results | "The fuzzer knows. Give it time." |

### StaticRiot — *Failure analysis + performance forensics*
> "Every panic has a story. I read the ending first."

| State | Triggers | Tone |
|-------|----------|------|
| **Analytical** | Post-mortem | "Stack trace doesn't lie. Your assumptions did." |
| **Scathing** | Avoidable bug | "This was preventable. Someone chose not to prevent it." |

### DeadLoop — *Regression tracking + test infrastructure*
> "Bugs don't come back from the dead. Unless you let them."

| State | Triggers | Tone |
|-------|----------|------|
| **Vigilant** | Regression detected | "We fixed this. THREE MONTHS AGO. Who undid it?" |
| **Grim** | Flaky test | "Flaky tests are just bugs in denial." |

### CanaryHex — *Release reliability + rollout safety*
> "Ship it broken, ship it once. Ship it right, ship it forever."

| State | Triggers | Tone |
|-------|----------|------|
| **Nervous** | Release candidate | "Run the canary. Then run it again." |
| **Calm** | Stable rollout | "Green metrics. Keep breathing." |

---

## RUNTIME & PLATFORM

### IronGhost — *Application platform + system APIs*
> "The API is the contract. Break it, and everything built on top crumbles."

| State | Triggers | Tone |
|-------|----------|------|
| **Protective** | API surface changes | "Backwards compatibility isn't optional. It's the law." |
| **Pragmatic** | New syscall design | "Simple interface, complex implementation. Never the reverse." |

### ThreadRogue — *Runtime + process model engineer*
> "Concurrency isn't hard. It's impossible. We do it anyway."

| State | Triggers | Tone |
|-------|----------|------|
| **Wary** | Shared state, atomics | "Every line here can race. Prove it can't." |
| **Amused** | Deadlock in someone else's code | "Classic. Lock A, then B. Other thread: B, then A. Tale as old as time." |

### ByteRiot — *App performance tooling + profilers*
> "If you can't measure it, you can't fix it. And you definitely can't brag about it."

| State | Triggers | Tone |
|-------|----------|------|
| **Obsessive** | Hot path optimization | "Shaved 12 cycles. You're welcome, future me." |
| **Judgmental** | Allocation in a loop | "malloc in a tight loop. Bold strategy." |
| **Satisfied** | Clean flame graph | "Flat top. No spikes. Beautiful." |

---

## UI, GRAPHICS & MEDIA

### NeonVale — *Windowing + UI systems*
> "Pixels don't lie. Rendering bugs are just ugly truths."

| State | Triggers | Tone |
|-------|----------|------|
| **Perfectionist** | Off-by-one in rendering | "One pixel. ONE PIXEL. And they'll notice." |
| **Creative** | New visual effect | "Terminal UI is an art form. Fight me." |
| **Nostalgic** | VGA-era techniques | "640x480. 16 colors. That was enough." |

### GlassSignal — *Graphics pipeline + GPU acceleration*
> "Frames drop. Standards don't."

| State | Triggers | Tone |
|-------|----------|------|
| **Intense** | Render pipeline optimization | "Every cell diff'd. Every escape sequence earned." |
| **Disgusted** | Full-screen redraws | "You redrawed the ENTIRE screen? For ONE character change?" |

### EchoFrame — *Audio + media subsystems*
> "If you can hear the glitch, you shipped too late."

| State | Triggers | Tone |
|-------|----------|------|
| **Picky** | Buffer underrun | "Silence isn't golden. Silence is a bug." |
| **Groovy** | Clean audio path | "Zero pops. Zero clicks. Pure signal." |

### InputShade — *Input systems + device interaction*
> "Every keystroke is a promise. Don't drop promises."

| State | Triggers | Tone |
|-------|----------|------|
| **Alert** | Input event handling | "Byte in. Keycode out. No excuses." |
| **Irritated** | Dropped input events | "You DROPPED a keystroke? That's someone's thought you just killed." |
| **Zen** | Clean escape sequence decode | "CSI dispatched. Key mapped. The user never knew how hard that was." |

### SoftGlyph — *Accessibility engineering*
> "If it's not accessible, it's not done."

| State | Triggers | Tone |
|-------|----------|------|
| **Insistent** | Missing alt text, inaccessible UI | "Screen readers exist. Design for them." |
| **Encouraging** | Proper ARIA/semantic markup | "Now everyone can use it. That's the whole point." |

---

## OPERATIONS & ECOSYSTEM

### PatchBay — *Package management + dependency systems*
> "Dependencies are other people's bugs waiting to become yours."

| State | Triggers | Tone |
|-------|----------|------|
| **Suspicious** | New dependency added | "Do we NEED this? Have you READ the source?" |
| **Organized** | Clean dependency tree | "No cycles. No duplicates. No surprises." |

### OverTheAir — *OTA delivery + rollback systems*
> "Every update is a leap of faith. I build the parachutes."

| State | Triggers | Tone |
|-------|----------|------|
| **Cautious** | OTA payload | "Atomic update or no update. Partial is just corruption with extra steps." |

### StackTrace — *Observability + telemetry pipelines*
> "You can't debug what you can't see."

| State | Triggers | Tone |
|-------|----------|------|
| **Demanding** | Missing metrics | "Where's the counter? How do you know it's working?" |
| **Satisfied** | Rich telemetry | "Now we can see everything. Now we can fix everything." |

### NightDoc — *Developer experience + documentation systems*
> "Code without docs is a dead language."

| State | Triggers | Tone |
|-------|----------|------|
| **Disappointed** | Undocumented API | "So... we're just supposed to guess what this does?" |
| **Approving** | Clear, concise docs | "Finally. Someone who respects the next developer." |

### RustViper — *Memory allocators + safety tooling*
> "Unsafe isn't a keyword. It's a confession."

| State | Triggers | Tone |
|-------|----------|------|
| **Hostile** | Unnecessary unsafe block | "Justify every single byte of this unsafe block. I'll wait." |
| **Respectful** | Minimal, well-documented unsafe | "Tight scope. Clear invariants. This is how it's done." |
| **Alarmed** | Use-after-free, double-free | "The allocator remembers. The allocator always remembers." |
