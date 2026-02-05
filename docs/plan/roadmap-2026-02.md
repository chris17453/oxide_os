# OXIDE OS Roadmap (Feb–Mar 2026)

## Scope
Lightweight 4–6 week plan focusing on correctness, modern syscall surface, container enablers, and terminal UX. Derived from ANALv10, docs/gaps.md, and in-tree TODOs.

## Workstreams
1) **Correctness & timing (week 1)**  
   - Finish reschedule IPI wiring; calibrate TSC via APIC/HPET.  
   - Harden umount by checking open files; clean tmpfs UID/GID propagation; retire exec TLS temp hack.  
   - Add unit/integration coverage where feasible.

2) **Storage & mount hygiene (week 2)**  
   - Implement real sysfs; solidify remount semantics.  
   - Guard unmount with open-file checks.  
   - Syscall ergonomics: statx, openat2, renameat2, faccessat2, mknodat.

3) **Container primitives (week 3)**  
   - Add unshare, setns, clone3.  
   - pidfd_open/send_signal/getfd for safe supervision.  
   - Document namespace model; draft cgroup/seccomp design note (no broad catches).

4) **Event-driven I/O (week 4)**  
   - timerfd_create/settime/gettime, signalfd, epoll_pwait2.  
   - recvmmsg/sendmmsg, preadv2/pwritev2.  
   - Userspace validation loops to confirm readiness semantics.

5) **Terminal UX polish (parallel)**  
   - CSI t (state query), OSC 7/8 (cwd & hyperlinks), CSI b (REP), CSI Z (CBT), F13–F24.  
   - Ensure DECRQSS responses satisfy vim/tmux.

6) **Security groundwork (design)**  
   - prctl essentials, capget/capset shape.  
   - Seccomp design doc (feature-gated); avoid silent fallbacks.

## Notes
- Keep debug output gated via debug-* features; no raw serial writes.  
- Target x86_64 first; design for portability.  
- Prefer small, self-contained PRs with validation logs (make build/test).  
- Document any new syscalls/tests in relevant subsystem docs.
