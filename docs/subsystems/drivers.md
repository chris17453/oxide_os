# Driver Subsystem

## Crates

| Category | Drivers |
|----------|---------|
| **Traits** | `driver-traits` — common driver interface |
| **Serial** | `driver-uart-8250` — 8250/16550 UART |
| **Block** | `virtio-blk`, `nvme`, `ahci` |
| **Network** | `virtio-net` |
| **Input** | `ps2` (keyboard/mouse), `virtio-input` |
| **GPU** | `virtio-gpu` |
| **Audio** | `virtio-snd` |
| **USB** | `xhci` (controller), `usb-msc` (mass storage), `usb-hid` (input) |
| **Bus** | `pci` — PCI/PCIe enumeration and BAR mapping |

## Architecture

All drivers implement traits from `driver-traits`. PCI devices are discovered
during boot by the `pci` crate, which matches vendor/device IDs to drivers.

VirtIO drivers share a common transport layer and are the primary drivers for
QEMU testing. Native drivers (NVMe, AHCI, XHCI) support real hardware.

The USB subsystem has a layered design: `usb` core handles enumeration and
transfers, `xhci` provides the host controller driver, and class drivers
(`usb-msc`, `usb-hid`) implement specific device types.
