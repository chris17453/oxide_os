#!/usr/bin/env python3
"""
RPM Package Handling Module for OXIDE Package Manager

Provides functionality for:
- Extracting source RPMs
- Parsing RPM spec files
- Reading RPM metadata
- Converting RPM packages to OXIDE format
"""

import os
import subprocess
import tempfile
import re
import json
from pathlib import Path
from typing import Dict, List, Optional, Tuple


class RPMPackage:
    """Represents an RPM package with metadata and operations"""
    
    def __init__(self, rpm_path: str):
        self.rpm_path = Path(rpm_path)
        self.name = None
        self.version = None
        self.release = None
        self.arch = None
        self.summary = None
        self.description = None
        self.license = None
        self.url = None
        self.source_files = []
        self.patch_files = []
        self.buildrequires = []
        self.requires = []
        
        if self.rpm_path.exists():
            self._read_metadata()
    
    def _read_metadata(self):
        """Extract metadata from RPM using rpm2cpio and cpio"""
        try:
            # Use rpm query if rpm is available
            result = subprocess.run(
                ['rpm', '-qp', '--queryformat', 
                 '%{NAME}|||%{VERSION}|||%{RELEASE}|||%{ARCH}|||%{SUMMARY}|||%{DESCRIPTION}|||%{LICENSE}|||%{URL}',
                 str(self.rpm_path)],
                capture_output=True, text=True, timeout=30
            )
            
            if result.returncode == 0:
                parts = result.stdout.split('|||')
                if len(parts) >= 8:
                    self.name = parts[0]
                    self.version = parts[1]
                    self.release = parts[2]
                    self.arch = parts[3]
                    self.summary = parts[4]
                    self.description = parts[5]
                    self.license = parts[6]
                    self.url = parts[7]
            
            # Get dependencies
            result = subprocess.run(
                ['rpm', '-qp', '--requires', str(self.rpm_path)],
                capture_output=True, text=True, timeout=30
            )
            if result.returncode == 0:
                self.requires = [line.strip() for line in result.stdout.splitlines() if line.strip()]
                
        except (subprocess.TimeoutExpired, FileNotFoundError) as e:
            print(f"Warning: Could not read RPM metadata: {e}")
    
    def extract(self, dest_dir: str) -> bool:
        """Extract SRPM contents to destination directory"""
        dest_path = Path(dest_dir)
        dest_path.mkdir(parents=True, exist_ok=True)
        
        try:
            # Extract using rpm2cpio | cpio
            with open(self.rpm_path, 'rb') as rpm_file:
                rpm2cpio = subprocess.Popen(
                    ['rpm2cpio'],
                    stdin=rpm_file,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE
                )
                
                cpio = subprocess.Popen(
                    ['cpio', '-idmv'],
                    stdin=rpm2cpio.stdout,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    cwd=str(dest_path)
                )
                
                rpm2cpio.stdout.close()
                cpio_out, cpio_err = cpio.communicate(timeout=120)
                
                if cpio.returncode == 0:
                    self._catalog_extracted_files(dest_path)
                    return True
                else:
                    print(f"Error extracting SRPM: {cpio_err.decode()}")
                    return False
                    
        except (subprocess.TimeoutExpired, FileNotFoundError) as e:
            print(f"Error during extraction: {e}")
            return False
    
    def _catalog_extracted_files(self, extract_dir: Path):
        """Catalog extracted source and patch files"""
        for item in extract_dir.iterdir():
            if item.is_file():
                name = item.name.lower()
                if name.endswith(('.tar.gz', '.tar.bz2', '.tar.xz', '.tgz', '.tar', '.zip')):
                    self.source_files.append(str(item))
                elif name.endswith('.patch') or name.endswith('.diff'):
                    self.patch_files.append(str(item))
    
    def find_spec(self, extract_dir: str) -> Optional[str]:
        """Find the RPM spec file in extracted directory"""
        extract_path = Path(extract_dir)
        
        # Look for .spec files
        spec_files = list(extract_path.glob('*.spec'))
        if spec_files:
            return str(spec_files[0])
        
        return None
    
    def to_dict(self) -> Dict:
        """Convert package metadata to dictionary"""
        return {
            'name': self.name,
            'version': self.version,
            'release': self.release,
            'arch': self.arch,
            'summary': self.summary,
            'description': self.description,
            'license': self.license,
            'url': self.url,
            'source_files': self.source_files,
            'patch_files': self.patch_files,
            'buildrequires': self.buildrequires,
            'requires': self.requires,
        }


class SpecFile:
    """Parser for RPM spec files"""
    
    def __init__(self, spec_path: str):
        self.spec_path = Path(spec_path)
        self.name = None
        self.version = None
        self.release = None
        self.summary = None
        self.license = None
        self.url = None
        self.source_files = []
        self.patch_files = []
        self.buildrequires = []
        self.requires = []
        self.build_commands = []
        self.install_commands = []
        
        if self.spec_path.exists():
            self._parse()
    
    def _parse(self):
        """Parse RPM spec file"""
        current_section = None
        
        with open(self.spec_path, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                line = line.rstrip()
                
                # Skip comments
                if line.strip().startswith('#'):
                    continue
                
                # Detect sections
                if line.startswith('%'):
                    section_match = re.match(r'^%(\w+)', line)
                    if section_match:
                        current_section = section_match.group(1)
                        continue
                
                # Parse header fields
                if ':' in line and not line.startswith(' '):
                    key, value = line.split(':', 1)
                    key = key.strip().lower()
                    value = value.strip()
                    
                    if key == 'name':
                        self.name = value
                    elif key == 'version':
                        self.version = value
                    elif key == 'release':
                        self.release = value
                    elif key == 'summary':
                        self.summary = value
                    elif key == 'license':
                        self.license = value
                    elif key == 'url':
                        self.url = value
                    elif key.startswith('source'):
                        self.source_files.append(value)
                    elif key.startswith('patch'):
                        self.patch_files.append(value)
                    elif key == 'buildrequires':
                        self.buildrequires.append(value)
                    elif key == 'requires':
                        self.requires.append(value)
                
                # Collect section content
                elif current_section == 'build':
                    if line.strip():
                        self.build_commands.append(line)
                elif current_section == 'install':
                    if line.strip():
                        self.install_commands.append(line)
    
    def get_configure_flags(self) -> List[str]:
        """Extract configure flags from build commands
        — GraveShift: 'RPM specs dump all kinds of garbage in %build. Only trust things that smell like flags.'"""
        flags = []
        for cmd in self.build_commands:
            if './configure' in cmd:
                parts = cmd.split('./configure', 1)
                if len(parts) > 1:
                    for token in parts[1].strip().split():
                        # — SableWire: 'Filter out RPM macros, bare paths, and random dots. Only real --flags survive.'
                        if token.startswith('--') and not token.startswith(('--host=', '--build=', '--target=')):
                            if '%{' not in token and '%(' not in token:
                                flags.append(token)
        return flags
    
    def to_dict(self) -> Dict:
        """Convert spec metadata to dictionary"""
        return {
            'name': self.name,
            'version': self.version,
            'release': self.release,
            'summary': self.summary,
            'license': self.license,
            'url': self.url,
            'source_files': self.source_files,
            'patch_files': self.patch_files,
            'buildrequires': self.buildrequires,
            'requires': self.requires,
            'build_commands': self.build_commands,
            'install_commands': self.install_commands,
        }


def is_srpm(file_path: str) -> bool:
    """Check if file is a source RPM"""
    path = Path(file_path)
    return path.suffix == '.rpm' and '.src.' in path.name


def extract_srpm(srpm_path: str, dest_dir: str) -> Tuple[bool, Optional[str]]:
    """
    Extract SRPM and return (success, spec_file_path)
    
    Args:
        srpm_path: Path to source RPM file
        dest_dir: Destination directory for extraction
        
    Returns:
        Tuple of (success, spec_file_path)
    """
    rpm = RPMPackage(srpm_path)
    
    if not rpm.extract(dest_dir):
        return False, None
    
    spec_file = rpm.find_spec(dest_dir)
    return True, spec_file


def read_spec_file(spec_path: str) -> Optional[SpecFile]:
    """Read and parse RPM spec file"""
    if not Path(spec_path).exists():
        return None
    
    return SpecFile(spec_path)


# — GraveShift: "RPM handling... because Fedora's got the goods, we just need to break 'em down for our graveyard shift."
