# Toggle which userspace apps build and get bundled by make run/create-rootfs.
# Copy this to build/userspace-packages.mk and set 0/1 by removing entries.

# Cargo packages (comment out or remove entries to skip)
USERSPACE_PACKAGES := \
	init esh getty login coreutils ssh sshd rdpd service networkd journald journalctl \
	soundd evtest argtest htop doom gwbasic curses-demo

# Extra non-Cargo targets (built via dedicated Make rules)
USERSPACE_EXTRA_TARGETS := tls-test thread-test
