# OXIDE OS Documentation Index

## Architecture

- [System Overview](architecture/overview.md) — high-level architecture diagram
- [Boot Flow](architecture/boot-flow.md) — UEFI boot sequence and filesystem mounting
- [Boot Protocols](architecture/boot-protocols.md) — kernel/bootloader handoff protocol
- [Userspace Architecture](architecture/userspace.md) — userspace program model
- [Assembly Inventory](architecture/assembly-inventory.md) — x86_64 assembly modules

### Porting

- [Porting Guide](architecture/porting/guide.md) — how to add a new architecture
- [MIPS64 Notes](architecture/porting/mips64-notes.md) — SGI MIPS64 platform notes
- [Migration Log](architecture/porting/migration-log.md) — x86_64 → multi-arch migration history

## Kernel Subsystems

- [Memory Management](subsystems/memory.md)
- [Scheduling & Processes](subsystems/scheduling.md)
- [Filesystem](subsystems/filesystem.md)
- [Networking](subsystems/networking.md)
- [Drivers](subsystems/drivers.md)
- [Security](subsystems/security.md)
- [Terminal & TTY](subsystems/terminal.md)
- [Containers](subsystems/containers.md)

## Development

- [Building](development/building.md)
- [Testing](development/testing.md)
- [Debugging](development/debugging.md)
- [Toolchain](development/toolchain.md)

## Applications

- [GW-BASIC](apps/gwbasic.md) — GW-BASIC interpreter analysis

## References

- [Syscall Analysis v1](references/syscall-analysis-v1.md) — EXIT syscall investigation
