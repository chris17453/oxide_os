# QEMU CPU SMAP/SMEP Requirement

— ColdCipher: Document the 3 AM debugging session that saved us from infinite invalid opcode loops.

## The Problem

The OXIDE kernel uses `STAC`/`CLAC` instructions (Set/Clear AC flag) for Supervisor Mode Access Prevention (SMAP) security. These instructions require CPU support for SMAP.

When QEMU is started with `-cpu qemu64` (the default basic model), the `STAC` instruction causes an **Invalid Opcode exception (#UD)** because the qemu64 model doesn't include SMAP support.

## The Solution

**ALWAYS** use `-cpu qemu64,+smap,+smep` when running OXIDE OS in QEMU.

```bash
qemu-system-x86_64 \
    -cpu qemu64,+smap,+smep \
    ...other args...
```

## Why This Matters

1. **STAC/CLAC** are used throughout the kernel for secure memory access
2. Without SMAP support, the kernel crashes early in boot (usually in `terminal::write`)
3. The crash happens at address `FFFFFFFF801AF2F8` with instruction `0f 01 cb` (STAC)
4. Error message: `!!!! X64 Exception Type - 06(#UD - Invalid Opcode)`

## What Was Fixed

Fixed in commit [HASH]:
- Updated `DEBUG_QEMU_ARGS` in `mk/qemu.mk` to include `+smap,+smep`
- Updated debug scripts (`scripts/debug-kernel.sh`) to use correct CPU flags
- All QEMU launch paths now consistently use SMAP-enabled CPU model

## Where This Applies

**ALL** QEMU invocations must use these flags:
- `make run` targets
- `make debug-*` targets
- Manual QEMU launches
- CI/CD test environments
- Developer documentation examples

## Verification

To verify QEMU is using SMAP:
```bash
# Check running QEMU process
ps aux | grep qemu | grep "smap"

# Or check with GDB
./scripts/gdb-autonomous.py --exec "info registers" | grep -i cr4
# CR4 should have SMAP bit set (bit 21)
```

## References

- x86 SMAP: https://en.wikipedia.org/wiki/Supervisor_Mode_Access_Prevention
- STAC instruction: Sets AC flag in EFLAGS to allow supervisor access to user pages
- CLAC instruction: Clears AC flag to re-enable SMAP protection

## Prevention

When adding new QEMU launch configurations:
1. Always start from existing working config
2. Include `+smap,+smep` in CPU flags
3. Test boot to verify no invalid opcode exceptions
4. Document any deviations

---

*Debugged autonomously via GDB automation (see `docs/AUTONOMOUS-DEBUGGING.md`)*
