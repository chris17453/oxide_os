## Customize what gets built during `make run`

`make run` always rebuilds the kernel and disk image. You can optionally choose which userspace apps build and get bundled via a local config file.

Create `build/userspace-packages.mk` (copy from `build/userspace-packages.sample.mk`):

```make
# Comment out apps you don't want built or included
USERSPACE_PACKAGES := init esh getty login coreutils ssh sshd rdpd service networkd journald journalctl soundd evtest argtest htop doom gwbasic curses-demo
USERSPACE_EXTRA_TARGETS := tls-test thread-test
```

You can still control rebuild behavior with `build/run.local.mk` (ignored if missing):

```make
# Skip userspace rebuilds during make run; reuse existing binaries in target/.
RUN_BUILD_USERSPACE = 0

# Optionally trim userspace packages for quicker builds (keep essentials like init/esh/login/getty/service).
# USERSPACE_PACKAGES = init esh getty login service networkd journald journalctl
```

Remove or set the flag to `1` to rebuild userspace again.
