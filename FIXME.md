# FIXME: Outstanding Implementation Gaps

- userspace/networkd/src/main.rs: Implement DHCP lease renewal handling and monitor interface state changes.
- crates/sched/sched/src/core.rs: Add cross-CPU reschedule IPIs and migrate tasks when their affinity/load requires moving off the current CPU.
- crates/arch/arch-x86_64/src/lib.rs: Calibrate timers using APIC timer or HPET instead of the current placeholder.
- kernel/src/init.rs: Resolve SMAP timing issue (AC flag clearing) and replace boot-time AP bring-up with proper ACPI MADT enumeration.
- crates/tty/vt/src/lib.rs: Notify the terminal emulator to switch screen buffer when changing the active VT.
- kernel/src/mount.rs: Complete remount semantics, implement real sysfs, and enforce open-file checks before remount/move.
- crates/syscall/syscall/src/socket.rs: Remove legacy BOUND_SOCKETS compatibility path and rely on the unified loopback registry.
- crates/syscall/syscall/src/time.rs: Track per-process/thread CPU time for PROCESS_CPUTIME_ID and THREAD_CPUTIME_ID clocks.
- crates/fs/ext4/src/inode.rs: Populate inode timestamps with real time instead of zero placeholders.
- crates/fs/ext4/src/vnode.rs: Create inodes with real uid/gid and set dtime using the real clock on deletion.
