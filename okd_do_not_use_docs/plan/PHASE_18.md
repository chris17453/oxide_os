# Phase 18: Hypervisor

**Stage:** 4 - Advanced
**Status:** Complete
**Dependencies:** Phase 9 (SMP)

---

## Goal

Implement Type-2 hypervisor using hardware virtualization.

---

## Deliverables

| Item | Status |
|------|--------|
| Hardware virtualization detection | [x] |
| VMCS/VGIC management | [x] |
| Nested page tables (EPT/Stage 2) | [x] |
| VM entry/exit handling | [x] |
| virtio device emulation | [x] |
| VM lifecycle (create/run/destroy) | [x] |

---

## Architecture Status

| Arch | Detection | VMM | NPT | virtio | Done |
|------|-----------|-----|-----|--------|------|
| x86_64 | [x] | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Hardware Virtualization Technologies

| Arch | Technology | Extensions |
|------|------------|------------|
| x86_64 | Intel VT-x | VMX, EPT, VPID |
| x86_64 | AMD-V | SVM, NPT, ASID |
| aarch64 | ARM VHE | EL2, Stage 2 |
| riscv | H-extension | HS/VS/VU modes |

---

## Hypervisor Architecture

```
┌─────────────────────────────────────────────────┐
│                  Guest VM                        │
│  ┌─────────────────────────────────────────┐    │
│  │         Guest OS (Linux, etc.)          │    │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐ │    │
│  │  │ Process │  │ Process │  │ Process │ │    │
│  │  └─────────┘  └─────────┘  └─────────┘ │    │
│  └─────────────────────────────────────────┘    │
│                       │                          │
│                  VM Exit                         │
│                       ▼                          │
├─────────────────────────────────────────────────┤
│              OXIDE Hypervisor                   │
│  ┌─────────────┐  ┌─────────────┐              │
│  │ VMCS/VGIC   │  │  EPT/Stage2 │              │
│  │ Management  │  │  Page Tables │              │
│  └─────────────┘  └─────────────┘              │
│  ┌─────────────────────────────────────────┐   │
│  │        virtio Device Emulation          │   │
│  │  (console, block, net)                  │   │
│  └─────────────────────────────────────────┘   │
├─────────────────────────────────────────────────┤
│                OXIDE Kernel                     │
└─────────────────────────────────────────────────┘
```

---

## Intel VT-x (VMX)

```rust
// VMCS fields (partial list)
mod vmcs {
    // Guest state
    pub const GUEST_CR0: u32 = 0x6800;
    pub const GUEST_CR3: u32 = 0x6802;
    pub const GUEST_CR4: u32 = 0x6804;
    pub const GUEST_RSP: u32 = 0x681C;
    pub const GUEST_RIP: u32 = 0x681E;
    pub const GUEST_RFLAGS: u32 = 0x6820;

    // Host state
    pub const HOST_CR0: u32 = 0x6C00;
    pub const HOST_CR3: u32 = 0x6C02;
    pub const HOST_RSP: u32 = 0x6C14;
    pub const HOST_RIP: u32 = 0x6C16;

    // Control fields
    pub const PIN_BASED_CONTROLS: u32 = 0x4000;
    pub const PROC_BASED_CONTROLS: u32 = 0x4002;
    pub const EXIT_CONTROLS: u32 = 0x400C;
    pub const ENTRY_CONTROLS: u32 = 0x4012;

    // EPT
    pub const EPT_POINTER: u32 = 0x201A;
}

// VMX instructions
fn vmxon(vmxon_region: PhysAddr) -> Result<()>;
fn vmclear(vmcs: PhysAddr) -> Result<()>;
fn vmptrld(vmcs: PhysAddr) -> Result<()>;
fn vmwrite(field: u32, value: u64) -> Result<()>;
fn vmread(field: u32) -> Result<u64>;
fn vmlaunch() -> !;
fn vmresume() -> !;
```

---

## VM Exit Reasons

| Exit Reason | Description | Handling |
|-------------|-------------|----------|
| 0 | Exception/NMI | Forward to guest or handle |
| 1 | External interrupt | Handle in host |
| 10 | CPUID | Emulate |
| 12 | HLT | Schedule away |
| 28 | Control register access | Emulate |
| 30 | I/O instruction | Emulate device |
| 48 | EPT violation | Handle page fault |
| 52 | VMCALL | Hypercall |

---

## Extended Page Tables (EPT)

```
┌─────────────────────────────────────────────────┐
│   Guest Virtual ──► Guest Physical ──► Host Physical
│                         │                  │
│                    Guest PT            EPT/Stage2
│                    (guest OS)         (hypervisor)
└─────────────────────────────────────────────────┘

EPT entry format (x86_64):
┌────────────────────────────────────────────────────┐
│ Bits 51:12: Host physical page frame number        │
│ Bit 7: Page size (1=2MB/1GB page)                  │
│ Bit 2: Execute (XD bit inverted if EPTP[6]=1)      │
│ Bit 1: Write permission                            │
│ Bit 0: Read permission                             │
└────────────────────────────────────────────────────┘
```

---

## VM Structure

```rust
pub struct VirtualMachine {
    /// VM identifier
    id: VmId,

    /// Virtual CPUs
    vcpus: Vec<Vcpu>,

    /// Guest physical memory
    memory: GuestMemory,

    /// EPT/Stage 2 page tables
    ept: EptPageTables,

    /// Emulated devices
    devices: Vec<Box<dyn VirtioDevice>>,

    /// State
    state: VmState,
}

pub struct Vcpu {
    /// VCPU identifier
    id: VcpuId,

    /// VMCS/VGIC structure
    vmcs: VmcsRegion,

    /// Guest register state (for exits)
    regs: VcpuRegs,

    /// Host thread running this VCPU
    thread: Option<ThreadId>,
}

pub enum VmState {
    Created,
    Running,
    Paused,
    Shutdown,
}
```

---

## virtio Device Emulation

```rust
pub trait VirtioDevice: Send + Sync {
    /// Device type (1=net, 2=block, 3=console, etc.)
    fn device_type(&self) -> u32;

    /// Handle configuration read
    fn read_config(&self, offset: u64, data: &mut [u8]);

    /// Handle configuration write
    fn write_config(&self, offset: u64, data: &[u8]);

    /// Process available virtqueue entries
    fn process_queue(&self, queue: u16) -> Result<()>;

    /// Reset device
    fn reset(&self);
}

// Emulated devices:
// - virtio-console: Guest serial console
// - virtio-blk: Guest disk (backed by host file)
// - virtio-net: Guest networking
```

---

## Key Files

```
crates/hypervisor/vmm/src/
├── lib.rs
├── vm.rs              # VM management
├── vcpu.rs            # VCPU handling
├── memory.rs          # Guest memory
├── exit.rs            # VM exit handling
└── device.rs          # Device emulation

crates/hypervisor/vmx/src/
├── lib.rs
├── vmcs.rs            # VMCS management
├── ept.rs             # EPT page tables
└── vmx.rs             # VMX instructions

crates/hypervisor/virtio-emu/src/
├── lib.rs
├── console.rs         # virtio-console
├── block.rs           # virtio-blk
└── net.rs             # virtio-net
```

---

## Syscalls/ioctls

| Name | Description |
|------|-------------|
| VM_CREATE | Create new VM |
| VM_DESTROY | Destroy VM |
| VCPU_CREATE | Create VCPU |
| VCPU_RUN | Run VCPU (blocking) |
| VM_SET_MEMORY | Set guest memory region |
| VM_GET_REGS | Get VCPU registers |
| VM_SET_REGS | Set VCPU registers |
| VM_INTERRUPT | Inject interrupt |

---

## Exit Criteria

- [x] VT-x/AMD-V/ARM EL2 detected and enabled
- [x] VM creates with guest memory
- [x] VCPU runs guest code
- [x] VM exits handled correctly
- [x] EPT/Stage 2 provides memory isolation
- [x] virtio-console works (guest prints to host)
- [x] Guest OS boots to serial prompt
- [ ] Works on all 8 architectures (x86_64 only for now)

---

## Test: Boot Guest

```c
int main() {
    // Create VM
    int vm_fd = open("/dev/vm", O_RDWR);
    ioctl(vm_fd, VM_CREATE, NULL);

    // Set up guest memory (1MB at 0x0)
    struct vm_memory mem = {
        .guest_phys = 0,
        .host_virt = mmap(NULL, 1024*1024, ...),
        .size = 1024*1024,
    };
    ioctl(vm_fd, VM_SET_MEMORY, &mem);

    // Load simple guest code at 0x1000
    // (code that prints 'H' to serial and halts)
    memcpy(mem.host_virt + 0x1000, guest_code, guest_size);

    // Create VCPU
    int vcpu_fd = ioctl(vm_fd, VCPU_CREATE, 0);

    // Set initial state
    struct vcpu_regs regs = {
        .rip = 0x1000,
        .rsp = 0x8000,
        .rflags = 0x2,
    };
    ioctl(vcpu_fd, VM_SET_REGS, &regs);

    // Run
    while (1) {
        struct vcpu_run run;
        ioctl(vcpu_fd, VCPU_RUN, &run);

        if (run.exit_reason == EXIT_HLT) {
            printf("Guest halted\n");
            break;
        }
        // Handle other exits...
    }

    close(vcpu_fd);
    close(vm_fd);
    return 0;
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 18 of OXIDE Implementation*
