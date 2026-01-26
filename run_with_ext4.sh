#!/bin/bash
# Run OXIDE with an ext4 test disk attached

mkdir -p /tmp/qemu-oxide
export TMPDIR=/tmp/qemu-oxide

exec qemu-system-x86_64 \
    -machine q35 \
    -cpu qemu64,+smap,+smep \
    -m 256M \
    -bios /usr/share/edk2/ovmf/OVMF_CODE.fd \
    -drive format=raw,file=fat:rw:target/boot,if=none,id=disk \
    -device ide-hd,drive=disk \
    -drive id=ext4disk,file=test_disk.img,format=raw,if=none \
    -device virtio-blk-pci,drive=ext4disk \
    -device virtio-net-pci,netdev=net0 \
    -netdev user,id=net0 \
    -display none \
    -monitor none \
    -serial stdio \
    -no-reboot
