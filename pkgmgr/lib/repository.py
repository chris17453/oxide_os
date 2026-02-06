#!/usr/bin/env python3
"""
Local Package Repository Management

Handles:
- Package installation
- Repository indexing
- Package database
- Metadata management
"""

import os
import json
import shutil
import subprocess
from pathlib import Path
from typing import Dict, List, Optional
from datetime import datetime


class LocalRepository:
    """Manages local OXIDE package repository"""
    
    def __init__(self, repo_dir: str):
        self.repo_dir = Path(repo_dir)
        self.packages_dir = self.repo_dir / 'packages'
        self.metadata_dir = self.repo_dir / 'metadata'
        self.sources_dir = self.repo_dir / 'sources'
        
        # Create directories
        self.packages_dir.mkdir(parents=True, exist_ok=True)
        self.metadata_dir.mkdir(parents=True, exist_ok=True)
        self.sources_dir.mkdir(parents=True, exist_ok=True)
        
        # Load database
        self.db_file = self.metadata_dir / 'packages.json'
        self.db = self._load_db()
    
    def _load_db(self) -> Dict:
        """Load package database"""
        if self.db_file.exists():
            with open(self.db_file, 'r') as f:
                return json.load(f)
        return {'packages': {}, 'last_updated': None}
    
    def _save_db(self):
        """Save package database"""
        self.db['last_updated'] = datetime.now().isoformat()
        with open(self.db_file, 'w') as f:
            json.dump(self.db, f, indent=2)
    
    def add_package(self, pkg_file: str) -> bool:
        """Add a package to the repository"""
        pkg_path = Path(pkg_file)
        if not pkg_path.exists():
            print(f"Package file not found: {pkg_file}")
            return False
        
        # Copy to packages directory
        dest_file = self.packages_dir / pkg_path.name
        shutil.copy2(pkg_path, dest_file)
        
        # Extract and read metadata
        metadata = self._read_package_metadata(dest_file)
        if not metadata:
            print("Failed to read package metadata")
            return False
        
        # Add to database
        pkg_id = f"{metadata['name']}-{metadata['version']}-{metadata['release']}"
        self.db['packages'][pkg_id] = {
            'file': str(dest_file),
            'metadata': metadata,
            'added': datetime.now().isoformat()
        }
        
        self._save_db()
        print(f"Added package: {pkg_id}")
        return True
    
    def _read_package_metadata(self, pkg_file: Path) -> Optional[Dict]:
        """Extract and read metadata from .opkg file"""
        import tempfile
        
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = Path(tmpdir)
            
            try:
                # Extract package
                subprocess.run(
                    ['tar', 'xzf', str(pkg_file), '-C', str(tmp_path)],
                    check=True,
                    timeout=60
                )
                
                # Read metadata
                metadata_file = tmp_path / 'metadata.json'
                if metadata_file.exists():
                    with open(metadata_file, 'r') as f:
                        return json.load(f)
                
            except Exception as e:
                print(f"Error reading package metadata: {e}")
        
        return None
    
    def search(self, query: str) -> List[Dict]:
        """Search for packages in repository"""
        results = []
        query_lower = query.lower()
        
        for pkg_id, pkg_info in self.db['packages'].items():
            metadata = pkg_info['metadata']
            name = metadata.get('name', '').lower()
            summary = metadata.get('summary', '').lower()
            
            if query_lower in name or query_lower in summary:
                results.append({
                    'id': pkg_id,
                    'name': metadata.get('name'),
                    'version': metadata.get('version'),
                    'release': metadata.get('release'),
                    'summary': metadata.get('summary'),
                })
        
        return results
    
    def get_package_info(self, package_name: str) -> Optional[Dict]:
        """Get information about a package"""
        # Find package by name (latest version)
        matches = []
        for pkg_id, pkg_info in self.db['packages'].items():
            if pkg_info['metadata']['name'] == package_name:
                matches.append((pkg_id, pkg_info))
        
        if not matches:
            return None
        
        # Return latest version (simple string comparison)
        matches.sort(key=lambda x: x[0], reverse=True)
        return matches[0][1]['metadata']
    
    def list_packages(self) -> List[Dict]:
        """List all packages in repository"""
        packages = []
        for pkg_id, pkg_info in self.db['packages'].items():
            metadata = pkg_info['metadata']
            packages.append({
                'id': pkg_id,
                'name': metadata.get('name'),
                'version': metadata.get('version'),
                'release': metadata.get('release'),
                'summary': metadata.get('summary'),
            })
        
        packages.sort(key=lambda x: x['name'])
        return packages
    
    def remove_package(self, pkg_id: str) -> bool:
        """Remove a package from repository"""
        if pkg_id not in self.db['packages']:
            print(f"Package not found: {pkg_id}")
            return False
        
        pkg_info = self.db['packages'][pkg_id]
        pkg_file = Path(pkg_info['file'])
        
        # Remove file
        if pkg_file.exists():
            pkg_file.unlink()
        
        # Remove from database
        del self.db['packages'][pkg_id]
        self._save_db()
        
        print(f"Removed package: {pkg_id}")
        return True


class InstalledPackages:
    """Manages installed packages database"""
    
    def __init__(self, db_file: str = '/var/lib/oxdnf/installed.json'):
        self.db_file = Path(db_file)
        self.db_file.parent.mkdir(parents=True, exist_ok=True)
        self.db = self._load_db()
    
    def _load_db(self) -> Dict:
        """Load installed packages database"""
        if self.db_file.exists():
            with open(self.db_file, 'r') as f:
                return json.load(f)
        return {'packages': {}}
    
    def _save_db(self):
        """Save installed packages database"""
        with open(self.db_file, 'w') as f:
            json.dump(self.db, f, indent=2)
    
    def install(self, pkg_file: str, install_root: str = '/') -> bool:
        """Install a package"""
        pkg_path = Path(pkg_file)
        if not pkg_path.exists():
            print(f"Package file not found: {pkg_file}")
            return False
        
        import tempfile
        
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = Path(tmpdir)
            
            try:
                # Extract package
                subprocess.run(
                    ['tar', 'xzf', str(pkg_path), '-C', str(tmp_path)],
                    check=True,
                    timeout=60
                )
                
                # Read metadata
                metadata_file = tmp_path / 'metadata.json'
                with open(metadata_file, 'r') as f:
                    metadata = json.load(f)
                
                # Extract files to install root
                files_tar = tmp_path / 'files.tar.xz'
                subprocess.run(
                    ['tar', 'xJf', str(files_tar), '-C', install_root],
                    check=True,
                    timeout=300
                )
                
                # Add to installed database
                pkg_id = f"{metadata['name']}-{metadata['version']}-{metadata['release']}"
                self.db['packages'][pkg_id] = {
                    'metadata': metadata,
                    'installed': datetime.now().isoformat(),
                    'install_root': install_root
                }
                
                self._save_db()
                print(f"Installed: {pkg_id}")
                return True
                
            except Exception as e:
                print(f"Installation failed: {e}")
                return False
    
    def remove(self, package_name: str, install_root: str = '/') -> bool:
        """Remove an installed package"""
        # Find package
        pkg_id = None
        for pid, pkg_info in self.db['packages'].items():
            if pkg_info['metadata']['name'] == package_name:
                pkg_id = pid
                break
        
        if not pkg_id:
            print(f"Package not installed: {package_name}")
            return False
        
        pkg_info = self.db['packages'][pkg_id]
        metadata = pkg_info['metadata']
        
        # Remove files
        for file_path in metadata.get('files', []):
            full_path = Path(install_root) / file_path.lstrip('/')
            if full_path.exists():
                try:
                    full_path.unlink()
                except Exception as e:
                    print(f"Warning: Could not remove {full_path}: {e}")
        
        # Remove from database
        del self.db['packages'][pkg_id]
        self._save_db()
        
        print(f"Removed: {pkg_id}")
        return True
    
    def list_installed(self) -> List[Dict]:
        """List installed packages"""
        packages = []
        for pkg_id, pkg_info in self.db['packages'].items():
            metadata = pkg_info['metadata']
            packages.append({
                'id': pkg_id,
                'name': metadata.get('name'),
                'version': metadata.get('version'),
                'release': metadata.get('release'),
                'installed': pkg_info.get('installed'),
            })
        
        packages.sort(key=lambda x: x['name'])
        return packages
    
    def is_installed(self, package_name: str) -> bool:
        """Check if a package is installed"""
        for pkg_info in self.db['packages'].values():
            if pkg_info['metadata']['name'] == package_name:
                return True
        return False


# — WireSaint: "Repository management... keeping the local stash organized so we don't lose track of what's already in the vault."
