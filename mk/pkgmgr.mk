# — PatchBay: Package manager targets for oxdnf.
# Fedora SRPM wrangling — because building from source is a lifestyle choice.

.PHONY: pkgmgr-sync pkgmgr-search pkgmgr-build pkgmgr-install pkgmgr-help
.PHONY: packages packages-base packages-list packages-clean
.PHONY: pkgmgr-fetch pkgmgr-graph pkgmgr-deps pkgmgr-topo pkgmgr-list
.PHONY: pkgmgr-crawl pkgmgr-dbstats pkgmgr-rdeps pkgmgr-list-releases

# Default Fedora release for versioned repos
FEDORA_RELEASE ?= 42

# ========================================
# SRPM & Dependency Graph targets (NEW)
# ========================================

# Sync repo metadata (run once — caches 72K+ packages locally)
pkgmgr-sync:
	@python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) sync

# Fetch SRPMs for base packages (or PKG= for specific ones)
pkgmgr-fetch:
	@if [ -z "$(PKG)" ]; then \
		echo "Fetching base SRPMs for Fedora $(FEDORA_RELEASE)..."; \
		python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) fetch; \
	else \
		echo "Fetching SRPM: $(PKG)"; \
		python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) fetch $(PKG); \
	fi

# Build dependency graph (recursive crawl with SQLite)
pkgmgr-graph:
	@if [ -z "$(PKG)" ]; then \
		python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) graph $(if $(DEPTH),--depth $(DEPTH),); \
	else \
		python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) graph $(PKG) $(if $(DEPTH),--depth $(DEPTH),); \
	fi

# Alias: crawl = graph
pkgmgr-crawl: pkgmgr-graph

# Show build dependencies for a package
pkgmgr-deps:
	@if [ -z "$(PKG)" ]; then \
		echo "Usage: make pkgmgr-deps PKG=<package-name>"; \
		exit 1; \
	fi
	@python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) deps $(PKG)

# Show reverse dependencies
pkgmgr-rdeps:
	@if [ -z "$(PKG)" ]; then \
		echo "Usage: make pkgmgr-rdeps PKG=<package-name>"; \
		exit 1; \
	fi
	@python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) rdeps $(PKG)

# Show topological build order
pkgmgr-topo:
	@python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) topo

# Show DB statistics
pkgmgr-dbstats:
	@python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) db-stats

# List cached SRPMs / available / releases
pkgmgr-list:
	@python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) list srpms

pkgmgr-list-releases:
	@python3 pkgmgr/bin/oxdnf list releases

# ========================================
# Pipeline targets for mass builds
# ========================================

# Build base packages (core userspace)
packages-base:
	@echo "Building base packages for OXIDE OS..."
	@python3 pkgmgr/bin/build-pipeline --sync pkgmgr/packages-base.txt

# Build packages from a custom list
packages:
	@if [ -z "$(LIST)" ]; then \
		echo "Usage: make packages LIST=<package-list-file>"; \
		echo "       make packages LIST=pkgmgr/packages-base.txt"; \
		exit 1; \
	fi
	@python3 pkgmgr/bin/build-pipeline --sync $(LIST)

# Build packages with parallel jobs
packages-parallel:
	@if [ -z "$(LIST)" ]; then \
		echo "Usage: make packages-parallel LIST=<file> JOBS=4"; \
		exit 1; \
	fi
	@python3 pkgmgr/bin/build-pipeline -j$(JOBS) --sync $(LIST)

# Show what would be built (dry run)
packages-list:
	@if [ -z "$(LIST)" ]; then \
		python3 pkgmgr/bin/build-pipeline --dry-run pkgmgr/packages-base.txt; \
	else \
		python3 pkgmgr/bin/build-pipeline --dry-run $(LIST); \
	fi

# Resume failed build
packages-resume:
	@echo "Resuming package builds..."
	@if [ -z "$(LIST)" ]; then \
		python3 pkgmgr/bin/build-pipeline -r -c pkgmgr/packages-base.txt; \
	else \
		python3 pkgmgr/bin/build-pipeline -r -c $(LIST); \
	fi

# Clean package build cache
packages-clean:
	@echo "Cleaning package build cache..."
	@rm -rf pkgmgr/cache/builds/*
	@rm -rf pkgmgr/cache/srpms/*
	@rm -f pkgmgr/cache/pipeline-state.json
	@echo "Done."

# ========================================
# Single package targets
# ========================================

# Search for packages (uses real dnf repoquery now)
pkgmgr-search:
	@if [ -z "$(PKG)" ]; then \
		echo "Usage: make pkgmgr-search PKG=<package-name>"; \
		exit 1; \
	fi
	@python3 pkgmgr/bin/oxdnf search $(PKG)

# Build package from SRPM
pkgmgr-build:
	@if [ -z "$(PKG)" ]; then \
		echo "Usage: make pkgmgr-build PKG=<package-name>"; \
		exit 1; \
	fi
	@echo "Building package: $(PKG)"
	@python3 pkgmgr/bin/oxdnf -r $(FEDORA_RELEASE) buildsrpm $(PKG)

# Install package
pkgmgr-install:
	@if [ -z "$(PKG)" ]; then \
		echo "Usage: make pkgmgr-install PKG=<package-name>"; \
		exit 1; \
	fi
	@echo "Installing package: $(PKG)"
	@python3 pkgmgr/bin/oxdnf install $(PKG)

# ========================================
# Help
# ========================================

# Show package manager help
pkgmgr-help:
	@echo "OXIDE Package Manager (oxdnf) - Make Targets"
	@echo ""
	@echo "SRPM & Dependency Graph (SQLite-backed):"
	@echo "  make pkgmgr-sync                     - Sync repo metadata (run once!)"
	@echo "  make pkgmgr-fetch                    - Fetch base SRPMs from Fedora"
	@echo "  make pkgmgr-fetch PKG=bash           - Fetch specific SRPM"
	@echo "  make pkgmgr-graph                    - Build graph from local SRPMs"
	@echo "  make pkgmgr-graph PKG='bash grep'    - Graph specific packages"
	@echo "  make pkgmgr-deps PKG=bash            - Show bash build-deps"
	@echo "  make pkgmgr-rdeps PKG=glibc          - Who depends on glibc?"
	@echo "  make pkgmgr-topo                     - Topological build order"
	@echo "  make pkgmgr-dbstats                  - Database statistics"
	@echo "  make pkgmgr-list                     - List cached SRPMs"
	@echo "  make pkgmgr-list-releases            - Show versioned repos"
	@echo ""
	@echo "Workflow:  make pkgmgr-sync → make pkgmgr-fetch → make pkgmgr-graph"
	@echo ""
	@echo "Pipeline Targets (mass builds):"
	@echo "  make packages-base                   - Build base OXIDE packages"
	@echo "  make packages LIST=file.txt          - Build packages from list"
	@echo "  make packages-parallel LIST=f JOBS=4 - Parallel builds"
	@echo "  make packages-list [LIST=file]       - Show build order (dry run)"
	@echo "  make packages-resume [LIST=file]     - Resume failed build"
	@echo "  make packages-clean                  - Clean build cache"
	@echo ""
	@echo "Single Package:"
	@echo "  make pkgmgr-search PKG=bash          - Search Fedora repos"
	@echo "  make pkgmgr-build PKG=bash           - Build single package"
	@echo "  make pkgmgr-install PKG=bash         - Install a built package"
	@echo ""
	@echo "Options:"
	@echo "  FEDORA_RELEASE=42                    - Target Fedora release (default: 42)"
	@echo ""
	@echo "Direct CLI:"
	@echo "  python3 pkgmgr/bin/oxdnf --help"
	@echo "  python3 pkgmgr/bin/srpm-graph --help"
