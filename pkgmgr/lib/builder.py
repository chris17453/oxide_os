#!/usr/bin/env python3
"""
SRPM Build Orchestration Module

Handles the complete build process:
- SRPM extraction
- Source preparation
- Cross-compilation setup
- Build execution
- Package creation
"""

import os
import subprocess
import tempfile
import shutil
import json
from pathlib import Path
from typing import Dict, List, Optional, Tuple
from datetime import datetime

from rpm import RPMPackage, SpecFile, extract_srpm
import configparser


class BuildConfig:
    """Build configuration management"""
    
    def __init__(self, config_file: str, repo_root: str):
        self.config = configparser.ConfigParser()
        self.config.read(config_file)
        self.repo_root = Path(repo_root)
        
        # Resolve paths
        self.toolchain_path = self.repo_root / "toolchain"
        self.sysroot = self.toolchain_path / "sysroot"
        self.build_dir = None
        self.make_jobs = self.config.get('oxide', 'make_jobs', fallback='4')
        
    def get_env(self) -> Dict[str, str]:
        """Get cross-compilation environment variables"""
        env = os.environ.copy()
        
        # Set toolchain
        env['PATH'] = f"{self.toolchain_path}/bin:{env.get('PATH', '')}"
        env['CC'] = 'oxide-cc'
        env['CXX'] = 'oxide-c++'
        env['AR'] = 'oxide-ar'
        env['LD'] = 'oxide-ld'
        env['RANLIB'] = 'ranlib'
        env['AS'] = 'oxide-as'
        
        # Set flags
        cflags = self.config.get('oxide', 'cflags', fallback='-O2 -fPIC')
        ldflags = self.config.get('oxide', 'ldflags', fallback='-static')
        
        env['CFLAGS'] = f"{cflags} -I{self.sysroot}/include"
        env['CXXFLAGS'] = env['CFLAGS']
        env['LDFLAGS'] = f"{ldflags} -L{self.sysroot}/lib"
        
        # PKG_CONFIG
        env['PKG_CONFIG_PATH'] = f"{self.sysroot}/lib/pkgconfig"
        env['PKG_CONFIG_SYSROOT_DIR'] = str(self.sysroot)
        
        return env


class PackageBuilder:
    """Orchestrates package building from SRPM"""
    
    def __init__(self, config: BuildConfig):
        self.config = config
        self.log_lines = []
        
    def log(self, message: str):
        """Add message to build log"""
        timestamp = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        line = f"[{timestamp}] {message}"
        self.log_lines.append(line)
        print(line)
    
    def save_log(self, log_file: str):
        """Save build log to file"""
        with open(log_file, 'w') as f:
            f.write('\n'.join(self.log_lines))
    
    def build_srpm(self, srpm_path: str, output_dir: str, 
                   extract_dir: str) -> Optional[str]:
        """
        Build a package from SRPM
        
        Args:
            srpm_path: Path to source RPM
            output_dir: Directory for output package
            extract_dir: Directory for extraction/build
            
        Returns:
            Path to created .opkg file or None on failure
        """
        self.log(f"Starting build of {srpm_path}")
        
        # Create working directories
        work_dir = Path(extract_dir) / f"build-{datetime.now().strftime('%Y%m%d%H%M%S')}"
        work_dir.mkdir(parents=True, exist_ok=True)
        
        extract_path = work_dir / "extract"
        build_path = work_dir / "build"
        install_path = work_dir / "install"
        
        extract_path.mkdir(exist_ok=True)
        build_path.mkdir(exist_ok=True)
        install_path.mkdir(exist_ok=True)
        
        try:
            # Step 1: Extract SRPM
            self.log("Extracting SRPM...")
            success, spec_file = extract_srpm(srpm_path, str(extract_path))
            if not success or not spec_file:
                self.log("ERROR: Failed to extract SRPM")
                return None
            
            self.log(f"Found spec file: {spec_file}")
            
            # Step 2: Parse spec file
            self.log("Parsing spec file...")
            spec = SpecFile(spec_file)
            if not spec.name:
                self.log("ERROR: Could not parse spec file")
                return None
            
            self.log(f"Package: {spec.name}-{spec.version}-{spec.release}")
            
            # Step 3: Extract source archives
            self.log("Extracting source archives...")
            rpm = RPMPackage(srpm_path)
            if not self._extract_sources(extract_path, build_path):
                self.log("WARNING: No source archives found or extraction failed")
            
            # Step 4: Detect build system
            self.log("Detecting build system...")
            build_system = self._detect_build_system(build_path)
            self.log(f"Build system: {build_system}")
            
            # Step 5: Configure
            self.log("Running configure...")
            if not self._configure(build_path, build_system, spec):
                self.log("ERROR: Configure failed")
                return None
            
            # Step 6: Build
            self.log("Building...")
            if not self._build(build_path, build_system):
                self.log("ERROR: Build failed")
                return None
            
            # Step 7: Install to staging
            self.log("Installing to staging directory...")
            if not self._install(build_path, install_path, build_system):
                self.log("ERROR: Install failed")
                return None
            
            # Step 8: Create package
            self.log("Creating OXIDE package...")
            pkg_file = self._create_package(spec, install_path, output_dir)
            if not pkg_file:
                self.log("ERROR: Package creation failed")
                return None
            
            self.log(f"SUCCESS: Package created at {pkg_file}")
            return pkg_file
            
        except Exception as e:
            self.log(f"ERROR: Build failed with exception: {e}")
            return None
        finally:
            # Save log
            log_file = work_dir / "build.log"
            self.save_log(str(log_file))
    
    def _extract_sources(self, extract_path: Path, build_path: Path) -> bool:
        """Extract source tarballs"""
        extracted = False
        
        for item in extract_path.iterdir():
            name = item.name.lower()
            
            if name.endswith(('.tar.gz', '.tgz')):
                subprocess.run(['tar', 'xzf', str(item), '-C', str(build_path)], 
                             check=True, timeout=300)
                extracted = True
            elif name.endswith('.tar.bz2'):
                subprocess.run(['tar', 'xjf', str(item), '-C', str(build_path)],
                             check=True, timeout=300)
                extracted = True
            elif name.endswith('.tar.xz'):
                subprocess.run(['tar', 'xJf', str(item), '-C', str(build_path)],
                             check=True, timeout=300)
                extracted = True
            elif name.endswith('.zip'):
                subprocess.run(['unzip', '-q', str(item), '-d', str(build_path)],
                             check=True, timeout=300)
                extracted = True
        
        return extracted
    
    def _detect_build_system(self, build_path: Path) -> str:
        """Detect the build system used by the package"""
        # Find actual source directory (often package-version)
        src_dirs = [d for d in build_path.iterdir() if d.is_dir()]
        if src_dirs:
            src_dir = src_dirs[0]
        else:
            src_dir = build_path
        
        if (src_dir / 'configure').exists() or (src_dir / 'configure.ac').exists():
            return 'autotools'
        elif (src_dir / 'CMakeLists.txt').exists():
            return 'cmake'
        elif (src_dir / 'meson.build').exists():
            return 'meson'
        elif (src_dir / 'Makefile').exists() or (src_dir / 'makefile').exists():
            return 'make'
        elif (src_dir / 'setup.py').exists():
            return 'python'
        else:
            return 'unknown'
    
    def _configure(self, build_path: Path, build_system: str, spec: SpecFile) -> bool:
        """Run configure step"""
        src_dirs = [d for d in build_path.iterdir() if d.is_dir()]
        if not src_dirs:
            return False
        
        src_dir = src_dirs[0]
        env = self.config.get_env()
        
        try:
            if build_system == 'autotools':
                # Get configure flags from spec
                spec_flags = spec.get_configure_flags()
                
                # Standard OXIDE flags
                flags = [
                    '--prefix=/usr',
                    '--sysconfdir=/etc',
                    '--localstatedir=/var',
                    '--host=x86_64-oxide',
                    '--enable-static',
                    '--disable-shared',
                ]
                
                flags.extend(spec_flags)
                
                subprocess.run(
                    ['./configure'] + flags,
                    cwd=str(src_dir),
                    env=env,
                    check=True,
                    timeout=600
                )
                
            elif build_system == 'cmake':
                cmake_dir = src_dir / 'build'
                cmake_dir.mkdir(exist_ok=True)
                
                flags = [
                    '-DCMAKE_BUILD_TYPE=Release',
                    '-DCMAKE_INSTALL_PREFIX=/usr',
                    '-DBUILD_SHARED_LIBS=OFF',
                ]
                
                subprocess.run(
                    ['cmake', '..'] + flags,
                    cwd=str(cmake_dir),
                    env=env,
                    check=True,
                    timeout=600
                )
                
            elif build_system in ('make', 'python', 'unknown'):
                # No configure step
                return True
            
            return True
            
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
            self.log(f"Configure failed: {e}")
            return False
    
    def _build(self, build_path: Path, build_system: str) -> bool:
        """Run build step"""
        src_dirs = [d for d in build_path.iterdir() if d.is_dir()]
        if not src_dirs:
            return False
        
        src_dir = src_dirs[0]
        env = self.config.get_env()
        
        try:
            if build_system == 'autotools' or build_system == 'make':
                subprocess.run(
                    ['make', f'-j{self.config.make_jobs}'],
                    cwd=str(src_dir),
                    env=env,
                    check=True,
                    timeout=1800
                )
            elif build_system == 'cmake':
                cmake_dir = src_dir / 'build'
                subprocess.run(
                    ['make', f'-j{self.config.make_jobs}'],
                    cwd=str(cmake_dir),
                    env=env,
                    check=True,
                    timeout=1800
                )
            elif build_system == 'python':
                subprocess.run(
                    ['python3', 'setup.py', 'build'],
                    cwd=str(src_dir),
                    env=env,
                    check=True,
                    timeout=1800
                )
            
            return True
            
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
            self.log(f"Build failed: {e}")
            return False
    
    def _install(self, build_path: Path, install_path: Path, build_system: str) -> bool:
        """Run install step to staging directory"""
        src_dirs = [d for d in build_path.iterdir() if d.is_dir()]
        if not src_dirs:
            return False
        
        src_dir = src_dirs[0]
        env = self.config.get_env()
        
        try:
            if build_system == 'autotools' or build_system == 'make':
                subprocess.run(
                    ['make', f'DESTDIR={install_path}', 'install'],
                    cwd=str(src_dir),
                    env=env,
                    check=True,
                    timeout=600
                )
            elif build_system == 'cmake':
                cmake_dir = src_dir / 'build'
                subprocess.run(
                    ['make', f'DESTDIR={install_path}', 'install'],
                    cwd=str(cmake_dir),
                    env=env,
                    check=True,
                    timeout=600
                )
            elif build_system == 'python':
                subprocess.run(
                    ['python3', 'setup.py', 'install', f'--root={install_path}', '--prefix=/usr'],
                    cwd=str(src_dir),
                    env=env,
                    check=True,
                    timeout=600
                )
            
            return True
            
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
            self.log(f"Install failed: {e}")
            return False
    
    def _create_package(self, spec: SpecFile, install_path: Path, 
                       output_dir: str) -> Optional[str]:
        """Create .opkg package from installed files"""
        output_path = Path(output_dir)
        output_path.mkdir(parents=True, exist_ok=True)
        
        pkg_name = f"{spec.name}-{spec.version}-{spec.release}.oxide.x86_64.opkg"
        pkg_file = output_path / pkg_name
        
        # Create metadata
        metadata = {
            'name': spec.name,
            'version': spec.version,
            'release': f"{spec.release}.oxide",
            'arch': 'x86_64',
            'summary': spec.summary,
            'description': spec.summary,  # Could be expanded
            'license': spec.license,
            'url': spec.url,
            'builddate': datetime.now().isoformat(),
            'requires': spec.requires,
            'files': self._list_files(install_path)
        }
        
        # Create package archive
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = Path(tmpdir)
            
            # Write metadata
            metadata_file = tmp_path / 'metadata.json'
            with open(metadata_file, 'w') as f:
                json.dump(metadata, f, indent=2)
            
            # Create files tarball
            files_tar = tmp_path / 'files.tar.xz'
            subprocess.run(
                ['tar', 'cJf', str(files_tar), '-C', str(install_path), '.'],
                check=True,
                timeout=300
            )
            
            # Create final package
            subprocess.run(
                ['tar', 'czf', str(pkg_file), '-C', str(tmp_path), 
                 'metadata.json', 'files.tar.xz'],
                check=True,
                timeout=300
            )
        
        return str(pkg_file)
    
    def _list_files(self, install_path: Path) -> List[str]:
        """List all files in install directory"""
        files = []
        for root, dirs, filenames in os.walk(install_path):
            for filename in filenames:
                full_path = Path(root) / filename
                rel_path = full_path.relative_to(install_path)
                files.append(f"/{rel_path}")
        return files


# — TorqueJax: "Building packages is like tuning an engine... you gotta get the timing right, or the whole thing blows up."
