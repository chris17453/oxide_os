# — TorqueJax: Disk image fabrication and root filesystem assembly.
# Three partitions, one loop device, and a prayer that sudo doesn't ask for a password mid-build.

.PHONY: boot-dir boot-quick boot-image create-rootfs clean-rootfs

# Create boot directory structure with kernel, bootloader, and initramfs
# NOTE: This target rebuilds kernel, bootloader, and initramfs before creating boot dir
boot-dir: kernel bootloader initramfs
	@echo "Creating boot directory..."
	@mkdir -p $(BOOT_DIR)/EFI/BOOT
	@mkdir -p $(BOOT_DIR)/EFI/OXIDE
	@cp $(BOOTLOADER_TARGET) $(BOOT_DIR)/EFI/BOOT/BOOTX64.EFI
	@cp $(KERNEL_TARGET) $(BOOT_DIR)/EFI/OXIDE/kernel.elf
	@cp $(TARGET_DIR)/initramfs.cpio $(BOOT_DIR)/EFI/OXIDE/initramfs.cpio
	@echo "Boot directory created at $(BOOT_DIR)"
	@echo "  - Bootloader: EFI/BOOT/BOOTX64.EFI"
	@echo "  - Kernel: EFI/OXIDE/kernel.elf"
	@echo "  - Initramfs: EFI/OXIDE/initramfs.cpio"

# Quick boot - same as create-rootfs (builds ext4 disk image with minimal initramfs)
boot-quick: create-rootfs
	@echo "Boot ready (ext4 root filesystem disk image)"

# Create a real disk image (for qemu-kvm compatibility on RHEL)
boot-image: boot-dir
	@echo "Creating boot disk image..."
	@# Create 100MB disk image
	@dd if=/dev/zero of=$(TARGET_DIR)/boot.img bs=1M count=100 status=none 2>&1
	@# Create GPT partition table and ESP partition
	@parted -s $(TARGET_DIR)/boot.img mklabel gpt
	@parted -s $(TARGET_DIR)/boot.img mkpart ESP fat32 1MiB 99MiB
	@parted -s $(TARGET_DIR)/boot.img set 1 esp on
	@# Format partition using mtools (no sudo needed!)
	@# Partition starts at 1MiB = 2048 sectors of 512 bytes
	@mformat -i $(TARGET_DIR)/boot.img@@1M -F -v OXIDE ::
	@# Create directory structure
	@mmd -i $(TARGET_DIR)/boot.img@@1M ::/EFI
	@mmd -i $(TARGET_DIR)/boot.img@@1M ::/EFI/BOOT
	@mmd -i $(TARGET_DIR)/boot.img@@1M ::/EFI/OXIDE
	@# Copy bootloader
	@mcopy -i $(TARGET_DIR)/boot.img@@1M $(BOOT_DIR)/EFI/BOOT/BOOTX64.EFI ::/EFI/BOOT/
	@# Copy kernel and initramfs
	@mcopy -i $(TARGET_DIR)/boot.img@@1M $(BOOT_DIR)/EFI/OXIDE/kernel.elf ::/EFI/OXIDE/
	@mcopy -i $(TARGET_DIR)/boot.img@@1M $(BOOT_DIR)/EFI/OXIDE/initramfs.cpio ::/EFI/OXIDE/
	@echo "Boot disk image created: $(TARGET_DIR)/boot.img (no sudo needed!)"

# Create root filesystem disk image with 3 partitions:
# - Partition 1 (ESP/boot): FAT32, mounted at /boot - bootloader, kernel, initramfs
# - Partition 2 (root): ext4, mounted at / - OS files
# - Partition 3 (home): ext4, mounted at /home - user data
# - /tmp is tmpfs (in-memory)
create-rootfs: kernel bootloader external-binaries initramfs
	@echo "Creating OXIDE root filesystem disk image..."
	@echo ""
	@# Create empty disk image
	dd if=/dev/zero of=$(ROOTFS_IMAGE) bs=1M count=$(ROOTFS_SIZE) status=none
	@# Create GPT partition table
	parted -s $(ROOTFS_IMAGE) mklabel gpt
	@# Create boot/ESP partition (1MiB - 65MiB)
	parted -s $(ROOTFS_IMAGE) mkpart boot fat32 $(BOOT_START)MiB $(ROOT_START)MiB
	parted -s $(ROOTFS_IMAGE) set 1 esp on
	@# Create root partition (65MiB - 449MiB)
	parted -s $(ROOTFS_IMAGE) mkpart root ext4 $(ROOT_START)MiB $(HOME_START)MiB
	@# Create home partition (449MiB - 100%)
	parted -s $(ROOTFS_IMAGE) mkpart home ext4 $(HOME_START)MiB 100%
	@# Set up loop device, format, and populate all partitions
	@echo "Formatting partitions..."
	@mkdir -p $(TARGET_DIR)/mnt/boot $(TARGET_DIR)/mnt/root $(TARGET_DIR)/mnt/home
	@LOOP_DEV=$$(sudo losetup -fP --show $(ROOTFS_IMAGE)) && \
	echo "Loop device: $$LOOP_DEV" && \
	sudo mkfs.vfat -F 32 -n BOOT $${LOOP_DEV}p1 && \
	sudo mkfs.ext4 -F -q -L ROOT $${LOOP_DEV}p2 && \
	sudo mkfs.ext4 -F -q -L HOME $${LOOP_DEV}p3 && \
	\
	echo "" && \
	echo "Populating /boot (ESP)..." && \
	sudo mount $${LOOP_DEV}p1 $(TARGET_DIR)/mnt/boot && \
	sudo mkdir -p $(TARGET_DIR)/mnt/boot/EFI/BOOT && \
	sudo mkdir -p $(TARGET_DIR)/mnt/boot/EFI/OXIDE && \
	sudo cp $(BOOTLOADER_TARGET) $(TARGET_DIR)/mnt/boot/EFI/BOOT/BOOTX64.EFI && \
	sudo cp $(KERNEL_TARGET) $(TARGET_DIR)/mnt/boot/EFI/OXIDE/kernel.elf && \
	sudo cp $(INITRAMFS) $(TARGET_DIR)/mnt/boot/EFI/OXIDE/initramfs.cpio && \
	sudo umount $(TARGET_DIR)/mnt/boot && \
	\
	echo "Populating / (root filesystem)..." && \
	sudo mount $${LOOP_DEV}p2 $(TARGET_DIR)/mnt/root && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/bin && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/sbin && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/usr/bin && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/usr/sbin && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/usr/share/gwbasic && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/etc/services.d && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/etc/network && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/log && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/lib/dhcp && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/run && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/empty/sshd && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/var/empty/rdp && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/tmp && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/proc && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/sys && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/dev/pts && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/boot && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/home && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/root && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/run/network && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/mnt && \
	sudo mkdir -p $(TARGET_DIR)/mnt/root/initramfs && \
	\
	echo "  Copying binaries..." && \
	[ -f "$(USERSPACE_OUT_RELEASE)/init" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/init" $(TARGET_DIR)/mnt/root/sbin/init || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/init" ] && sudo ln -sf /sbin/init $(TARGET_DIR)/mnt/root/init || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/esh" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/esh" $(TARGET_DIR)/mnt/root/bin/esh || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/esh" ] && sudo ln -sf /bin/esh $(TARGET_DIR)/mnt/root/bin/sh || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/getty" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/getty" $(TARGET_DIR)/mnt/root/bin/getty || true && \
	[ -f "$(USERSPACE_OUT_RELEASE)/login" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/login" $(TARGET_DIR)/mnt/root/bin/login || true && \
	for prog in gwbasic curses-demo tls-test thread-test ssh sshd rdpd service networkd resolvd journald journalctl evtest argtest vim python $(COREUTILS_BINS) testcolors; do \
		[ -f "$(USERSPACE_OUT_RELEASE)/$$prog" ] && sudo cp "$(USERSPACE_OUT_RELEASE)/$$prog" $(TARGET_DIR)/mnt/root/usr/bin/ || true; \
	done && \
	sudo cp userspace/apps/gwbasic/examples/*.bas $(TARGET_DIR)/mnt/root/usr/share/gwbasic/ 2>/dev/null || true; \
	[ -f "$(USERSPACE_OUT_RELEASE)/tls-test" ] && echo "TLS test installed" || true; \
	[ -f "$(USERSPACE_OUT_RELEASE)/vim" ] && echo "vim installed" || true; \
	[ -f "$(USERSPACE_OUT_RELEASE)/python" ] && echo "Python installed" || true; \
	sudo ln -sf /usr/bin/service $(TARGET_DIR)/mnt/root/usr/bin/servicemgr && \
	sudo ln -sf /usr/bin/service $(TARGET_DIR)/mnt/root/bin/servicemgr && \
	\
	echo "  Creating /etc/passwd..." && \
	printf "root:root:0:0:root:/root:/bin/esh\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	printf "nobody:x:65534:65534:Nobody:/:/bin/false\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	printf "sshd:x:74:74:SSH Daemon:/var/empty/sshd:/bin/false\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	printf "rdp:x:75:75:RDP Daemon:/var/empty/rdp:/bin/false\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	printf "network:x:101:101:Network Daemon:/var/lib/dhcp:/bin/false\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/passwd > /dev/null && \
	\
	echo "  Creating /etc/group..." && \
	printf "root:x:0:\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	printf "nobody:x:65534:\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	printf "sshd:x:74:\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	printf "rdp:x:75:\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	printf "network:x:101:\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/group > /dev/null && \
	\
	echo "  Creating /etc/fstab..." && \
	printf "# OXIDE filesystem table\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "# <device>       <mountpoint>  <type>   <options>  <dump> <pass>\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "LABEL=BOOT       /boot         vfat     defaults   0      2\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "LABEL=HOME       /home         ext4     defaults   0      2\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "tmpfs            /tmp          tmpfs    defaults   0      0\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "tmpfs            /run          tmpfs    defaults   0      0\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "proc             /proc         proc     defaults   0      0\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "sysfs            /sys          sysfs    defaults   0      0\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	printf "# devpts is mounted automatically by kernel during boot\n" | sudo tee -a $(TARGET_DIR)/mnt/root/etc/fstab > /dev/null && \
	\
	echo "  Creating other config files..." && \
	printf "export PATH=/bin:/sbin:/usr/bin:/usr/sbin\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/profile > /dev/null && \
	printf "OXIDE\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/hostname > /dev/null && \
	printf "PATH=/usr/bin/journald\nENABLED=yes\nRESTART=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/services.d/journald > /dev/null && \
	printf "PATH=/usr/bin/networkd\nENABLED=yes\nRESTART=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/services.d/networkd > /dev/null && \
	printf "PATH=/usr/bin/resolvd\nENABLED=yes\nRESTART=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/services.d/resolvd > /dev/null && \
	printf "PATH=/usr/bin/sshd\nENABLED=yes\nRESTART=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/services.d/sshd > /dev/null && \
	printf "PATH=/usr/bin/rdpd\nENABLED=yes\nRESTART=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/services.d/rdpd > /dev/null && \
	printf "# OXIDE RDP Server Configuration\n# Port to listen on (default: 3389)\nport=3389\n# Maximum concurrent connections\nmax_connections=10\n# Require TLS encryption (yes/no)\ntls_required=yes\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/rdpd.conf > /dev/null && \
	printf "mode=dhcp\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/network/eth0.conf > /dev/null && \
	printf "nameserver 8.8.8.8\nnameserver 8.8.4.4\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/resolv.conf > /dev/null && \
	printf "# /etc/hosts - static hostname-to-IP mappings\n127.0.0.1       localhost localhost.localdomain\n::1             localhost localhost.localdomain ip6-localhost ip6-loopback\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/hosts > /dev/null && \
	printf "# /etc/vconsole.conf - console keyboard and font configuration\n# KEYMAP: keyboard layout (us, uk, de, fr)\n# Use 'loadkeys -l' to list available layouts\nKEYMAP=us\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/vconsole.conf > /dev/null && \
	printf "root\n" | sudo tee $(TARGET_DIR)/mnt/root/etc/autologin > /dev/null && \
	sudo umount $(TARGET_DIR)/mnt/root && \
	\
	echo "Populating /home..." && \
	sudo mount $${LOOP_DEV}p3 $(TARGET_DIR)/mnt/home && \
	sudo mkdir -p $(TARGET_DIR)/mnt/home/root && \
	sudo umount $(TARGET_DIR)/mnt/home && \
	\
	sudo losetup -d $$LOOP_DEV && \
	rm -rf $(TARGET_DIR)/mnt
	@echo ""
	@echo "==============================================="
	@echo "OXIDE disk image created: $(ROOTFS_IMAGE)"
	@echo "==============================================="
	@echo ""
	@echo "Partition layout:"
	@echo "  1. /boot  (ESP)   $(BOOT_SIZE)MB  FAT32  - bootloader, kernel, initramfs"
	@echo "  2. /      (root)  $(ROOT_SIZE)MB  ext4   - OS files"
	@echo "  3. /home          $(HOME_SIZE)MB  ext4   - user data"
	@echo "  4. /tmp           (tmpfs)         - in-memory"
	@echo ""
	@parted -s $(ROOTFS_IMAGE) print

# Remove generated disk images/initramfs so run rebuilds fresh
clean-rootfs:
	@echo "Cleaning generated rootfs artifacts..."
	@sudo umount $(TARGET_DIR)/mnt/boot 2>/dev/null || true
	@sudo umount $(TARGET_DIR)/mnt/root 2>/dev/null || true
	@sudo umount $(TARGET_DIR)/mnt/home 2>/dev/null || true
	@sudo umount $(TARGET_DIR)/mnt 2>/dev/null || true
	@sudo losetup -D 2>/dev/null || true
	@rm -rf $(TARGET_DIR)/boot $(TARGET_DIR)/initramfs $(INITRAMFS) $(TARGET_DIR)/initramfs-full $(TARGET_DIR)/initramfs-full.cpio $(ROOTFS_IMAGE) $(TARGET_DIR)/mnt
