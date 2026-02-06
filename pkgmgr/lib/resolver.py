#!/usr/bin/env python3
"""
Dependency Resolution Module

Handles:
- Dependency graph building
- Topological sorting
- Circular dependency detection
- Build order determination
"""

from typing import Dict, List, Set, Optional, Tuple
from collections import defaultdict, deque


class DependencyResolver:
    """Resolves package dependencies"""
    
    def __init__(self):
        self.graph = defaultdict(set)  # package -> set of dependencies
        self.reverse_graph = defaultdict(set)  # dependency -> set of packages needing it
        self.packages = {}  # package_name -> package_info
    
    def add_package(self, name: str, dependencies: List[str], info: Dict = None):
        """Add a package and its dependencies"""
        self.packages[name] = info or {}
        
        for dep in dependencies:
            # Strip version constraints (e.g., "package >= 1.0" -> "package")
            dep_name = self._strip_version(dep)
            self.graph[name].add(dep_name)
            self.reverse_graph[dep_name].add(name)
    
    def _strip_version(self, dep_spec: str) -> str:
        """Strip version constraints from dependency specification"""
        # Handle things like "package >= 1.0", "package = 2.0", etc.
        for op in ['>=', '<=', '==', '=', '>', '<']:
            if op in dep_spec:
                return dep_spec.split(op)[0].strip()
        return dep_spec.strip()
    
    def resolve(self, package_name: str) -> Optional[List[str]]:
        """
        Resolve dependencies for a package
        
        Returns list of packages in build order (dependencies first)
        Returns None if circular dependency detected
        """
        if package_name not in self.packages:
            return None
        
        visited = set()
        visiting = set()
        order = []
        
        if not self._dfs(package_name, visited, visiting, order):
            return None  # Circular dependency
        
        return order
    
    def _dfs(self, package: str, visited: Set[str], visiting: Set[str], 
             order: List[str]) -> bool:
        """
        Depth-first search for topological sort
        
        Returns False if circular dependency detected
        """
        if package in visiting:
            # Circular dependency
            return False
        
        if package in visited:
            return True
        
        visiting.add(package)
        
        # Visit dependencies first
        for dep in self.graph.get(package, set()):
            if dep in self.packages:  # Only process known packages
                if not self._dfs(dep, visited, visiting, order):
                    return False
        
        visiting.remove(package)
        visited.add(package)
        order.append(package)
        
        return True
    
    def get_build_order(self, packages: List[str]) -> Optional[List[str]]:
        """
        Get build order for multiple packages
        
        Returns list of all packages in build order
        Returns None if circular dependency detected
        """
        visited = set()
        visiting = set()
        order = []
        
        for package in packages:
            if package not in visited:
                if not self._dfs(package, visited, visiting, order):
                    return None
        
        return order
    
    def find_cycles(self) -> List[List[str]]:
        """Find all circular dependencies"""
        cycles = []
        visited = set()
        
        for package in self.packages:
            if package not in visited:
                path = []
                if self._find_cycle_dfs(package, visited, path):
                    cycles.append(path)
        
        return cycles
    
    def _find_cycle_dfs(self, package: str, visited: Set[str], 
                        path: List[str]) -> bool:
        """DFS to find cycles"""
        if package in path:
            # Found cycle
            cycle_start = path.index(package)
            path[:] = path[cycle_start:]
            return True
        
        if package in visited:
            return False
        
        visited.add(package)
        path.append(package)
        
        for dep in self.graph.get(package, set()):
            if dep in self.packages:
                if self._find_cycle_dfs(dep, visited, path):
                    return True
        
        path.pop()
        return False
    
    def get_dependents(self, package_name: str) -> Set[str]:
        """Get all packages that depend on this package"""
        return self.reverse_graph.get(package_name, set())
    
    def get_dependencies(self, package_name: str) -> Set[str]:
        """Get direct dependencies of a package"""
        return self.graph.get(package_name, set())
    
    def get_all_dependencies(self, package_name: str) -> Set[str]:
        """Get all transitive dependencies of a package"""
        all_deps = set()
        to_visit = deque([package_name])
        
        while to_visit:
            current = to_visit.popleft()
            
            for dep in self.graph.get(current, set()):
                if dep not in all_deps:
                    all_deps.add(dep)
                    to_visit.append(dep)
        
        return all_deps
    
    def can_remove_safely(self, package_name: str) -> Tuple[bool, List[str]]:
        """
        Check if a package can be removed safely
        
        Returns (can_remove, list_of_dependent_packages)
        """
        dependents = list(self.get_dependents(package_name))
        return len(dependents) == 0, dependents
    
    def suggest_build_groups(self) -> List[List[str]]:
        """
        Suggest groups of packages that can be built in parallel
        
        Returns list of groups, where each group can be built in parallel
        """
        in_degree = {}
        for package in self.packages:
            in_degree[package] = len(self.graph.get(package, set()))
        
        groups = []
        remaining = set(self.packages.keys())
        
        while remaining:
            # Find packages with no dependencies in remaining set
            current_group = []
            for package in remaining:
                deps = self.graph.get(package, set())
                if not deps.intersection(remaining):
                    current_group.append(package)
            
            if not current_group:
                # Circular dependency or error
                break
            
            groups.append(current_group)
            remaining -= set(current_group)
        
        return groups


class VersionResolver:
    """Resolves version constraints for dependencies"""
    
    @staticmethod
    def parse_constraint(constraint: str) -> Tuple[str, str, str]:
        """
        Parse version constraint
        
        Returns (package_name, operator, version)
        Example: "package >= 1.0" -> ("package", ">=", "1.0")
        """
        for op in ['>=', '<=', '==', '>', '<', '=']:
            if op in constraint:
                parts = constraint.split(op, 1)
                return parts[0].strip(), op, parts[1].strip()
        
        return constraint.strip(), '', ''
    
    @staticmethod
    def satisfies(available_version: str, constraint_op: str, 
                  constraint_version: str) -> bool:
        """
        Check if available version satisfies constraint
        
        Simple lexicographic comparison (not semantic versioning)
        """
        if not constraint_op:
            return True
        
        # Simple string comparison (should use proper version comparison)
        if constraint_op == '>=':
            return available_version >= constraint_version
        elif constraint_op == '<=':
            return available_version <= constraint_version
        elif constraint_op == '==':
            return available_version == constraint_version
        elif constraint_op == '>':
            return available_version > constraint_version
        elif constraint_op == '<':
            return available_version < constraint_version
        elif constraint_op == '=':
            return available_version == constraint_version
        
        return False
    
    @staticmethod
    def find_compatible_version(package_name: str, constraint: str,
                               available_versions: List[str]) -> Optional[str]:
        """
        Find a compatible version from available versions
        
        Returns the highest compatible version or None
        """
        name, op, version = VersionResolver.parse_constraint(constraint)
        
        compatible = []
        for avail_ver in available_versions:
            if VersionResolver.satisfies(avail_ver, op, version):
                compatible.append(avail_ver)
        
        if not compatible:
            return None
        
        # Return highest version (simple sort)
        compatible.sort(reverse=True)
        return compatible[0]


# — ShadePacket: "Dependencies... the tangled web we weave. Gotta trace every thread before we can cut it loose."
