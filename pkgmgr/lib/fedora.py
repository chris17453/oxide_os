#!/usr/bin/env python3
"""
Fedora Repository Interaction Module

Handles:
- Repository metadata parsing
- Package searching
- SRPM downloading
- Mirror management
"""

import os
import re
import urllib.request
import urllib.error
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Dict, List, Optional, Tuple
import configparser
import gzip


class FedoraRepo:
    """Represents a Fedora source repository"""
    
    def __init__(self, name: str, config: Dict[str, str]):
        self.name = name
        self.display_name = config.get('name', name)
        self.baseurl = config.get('baseurl', '')
        self.metalink = config.get('metalink', '')
        self.enabled = config.get('enabled', '1') == '1'
        self.gpgcheck = config.get('gpgcheck', '1') == '1'
        self.priority = int(config.get('priority', '99'))
        self.repo_type = config.get('type', 'rpm-src')
        self.metadata = None
        self.packages_cache = {}
    
    def fetch_metadata(self, cache_dir: str) -> bool:
        """Fetch repository metadata (repomd.xml)"""
        if not self.enabled:
            return False
        
        cache_path = Path(cache_dir) / self.name
        cache_path.mkdir(parents=True, exist_ok=True)
        
        repomd_url = f"{self.baseurl}/repodata/repomd.xml"
        repomd_file = cache_path / "repomd.xml"
        
        try:
            print(f"Fetching metadata from {self.name}...")
            urllib.request.urlretrieve(repomd_url, str(repomd_file))
            self.metadata = self._parse_repomd(repomd_file)
            return True
        except urllib.error.URLError as e:
            print(f"Error fetching metadata from {self.name}: {e}")
            return False
    
    def _parse_repomd(self, repomd_file: Path) -> Dict:
        """Parse repomd.xml to find primary metadata location"""
        try:
            tree = ET.parse(str(repomd_file))
            root = tree.getroot()
            
            # Find primary metadata location
            ns = {'repo': 'http://linux.duke.edu/metadata/repo'}
            for data in root.findall('.//repo:data', ns):
                if data.get('type') == 'primary':
                    location = data.find('.//repo:location', ns)
                    if location is not None:
                        return {'primary': location.get('href')}
            
            return {}
        except Exception as e:
            print(f"Error parsing repomd.xml: {e}")
            return {}
    
    def fetch_primary_metadata(self, cache_dir: str) -> bool:
        """Download and extract primary metadata (package list)"""
        if not self.metadata or 'primary' not in self.metadata:
            return False
        
        cache_path = Path(cache_dir) / self.name
        primary_url = f"{self.baseurl}/{self.metadata['primary']}"
        primary_file = cache_path / "primary.xml.gz"
        
        try:
            print(f"Downloading package list for {self.name}...")
            urllib.request.urlretrieve(primary_url, str(primary_file))
            
            # Extract and parse
            primary_xml = cache_path / "primary.xml"
            with gzip.open(str(primary_file), 'rb') as f_in:
                with open(str(primary_xml), 'wb') as f_out:
                    f_out.write(f_in.read())
            
            self._parse_primary(primary_xml)
            return True
            
        except Exception as e:
            print(f"Error fetching primary metadata: {e}")
            return False
    
    def _parse_primary(self, primary_xml: Path):
        """Parse primary.xml to build package cache"""
        try:
            tree = ET.parse(str(primary_xml))
            root = tree.getroot()
            
            ns = {'': 'http://linux.duke.edu/metadata/common'}
            
            for package in root.findall('.//package', ns):
                name_elem = package.find('.//name', ns)
                if name_elem is None:
                    continue
                
                name = name_elem.text
                
                version_elem = package.find('.//version', ns)
                version = version_elem.get('ver', '') if version_elem is not None else ''
                release = version_elem.get('rel', '') if version_elem is not None else ''
                
                location_elem = package.find('.//location', ns)
                location = location_elem.get('href', '') if location_elem is not None else ''
                
                summary_elem = package.find('.//summary', ns)
                summary = summary_elem.text if summary_elem is not None else ''
                
                self.packages_cache[name] = {
                    'name': name,
                    'version': version,
                    'release': release,
                    'location': location,
                    'summary': summary,
                    'repo': self.name
                }
        except Exception as e:
            print(f"Error parsing primary metadata: {e}")
    
    def search_package(self, query: str) -> List[Dict]:
        """Search for packages matching query"""
        results = []
        query_lower = query.lower()
        
        for pkg_name, pkg_info in self.packages_cache.items():
            if query_lower in pkg_name.lower() or query_lower in pkg_info.get('summary', '').lower():
                results.append(pkg_info)
        
        return results
    
    def get_package_info(self, package_name: str) -> Optional[Dict]:
        """Get information about a specific package"""
        return self.packages_cache.get(package_name)
    
    def download_package(self, package_name: str, dest_dir: str) -> Optional[str]:
        """Download a package SRPM"""
        pkg_info = self.get_package_info(package_name)
        if not pkg_info:
            return None
        
        dest_path = Path(dest_dir)
        dest_path.mkdir(parents=True, exist_ok=True)
        
        url = f"{self.baseurl}/{pkg_info['location']}"
        filename = Path(pkg_info['location']).name
        dest_file = dest_path / filename
        
        try:
            print(f"Downloading {package_name} from {self.name}...")
            urllib.request.urlretrieve(url, str(dest_file))
            print(f"Downloaded to {dest_file}")
            return str(dest_file)
        except Exception as e:
            print(f"Error downloading package: {e}")
            return None


class FedoraRepoManager:
    """Manages multiple Fedora repositories"""
    
    def __init__(self, config_dir: str):
        self.config_dir = Path(config_dir)
        self.repos = []
        self._load_repos()
    
    def _load_repos(self):
        """Load repository configurations from repos.d directory"""
        repos_d = self.config_dir / 'repos.d'
        if not repos_d.exists():
            return
        
        for repo_file in repos_d.glob('*.repo'):
            self._parse_repo_file(repo_file)
    
    def _parse_repo_file(self, repo_file: Path):
        """Parse a .repo configuration file"""
        config = configparser.ConfigParser()
        try:
            config.read(str(repo_file))
            
            for section in config.sections():
                repo_config = dict(config[section])
                
                # Only load rpm-src type repos
                if repo_config.get('type', 'rpm-src') == 'rpm-src':
                    repo = FedoraRepo(section, repo_config)
                    self.repos.append(repo)
                    
        except Exception as e:
            print(f"Error parsing repo file {repo_file}: {e}")
    
    def sync_all(self, cache_dir: str) -> int:
        """Sync all enabled repositories"""
        synced = 0
        for repo in self.repos:
            if repo.enabled and repo.repo_type == 'rpm-src':
                if repo.fetch_metadata(cache_dir):
                    if repo.fetch_primary_metadata(cache_dir):
                        synced += 1
        return synced
    
    def search(self, query: str) -> List[Dict]:
        """Search all repositories for packages"""
        results = []
        for repo in self.repos:
            if repo.enabled:
                results.extend(repo.search_package(query))
        
        # Sort by relevance (exact match first, then by name)
        query_lower = query.lower()
        results.sort(key=lambda x: (
            0 if x['name'].lower() == query_lower else 1,
            x['name'].lower()
        ))
        
        return results
    
    def get_package(self, package_name: str) -> Optional[Tuple[FedoraRepo, Dict]]:
        """Find package in repositories (by priority)"""
        sorted_repos = sorted(
            [r for r in self.repos if r.enabled],
            key=lambda r: r.priority
        )
        
        for repo in sorted_repos:
            pkg_info = repo.get_package_info(package_name)
            if pkg_info:
                return repo, pkg_info
        
        return None
    
    def download_srpm(self, package_name: str, dest_dir: str) -> Optional[str]:
        """Download SRPM for a package"""
        result = self.get_package(package_name)
        if not result:
            print(f"Package {package_name} not found in any repository")
            return None
        
        repo, pkg_info = result
        return repo.download_package(package_name, dest_dir)
    
    def list_repos(self) -> List[FedoraRepo]:
        """List all configured repositories"""
        return self.repos


# — SableWire: "Fedora's got the source... we're just pulling the strings to make it dance on OXIDE's stage."
