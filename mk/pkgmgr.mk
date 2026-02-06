# — PatchBay: Package manager targets for oxdnf.
# Fedora SRPM wrangling — because building from source is a lifestyle choice.

.PHONY: pkgmgr-sync pkgmgr-search pkgmgr-build pkgmgr-install pkgmgr-help
.PHONY: packages packages-base packages-list packages-clean

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

# Sync Fedora repository metadata
pkgmgr-sync:
	@echo "Syncing package repository metadata..."
	@python3 pkgmgr/bin/repo-sync

# Search for packages
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
	@python3 pkgmgr/bin/oxdnf buildsrpm $(PKG)

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
	@echo "Pipeline Targets (mass builds):"
	@echo "  make packages-base                  - Build base OXIDE packages"
	@echo "  make packages LIST=file.txt         - Build packages from list"
	@echo "  make packages-parallel LIST=f JOBS=4 - Parallel builds"
	@echo "  make packages-list [LIST=file]      - Show build order (dry run)"
	@echo "  make packages-resume [LIST=file]    - Resume failed build"
	@echo "  make packages-clean                 - Clean build cache"
	@echo ""
	@echo "Single Package Targets:"
	@echo "  make pkgmgr-sync                    - Sync Fedora metadata"
	@echo "  make pkgmgr-search PKG=bash         - Search for packages"
	@echo "  make pkgmgr-build PKG=bash          - Build single package"
	@echo "  make pkgmgr-install PKG=bash        - Install a built package"
	@echo ""
	@echo "Direct CLI usage:"
	@echo "  python3 pkgmgr/bin/oxdnf --help"
	@echo "  python3 pkgmgr/bin/build-pipeline --help"
	@echo ""
	@echo "See pkgmgr/README.md for full documentation"
