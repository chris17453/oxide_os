## TODOs across codebase (prioritized)

### Phase 1 (immediate correctness / syscall plumbing)
- [x] userspace/coreutils/src/bin/touch.rs:167 — implement actual time syscall (uses gettimeofday)
- [x] userspace/coreutils/src/bin/df.rs — use statfs() syscall (now uses libc::statfs)
- [x] userspace/coreutils/src/bin/df.rs — get filesystem stats from statfs() (uses fsstat.f_files etc)
- [x] userspace/coreutils/src/bin/mv.rs:240 — implement proper stat-based update check (uses fstat/stat)
- [x] userspace/coreutils/src/bin/uname.rs — replace with actual uname() syscall (now uses libc::sys_uname)
- [x] crates/syscall/syscall/src/lib.rs — userspace memory access (copy_to_user/copy_from_user with STAC/CLAC)
- [ ] crates/syscall/syscall/src/time.rs:242 — implement proper sleep queue in scheduler for efficiency
- [ ] crates/syscall/syscall/src/time.rs:109 — track per-process/thread CPU time
- [x] crates/syscall/syscall/src/poll.rs — apply signal mask in ppoll (read_sigset/swap_signal_mask)
- [x] crates/syscall/syscall/src/poll.rs — apply signal mask in pselect6 (read_sigset/swap_signal_mask)
- [x] crates/syscall/syscall/src/socket.rs — handle IPv6 loopback check (parse_sockaddr_in6)
- [x] crates/net/tcpip/src/lib.rs:266 — send ICMP unreachable or TCP RST

### Phase 2 (filesystem correctness & metadata)
- [x] crates/fs/oxidefs/src/lib.rs — get current uid (get_current_uid helper)
- [x] crates/fs/oxidefs/src/lib.rs — get current gid (get_current_gid helper)
- [x] crates/fs/oxidefs/src/lib.rs — get current time for atime/mtime/ctime (get_current_time helper)
- [x] crates/vfs/vfs/src/file.rs — poll_read_ready/poll_write_ready for sockets/pipes (via VnodeOps trait)
- [x] crates/vfs/vfs/src/pipe.rs — poll_read_ready/poll_write_ready for pipe ends
- [x] crates/tty/tty/src/tty.rs — poll_read_ready/poll_write_ready for TTY
- [x] crates/tty/pty/src/lib.rs — poll_read_ready/poll_write_ready for PTY master/slave
- [x] crates/vfs/initramfs/src/lib.rs — handle symlinks and device nodes in CPIO loader

### Phase 3 (platform/hardware enablement)
- kernel/src/init.rs:215 — replace with proper ACPI MADT enumeration and AP boot
- crates/arch/arch-x86_64/src/lib.rs:362 — calibrate properly using APIC timer or HPET
- crates/smp/smp/src/cpu.rs:95 — read from per-CPU data via GS segment
- [x] crates/block/gpt/src/lib.rs — use entry.name_string() for partition names (via Box::leak)

### Phase 4 (user tools/parity)
- [x] userspace/coreutils/src/bin/more.rs — terminal size detection via tcgetwinsize ioctl
- userspace/coreutils/src/bin/ping.rs:183 — implement DNS resolution by calling nslookup logic
- apps/gwbasic/src/oxide_main.rs:133 — implement proper RTC reading via syscall

### Deferred (awaiting supporting kernel features)
- Statfs/uname/RTC/time syscalls depend on Phase 1/2 kernel timekeeping & syscall work; revisit after those land.
