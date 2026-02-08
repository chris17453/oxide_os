# OXIDE Package Manager (oxdnf)

## Overview

The OXIDE Package Manager (`oxdnf`) is a DNF-like package management system for OXIDE OS. It uses the host system's `dnf` and `rpm` tools to download Fedora source RPMs, builds full SRPM-level dependency graphs backed by SQLite, cross-compiles packages for OXIDE OS, and maintains versioned local repositories (per Fedora release).

## Architecture

```
pkgmgr/
├── bin/               # Executable tools
│   ├── oxdnf          # Main CLI (sync/fetch/graph/deps/topo/build)
│   ├── srpm-graph     # Standalone dependency graph builder
│   ├── build-pipeline # Mass build orchestration
│   ├── srpm-fetch     # Download Fedora SRPMs (legacy)
│   ├── srpm-build     # Cross-compile SRPMs (legacy)
│   └── repo-sync      # Sync repo metadata (legacy)
├── lib/               # Shared modules
│   ├── srpm_graph.py  # ★ Core graph engine (SQLite-backed, local-first)
│   ├── rpm.py         # RPM parsing and extraction
│   ├── fedora.py      # Fedora repo interaction
│   ├── builder.py     # SRPM build orchestration
│   ├── resolver.py    # Dependency resolution
│   └── repository.py  # Local repo management
├── repo/              # Versioned package repositories
│   └── fedora-42/     # ← one dir per Fedora release
│       ├── srpms/     # Downloaded .src.rpm files
│       ├── graph/     # deps.dot, deps.svg
│       ├── metadata/  # graph.db (SQLite — all graph data)
│       └── packages/  # Compiled .opkg packages
├── cache/             # Build cache and temp files
├── config/            # Configuration
│   ├── oxdnf.conf
│   ├── build.conf
│   └── repos.d/
├── specs/overrides/   # OXIDE-specific build overrides
└── packages-base.txt  # Base package list
```

## Quick Start

### Prerequisites
```bash
# Host tools (Fedora/RHEL)
sudo dnf -y install rpm-build python3 graphviz

# Enable source repos
sudo dnf config-manager setopt fedora-source.enabled=1
sudo dnf config-manager setopt updates-source.enabled=1
```

### Three-Step Workflow
```bash
# 1. Sync repo metadata locally (run once — ~3 seconds)
#    Caches 72K binary→source mappings + 742K provides into SQLite
python3 pkgmgr/bin/oxdnf sync

# 2. Fetch SRPMs
python3 pkgmgr/bin/oxdnf fetch                     # all from packages-base.txt
python3 pkgmgr/bin/oxdnf fetch bash grep sed        # specific packages

# 3. Build graph offline (seconds, not minutes)
python3 pkgmgr/bin/oxdnf graph
```

### Explore Dependencies
```bash
python3 pkgmgr/bin/oxdnf deps bash                  # bash's build deps
python3 pkgmgr/bin/oxdnf rdeps glibc                # who needs glibc?
python3 pkgmgr/bin/oxdnf topo                       # topological build order
python3 pkgmgr/bin/oxdnf db-stats                   # database statistics
python3 pkgmgr/bin/oxdnf list srpms                 # cached SRPMs
python3 pkgmgr/bin/oxdnf list available             # packages in graph
```

### Make Targets
```bash
make pkgmgr-sync                     # Sync repo metadata (run once)
make pkgmgr-fetch                    # Fetch base SRPMs
make pkgmgr-fetch PKG=bash           # Fetch specific SRPM
make pkgmgr-graph                    # Build dependency graph (offline)
make pkgmgr-deps PKG=bash            # Show bash build-deps
make pkgmgr-rdeps PKG=glibc          # Who depends on glibc?
make pkgmgr-topo                     # Topological build order
make pkgmgr-dbstats                  # Database statistics
make pkgmgr-list                     # List cached SRPMs
make pkgmgr-help                     # Full help
```

## Versioned Repositories

Repos are versioned by Fedora release under `pkgmgr/repo/`:
```
repo/
  fedora-42/    ← Fedora 42 SRPMs, graphs, packages
  fedora-43/    ← future releases
```

Switch release with `--release` / `-r`:
```bash
python3 pkgmgr/bin/oxdnf -r 43 sync
python3 pkgmgr/bin/oxdnf -r 43 fetch bash
make pkgmgr-fetch FEDORA_RELEASE=43
```

## Dependency Graph Engine

The graph engine (`lib/srpm_graph.py`) uses a **local-first** architecture:

### Sync Phase (once)
Two bulk `dnf repoquery` calls cache the entire Fedora repository index in SQLite:
- **72K+ binary packages** → source RPM mappings
- **742K+ provides** → binary package mappings

### Build Phase (instant)
1. **`rpm -qpR`** — extract BuildRequires from local SRPM headers
2. **SQLite lookup** — resolve capability → binary → source (microseconds each)
3. **Edge creation** — wire the dependency graph

No network calls during graph building. Resolution uses the local SQLite index exclusively.

### Performance
| Operation | Time |
|-----------|------|
| Sync (72K packages + 742K provides) | ~3 seconds |
| Graph 27 base packages (275 BuildRequires) | ~0.5 seconds |
| Graph 115 SRPMs (2028 BuildRequires) | ~2.4 seconds |
| Single capability resolution | <1 millisecond |

### Graph Output Formats
- **DOT** (Graphviz) → pipe to `dot -Tsvg` for visual graphs
- **SVG** — auto-rendered if graphviz is installed
- **JSON** — machine-readable full graph with metadata
- **Summary** — human-readable stats with cycle detection

### Algorithms
- **Topological sort** (Kahn's) for linear build order
- **Tarjan's SCC** for cycle detection (mutual build deps)
- **Parallel build layers** — groups of packages buildable simultaneously

### SQLite Schema
- `source_packages` — name, version, release, metadata, crawl state
- `binary_packages` — name, version, source_package link
- `build_requires` — per-capability resolution with binary + source links
- `edges` — source→source dependency graph
- `repo_packages` — bulk binary→source index (from sync)
- `repo_provides` — bulk capability→binary index (from sync)

## Command Reference

| Command | Description |
|---------|-------------|
| `oxdnf sync` | **Sync repo metadata (run once)** |
| `oxdnf fetch [packages...]` | Download SRPMs |
| `oxdnf graph [packages...]` | Build dependency graph (offline) |
| `oxdnf search <query>` | Search Fedora repos |
| `oxdnf info <package>` | Package details |
| `oxdnf deps <package>` | Show build dependencies |
| `oxdnf rdeps <package>` | Show reverse build dependencies |
| `oxdnf topo` | Topological build order |
| `oxdnf db-stats` | Database statistics |
| `oxdnf list srpms\|available\|installed\|releases` | List things |
| `oxdnf buildsrpm <package>` | Cross-compile for OXIDE |
| `oxdnf clean all\|srpms\|graph` | Clean cache |

## Configuration

### Source Repos
The graph engine uses `fedora-source` and `updates-source` repos.
Ensure they're enabled on your host:
```bash
dnf repolist | grep source
```

### Build Overrides
For OXIDE-specific build modifications, see `specs/overrides/`.

## References

- [RPM Packaging Guide](https://rpm-packaging-guide.github.io/)
- [Fedora Source RPMs](https://src.fedoraproject.org/)
- [DNF Documentation](https://dnf.readthedocs.io/)
- [OXIDE Toolchain](../toolchain/README.md)

---
*"Two queries. 72K packages. 742K provides. All local. Graph builds in seconds, not minutes."* — ColdCipher
