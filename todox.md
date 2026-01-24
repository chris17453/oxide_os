## TODOs across codebase (prioritized)

### Phase 1 (immediate correctness / syscall plumbing)
- [x] userspace/coreutils/src/bin/touch.rs:167 — implement actual time syscall (uses gettimeofday)
- [x] userspace/coreutils/src/bin/df.rs — use statfs() syscall (now uses libc::statfs)
- [x] userspace/coreutils/src/bin/df.rs — get filesystem stats from statfs() (uses fsstat.f_files etc)
- [ ] userspace/coreutils/src/bin/mv.rs:240 — implement proper stat-based update check
- [x] userspace/coreutils/src/bin/uname.rs — replace with actual uname() syscall (now uses libc::sys_uname)
- [ ] crates/syscall/syscall/src/lib.rs:765 — fix userspace memory access properly
- [ ] crates/syscall/syscall/src/time.rs:242 — implement proper sleep queue in scheduler for efficiency
- [ ] crates/syscall/syscall/src/time.rs:109 — track per-process/thread CPU time
- [x] crates/syscall/syscall/src/poll.rs — apply signal mask in ppoll (read_sigset/swap_signal_mask)
- [x] crates/syscall/syscall/src/poll.rs — apply signal mask in pselect6 (read_sigset/swap_signal_mask)
- [x] crates/syscall/syscall/src/socket.rs — handle IPv6 loopback check (parse_sockaddr_in6)
- [x] crates/net/tcpip/src/lib.rs:266 — send ICMP unreachable or TCP RST

### Phase 2 (filesystem correctness & metadata)
- crates/fs/oxidefs/src/lib.rs:581 — get current uid
- crates/fs/oxidefs/src/lib.rs:582 — get current gid
- crates/fs/oxidefs/src/lib.rs:584 — get current time for atime
- crates/fs/oxidefs/src/lib.rs:609 — set inode mtime to current time
- crates/fs/oxidefs/src/lib.rs:656 — set inode mtime to current time
- crates/fs/oxidefs/src/lib.rs:1055 — set inode ctime to current time
- crates/vfs/vfs/src/file.rs:197 — for sockets/pipes, check if data is available in buffer
- crates/vfs/vfs/src/file.rs:207 — for sockets/pipes, check if buffer has space
- crates/vfs/initramfs/src/lib.rs:339 — handle symlinks, devices, etc.

### Phase 3 (platform/hardware enablement)
- kernel/src/init.rs:215 — replace with proper ACPI MADT enumeration and AP boot
- crates/arch/arch-x86_64/src/lib.rs:362 — calibrate properly using APIC timer or HPET
- crates/smp/smp/src/cpu.rs:95 — read from per-CPU data via GS segment
- crates/block/gpt/src/lib.rs:323 — use entry.name_string() (requires name &'static)

### Phase 4 (user tools/parity)
- userspace/coreutils/src/bin/more.rs:124 — implement proper terminal size detection when ioctl is available
- userspace/coreutils/src/bin/ping.rs:183 — implement DNS resolution by calling nslookup logic
- apps/gwbasic/src/oxide_main.rs:133 — implement proper RTC reading via syscall

### Deferred (awaiting supporting kernel features)
- Statfs/uname/RTC/time syscalls depend on Phase 1/2 kernel timekeeping & syscall work; revisit after those land.
