# Coreutils Test Plan

This plan lists the tasks and test cases needed to validate every coreutils (Linux-compatible) function shipped in `userspace/coreutils/src/bin`. Each utility includes a minimal set of black-box tests covering happy-path, option/flag handling, and error cases. Where kernel support is not yet available, mark the test as expected-to-fail and track in the task list.

## Test Execution Notes
- Tests should run in a userspace integration harness that can launch each binary with argv/env, capture stdout/stderr, and assert exit codes.
- File-system tests should use a throwaway tmp directory to avoid polluting the root FS.
- Network tests require loopback or a mock TCP/IP stack; if networking is unavailable, mark as skipped with rationale.
- Timing-dependent tests (e.g., `sleep`, `timeout`) should use generous thresholds and monotonic time sources.

## Global Tasks
- [ ] Implement a userspace test harness to invoke binaries, capture output, and compare against golden expectations.
- [ ] Add helpers for temporary files/directories and deterministic fixture data.
- [ ] Provide a skip/xfail mechanism for utilities requiring missing kernel features (marked below).

## Per-Utility Test Tasks
- **archived/graphics/input/system-info**
  - [ ] `fbtest`: run once, verify non-zero bytes written to framebuffer device (requires fb driver; xfail if absent).
  - [ ] `fbperf`: run benchmark flag, confirm reports >=1 measurement line.
  - [ ] `loadkeys`: load sample keymap, verify success; error on missing file.
  - [ ] `uptime`: prints uptime, ensure exit 0 and contains “up”.
  - [ ] `df`: run, ensure header includes “Filesystem”.
  - [ ] `dmesg`: prints kernel log, ensure non-empty output or handled permission error.
  - [ ] `free`: shows memory summary, ensure columns “total” and “used”.
  - [ ] `id`: prints uid/gid, ensure numeric uid present.
  - [ ] `whoami`: prints username, match current user.
  - [ ] `hostname`: prints hostname, exit 0.
  - [ ] `uname`: `-a` shows kernel name; without args prints sysname.
  - [ ] `date`: prints date; `-u` outputs UTC; invalid format string errors.
  - [ ] `uptime`: duplicate entry—see above (ensure single implementation tested once).
  - [ ] `ps`: lists processes; ensure header includes PID.
  - [ ] `kill`: send SIGTERM to self-spawned sleeper, verify process exit; invalid PID errors.
  - [ ] `nice`: run command with adjusted nice level; verify unchanged when lacking permission.
  - [ ] `nohup`: run echo, ensure output redirected to nohup.out.
  - [ ] `timeout`: wrap `sleep 2` with `timeout 1`, expect exit 124.
  - [ ] `pgrep`: find known process by name; no match returns non-zero.
  - [ ] `pkill`: start helper proc, pkill by name, ensure terminated.
  - [ ] `reset`/`clear`: ensure terminal control sequences emitted; exit 0.
  - [ ] `uptime`: already covered; ensure deduplication in harness.

- **file operations**
  - [ ] `cat`: concat file, handles `-n`, `-b`, `-E`, `-T`, `-A`, stdin on “-”.
  - [ ] `cp`: copy file, `-r` directory, preserves mode with `-p`; error on missing src.
  - [ ] `mv`: move file, cross-directory, overwrite with prompt/force; missing src error.
  - [ ] `rm`: remove file, `-r` directory, `-f` suppresses errors; attempt on missing file.
  - [ ] `ln`: hard link, `-s` symlink, dangling symlink allowed; duplicate link error.
  - [ ] `touch`: create new file, update mtime, `-c` no-create, `-t` sets timestamp.
  - [ ] `mkdir`: create single and nested (`-p`) dirs; error when parent missing without `-p`.
  - [ ] `rmdir`: remove empty dir; non-empty dir should fail.
  - [ ] `ls`: basic listing, `-l`, `-a`, directory argument, symlink display.
  - [ ] `stat`: outputs mode/size, format options, error on missing path.
  - [ ] `chmod`: change mode numeric and symbolic; verify with `stat`.
  - [ ] `chown`: change owner/group (xfail until kernel supports); error handling.
  - [ ] `readlink`: resolve symlink; error on non-symlink.
  - [ ] `realpath`: canonical path resolution with relative and `..`.
  - [ ] `pwd`: prints cwd; after `chdir` matches expected.
  - [ ] `du`: directory summary, `-s`, `-h` formatting; follows/ignores symlinks appropriately.
  - [ ] `df`: already above; ensure deduped.
  - [ ] `file`: identify file type using magic; unknown types handled gracefully.

- **text processing**
  - [ ] `echo`: plain output, `-n`, escape handling.
  - [ ] `head`: default 10 lines, `-n 5`, file and stdin.
  - [ ] `tail`: last 10 lines, `-n +3` from offset, follow `-f` (timeout-limited).
  - [ ] `wc`: counts lines/words/bytes; multiple files with totals.
  - [ ] `grep`: literal and regex search, `-i`, `-v`, `-n`, file and stdin.
  - [ ] `sort`: basic sort, numeric `-n`, reverse `-r`, stability on equal keys.
  - [ ] `uniq`: collapse adjacent duplicates, `-c` counts, works with unsorted input unchanged.
  - [ ] `cut`: delimiter `-d`, fields `-f`, byte/char ranges; invalid field errors.
  - [ ] `tr`: translate chars, delete `-d`, squeeze `-s`; handles ranges.
  - [ ] `tee`: splits stdin to file and stdout; append `-a`; permission errors.
  - [ ] `yes`: repeated string; stop after limited reads to verify content.
  - [ ] `seq`: start/stop/step variants; formatting; negative steps.
  - [ ] `expr`: arithmetic, string length/match; divide-by-zero error.
  - [ ] `strings`: extract printable sequences; min length option.
  - [ ] `hexdump`/`od`: hex/oct dumps; `-C` canonical; short file handling.
  - [ ] `diff`: identical files exit 0; differing files exit 1; unified output formats.
  - [ ] `test`: integer/string/file operators; proper exit codes; symlink file tests.
  - [ ] `true`/`false`: verify exit codes 0/1.

- **archive/compression**
  - [ ] `tar`: create archive from dir, extract archive, list contents; handles symlinks.
  - [ ] `gzip`/`gunzip`: compress/decompress round-trip; invalid data error; xfail pending DEFLATE completeness.
  - [ ] `dd`: copy with block size, count, skip/seek; verifies bytes written.

- **path utilities**
  - [ ] `basename`: strips path, optional suffix removal.
  - [ ] `dirname`: returns parent directory path.
  - [ ] `which`: finds executable in PATH; missing binary returns non-zero.
  - [ ] `realpath`/`readlink`/`pwd`: see file ops (ensure single test coverage).

- **networking**
  - [ ] `ping`: send ICMP to 127.0.0.1, expect replies; timeout handling.
  - [ ] `wget`: fetch http://localhost fixture server; save file; 404 error case.
  - [ ] `ifconfig`/`ip`: list interfaces; set address on dummy iface if supported.
  - [ ] `netstat`: shows tcp/udp sockets; empty list allowed.
  - [ ] `nc`: TCP connect to local echo server; UDP mode; listen mode basic.
  - [ ] `nslookup`: query localhost DNS server; xfail if resolver missing.
  - [ ] `ping` repeated to ensure idempotence (dedupe in harness).

- **binary/debug**
  - [ ] `fbtest`/`fbperf`: see graphics section (dedupe).
  - [ ] `strings`: covered above.
  - [ ] `hexdump`/`od`: covered above.
  - [ ] `file`: covered above.

- **misc/system behavior**
  - [ ] `more`/`less`: paginate file; ensure quits after sending “q”; search `/pattern` works; skip if terminal interaction unsupported.
  - [ ] `env`: prints environment; supports KEY=VAL prefix and `-u` unset.
  - [ ] `timeout`: covered in process utilities.
  - [ ] `sleep`: sleeps approx duration; accept minor drift.
  - [ ] `uptime`: covered above.
  - [ ] `reset`/`clear`: covered above.
  - [ ] `fb*`: covered above.

## Coverage Tracking
- Maintain a simple checklist per utility in the harness (pass/fail/xfail).
- Record skips with reason (e.g., “network stack unavailable”).
- Store golden outputs for deterministic commands (cat, echo, seq) under `tests/fixtures/`.

## Deliverable
- A runnable integration test suite in `userspace/coreutils/tests/` implementing the above cases, plus CI job to execute them once kernel/userspace test harness is available.
