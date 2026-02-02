# Repository Guidelines

## Project Structure & Module Organization
`kernel/` contains the Rust kernel and all subsystem crates (mm, vfs, drivers, net, sched, security, etc.), `bootloader/` builds the handoff UEFI image, and `userspace/` holds all userspace programs organized by role (system, shell, coreutils, services, apps, libs, devtools, tests). Host-side helpers and scripts are in `tools/` and `scripts/`. Keep specs and walkthroughs in `docs/`, leave third-party source drops inside `external/`, and treat `target/` as disposable output.

## Build, Test, and Development Commands
- `make build` – compile kernel and bootloader for the current `ARCH`/`PROFILE`.
- `make build-full` – produce kernel, bootloader, userspace packages, and initramfs for complete boots.
- `make run` / `make run-fedora` – run the image in QEMU; ideal for console debugging.
- `make userspace` or `make userspace-pkg PKG=name` – rebuild all or one user program with the correct linker flags.
- `make toolchain` – update the custom cross toolchain before touching C demos.

## Coding Style & Naming Conventions
Use the pinned nightly toolchain from `rust-toolchain.toml`, run `cargo fmt --all` and `cargo clippy --all-targets -D warnings`, and keep diffs clean. Prefer four-space indentation, `snake_case` for modules/functions, and `CamelCase` for types; mirror that spacing in the C utilities. Document every `unsafe` block and split large modules into smaller files rather than leaving 1K-line sources.

## Testing Guidelines
`make test` rebuilds the rootfs, boots QEMU headless, and searches `target/serial.log` for the `OXIDE` banner—share that log whenever touching boot or init. Use `cargo test -p <crate>` for library crates and user binaries so they can be linted without QEMU; name tests descriptively (e.g., `test_serial_backspace`). Record any manual setup in `docs/` when new daemons or syscalls appear.

## Commit & Pull Request Guidelines
Commits in this repo use short, imperative subjects such as `Fix readline prompt` or `Add ioctl for ConsoleDevice`; keep body text for motivation or follow-ups. Pull requests must describe the change, enumerate validation commands (`make build-full`, `make test`, `cargo clippy`, etc.), link issues, and attach screenshots or serial excerpts whenever behavior is visible only during boot. Highlight any host dependencies (QEMU, OVMF, toolchain rebuilds) so reviewers can reproduce your run.

## Toolchain & Environment Notes
Install `qemu-system-x86_64`, `edk2-ovmf`, and the packages listed in `make help` before invoking run or test targets. Export `PATH=$PWD/toolchain/bin:$PATH` after `make toolchain` so `oxide-cc` resolves, and keep external source bumps paired with updates to their scripts in `scripts/`.
