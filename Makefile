
# ========================================
# Package Manager Targets
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

# Show package manager help
pkgmgr-help:
	@echo "OXIDE Package Manager (oxdnf) - Make Targets"
	@echo ""
	@echo "Available targets:"
	@echo "  make pkgmgr-sync              - Sync Fedora repository metadata"
	@echo "  make pkgmgr-search PKG=bash   - Search for packages"
	@echo "  make pkgmgr-build PKG=bash    - Build package from Fedora SRPM"
	@echo "  make pkgmgr-install PKG=bash  - Install a built package"
	@echo ""
	@echo "Direct oxdnf usage:"
	@echo "  python3 pkgmgr/bin/oxdnf --help"
	@echo ""
	@echo "Examples:"
	@echo "  make pkgmgr-sync"
	@echo "  make pkgmgr-search PKG=vim"
	@echo "  make pkgmgr-build PKG=vim"
	@echo "  make pkgmgr-install PKG=vim"
	@echo ""
	@echo "See pkgmgr/README.md for full documentation"
