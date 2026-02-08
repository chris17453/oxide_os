#!/usr/bin/env python3
"""
SRPM Dependency Graph Engine — SQLite-backed, recursive crawl

— BlackLatch: "Every package is a node in the web. Pull one thread
  and the whole damn dependency tree comes alive. We crawl it ALL now,
  recursively, down to the bedrock. SQLite keeps the receipts."

Uses real dnf/rpm on the host to:
  - Download SRPMs via `dnf download --srpm`
  - Extract BuildRequires via `rpm -qpR`
  - Resolve capability → binary pkg → source RPM via `dnf repoquery`
  - Recursively crawl: resolve deps → fetch THEIR SRPMs → resolve THOSE deps
  - Store everything in SQLite for sub-millisecond lookups
  - Emit Graphviz DOT, topological build order, SCC cycle groups
"""

import os
import re
import json
import sqlite3
import subprocess
import sys
import time
from collections import defaultdict, deque
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple


# — SableWire: "Noise we filter out of BuildRequires. rpmlib() is RPM internals,
#   config() is post-install garbage. We only care about real build deps."
FILTER_PREFIXES = (
    "rpmlib(",
    "config(",
    "usrmerge(",
)

# — ColdCipher: "Source repos we actually trust. Everything else is third-party noise."
SOURCE_REPOS = ["fedora-source", "updates-source"]

# — TorqueJax: "Safety valve. Without a depth limit, bash's transitive closure
#   is half of Fedora. Ask me how I know."
DEFAULT_MAX_DEPTH = 5


def _run(cmd: list, timeout: int = 120) -> Tuple[int, str, str]:
    """Run a subprocess, return (rc, stdout, stderr)."""
    try:
        p = subprocess.run(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, timeout=timeout
        )
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return -1, "", "timeout"
    except FileNotFoundError:
        return -1, "", f"command not found: {cmd[0]}"


def _dnf_repo_flags() -> list:
    """Build --repo flags for source repos only during download."""
    flags = []
    for r in SOURCE_REPOS:
        flags.extend(["--repo", r])
    return flags


# ═══════════════════════════════════════════════════════════════════
#  SQLite Schema
# ═══════════════════════════════════════════════════════════════════

SCHEMA_SQL = """
CREATE TABLE IF NOT EXISTS source_packages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    UNIQUE NOT NULL,
    version     TEXT,
    release     TEXT,
    summary     TEXT,
    license     TEXT,
    url         TEXT,
    srpm_path   TEXT,
    crawled     INTEGER DEFAULT 0,
    depth       INTEGER DEFAULT -1,
    created_at  TEXT    DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS binary_packages (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    name              TEXT    UNIQUE NOT NULL,
    version           TEXT,
    release           TEXT,
    source_package_id INTEGER REFERENCES source_packages(id),
    created_at        TEXT    DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS build_requires (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    source_package_id   INTEGER NOT NULL REFERENCES source_packages(id),
    capability          TEXT    NOT NULL,
    cap_name            TEXT    NOT NULL,
    cap_op              TEXT,
    cap_version         TEXT,
    resolved_binary_id  INTEGER REFERENCES binary_packages(id),
    resolved_source_id  INTEGER REFERENCES source_packages(id),
    UNIQUE(source_package_id, capability)
);

CREATE TABLE IF NOT EXISTS edges (
    from_source_id INTEGER NOT NULL REFERENCES source_packages(id),
    to_source_id   INTEGER NOT NULL REFERENCES source_packages(id),
    PRIMARY KEY (from_source_id, to_source_id)
);

-- — NeonRoot: "The local repo index. Two big dnf dumps — binary→source and
--   capability→binary — cached here so graph building never touches the network."
CREATE TABLE IF NOT EXISTS repo_packages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    UNIQUE NOT NULL,
    sourcerpm   TEXT,
    source_name TEXT
);

CREATE TABLE IF NOT EXISTS repo_provides (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    capability  TEXT    NOT NULL,
    package_id  INTEGER NOT NULL REFERENCES repo_packages(id)
);

CREATE INDEX IF NOT EXISTS idx_br_source ON build_requires(source_package_id);
CREATE INDEX IF NOT EXISTS idx_br_cap    ON build_requires(cap_name);
CREATE INDEX IF NOT EXISTS idx_bin_src   ON binary_packages(source_package_id);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_source_id);
CREATE INDEX IF NOT EXISTS idx_edges_to   ON edges(to_source_id);
CREATE INDEX IF NOT EXISTS idx_rp_name   ON repo_packages(name);
CREATE INDEX IF NOT EXISTS idx_rp_src    ON repo_packages(source_name);
CREATE INDEX IF NOT EXISTS idx_rprov_cap ON repo_provides(capability);
CREATE INDEX IF NOT EXISTS idx_rprov_pkg ON repo_provides(package_id);
"""


# ═══════════════════════════════════════════════════════════════════
#  SRPM Download
# ═══════════════════════════════════════════════════════════════════

def download_srpms(packages: List[str], dest_dir: str,
                   skip_existing: bool = True) -> Dict[str, str]:
    """
    Download SRPMs for a list of package names.
    Returns dict of {package_name: srpm_path}.

    — TorqueJax: "We yank SRPMs straight from Fedora's veins.
      No custom HTTP parsing, no XML nightmares. Just dnf doing its job."
    """
    dest = Path(dest_dir)
    dest.mkdir(parents=True, exist_ok=True)

    results = {}
    existing = {}

    if skip_existing:
        for f in dest.glob("*.src.rpm"):
            n = _srpm_name_from_filename(f.name)
            if n:
                existing[n] = str(f)

    to_download = []
    for pkg in packages:
        if pkg in existing:
            results[pkg] = existing[pkg]
        else:
            to_download.append(pkg)

    if not to_download:
        return results

    cmd = ["dnf", "download", "--srpm", "--destdir", str(dest)]
    cmd.extend(_dnf_repo_flags())
    cmd.extend(["--skip-unavailable"])
    cmd.extend(to_download)

    print(f"⚡ Downloading {len(to_download)} SRPMs...")
    rc, out, err = _run(cmd, timeout=600)

    if rc != 0 and rc != -1:
        print(f"⚠ dnf download returned {rc}", file=sys.stderr)
        if err.strip():
            for line in err.strip().splitlines()[:5]:
                print(f"  {line}", file=sys.stderr)

    # Rescan dest for what we got
    for f in dest.glob("*.src.rpm"):
        name = _srpm_name_from_filename(f.name)
        if name and name in to_download:
            results[name] = str(f)

    downloaded = set(results.keys()) - set(existing.keys())
    skipped = set(to_download) - set(results.keys())

    if downloaded:
        print(f"✓ Downloaded: {', '.join(sorted(downloaded))}")
    if skipped:
        print(f"✗ Not found: {', '.join(sorted(skipped))}")

    return results


def _srpm_name_from_filename(filename: str) -> Optional[str]:
    """
    'bash-5.2.37-1.fc42.src.rpm' → 'bash'

    — GraveShift: "Regex roulette with RPM naming. The name can have dashes,
      the version can't start with a dash. Pray the maintainer was sane."
    """
    m = re.match(r"^(.+)-[^-]+-[^-]+\.src\.rpm$", filename)
    return m.group(1) if m else None


# ═══════════════════════════════════════════════════════════════════
#  RPM metadata extraction
# ═══════════════════════════════════════════════════════════════════

def get_build_requires(srpm_path: str) -> List[str]:
    """
    Extract BuildRequires from an SRPM using rpm -qpR.

    — ShadePacket: "rpm -qpR is the cheat code. No spec parsing,
      no macro expansion headaches. Just raw dependency truth."
    """
    rc, out, _ = _run(["rpm", "-qpR", srpm_path])
    if rc != 0:
        return []

    reqs = []
    for line in out.splitlines():
        s = line.strip()
        if not s:
            continue
        if any(s.startswith(prefix) for prefix in FILTER_PREFIXES):
            continue
        reqs.append(s)
    return reqs


def get_srpm_metadata(srpm_path: str) -> Dict:
    """
    Get name/version/release/summary from an SRPM.

    — WireSaint: "The SRPM header is the birth certificate."
    """
    fmt = "%{NAME}|||%{VERSION}|||%{RELEASE}|||%{SUMMARY}|||%{LICENSE}|||%{URL}"
    rc, out, _ = _run(["rpm", "-qp", "--queryformat", fmt, srpm_path])
    if rc != 0:
        return {}

    parts = out.split("|||")
    if len(parts) < 6:
        return {}

    return {
        "name": parts[0],
        "version": parts[1],
        "release": parts[2],
        "summary": parts[3],
        "license": parts[4],
        "url": parts[5],
    }


# ═══════════════════════════════════════════════════════════════════
#  Version / capability parsing
# ═══════════════════════════════════════════════════════════════════

def _parse_capability(cap_str: str) -> Tuple[str, str, str]:
    """
    Parse 'pkg >= 1.0' → ('pkg', '>=', '1.0')
    Parse 'pkg' → ('pkg', '', '')
    """
    for op in (">=", "<=", "==", "!=", ">", "<", "="):
        if f" {op} " in cap_str:
            parts = cap_str.split(f" {op} ", 1)
            return parts[0].strip(), op, parts[1].strip()
    return cap_str.strip(), "", ""


def _strip_version(dep_spec: str) -> str:
    """Strip version constraints: 'pkg >= 1.0' → 'pkg'"""
    name, _, _ = _parse_capability(dep_spec)
    return name


# ═══════════════════════════════════════════════════════════════════
#  Repository Metadata Sync (local-first resolution)
# ═══════════════════════════════════════════════════════════════════

def sync_repo_metadata(db: sqlite3.Connection,
                       progress: bool = True) -> Dict[str, int]:
    """
    Pre-build the local provides → source index from two bulk dnf queries.
    This replaces thousands of per-capability subprocess calls with TWO big
    queries that take ~15 seconds total and never need to run again.

    — ColdCipher: "Two queries. 72K binary packages. 742K provides.
      All cached in SQLite. Now graph building is offline and instant.
      No more serial dnf calls. No more waiting. No more excuses."

    Query 1: binary_package → source_rpm (all 72K+ entries)
    Query 2: capability → binary_package (all 742K+ provides)
    """
    t0 = time.time()

    # ── Query 1: Binary packages → Source RPMs ─────────────────
    if progress:
        print("  Syncing binary→source mappings...", end="", flush=True)

    rc, out, err = _run([
        "dnf", "repoquery", "--available",
        "--queryformat", "%{name}|||%{version}|||%{release}|||%{sourcerpm}\n",
        "--latest-limit", "1"
    ], timeout=300)

    if rc != 0:
        if progress:
            print(f" FAILED (rc={rc})")
        return {"packages": 0, "provides": 0, "time_s": 0}

    # — TorqueJax: "Batch insert with transaction. Anything less is amateur hour."
    db.execute("DELETE FROM repo_provides")
    db.execute("DELETE FROM repo_packages")
    db.commit()

    pkg_count = 0
    pkg_ids = {}  # name → id

    db.execute("BEGIN")
    for line in out.splitlines():
        line = line.strip()
        if not line or "|||" not in line:
            continue
        parts = line.split("|||")
        if len(parts) < 4:
            continue
        bin_name = parts[0]
        sourcerpm = parts[3]
        if not sourcerpm or not bin_name:
            continue

        # Parse source name from sourcerpm filename
        src_name = _srpm_name_from_filename(sourcerpm)
        if not src_name:
            continue

        try:
            cur = db.execute(
                "INSERT OR IGNORE INTO repo_packages (name, sourcerpm, source_name) "
                "VALUES (?, ?, ?)", (bin_name, sourcerpm, src_name)
            )
            if cur.lastrowid:
                pkg_ids[bin_name] = cur.lastrowid
                pkg_count += 1
            else:
                row = db.execute(
                    "SELECT id FROM repo_packages WHERE name = ?", (bin_name,)
                ).fetchone()
                if row:
                    pkg_ids[bin_name] = row[0]
        except sqlite3.IntegrityError:
            pass

    db.commit()
    t1 = time.time()
    if progress:
        print(f" {pkg_count} packages ({t1 - t0:.1f}s)")

    # ── Query 2: Provides → Binary packages ────────────────────
    if progress:
        print("  Syncing provides→binary mappings...", end="", flush=True)

    rc2, out2, _ = _run([
        "dnf", "repoquery", "--available",
        "--queryformat", "%{name}|||%{provides}\n",
        "--latest-limit", "1"
    ], timeout=300)

    if rc2 != 0:
        if progress:
            print(f" FAILED (rc={rc2})")
        return {"packages": pkg_count, "provides": 0, "time_s": t1 - t0}

    prov_count = 0
    batch = []
    BATCH_SIZE = 5000

    for line in out2.splitlines():
        line = line.strip()
        if not line or "|||" not in line:
            continue
        parts = line.split("|||", 1)
        if len(parts) < 2:
            continue
        bin_name = parts[0]
        cap_raw = parts[1].strip()
        if not cap_raw or not bin_name:
            continue

        pkg_id = pkg_ids.get(bin_name)
        if not pkg_id:
            row = db.execute(
                "SELECT id FROM repo_packages WHERE name = ?", (bin_name,)
            ).fetchone()
            if row:
                pkg_id = row[0]
                pkg_ids[bin_name] = pkg_id
            else:
                continue

        # Strip version from capability for indexing
        cap_name = _strip_version(cap_raw)
        batch.append((cap_name, pkg_id))
        prov_count += 1

        if len(batch) >= BATCH_SIZE:
            db.executemany(
                "INSERT INTO repo_provides (capability, package_id) VALUES (?, ?)",
                batch
            )
            db.commit()
            batch.clear()

    if batch:
        db.executemany(
            "INSERT INTO repo_provides (capability, package_id) VALUES (?, ?)",
            batch
        )
        db.commit()

    t2 = time.time()
    if progress:
        print(f" {prov_count} provides ({t2 - t1:.1f}s)")
        print(f"  ✓ Sync complete: {pkg_count} packages, {prov_count} provides "
              f"in {t2 - t0:.1f}s")

    return {"packages": pkg_count, "provides": prov_count, "time_s": round(t2 - t0, 1)}


def is_synced(db: sqlite3.Connection) -> bool:
    """Check if repo metadata has been synced."""
    try:
        row = db.execute("SELECT COUNT(*) FROM repo_packages").fetchone()
        return row[0] > 0
    except Exception:
        return False


# ═══════════════════════════════════════════════════════════════════
#  Capability → Source RPM Resolution (with SQLite caching)
# ═══════════════════════════════════════════════════════════════════

class CapabilityResolver:
    """
    Resolves RPM capabilities to source package names.
    capability → binary package → source RPM name

    LOCAL-FIRST: If repo_packages/repo_provides are synced, resolves
    entirely from SQLite (microseconds). Falls back to dnf subprocess
    only if the local index misses.

    — ColdCipher: "Two hops to trace any build dep back to its source.
      After sync, the entire Fedora provides database is local.
      No network, no subprocess, no waiting."
    """

    def __init__(self, db: sqlite3.Connection):
        self._db = db
        self._stats = {"cap_hits": 0, "cap_misses": 0,
                       "src_hits": 0, "src_misses": 0,
                       "dnf_queries": 0, "local_hits": 0}
        self._synced = is_synced(db)

    def whatprovides(self, capability: str) -> Optional[Tuple[str, str, str]]:
        """
        Resolve capability → (binary_pkg_name, version, release).
        Returns None if nothing provides it.
        """
        cap_name = _strip_version(capability)

        # ── Check DB cache first (from previous resolution) ────
        row = self._db.execute(
            "SELECT bp.name, bp.version, bp.release FROM binary_packages bp "
            "JOIN build_requires br ON br.resolved_binary_id = bp.id "
            "WHERE br.cap_name = ? LIMIT 1", (cap_name,)
        ).fetchone()

        if row:
            self._stats["cap_hits"] += 1
            return row[0], row[1] or "", row[2] or ""

        # Check if we have the binary package directly by name
        row = self._db.execute(
            "SELECT name, version, release FROM binary_packages WHERE name = ?",
            (cap_name,)
        ).fetchone()
        if row:
            self._stats["cap_hits"] += 1
            return row[0], row[1] or "", row[2] or ""

        # ── Check local repo index (from sync) ────────────────
        if self._synced:
            row = self._db.execute(
                "SELECT rp.name FROM repo_provides rprov "
                "JOIN repo_packages rp ON rprov.package_id = rp.id "
                "WHERE rprov.capability = ? LIMIT 1", (cap_name,)
            ).fetchone()

            if not row:
                # Try exact package name match
                row = self._db.execute(
                    "SELECT name FROM repo_packages WHERE name = ?", (cap_name,)
                ).fetchone()

            if row:
                pkg_name = row[0]
                self._stats["local_hits"] += 1
                self._stats["cap_hits"] += 1
                self._ensure_binary_package(pkg_name)
                return pkg_name, "", ""

            self._stats["cap_misses"] += 1
            return None

        # ── Fallback to dnf subprocess (pre-sync path) ────────
        self._stats["cap_misses"] += 1
        self._stats["dnf_queries"] += 1

        rc, out, _ = _run([
            "dnf", "repoquery", "--whatprovides", cap_name,
            "--queryformat", "%{name}|||%{version}|||%{release}",
            "--latest-limit", "1"
        ])

        if rc != 0 or not out.strip():
            return None

        lines = [l.strip() for l in out.strip().splitlines() if l.strip()]
        if not lines:
            return None

        parts = lines[0].split("|||")
        pkg_name = parts[0]
        pkg_ver = parts[1] if len(parts) > 1 else ""
        pkg_rel = parts[2] if len(parts) > 2 else ""

        self._ensure_binary_package(pkg_name, pkg_ver, pkg_rel)
        return pkg_name, pkg_ver, pkg_rel

    def sourcerpm_of(self, binary_pkg: str) -> Optional[Tuple[str, str, str]]:
        """
        Resolve binary package → (source_pkg_name, version, release).
        """
        # Check if binary already has a source_package_id
        row = self._db.execute(
            "SELECT sp.name, sp.version, sp.release "
            "FROM binary_packages bp "
            "JOIN source_packages sp ON bp.source_package_id = sp.id "
            "WHERE bp.name = ?", (binary_pkg,)
        ).fetchone()

        if row:
            self._stats["src_hits"] += 1
            return row[0], row[1] or "", row[2] or ""

        # ── Check local repo index ────────────────────────────
        if self._synced:
            row = self._db.execute(
                "SELECT source_name, sourcerpm FROM repo_packages WHERE name = ?",
                (binary_pkg,)
            ).fetchone()

            if row and row[0]:
                src_name = row[0]
                sourcerpm = row[1] or ""
                src_ver, src_rel = "", ""
                if sourcerpm and src_name:
                    remainder = sourcerpm[len(src_name) + 1:]
                    remainder = remainder.replace(".src.rpm", "")
                    vr_parts = remainder.rsplit("-", 1)
                    src_ver = vr_parts[0] if vr_parts else ""
                    src_rel = vr_parts[1] if len(vr_parts) > 1 else ""

                self._stats["local_hits"] += 1
                self._stats["src_hits"] += 1

                src_id = self._ensure_source_package(src_name, src_ver, src_rel)
                bin_id = self._get_binary_id(binary_pkg)
                if bin_id and src_id:
                    self._db.execute(
                        "UPDATE binary_packages SET source_package_id = ? "
                        "WHERE id = ?", (src_id, bin_id)
                    )
                return src_name, src_ver, src_rel

        # ── Fallback to dnf subprocess ────────────────────────
        self._stats["src_misses"] += 1
        self._stats["dnf_queries"] += 1

        rc, out, _ = _run([
            "dnf", "repoquery", binary_pkg,
            "--sourcerpm", "--latest-limit", "1"
        ])

        if rc != 0 or not out.strip():
            return None

        lines = [l.strip() for l in out.strip().splitlines() if l.strip()]
        if not lines:
            return None

        src_filename = lines[0]
        src_name = _srpm_name_from_filename(src_filename)
        if not src_name:
            return None

        remainder = src_filename[len(src_name) + 1:]
        remainder = remainder.replace(".src.rpm", "")
        vr_parts = remainder.rsplit("-", 1)
        src_ver = vr_parts[0] if vr_parts else ""
        src_rel = vr_parts[1] if len(vr_parts) > 1 else ""

        src_id = self._ensure_source_package(src_name, src_ver, src_rel)
        bin_id = self._get_binary_id(binary_pkg)
        if bin_id and src_id:
            self._db.execute(
                "UPDATE binary_packages SET source_package_id = ? WHERE id = ?",
                (src_id, bin_id)
            )
            self._db.commit()

        return src_name, src_ver, src_rel

    def resolve_to_source(self, capability: str) -> Optional[str]:
        """Full pipeline: capability → source package name."""
        result = self.whatprovides(capability)
        if not result:
            return None
        pkg_name, _, _ = result
        src = self.sourcerpm_of(pkg_name)
        if not src:
            return None
        return src[0]

    def _ensure_binary_package(self, name: str, version: str = "",
                                release: str = "") -> int:
        """Insert or get binary package, return id."""
        row = self._db.execute(
            "SELECT id FROM binary_packages WHERE name = ?", (name,)
        ).fetchone()
        if row:
            # Update version if we have newer info
            if version:
                self._db.execute(
                    "UPDATE binary_packages SET version = ?, release = ? "
                    "WHERE id = ? AND (version IS NULL OR version = '')",
                    (version, release, row[0])
                )
                self._db.commit()
            return row[0]

        cur = self._db.execute(
            "INSERT INTO binary_packages (name, version, release) VALUES (?, ?, ?)",
            (name, version, release)
        )
        self._db.commit()
        return cur.lastrowid

    def _ensure_source_package(self, name: str, version: str = "",
                                release: str = "") -> int:
        """Insert or get source package, return id."""
        row = self._db.execute(
            "SELECT id FROM source_packages WHERE name = ?", (name,)
        ).fetchone()
        if row:
            if version:
                self._db.execute(
                    "UPDATE source_packages SET version = ?, release = ? "
                    "WHERE id = ? AND (version IS NULL OR version = '')",
                    (version, release, row[0])
                )
                self._db.commit()
            return row[0]

        cur = self._db.execute(
            "INSERT INTO source_packages (name, version, release) VALUES (?, ?, ?)",
            (name, version, release)
        )
        self._db.commit()
        return cur.lastrowid

    def _get_binary_id(self, name: str) -> Optional[int]:
        row = self._db.execute(
            "SELECT id FROM binary_packages WHERE name = ?", (name,)
        ).fetchone()
        return row[0] if row else None

    @property
    def stats(self) -> Dict:
        return dict(self._stats)


# ═══════════════════════════════════════════════════════════════════
#  The Graph — SQLite-backed, recursive BFS crawl
# ═══════════════════════════════════════════════════════════════════

class SRPMGraph:
    """
    SRPM build dependency graph backed by SQLite.

    Nodes = source packages, Edges = build-dependency relationships.
    Recursive crawl: resolve deps → fetch dep SRPMs → resolve THEIR deps.

    — BlackLatch: "The dependency graph is the map of the underworld.
      Every node is a package that owes its existence to its neighbors.
      SQLite is the ledger. We never forget."
    """

    def __init__(self, db_path: str):
        """
        Open or create the graph database.

        Args:
            db_path: Path to SQLite database file
        """
        self._db_path = db_path
        self._db = sqlite3.connect(db_path)
        self._db.execute("PRAGMA journal_mode=WAL")
        self._db.execute("PRAGMA synchronous=NORMAL")
        self._db.executescript(SCHEMA_SQL)
        self._resolver = CapabilityResolver(self._db)

    def close(self):
        self._db.close()

    @property
    def db(self) -> sqlite3.Connection:
        return self._db

    # ── Ingest a single SRPM ──────────────────────────────────

    def ingest_srpm(self, srpm_path: str, depth: int = 0) -> Optional[str]:
        """
        Add an SRPM to the graph: read metadata, extract BuildRequires,
        resolve each to a source package, record edges.

        Returns source package name, or None on failure.
        """
        meta = get_srpm_metadata(srpm_path)
        name = meta.get("name") or _srpm_name_from_filename(
            os.path.basename(srpm_path))
        if not name:
            return None

        # Upsert source package
        row = self._db.execute(
            "SELECT id, crawled FROM source_packages WHERE name = ?", (name,)
        ).fetchone()

        if row:
            src_id = row[0]
            if row[1]:
                return name  # already crawled
            self._db.execute(
                "UPDATE source_packages SET version=?, release=?, summary=?, "
                "license=?, url=?, srpm_path=?, crawled=1, depth=? WHERE id=?",
                (meta.get("version"), meta.get("release"), meta.get("summary"),
                 meta.get("license"), meta.get("url"), srpm_path, depth, src_id)
            )
        else:
            cur = self._db.execute(
                "INSERT INTO source_packages "
                "(name, version, release, summary, license, url, srpm_path, crawled, depth) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?)",
                (name, meta.get("version"), meta.get("release"),
                 meta.get("summary"), meta.get("license"), meta.get("url"),
                 srpm_path, depth)
            )
            src_id = cur.lastrowid

        self._db.commit()

        # Extract and resolve BuildRequires
        reqs = get_build_requires(srpm_path)
        for cap in reqs:
            self._resolve_and_record(src_id, name, cap)

        self._db.commit()
        return name

    def _resolve_and_record(self, src_id: int, src_name: str, capability: str):
        """Resolve one BuildRequires capability and record in DB."""
        cap_name, cap_op, cap_ver = _parse_capability(capability)

        # Check if already recorded
        existing = self._db.execute(
            "SELECT id FROM build_requires "
            "WHERE source_package_id = ? AND capability = ?",
            (src_id, capability)
        ).fetchone()
        if existing:
            return

        # Resolve capability → binary → source
        bin_result = self._resolver.whatprovides(capability)
        bin_id = None
        dep_src_id = None
        dep_src_name = None

        if bin_result:
            bin_name, bin_ver, bin_rel = bin_result
            bin_id = self._resolver._ensure_binary_package(bin_name, bin_ver, bin_rel)

            src_result = self._resolver.sourcerpm_of(bin_name)
            if src_result:
                dep_src_name, dep_src_ver, dep_src_rel = src_result
                dep_src_id = self._resolver._ensure_source_package(
                    dep_src_name, dep_src_ver, dep_src_rel)

        # Record the build_requires row
        self._db.execute(
            "INSERT OR IGNORE INTO build_requires "
            "(source_package_id, capability, cap_name, cap_op, cap_version, "
            " resolved_binary_id, resolved_source_id) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (src_id, capability, cap_name, cap_op or None, cap_ver or None,
             bin_id, dep_src_id)
        )

        # Record the edge (src → dep_src)
        if dep_src_id and dep_src_id != src_id:
            self._db.execute(
                "INSERT OR IGNORE INTO edges (from_source_id, to_source_id) "
                "VALUES (?, ?)", (src_id, dep_src_id)
            )

    # ── Recursive crawl ───────────────────────────────────────

    def crawl(self, root_packages: List[str], srpms_dir: str,
              max_depth: int = DEFAULT_MAX_DEPTH,
              download: bool = True) -> Dict[str, int]:
        """
        Recursive BFS crawl starting from root packages.

        1. Download root SRPMs
        2. Ingest each → extract BuildRequires → resolve to source pkgs
        3. For each unvisited dep source pkg, download ITS SRPM
        4. Repeat up to max_depth layers

        Returns {pkg_name: depth} for all crawled packages.

        — NeonRoot: "The recursive crawl is the beating heart. We start with
          your root packages and chase every BuildRequires down the rabbit hole.
          SQLite keeps score so we never re-query. max_depth is your sanity valve."
        """
        crawled = {}
        queue = deque()  # (package_name, depth)

        # Seed with root packages
        if download:
            results = download_srpms(root_packages, srpms_dir)
        else:
            results = {}
            for f in Path(srpms_dir).glob("*.src.rpm"):
                n = _srpm_name_from_filename(f.name)
                if n and n in root_packages:
                    results[n] = str(f)

        for pkg, path in results.items():
            queue.append((pkg, path, 0))

        while queue:
            pkg_name, srpm_path, depth = queue.popleft()

            if pkg_name in crawled:
                continue
            if depth > max_depth:
                continue

            print(f"  {'  ' * depth}↳ [{depth}] {pkg_name}")
            name = self.ingest_srpm(srpm_path, depth)
            if not name:
                continue
            crawled[name] = depth

            # Find uncrawled dependencies
            if depth < max_depth:
                uncrawled = self._get_uncrawled_deps(name)
                if uncrawled:
                    # Download their SRPMs
                    dep_results = download_srpms(uncrawled, srpms_dir)
                    for dep_name, dep_path in dep_results.items():
                        if dep_name not in crawled:
                            queue.append((dep_name, dep_path, depth + 1))

        return crawled

    def _get_uncrawled_deps(self, package_name: str) -> List[str]:
        """Get dependency source packages that haven't been crawled yet."""
        rows = self._db.execute(
            "SELECT DISTINCT sp.name FROM edges e "
            "JOIN source_packages sp_from ON e.from_source_id = sp_from.id "
            "JOIN source_packages sp ON e.to_source_id = sp.id "
            "WHERE sp_from.name = ? AND sp.crawled = 0",
            (package_name,)
        ).fetchall()
        return [r[0] for r in rows]

    # ── Local-first graph build ───────────────────────────────

    def build_local(self, srpms_dir: str, packages: List[str] = None) -> int:
        """
        Build graph from locally downloaded SRPMs. No network calls.
        Requires sync_repo_metadata() to have been run first.

        Three-phase pipeline:
          1. EXTRACT: Scan all SRPMs, pull metadata + BuildRequires (rpm -qpR)
          2. RESOLVE: Map every capability → source package via local SQLite index
          3. WIRE: Create edges, commit

        Returns number of packages processed.

        — BlackLatch: "Phase 1 is fast — just rpm headers. Phase 2 is instant —
          pure SQLite lookups. Phase 3 is a batch INSERT. The whole thing takes
          seconds, not minutes. That's the power of syncing first."
        """
        if not is_synced(self._db):
            print("⚠ Repo metadata not synced. Run: oxdnf sync", file=sys.stderr)
            print("  (This pre-caches 72K+ binary→source mappings locally)",
                  file=sys.stderr)
            return 0

        import glob as glob_mod
        t0 = time.time()

        srpms = sorted(glob_mod.glob(os.path.join(srpms_dir, "*.src.rpm")))
        if not srpms:
            print(f"No SRPMs in {srpms_dir}", file=sys.stderr)
            return 0

        # Filter to specific packages if requested
        if packages:
            pkg_set = set(packages)
            srpms = [s for s in srpms
                     if _srpm_name_from_filename(os.path.basename(s)) in pkg_set]
            if not srpms:
                print(f"None of {packages} found in {srpms_dir}", file=sys.stderr)
                return 0

        # ── Phase 1: Extract metadata from all SRPMs ──────────
        print(f"  Phase 1: Extracting metadata from {len(srpms)} SRPMs...")
        t1 = time.time()

        pkg_data = {}  # name → (src_id, [capabilities])
        for srpm_path in srpms:
            meta = get_srpm_metadata(srpm_path)
            name = meta.get("name") or _srpm_name_from_filename(
                os.path.basename(srpm_path))
            if not name:
                continue

            # Upsert source package
            row = self._db.execute(
                "SELECT id, crawled FROM source_packages WHERE name = ?", (name,)
            ).fetchone()

            if row:
                src_id = row[0]
                if row[1]:
                    # Already crawled — skip re-ingestion but keep in pkg_data
                    reqs = get_build_requires(srpm_path)
                    pkg_data[name] = (src_id, reqs)
                    continue
                self._db.execute(
                    "UPDATE source_packages SET version=?, release=?, summary=?, "
                    "license=?, url=?, srpm_path=?, crawled=1, depth=0 WHERE id=?",
                    (meta.get("version"), meta.get("release"), meta.get("summary"),
                     meta.get("license"), meta.get("url"), srpm_path, src_id)
                )
            else:
                cur = self._db.execute(
                    "INSERT INTO source_packages "
                    "(name, version, release, summary, license, url, srpm_path, crawled, depth) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?, 1, 0)",
                    (name, meta.get("version"), meta.get("release"),
                     meta.get("summary"), meta.get("license"), meta.get("url"),
                     srpm_path)
                )
                src_id = cur.lastrowid

            reqs = get_build_requires(srpm_path)
            pkg_data[name] = (src_id, reqs)

        self._db.commit()
        t2 = time.time()
        total_caps = sum(len(caps) for _, caps in pkg_data.values())
        print(f"    {len(pkg_data)} packages, {total_caps} BuildRequires ({t2 - t1:.1f}s)")

        # ── Phase 2: Resolve capabilities → source packages ───
        print(f"  Phase 2: Resolving capabilities (local DB)...")
        resolved_count = 0
        unresolved_count = 0

        for pkg_name, (src_id, reqs) in pkg_data.items():
            for cap in reqs:
                cap_name, cap_op, cap_ver = _parse_capability(cap)

                # Skip if already recorded
                existing = self._db.execute(
                    "SELECT id FROM build_requires "
                    "WHERE source_package_id = ? AND capability = ?",
                    (src_id, cap)
                ).fetchone()
                if existing:
                    continue

                # Resolve capability → binary → source (all local)
                bin_result = self._resolver.whatprovides(cap)
                bin_id = None
                dep_src_id = None

                if bin_result:
                    bin_name, bin_ver, bin_rel = bin_result
                    bin_id = self._resolver._ensure_binary_package(
                        bin_name, bin_ver, bin_rel)

                    src_result = self._resolver.sourcerpm_of(bin_name)
                    if src_result:
                        dep_src_name, dep_src_ver, dep_src_rel = src_result
                        dep_src_id = self._resolver._ensure_source_package(
                            dep_src_name, dep_src_ver, dep_src_rel)
                        resolved_count += 1
                    else:
                        unresolved_count += 1
                else:
                    unresolved_count += 1

                # Record build_requires
                self._db.execute(
                    "INSERT OR IGNORE INTO build_requires "
                    "(source_package_id, capability, cap_name, cap_op, cap_version, "
                    " resolved_binary_id, resolved_source_id) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (src_id, cap, cap_name, cap_op or None, cap_ver or None,
                     bin_id, dep_src_id)
                )

                # Record edge
                if dep_src_id and dep_src_id != src_id:
                    self._db.execute(
                        "INSERT OR IGNORE INTO edges (from_source_id, to_source_id) "
                        "VALUES (?, ?)", (src_id, dep_src_id)
                    )

        self._db.commit()
        t3 = time.time()
        print(f"    {resolved_count} resolved, {unresolved_count} unresolved ({t3 - t2:.1f}s)")

        # ── Done ──────────────────────────────────────────────
        total = time.time() - t0
        print(f"  ✓ Graph built in {total:.1f}s "
              f"(0 dnf queries, {self._resolver.stats.get('local_hits', 0)} local hits)")

        return len(pkg_data)

    # ── Query methods ─────────────────────────────────────────

    def get_nodes(self) -> Set[str]:
        """All source package names in the graph."""
        rows = self._db.execute("SELECT name FROM source_packages").fetchall()
        return {r[0] for r in rows}

    def get_edges(self) -> Set[Tuple[str, str]]:
        """All (from_name, to_name) edges."""
        rows = self._db.execute(
            "SELECT sf.name, st.name FROM edges e "
            "JOIN source_packages sf ON e.from_source_id = sf.id "
            "JOIN source_packages st ON e.to_source_id = st.id"
        ).fetchall()
        return {(r[0], r[1]) for r in rows}

    def deps_of(self, package: str) -> Set[str]:
        """Direct build-dependencies of a source package."""
        rows = self._db.execute(
            "SELECT sp.name FROM edges e "
            "JOIN source_packages sf ON e.from_source_id = sf.id "
            "JOIN source_packages sp ON e.to_source_id = sp.id "
            "WHERE sf.name = ?", (package,)
        ).fetchall()
        return {r[0] for r in rows}

    def rdeps_of(self, package: str) -> Set[str]:
        """Packages that build-depend on this one."""
        rows = self._db.execute(
            "SELECT sf.name FROM edges e "
            "JOIN source_packages sf ON e.from_source_id = sf.id "
            "JOIN source_packages sp ON e.to_source_id = sp.id "
            "WHERE sp.name = ?", (package,)
        ).fetchall()
        return {r[0] for r in rows}

    def transitive_deps(self, package: str) -> Set[str]:
        """All transitive build-dependencies via BFS."""
        visited = set()
        q = deque([package])
        while q:
            cur = q.popleft()
            for dep in self.deps_of(cur):
                if dep not in visited:
                    visited.add(dep)
                    q.append(dep)
        return visited

    def transitive_rdeps(self, package: str) -> Set[str]:
        """All packages transitively depending on this one."""
        visited = set()
        q = deque([package])
        while q:
            cur = q.popleft()
            for rdep in self.rdeps_of(cur):
                if rdep not in visited:
                    visited.add(rdep)
                    q.append(rdep)
        return visited

    def get_build_requires_for(self, package: str) -> List[Dict]:
        """
        Get detailed BuildRequires for a package with resolution info.

        — ShadePacket: "The full story on every build dep — raw capability,
          which binary provides it, which source it traces back to. All of it."
        """
        rows = self._db.execute(
            "SELECT br.capability, br.cap_name, br.cap_op, br.cap_version, "
            "       bp.name AS bin_name, bp.version AS bin_ver, "
            "       sp_dep.name AS src_name, sp_dep.version AS src_ver "
            "FROM build_requires br "
            "JOIN source_packages sp ON br.source_package_id = sp.id "
            "LEFT JOIN binary_packages bp ON br.resolved_binary_id = bp.id "
            "LEFT JOIN source_packages sp_dep ON br.resolved_source_id = sp_dep.id "
            "WHERE sp.name = ? "
            "ORDER BY br.cap_name",
            (package,)
        ).fetchall()

        return [{
            "capability": r[0],
            "cap_name": r[1],
            "cap_op": r[2],
            "cap_version": r[3],
            "binary_package": r[4],
            "binary_version": r[5],
            "source_package": r[6],
            "source_version": r[7],
        } for r in rows]

    def get_package_info(self, package: str) -> Optional[Dict]:
        """Get full metadata for a source package."""
        row = self._db.execute(
            "SELECT name, version, release, summary, license, url, "
            "       srpm_path, crawled, depth FROM source_packages WHERE name = ?",
            (package,)
        ).fetchone()
        if not row:
            return None

        return {
            "name": row[0], "version": row[1], "release": row[2],
            "summary": row[3], "license": row[4], "url": row[5],
            "srpm_path": row[6], "crawled": bool(row[7]), "depth": row[8],
        }

    def get_unresolved(self, package: str = None) -> List[Dict]:
        """Get unresolved BuildRequires (no source package found)."""
        if package:
            rows = self._db.execute(
                "SELECT sp.name, br.capability, br.cap_name "
                "FROM build_requires br "
                "JOIN source_packages sp ON br.source_package_id = sp.id "
                "WHERE sp.name = ? AND br.resolved_source_id IS NULL",
                (package,)
            ).fetchall()
        else:
            rows = self._db.execute(
                "SELECT sp.name, br.capability, br.cap_name "
                "FROM build_requires br "
                "JOIN source_packages sp ON br.source_package_id = sp.id "
                "WHERE br.resolved_source_id IS NULL"
            ).fetchall()

        return [{"package": r[0], "capability": r[1], "cap_name": r[2]}
                for r in rows]

    # ── DB stats ──────────────────────────────────────────────

    def db_stats(self) -> Dict:
        """Database statistics."""
        src_count = self._db.execute(
            "SELECT COUNT(*) FROM source_packages").fetchone()[0]
        src_crawled = self._db.execute(
            "SELECT COUNT(*) FROM source_packages WHERE crawled = 1").fetchone()[0]
        bin_count = self._db.execute(
            "SELECT COUNT(*) FROM binary_packages").fetchone()[0]
        br_count = self._db.execute(
            "SELECT COUNT(*) FROM build_requires").fetchone()[0]
        br_resolved = self._db.execute(
            "SELECT COUNT(*) FROM build_requires "
            "WHERE resolved_source_id IS NOT NULL").fetchone()[0]
        edge_count = self._db.execute(
            "SELECT COUNT(*) FROM edges").fetchone()[0]

        return {
            "source_packages": src_count,
            "source_crawled": src_crawled,
            "binary_packages": bin_count,
            "build_requires_total": br_count,
            "build_requires_resolved": br_resolved,
            "build_requires_unresolved": br_count - br_resolved,
            "edges": edge_count,
            "db_size_kb": os.path.getsize(self._db_path) // 1024,
            "resolver": self._resolver.stats,
        }

    # ── Topological sort ──────────────────────────────────────

    def topo_sort(self) -> Optional[List[str]]:
        """
        Kahn's algorithm topological sort.
        Returns None if cycles exist.

        — ShadePacket: "Topo sort is the build order bible.
          If it returns None, you've got circular deps."
        """
        nodes = self.get_nodes()
        edges = self.get_edges()

        in_degree = defaultdict(int)
        for n in nodes:
            in_degree.setdefault(n, 0)
        for _, b in edges:
            in_degree[b] += 1

        q = deque(sorted(n for n in nodes if in_degree[n] == 0))
        order = []

        while q:
            node = q.popleft()
            order.append(node)
            for dep in sorted(self.deps_of(node)):
                in_degree[dep] -= 1
                if in_degree[dep] == 0:
                    q.append(dep)

        if len(order) != len(nodes):
            return None
        return list(reversed(order))

    def scc_groups(self) -> List[List[str]]:
        """
        Tarjan's SCC — find strongly connected components (cycles).

        — NeonRoot: "Mutual build deps — the chicken-and-egg problem.
          Tarjan finds 'em, we break 'em with bootstrap builds."
        """
        nodes = self.get_nodes()
        adj = defaultdict(set)
        for a, b in self.get_edges():
            adj[a].add(b)

        index_counter = [0]
        stack = []
        lowlink = {}
        index = {}
        on_stack = set()
        sccs = []

        def strongconnect(v):
            index[v] = index_counter[0]
            lowlink[v] = index_counter[0]
            index_counter[0] += 1
            stack.append(v)
            on_stack.add(v)

            for w in sorted(adj.get(v, set())):
                if w not in index:
                    strongconnect(w)
                    lowlink[v] = min(lowlink[v], lowlink[w])
                elif w in on_stack:
                    lowlink[v] = min(lowlink[v], index[w])

            if lowlink[v] == index[v]:
                scc = []
                while True:
                    w = stack.pop()
                    on_stack.discard(w)
                    scc.append(w)
                    if w == v:
                        break
                sccs.append(sorted(scc))

        for v in sorted(nodes):
            if v not in index:
                strongconnect(v)

        return sccs

    def parallel_build_groups(self) -> List[List[str]]:
        """
        Layer packages into groups buildable in parallel.

        — TorqueJax: "Parallelism is free speed. Layer the build order
          and throw cores at it."
        """
        sccs = self.scc_groups()
        nodes = self.get_nodes()
        edges = self.get_edges()

        scc_map = {}
        for i, scc in enumerate(sccs):
            for node in scc:
                scc_map[node] = i

        cond_edges = set()
        for a, b in edges:
            si, sj = scc_map.get(a), scc_map.get(b)
            if si is not None and sj is not None and si != sj:
                cond_edges.add((si, sj))

        in_degree = defaultdict(int)
        for i in range(len(sccs)):
            in_degree.setdefault(i, 0)
        for a, b in cond_edges:
            in_degree[b] += 1

        q = deque(i for i in range(len(sccs)) if in_degree[i] == 0)
        groups = []

        while q:
            current = list(q)
            q.clear()
            layer = []
            for ci in current:
                layer.extend(sccs[ci])

            groups.append(sorted(layer))

            for ci in current:
                for a, b in cond_edges:
                    if a == ci:
                        in_degree[b] -= 1
                        if in_degree[b] == 0:
                            q.append(b)

        return groups

    # ── Output formats ────────────────────────────────────────

    def to_dot(self, title: str = "srpm_build_deps") -> str:
        """
        Emit Graphviz DOT format.

        — GlassSignal: "DOT is ugly text that makes beautiful graphs.
          Nodes colored by crawl depth. Cycles in red."
        """
        nodes = self.get_nodes()
        edges = self.get_edges()

        # Depth coloring
        depth_colors = [
            "#00ff41",  # depth 0 — roots (green)
            "#00ccff",  # depth 1 (cyan)
            "#ff9900",  # depth 2 (orange)
            "#ff00ff",  # depth 3 (magenta)
            "#ffff00",  # depth 4 (yellow)
            "#888888",  # depth 5+ (grey)
        ]

        cycle_nodes = set()
        for scc in self.scc_groups():
            if len(scc) > 1:
                cycle_nodes.update(scc)

        lines = [f'digraph {title} {{']
        lines.append('  rankdir="LR";')
        lines.append('  node [shape=box, style="filled", fontname="monospace"];')
        lines.append('  edge [color="#ff006688"];')
        lines.append(f'  label="OXIDE OS SRPM Build Dependencies '
                      f'({len(nodes)} src pkgs, {len(edges)} edges)";')
        lines.append('  labelloc="t";')
        lines.append('  fontcolor="#00ff41";')
        lines.append('  fontname="monospace";')
        lines.append('  bgcolor="#0d0d0d";')
        lines.append("")

        for n in sorted(nodes):
            safe = n.replace('"', '\\"')
            info = self.get_package_info(n)
            depth = info.get("depth", -1) if info else -1
            ver = info.get("version", "") if info else ""
            crawled = info.get("crawled", False) if info else False

            label = f"{n}\\n{ver}" if ver else n

            if n in cycle_nodes:
                lines.append(f'  "{safe}" [label="{label}", '
                             f'fillcolor="#660000", fontcolor="#ff4444"];')
            elif not crawled:
                lines.append(f'  "{safe}" [label="{label}", '
                             f'fillcolor="#1a1a2e", fontcolor="#555555", '
                             f'style="filled,dashed"];')
            else:
                ci = min(depth, len(depth_colors) - 1) if depth >= 0 else len(depth_colors) - 1
                fc = depth_colors[ci]
                lines.append(f'  "{safe}" [label="{label}", '
                             f'fillcolor="#1a1a2e", fontcolor="{fc}"];')

        lines.append("")
        for a, b in sorted(edges):
            lines.append(f'  "{a.replace(chr(34), chr(92)+chr(34))}" -> '
                         f'"{b.replace(chr(34), chr(92)+chr(34))}";')

        lines.append("}")
        return "\n".join(lines)

    def to_json(self) -> str:
        """Export full graph state as JSON."""
        nodes_data = {}
        for row in self._db.execute(
            "SELECT name, version, release, summary, license, url, crawled, depth "
            "FROM source_packages ORDER BY name"
        ).fetchall():
            nodes_data[row[0]] = {
                "version": row[1], "release": row[2], "summary": row[3],
                "license": row[4], "url": row[5],
                "crawled": bool(row[6]), "depth": row[7],
            }

        edges_list = sorted([list(e) for e in self.get_edges()])

        return json.dumps({
            "nodes": nodes_data,
            "edges": edges_list,
            "stats": self.db_stats(),
        }, indent=2)

    def summary(self) -> str:
        """Human-readable summary."""
        stats = self.db_stats()
        sccs = self.scc_groups()
        cycles = [scc for scc in sccs if len(scc) > 1]

        lines = []
        lines.append(f"╔══════════════════════════════════════════════════════╗")
        lines.append(f"║  SRPM Dependency Graph — SQLite-backed               ║")
        lines.append(f"╠══════════════════════════════════════════════════════╣")
        lines.append(f"║  Source packages:     {stats['source_packages']:<8} "
                     f"({stats['source_crawled']} crawled)       ║")
        lines.append(f"║  Binary packages:     {stats['binary_packages']:<30}║")
        lines.append(f"║  BuildRequires:       {stats['build_requires_total']:<8} "
                     f"({stats['build_requires_resolved']} resolved)     ║")
        lines.append(f"║  Unresolved:          {stats['build_requires_unresolved']:<30}║")
        lines.append(f"║  Edges:               {stats['edges']:<30}║")
        lines.append(f"║  Cycles (SCCs):       {len(cycles):<30}║")
        lines.append(f"║  DB size:             {stats['db_size_kb']} KB"
                     f"{' ' * (28 - len(str(stats['db_size_kb'])))}║")
        lines.append(f"╚══════════════════════════════════════════════════════╝")

        if cycles:
            lines.append("")
            lines.append("⚠ Circular dependencies:")
            for i, scc in enumerate(cycles, 1):
                lines.append(f"  Cycle {i}: {' ↔ '.join(scc)}")

        lines.append("")
        lines.append(f"Resolver stats: {stats['resolver']}")

        return "\n".join(lines)


# ═══════════════════════════════════════════════════════════════════
#  Standalone helpers (for CLI)
# ═══════════════════════════════════════════════════════════════════

def search_packages(query: str) -> List[Dict]:
    """Search Fedora repos using dnf repoquery."""
    rc, out, _ = _run([
        "dnf", "repoquery", f"*{query}*",
        "--queryformat", "%{name}|||%{version}|||%{summary}",
        "--latest-limit", "1", "--available"
    ])
    if rc != 0:
        return []

    results = []
    seen = set()
    for line in out.splitlines():
        parts = line.strip().split("|||")
        if len(parts) >= 3 and parts[0] not in seen:
            seen.add(parts[0])
            results.append({
                "name": parts[0], "version": parts[1], "summary": parts[2],
            })
    return sorted(results, key=lambda x: x["name"])


def get_package_info_from_dnf(package: str) -> Optional[Dict]:
    """Get detailed info for a single package from dnf."""
    fmt = "%{name}|||%{version}|||%{release}|||%{summary}|||%{license}|||%{url}|||%{sourcerpm}"
    rc, out, _ = _run([
        "dnf", "repoquery", package,
        "--queryformat", fmt, "--latest-limit", "1"
    ])
    if rc != 0 or not out.strip():
        return None

    line = out.strip().splitlines()[0]
    parts = line.split("|||")
    if len(parts) < 7:
        return None

    rc2, out2, _ = _run([
        "dnf", "repoquery", package, "--requires", "--latest-limit", "1"
    ])
    requires = [l.strip() for l in out2.splitlines() if l.strip()] if rc2 == 0 else []

    return {
        "name": parts[0], "version": parts[1], "release": parts[2],
        "summary": parts[3], "license": parts[4], "url": parts[5],
        "sourcerpm": parts[6], "requires": requires,
    }


# — BlackLatch: "The graph engine. SQLite-backed, recursive, relentless.
#   Every package gets crawled, every dep gets traced, every edge gets stored.
#   This is how you build an OS from source."
