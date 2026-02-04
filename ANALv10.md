## Syscall coverage (snapshot)

- Dispatcher: `kernel/syscall/syscall/src/lib.rs` handles ~100 syscalls across process/thread, VFS/dirs, sockets, timers, poll/epoll, signals, memory (mmap/mprotect/mremap/brk), mount/pivot_root, random, and assorted extensions (eventfd2, epoll_create1/ctl/wait, copy_file_range, memfd_create, splice, sendfile, close_range, prlimit, getrusage, accept4, socketpair, arch_prctl, futex, set_tid_address).
- Broad Linux-compat layer: `*at` family (openat/mkdirat/unlinkat/renameat/readlinkat/linkat/symlinkat/fchmodat/fchownat/faccessat/utimensat/futimens) present; vectored I/O (readv/writev/preadv/pwritev); basic scheduling and nice; user/group id setters/getters; signals (sigaction/sigprocmask/sigsuspend/pause/sigaltstack) supported.
- Effectiveness: sufficient for POSIX-y userspace (init, shells, coreutils, networking daemons) with polling, epoll, sockets, mmap, futexes, and mount primitives implemented. Debug/compat hooks: ENOSYS fallback for unknown numbers; QUERY_MODULE explicitly ENOSYS.

## Not implemented / missing notable syscalls

- Process/thread control: `clone3`, `vfork`, `ptrace`, `prctl` (general), `unshare`, `setns`, `pidfd_*` (open/send_signal/getfd), `kcmp`.
- File system/FS metadata: `statx`, `openat2`, `renameat2`, `faccessat2`, `open_tree`/`move_mount`, `mount_setattr`, `chroot`, `mknod`/`mknodat`, `umount2` variants, `fanotify_*`, `inotify_*`.
- Time/timers: `clock_settime`, `timer_create`/`timer_settime`/`timer_gettime`/`timer_delete`, `timerfd_*`, `clock_adjtime`, `adjtimex`.
- I/O multiplexing and event delivery: `epoll_pwait`/`epoll_pwait2`, `signalfd`, `recvmmsg`/`sendmmsg`, `io_uring_*`, `aio` family (preadv2/pwritev2 also absent), `tee` (splice companion).
- Memory management: `mlock`/`mlock2`/`munlockall`, `madvise` present but no `process_vm_{readv,writev}`, `userfaultfd`, hugepage controls, POSIX shared memory (`shmget/shmat/shmdt/shmctl`) and `mmap`-based SysV IPC, `remap_file_pages`, `mprotect_key`.
- Security and sandboxing: `seccomp`, `bpf`, `landlock`, `keyctl`, `capset/capget`, `settimeofday`/`stime`.
- Misc: `ustat`, `quotactl`, `kexec_*`, `perf_event_open`, `rseq`, `getcpu`, `cacheflush` (arch), `rt_sigqueueinfo`/`rt_sigtimedwait` (only basic signal set provided), `getrandom` present but no `getentropy`.

## Gaps to prioritize

1. **Namespaces/containers**: add `unshare`, `setns`, `clone3`, pidfd operations for safer process management.
2. **Modern FS syscalls**: implement `statx`, `openat2`, `renameat2`, `faccessat2`, `mknodat`, `chroot`, `inotify/fanotify` for userland tooling.
3. **Timers & async I/O**: add `timerfd_*`, `epoll_pwait2`, `signalfd`, `recvmmsg/sendmmsg`, `preadv2/pwritev2`, `io_uring` or `kqueue`-style interface.
4. **Security/sandbox**: support `seccomp`, `bpf`, capabilities (`capget/capset`), `prctl` essentials.
5. **Memory control**: add `mlock*`, `userfaultfd`, shared-memory IPC, and hugepage hints.

## Effectiveness summary

Core POSIX workloads should function (process control, fork/exec/wait, mmap/futex, sockets, epoll/poll, mounts, signals), but advanced containerization, modern fs metadata, high-performance networking, and timerfd/signalfd-based event loops will require the missing syscalls above. Unknown numbers cleanly return `-ENOSYS`, so compatibility layers can feature-detect without crashing.***
