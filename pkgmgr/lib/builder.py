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


class PackageOverride:
    """Loads and manages package-specific build overrides"""
    
    def __init__(self, override_file: str):
        self.configure_flags = []
        self.extra_cflags = ""
        self.extra_ldflags = ""
        self.pre_build_script = None
        self.post_configure_script = None
        self.post_build_script = None
        self.patches = []
        self.skip_configure = False
        self.skip_build = False
        self.custom_build_cmd = None
        self.custom_install_cmd = None
        self.make_target = None  # — WireSaint: 'Override the make target when "all" tries to build tests that fail in cross-compile'
        
        if Path(override_file).exists():
            self._parse(override_file)
    
    def _parse(self, override_file: str):
        """Parse override file (shell-like format)"""
        content = Path(override_file).read_text()
        
        # Extract CONFIGURE_FLAGS
        import re
        flags_match = re.search(r'CONFIGURE_FLAGS\s*=\s*"([^"]*)"', content, re.DOTALL)
        if flags_match:
            self.configure_flags = flags_match.group(1).split()
        
        # Extract EXTRA_CFLAGS
        cflags_match = re.search(r'EXTRA_CFLAGS\s*=\s*"([^"]*)"', content)
        if cflags_match:
            self.extra_cflags = cflags_match.group(1)
        
        # Extract EXTRA_LDFLAGS
        ldflags_match = re.search(r'EXTRA_LDFLAGS\s*=\s*"([^"]*)"', content)
        if ldflags_match:
            self.extra_ldflags = ldflags_match.group(1)
        
        # Extract pre_build function
        pre_build_match = re.search(r'pre_build\s*\(\)\s*\{([^}]*)\}', content, re.DOTALL)
        if pre_build_match:
            self.pre_build_script = pre_build_match.group(1).strip()
        
        # Extract post_configure function
        post_configure_match = re.search(r'post_configure\s*\(\)\s*\{([^}]*)\}', content, re.DOTALL)
        if post_configure_match:
            self.post_configure_script = post_configure_match.group(1).strip()

        # Extract post_build function
        post_build_match = re.search(r'post_build\s*\(\)\s*\{([^}]*)\}', content, re.DOTALL)
        if post_build_match:
            self.post_build_script = post_build_match.group(1).strip()

        # Extract MAKE_TARGET
        make_target_match = re.search(r'MAKE_TARGET\s*=\s*"([^"]*)"', content)
        if make_target_match:
            self.make_target = make_target_match.group(1).strip()
        
        # Extract PATCHES array
        patches_match = re.search(r'PATCHES\s*=\s*\(([^)]*)\)', content)
        if patches_match:
            self.patches = [p.strip().strip('"\'') for p in patches_match.group(1).split() if p.strip()]
        
        # Extract SKIP_CONFIGURE
        if re.search(r'SKIP_CONFIGURE\s*=\s*1', content):
            self.skip_configure = True
        
        # Extract CUSTOM_BUILD_CMD
        custom_build_match = re.search(r'CUSTOM_BUILD_CMD\s*=\s*"([^"]*)"', content)
        if custom_build_match:
            self.custom_build_cmd = custom_build_match.group(1)
        
        # Extract CUSTOM_INSTALL_CMD
        custom_install_match = re.search(r'CUSTOM_INSTALL_CMD\s*=\s*"([^"]*)"', content)
        if custom_install_match:
            self.custom_install_cmd = custom_install_match.group(1)


class BuildConfig:
    """Build configuration management"""
    
    def __init__(self, config_file: str, repo_root: str):
        self.config = configparser.ConfigParser()
        self.config.read(config_file)
        self.repo_root = Path(repo_root)
        self.pkgmgr_root = self.repo_root / "pkgmgr"
        
        # Resolve paths
        self.toolchain_path = self.repo_root / "toolchain"
        self.sysroot = self.toolchain_path / "sysroot"
        self.meson_cross_file = self.toolchain_path / "meson" / "oxide-cross.txt"
        self.cmake_toolchain = self.toolchain_path / "cmake" / "oxide-toolchain.cmake"
        self.build_dir = None
        self.make_jobs = self.config.get('oxide', 'make_jobs', fallback='4')
        
        # — SableWire: 'config.sub doesn't know "oxide" from a hole in the ground. Use linux-musl so autotools stays calm.'
        self.target_triple = "x86_64-unknown-linux-musl"
        
    def get_env(self, override: PackageOverride = None) -> Dict[str, str]:
        """Get cross-compilation environment variables
        — TorqueJax: 'Every env var is a loaded gun. Set them right or the whole build backfires.'"""
        env = os.environ.copy()

        # — BlackLatch: 'Host config.site poisons cross-builds. Nuke it from orbit.'
        env['CONFIG_SITE'] = ''
        
        # Set toolchain
        env['PATH'] = f"{self.toolchain_path}/bin:{env.get('PATH', '')}"
        env['CC'] = 'oxide-cc'
        env['CXX'] = 'oxide-c++'
        env['AR'] = 'oxide-ar'
        env['LD'] = 'oxide-ld'
        env['RANLIB'] = 'llvm-ranlib'
        env['AS'] = 'oxide-as'
        env['STRIP'] = 'llvm-strip'
        env['NM'] = 'llvm-nm'
        env['OBJCOPY'] = 'llvm-objcopy'
        env['OBJDUMP'] = 'llvm-objdump'
        
        # Cross-compile target
        env['CROSS_COMPILE'] = 'oxide-'
        env['HOST'] = self.target_triple
        env['BUILD'] = 'x86_64-linux-gnu'
        
        # Set flags
        cflags = self.config.get('oxide', 'cflags', fallback='-O2 -fPIC')
        ldflags = self.config.get('oxide', 'ldflags', fallback='-static')
        
        # Apply override flags
        if override and override.extra_cflags:
            cflags = f"{cflags} {override.extra_cflags}"
        if override and override.extra_ldflags:
            ldflags = f"{ldflags} {override.extra_ldflags}"
        
        env['CFLAGS'] = f"{cflags} -I{self.sysroot}/include -DOXIDE_OS"
        env['CXXFLAGS'] = f"{env['CFLAGS']} -fno-exceptions -fno-rtti"
        env['LDFLAGS'] = f"{ldflags} -L{self.sysroot}/lib"
        env['CPPFLAGS'] = f"-I{self.sysroot}/include -DOXIDE_OS"
        
        # PKG_CONFIG
        env['PKG_CONFIG'] = str(self.toolchain_path / 'bin' / 'oxide-pkg-config')
        env['PKG_CONFIG_PATH'] = f"{self.sysroot}/lib/pkgconfig"
        env['PKG_CONFIG_SYSROOT_DIR'] = str(self.sysroot)
        env['PKG_CONFIG_LIBDIR'] = f"{self.sysroot}/lib/pkgconfig"
        
        # Meson/CMake cross files
        env['OXIDE_MESON_CROSS'] = str(self.meson_cross_file)
        env['OXIDE_CMAKE_TOOLCHAIN'] = str(self.cmake_toolchain)
        
        return env
    
    def get_override(self, package_name: str) -> Optional[PackageOverride]:
        """Load override for a package if it exists"""
        override_file = self.pkgmgr_root / "specs" / "overrides" / f"{package_name}.override"
        if override_file.exists():
            return PackageOverride(str(override_file))
        return None


class PackageBuilder:
    """Orchestrates package building from SRPM"""
    
    def __init__(self, config: BuildConfig):
        self.config = config
        self.log_lines = []
        self.override = None
        
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
                   extract_dir: str, package_name: str = None) -> Optional[str]:
        """
        Build a package from SRPM
        
        Args:
            srpm_path: Path to source RPM
            output_dir: Directory for output package
            extract_dir: Directory for extraction/build
            package_name: Package name for override lookup (optional)
            
        Returns:
            Path to created .opkg file or None on failure
        """
        self.log_lines = []  # Reset log for new build
        self.log(f"Starting build of {srpm_path}")
        
        # Load override if package name provided or can be determined
        if package_name:
            self.override = self.config.get_override(package_name)
            if self.override:
                self.log(f"Loaded override for {package_name}")
        
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
            
            # Load override by spec name if not already loaded
            if not self.override:
                self.override = self.config.get_override(spec.name)
                if self.override:
                    self.log(f"Loaded override for {spec.name}")
            
            # Step 3: Extract source archives
            self.log("Extracting source archives...")
            rpm = RPMPackage(srpm_path)
            if not self._extract_sources(extract_path, build_path):
                self.log("WARNING: No source archives found or extraction failed")
            
            # Step 4: Run pre-build hook if defined
            if self.override and self.override.pre_build_script:
                self.log("Running pre-build hook...")
                if not self._run_hook(build_path, self.override.pre_build_script):
                    self.log("WARNING: Pre-build hook had issues")
            
            # Step 5: Detect build system
            self.log("Detecting build system...")
            build_system = self._detect_build_system(build_path)
            self.log(f"Build system: {build_system}")
            
            # Step 6: Configure
            if not (self.override and self.override.skip_configure):
                self.log("Running configure...")
                if not self._configure(build_path, build_system, spec):
                    self.log("ERROR: Configure failed")
                    return None
            else:
                self.log("Skipping configure (override)")
            
            # Step 6.5: Run post-configure hook if defined
            if self.override and self.override.post_configure_script:
                self.log("Running post-configure hook...")
                if not self._run_hook(build_path, self.override.post_configure_script):
                    self.log("WARNING: Post-configure hook had issues")

            # Step 7: Build
            self.log("Building...")
            if not self._build(build_path, build_system):
                self.log("ERROR: Build failed")
                return None
            
            # Step 8: Run post-build hook if defined
            if self.override and self.override.post_build_script:
                self.log("Running post-build hook...")
                if not self._run_hook(build_path, self.override.post_build_script):
                    self.log("WARNING: Post-build hook had issues")
            
            # Step 9: Install to staging
            self.log("Installing to staging directory...")
            if not self._install(build_path, install_path, build_system):
                self.log("ERROR: Install failed")
                return None
            
            # Step 10: Create package
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
    
    def _run_hook(self, build_path: Path, script: str) -> bool:
        """Run a pre/post build hook script"""
        src_dirs = [d for d in build_path.iterdir() if d.is_dir()]
        if src_dirs:
            src_dir = src_dirs[0]
        else:
            src_dir = build_path
        
        env = self.config.get_env(self.override)
        
        try:
            subprocess.run(
                ['bash', '-c', script],
                cwd=str(src_dir),
                env=env,
                check=True,
                timeout=300
            )
            return True
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
            self.log(f"Hook failed: {e}")
            return False
    
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
        env = self.config.get_env(self.override)
        
        try:
            if build_system == 'autotools':
                # Get configure flags from spec
                spec_flags = spec.get_configure_flags()
                
                # — BlackLatch: 'Autotools needs both --build and --host or it gets confused and segfaults on config.sub.'
                flags = [
                    '--prefix=/usr',
                    '--sysconfdir=/etc',
                    '--localstatedir=/var',
                    f'--build=x86_64-linux-gnu',
                    f'--host={self.config.target_triple}',
                    '--enable-static',
                    '--disable-shared',
                    '--disable-nls',
                ]

                flags.extend(spec_flags)
                
                # Add override flags
                if self.override and self.override.configure_flags:
                    flags.extend(self.override.configure_flags)
                
                # Generate configure if only configure.ac exists
                if not (src_dir / 'configure').exists() and (src_dir / 'configure.ac').exists():
                    self.log("Running autoreconf...")
                    subprocess.run(['autoreconf', '-fi'], cwd=str(src_dir), env=env,
                                   check=True, timeout=300)
                
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
                    f'-DCMAKE_TOOLCHAIN_FILE={self.config.cmake_toolchain}',
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
            
            elif build_system == 'meson':
                meson_dir = src_dir / 'builddir'
                
                # Create meson cross file with resolved paths
                cross_file = self._generate_meson_cross(src_dir)
                
                subprocess.run(
                    ['meson', 'setup', str(meson_dir), 
                     f'--cross-file={cross_file}',
                     '--prefix=/usr'],
                    cwd=str(src_dir),
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
    
    def _generate_meson_cross(self, src_dir: Path) -> str:
        """Generate a meson cross-file with resolved paths"""
        template = self.config.meson_cross_file.read_text()
        resolved = template.replace('@SYSROOT@', str(self.config.sysroot))
        
        cross_file = src_dir / 'oxide-cross.txt'
        cross_file.write_text(resolved)
        return str(cross_file)
    
    def _build(self, build_path: Path, build_system: str) -> bool:
        """Run build step"""
        src_dirs = [d for d in build_path.iterdir() if d.is_dir()]
        if not src_dirs:
            return False
        
        src_dir = src_dirs[0]
        env = self.config.get_env(self.override)
        
        try:
            # Custom build command from override
            if self.override and self.override.custom_build_cmd:
                subprocess.run(
                    ['bash', '-c', self.override.custom_build_cmd],
                    cwd=str(src_dir),
                    env=env,
                    check=True,
                    timeout=1800
                )
            elif build_system == 'autotools' or build_system == 'make':
                # — WireSaint: 'Some packages fail on "all" because tests try to link. Let overrides target just the lib.'
                make_cmd = ['make', f'-j{self.config.make_jobs}']
                if self.override and self.override.make_target:
                    make_cmd.append(self.override.make_target)
                subprocess.run(
                    make_cmd,
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
            elif build_system == 'meson':
                meson_dir = src_dir / 'builddir'
                subprocess.run(
                    ['meson', 'compile', '-C', str(meson_dir)],
                    cwd=str(src_dir),
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
        env = self.config.get_env(self.override)
        
        try:
            # Custom install command from override
            if self.override and self.override.custom_install_cmd:
                # Replace DESTDIR placeholder
                cmd = self.override.custom_install_cmd.replace('$DESTDIR', str(install_path))
                env['DESTDIR'] = str(install_path)
                subprocess.run(
                    ['bash', '-c', cmd],
                    cwd=str(src_dir),
                    env=env,
                    check=True,
                    timeout=600
                )
            elif build_system == 'autotools' or build_system == 'make':
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
            elif build_system == 'meson':
                meson_dir = src_dir / 'builddir'
                subprocess.run(
                    ['meson', 'install', '-C', str(meson_dir), 
                     f'--destdir={install_path}'],
                    cwd=str(src_dir),
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
