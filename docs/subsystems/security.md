# Security Subsystem

## Crates

| Crate | Purpose |
|-------|---------|
| `crypto` | Cryptographic primitives (AES, SHA, RSA, ChaCha20, Poly1305) |
| `trust` | TPM integration and measured boot |
| `quarantine` | Security isolation and sandboxing |
| `x509` | X.509 certificate parsing and validation |
| `seccomp` | Syscall filtering (BPF-based) |
| `namespace` | Linux-style namespaces (PID, mount, network, user) |
| `cgroup` | Control groups for resource limits |

## Architecture

Security is layered across multiple subsystems. The `crypto` crate provides
primitives used by SSH, TLS, and disk encryption. `trust` integrates with
TPM hardware for measured boot chains. `x509` handles certificate validation
for TLS connections.

Container isolation uses Linux-compatible primitives: `namespace` provides
PID/mount/network isolation, `cgroup` enforces resource limits, and `seccomp`
filters dangerous syscalls.
