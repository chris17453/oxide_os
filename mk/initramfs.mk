# — BlackLatch: Initramfs — the bootstrap, not the kitchen sink.
# This is the MINIMUM needed to get init running so it can pivot_root to the real ext4 root.
# After pivot, / is ext4 and the initramfs gets buried at /initramfs.
# EVERYTHING ELSE belongs on the rootfs. No exceptions.
#
# Boot flow:
#   1. UEFI loads bootloader from ESP
#   2. Bootloader loads kernel + initramfs from ESP
#   3. Kernel unpacks initramfs as initial /
#   4. init (PID 1) calls pivot_root("/mnt/root", "/mnt/root/initramfs")
#   5. ext4 root is now /, old initramfs at /initramfs
#   6. init reads /etc/fstab from ext4, mounts /boot, /home
#   7. servicemgr reads /etc/services.d/ from ext4, starts daemons
#   8. getty/login from ext4 provides user access

.PHONY: initramfs list-bins

# initramfs — minimal bootstrap to pivot_root into ext4
initramfs: $(INITRAMFS_PREREQ)
	@echo "Creating initramfs (bootstrap)..."
	@rm -rf $(TARGET_DIR)/initramfs
	@# — BlackLatch: Directory stubs for mount points before pivot.
	@mkdir -p $(TARGET_DIR)/initramfs/bin
	@mkdir -p $(TARGET_DIR)/initramfs/sbin
	@mkdir -p $(TARGET_DIR)/initramfs/etc
	@mkdir -p $(TARGET_DIR)/initramfs/dev
	@mkdir -p $(TARGET_DIR)/initramfs/dev/pts
	@mkdir -p $(TARGET_DIR)/initramfs/proc
	@mkdir -p $(TARGET_DIR)/initramfs/sys
	@mkdir -p $(TARGET_DIR)/initramfs/tmp
	@mkdir -p $(TARGET_DIR)/initramfs/var/log
	@mkdir -p $(TARGET_DIR)/initramfs/var/run
	@mkdir -p $(TARGET_DIR)/initramfs/var/lib
	@mkdir -p $(TARGET_DIR)/initramfs/boot
	@mkdir -p $(TARGET_DIR)/initramfs/home
	@mkdir -p $(TARGET_DIR)/initramfs/root
	@mkdir -p $(TARGET_DIR)/initramfs/run
	@mkdir -p $(TARGET_DIR)/initramfs/mnt
	@mkdir -p $(TARGET_DIR)/initramfs/initramfs
	@# — BlackLatch: init does the pivot. esh is the "oh shit" recovery shell.
	@# coreutils gives you ls/cat/mount if pivot fails at 3AM.
	@if [ -f "$(USERSPACE_OUT_RELEASE)/init" ]; then \
		cp "$(USERSPACE_OUT_RELEASE)/init" "$(TARGET_DIR)/initramfs/sbin/init"; \
		ln -sf /sbin/init "$(TARGET_DIR)/initramfs/init"; \
	fi
	@if [ -f "$(USERSPACE_OUT_RELEASE)/esh" ]; then \
		cp "$(USERSPACE_OUT_RELEASE)/esh" "$(TARGET_DIR)/initramfs/bin/esh"; \
		ln -sf /bin/esh "$(TARGET_DIR)/initramfs/bin/sh"; \
	fi
	@for prog in $(COREUTILS_BINS); do \
		if [ -f "$(USERSPACE_OUT_RELEASE)/$$prog" ]; then \
			cp "$(USERSPACE_OUT_RELEASE)/$$prog" "$(TARGET_DIR)/initramfs/bin/"; \
		fi; \
	done
	@ln -sf /bin/true "$(TARGET_DIR)/initramfs/bin/:" 2>/dev/null || true
	@ln -sf /bin/ls "$(TARGET_DIR)/initramfs/bin/dir" 2>/dev/null || true
	@# — BlackLatch: Bare minimum /etc. After pivot_root, ext4's /etc takes over.
	@echo "root:root:0:0:root:/root:/bin/esh" > $(TARGET_DIR)/initramfs/etc/passwd
	@echo "root:x:0:" > $(TARGET_DIR)/initramfs/etc/group
	@echo "export PATH=/bin:/sbin:/usr/bin:/usr/sbin" > $(TARGET_DIR)/initramfs/etc/profile
	@echo "OXIDE" > $(TARGET_DIR)/initramfs/etc/hostname
	@echo "127.0.0.1 localhost" > $(TARGET_DIR)/initramfs/etc/hosts
	@echo "::1 localhost" >> $(TARGET_DIR)/initramfs/etc/hosts
	@# — BlackLatch: Console keyboard config. Init reads this on boot.
	@echo "# Console keyboard layout (us, uk, de, fr)" > $(TARGET_DIR)/initramfs/etc/vconsole.conf
	@echo "KEYMAP=us" >> $(TARGET_DIR)/initramfs/etc/vconsole.conf
	@# — BlackLatch: Fallback fstab. If pivot succeeds, ext4's fstab wins.
	@echo "# initramfs fallback (ext4 fstab takes over after pivot)" > $(TARGET_DIR)/initramfs/etc/fstab
	@echo "proc   /proc  proc   defaults  0  0" >> $(TARGET_DIR)/initramfs/etc/fstab
	@echo "sysfs  /sys   sysfs  defaults  0  0" >> $(TARGET_DIR)/initramfs/etc/fstab
	@# Create CPIO archive
	@cd $(TARGET_DIR)/initramfs && find . | cpio -o -H newc > ../initramfs.cpio 2>/dev/null
	@echo "Initramfs created: $(INITRAMFS)"
	@ls -la $(INITRAMFS)

# List what goes where
list-bins:
	@echo "Initramfs (bootstrap only):"
	@echo "  init, esh (sh), coreutils"
	@echo ""
	@echo "Rootfs (ext4 — everything else):"
	@echo "  System:   login, getty, service/servicemgr"
	@echo "  Daemons:  journald, journalctl, networkd, resolvd, sshd, ssh, rdpd, soundd"
	@echo "  Apps:     gwbasic, curses-demo, htop, doom, python, vim"
	@echo "  Test:     evtest, argtest, tls-test, thread-test, signal-test, testcolors"
