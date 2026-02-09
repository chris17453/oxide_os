# Dynamic Kernel Module Loading for OXIDE OS

## Context

The user wants to implement Linux-style dynamic kernel module loading where:
- Kernel modules can be loaded and unloaded at runtime
- PCI device enumeration automatically matches devices to drivers
- Drivers probe devices and load only if hardware is present
- Unused modules can be unloaded to free memory

Currently, OXIDE OS drivers are **statically linked** into the kernel binary and initialized via hardcoded calls in `kernel/src/init.rs`. This plan introduces a dynamic driver loading system that matches the Linux model.

**Key Discovery**: OXIDE OS already has a **complete kernel module loading system** (`kernel/module/`) with ELF parsing, x86_64 relocation, symbol resolution, and dependency management. We only need to bridge this with the driver subsystem.

## Current State

### What Exists

✅ **Module Loading Infrastructure** (`kernel/module/`)
- Full ELF relocatable object (.ko) loader
- x86_64 relocation support (R64, PC32, PC64, GOTPCREL, etc.)
- Kernel symbol exports and resolution
- Dependency resolution with cycle detection
- Module lifecycle management (Coming → Live → Going → Unloaded)
- `module!()` macro for defining modules

✅ **PCI Enumeration** (`kernel/drivers/pci/`)
- Bus scanning and device detection
- BAR mapping and capability parsing
- VirtIO PCI transport support
- Device lookup functions (find_virtio_net, find_virtio_blk, etc.)

✅ **Subsystem Registration**
- Block devices: `block::register_device(name, Box<dyn BlockDevice>)`
- Network devices: `net::register_device(Arc<dyn NetworkDevice>)`
- Input devices: `input::register_device(Arc<dyn InputDevice>)`
- Audio devices: `audio::register_device(Arc<dyn AudioDevice>)`

✅ **Driver Organization**
- Drivers organized by category (net, block, gpu, audio, input, usb, serial)
- Each driver is a separate Cargo crate
- VirtIO drivers share common patterns (virtqueues, PCI caps)

### What's Missing

❌ **No Driver-Device Matching**
- No device ID tables (vendor:device → driver mapping)
- No automatic probe on PCI enumeration
- Drivers manually called in init.rs

❌ **No Unified Driver Interface**
- Each driver has different entry points (from_pci, init_from_pci, probe_all_pci)
- No standard probe/remove lifecycle
- No hot-plug support

❌ **No Driver Registry**
- No central list of available drivers
- No way to query what drivers are loaded
- No /proc/modules equivalent

❌ **Code Duplication**
- VirtIO drivers duplicate virtqueue management (~500 LOC each)
- Each driver implements its own PCI capability parsing

## Proposed Solution

### Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Layer 1: Driver Core (driver-core crate)              │
│  - PciDriver/IsaDriver traits                          │
│  - DriverRegistry (device ID matching)                 │
│  - DeviceBinding (tracks device → driver associations) │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│  Layer 2: VirtIO Core (virtio-core crate)              │
│  - Shared virtqueue management                         │
│  - VirtioPciTransport (extracted from pci crate)       │
│  - Common feature negotiation                          │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│  Layer 3: Individual Drivers                           │
│  - Implement PciDriver trait                           │
│  - Use virtio-core shared code                         │
│  - Register with driver-core via macro                 │
└─────────────────────────────────────────────────────────┘
```

### Core Design

**PCI Driver Interface:**
```rust
pub trait PciDriver: Send + Sync {
    fn name(&self) -> &'static str;
    fn id_table(&self) -> &'static [PciDeviceId];
    fn probe(&self, dev: &PciDevice, id: &PciDeviceId) -> Result<DriverBindingData, DriverError>;
    unsafe fn remove(&self, dev: &PciDevice, binding_data: DriverBindingData);
}

pub struct PciDeviceId {
    pub vendor: u16,
    pub device: u16,
    // ... subvendor, class, etc.
}
```

**Driver Registration (Compile-Time):**
```rust
// Drivers register via linker sections (zero runtime cost)
register_pci_driver!(VIRTIO_NET_DRIVER);

// Linker script collects drivers into .pci_drivers section
// kernel/src/init.rs walks section at boot
```

**Automatic Device Matching:**
```rust
// kernel/src/init.rs (NEW boot flow)
pci::enumerate();  // Scan bus
driver_core::init_driver_registry();  // Load drivers from .pci_drivers section
driver_core::probe_all_devices();  // Automatically match and probe

// Old manual probing REMOVED:
// let virtio_net_devices = pci::find_virtio_net();
// for dev in virtio_net_devices { ... }
```

### Example Driver Conversion

**Before (virtio-net current):**
```rust
pub unsafe fn from_pci(pci_dev: &PciDevice) -> Option<VirtioNet> {
    // 300+ lines of virtqueue setup, feature negotiation
    // Hardcoded in init.rs
}
```

**After (virtio-net with driver-core):**
```rust
struct VirtioNetDriver;

impl PciDriver for VirtioNetDriver {
    fn name(&self) -> &'static str { "virtio-net" }

    fn id_table(&self) -> &'static [PciDeviceId] {
        &[
            PciDeviceId::new(0x1AF4, 0x1000),  // Legacy
            PciDeviceId::new(0x1AF4, 0x1041),  // Modern
        ]
    }

    fn probe(&self, dev: &PciDevice, id: &PciDeviceId) -> Result<DriverBindingData, DriverError> {
        // Use virtio-core shared code
        let transport = virtio_core::VirtioPciTransport::from_pci(dev)?;
        let queue = virtio_core::Virtqueue::new(queue_size)?;

        let device = Arc::new(VirtioNet { transport, queue, ... });
        net::register_device(device.clone());

        Ok(DriverBindingData::new(Arc::into_raw(device) as usize))
    }

    unsafe fn remove(&self, dev: &PciDevice, binding_data: DriverBindingData) {
        let device = Arc::from_raw(binding_data.as_ptr::<VirtioNet>());
        net::unregister_device(device);
    }
}

static DRIVER: VirtioNetDriver = VirtioNetDriver;
register_pci_driver!(DRIVER);
```

## Implementation Plan

### Phase 1: Infrastructure (Weeks 1-2)

**Week 1: Create Core Crates**
1. Create `kernel/drivers/driver-core/`
   - PciDriver/IsaDriver traits
   - DriverRegistry implementation
   - DeviceBinding tracking
   - Compile-time registration macros

2. Create `kernel/drivers/virtio-core/`
   - Extract VirtioPciTransport from pci crate
   - Extract Virtqueue from virtio-blk (most complete)
   - Shared feature negotiation helpers

3. Update workspace
   - Add crates to Cargo.toml members
   - Add to workspace.dependencies

**Week 2: Linker Integration**
1. Update `kernel/linker.ld`
   - Add `.pci_drivers` section
   - Add `.isa_drivers` section

2. Implement `driver_core::init_driver_registry()`
   - Walk linker sections at boot
   - Register all static drivers

3. Implement `driver_core::probe_all_devices()`
   - Automatic device matching
   - Call probe() on matches

### Phase 2: Driver Conversion (Weeks 3-5)

**Week 3: Pilot Driver (virtio-blk)**
- Convert to PciDriver trait
- Use virtio-core shared code
- Test block I/O functionality
- This is the reference for other drivers

**Week 4: VirtIO Drivers**
- Convert virtio-net (network)
- Convert virtio-input (keyboard/mouse)
- Convert virtio-gpu (display)
- Convert virtio-snd (audio)
- All use virtio-core shared code

**Week 5: Legacy Drivers**
- Convert ps2 (ISA keyboard/mouse)
- Convert intel-hda (PCI audio)
- Update init.rs to use driver-core API

### Phase 3: Boot Integration (Week 6)

**Update kernel/src/init.rs:**
```rust
// OLD (REMOVE ~500 LOC of manual probing):
let virtio_net_devices = pci::find_virtio_net();
for dev in virtio_net_devices {
    if let Some(net) = VirtioNet::from_pci(&dev) {
        net::register_device(Arc::new(net));
    }
}
// ... repeat for each driver type

// NEW (~50 LOC):
driver_core::init_driver_registry();
pci::enumerate();
driver_core::probe_all_devices();
driver_core::probe_isa_devices();
```

### Phase 4: Dynamic Loading (Weeks 7-8) [OPTIONAL]

This phase builds on the existing `kernel/module/` infrastructure.

**Add Driver Module Support:**
1. Export kernel symbols for drivers
   - mm_alloc_contiguous, pci_config_read32, etc.
   - Create kernel/exports.rs with #[no_mangle] functions

2. Runtime driver registration
   - `driver_core::register_pci_driver_runtime(driver)`
   - Auto-probe existing devices after load

3. Build system for .ko files
   - `make build-driver-module DRIVER=virtio-net`
   - Produces virtio-net.ko in /lib/modules/

4. Module loading from disk
   - Use existing `module::load_module(elf_data)`
   - Extract .drivers section from module
   - Register drivers and probe devices

**User Interface:**
```bash
# From OXIDE shell
modprobe virtio-net    # Load module from /lib/modules/virtio-net.ko
lsmod                  # List loaded modules
rmmod virtio-net       # Unload module
```

## Critical Files to Modify

### New Files (Create)
- `kernel/drivers/driver-core/src/lib.rs` - Core traits and registry
- `kernel/drivers/driver-core/src/registry.rs` - DriverRegistry implementation
- `kernel/drivers/driver-core/src/binding.rs` - DeviceBinding tracking
- `kernel/drivers/virtio-core/src/lib.rs` - Shared VirtIO code
- `kernel/drivers/virtio-core/src/virtqueue.rs` - Shared virtqueue
- `kernel/drivers/virtio-core/src/transport.rs` - VirtioPciTransport
- `kernel/exports.rs` - Kernel symbol exports for modules

### Modified Files (Update)
- `kernel/drivers/net/virtio-net/src/lib.rs` - Convert to PciDriver trait
- `kernel/drivers/block/virtio-blk/src/lib.rs` - Convert to PciDriver trait
- `kernel/drivers/input/virtio-input/src/lib.rs` - Convert to PciDriver trait
- `kernel/drivers/gpu/virtio-gpu/src/lib.rs` - Convert to PciDriver trait
- `kernel/drivers/audio/virtio-snd/src/lib.rs` - Convert to PciDriver trait
- `kernel/drivers/input/ps2/src/lib.rs` - Convert to IsaDriver trait
- `kernel/src/init.rs` - Replace manual probing with driver-core API
- `kernel/linker.ld` - Add .pci_drivers/.isa_drivers sections
- `Cargo.toml` - Add driver-core, virtio-core to workspace
- `kernel/Cargo.toml` - Add dependencies on new crates

## Verification Strategy

### After Each Driver Conversion
1. **Build test**: `make build` should compile without errors
2. **Boot test**: `make run` and watch for driver probe messages
3. **Functional test**: Verify device works (block I/O, network ping, keyboard input)
4. **Regression test**: `make test` - all existing tests pass

### After Phase 2 (All Drivers Converted)
1. **Performance baseline**: Compare boot time vs. before migration
2. **Code metrics**: Verify ~1000 LOC reduction from shared virtio-core
3. **Clean build**: Remove old from_pci() functions

### After Phase 4 (Module Loading)
1. **Module build**: `make build-driver-module DRIVER=virtio-blk`
2. **Hot-load**: Boot without driver, then `modprobe virtio-blk`
3. **Unload**: `rmmod virtio-blk` - verify no memory leaks
4. **Device persistence**: Verify /dev/vda appears after modprobe

### Integration Tests
```rust
#[test]
fn test_driver_registration() {
    driver_core::init_driver_registry();
    let drivers = driver_core::list_drivers();
    assert!(drivers.contains(&"virtio-net"));
}

#[test]
fn test_device_matching() {
    let pci_dev = PciDevice { vendor_id: 0x1AF4, device_id: 0x1041, ... };
    let driver = driver_core::match_device(&pci_dev);
    assert!(driver.is_some());
    assert_eq!(driver.unwrap().name(), "virtio-net");
}
```

## Benefits

### Immediate (Phases 1-3)
- ✅ **Code Reduction**: ~1000 LOC eliminated via virtio-core shared code
- ✅ **Simplified Init**: init.rs reduced from ~500 LOC to ~50 LOC for driver setup
- ✅ **Unified Interface**: All drivers use same PciDriver trait
- ✅ **Hot-Plug Ready**: Architecture supports runtime device addition

### Long-Term (Phase 4)
- ✅ **Dynamic Loading**: Load drivers as .ko files from disk
- ✅ **Memory Efficiency**: Unload unused drivers
- ✅ **Modularity**: Distribute drivers separately from kernel
- ✅ **Development Speed**: Test drivers without rebuilding kernel

## Risk Mitigation

### High-Risk Areas
1. **DMA Buffer Management**: Shared virtio-core must work for all VirtIO drivers
   - Mitigation: Add canary values, extensive testing
2. **Driver Unload**: Resource cleanup must be complete
   - Mitigation: Reference counting, memory leak detection
3. **Interrupt Handling**: Clean IRQ unregistration
   - Mitigation: IRQ management in driver-core

### Rollback Strategy
- Keep old `from_pci()` functions during migration
- Use Cargo feature flags for gradual rollout
- If issues found, disable new driver via feature flag

## Timeline Estimate

- **Weeks 1-2**: Infrastructure (driver-core, virtio-core)
- **Weeks 3-5**: Driver conversion (all drivers)
- **Week 6**: Boot integration and testing
- **Weeks 7-8**: Dynamic loading (optional, future enhancement)

**Total: 6-8 weeks for full implementation**

## Success Criteria

✅ All existing devices detected and functional
✅ No regression in I/O performance (block, network)
✅ Boot time not degraded (target: <100ms increase)
✅ Code reduction: ~1000 LOC via shared virtio-core
✅ Clean init.rs: driver probing in <50 LOC
✅ All `make test` tests pass

## Next Steps

After plan approval:
1. Create driver-core crate skeleton
2. Create virtio-core crate skeleton
3. Convert virtio-blk as pilot driver
4. Iterate on remaining drivers
5. Update init.rs with new boot flow
6. Add dynamic loading support (optional)
