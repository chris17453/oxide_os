# OXIDE Coreutils Analysis
**Date:** 2026-02-02
**Total Utilities:** 86
**Total Lines:** ~25,000

## Summary

| Status | Count | Percentage |
|--------|-------|------------|
| ✅ Complete | 73 | 85% |
| ⚠️ Partial | 13 | 15% |

---

## Utilities with Incomplete Features

### diff
**Issue:** Unified/context output formats not implemented
```
-u    Output in unified format (not yet implemented)
-c    Output in context format (not yet implemented)
```
**Fix:** Implement `-u` and `-c` output modes

### fbperf
**Issue:** Framebuffer ioctl placeholder
**Fix:** Low priority, testing utility only

### more
**Issue:** Search functionality not implemented
```
/pattern    Search for pattern (not yet implemented)
```
**Fix:** Implement regex search

### mount
**Issue:** Error code 38 message only
**Fix:** Minor, cosmetic

### nice
**Issue:** setpriority syscall not implemented in kernel
```
nice: setpriority syscall not yet implemented
```
**Fix:** Implement `setpriority` syscall in kernel

### nohup
**Issue:** Signal handling incomplete
**Fix:** Depends on signal syscall improvements

### nslookup
**Issue:** Some DNS response codes not handled
**Fix:** Low priority edge cases

### pgrep / pkill
**Issue:** /proc filesystem dependency
```
pgrep: /proc filesystem not yet fully implemented
```
**Fix:** Depends on procfs improvements

### tail
**Issue:** Following multiple files not fully implemented
**Fix:** Implement multi-file `-f` support

### timeout
**Issue:** Timer syscalls not implemented
```
timeout: timer syscalls not yet implemented
```
**Fix:** Implement `alarm()` or `timer_create()` in kernel

### wget
**Issue:** DNS hostname resolution not implemented
```
wget: hostname resolution not implemented, use IP address
```
**Fix:** Implement DNS resolution in libc or use nslookup

---

## Utilities by Category

### File Operations
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| cat | 275 | ✅ | Full implementation |
| cp | 448 | ✅ | Recursive, preserve mode |
| mv | 385 | ✅ | Cross-filesystem support |
| rm | 329 | ✅ | Recursive, force modes |
| ln | 324 | ✅ | Hard and soft links |
| mkdir | 255 | ✅ | -p parent creation |
| rmdir | 42 | ✅ | Basic |
| touch | 368 | ✅ | Create/update times |
| chmod | 441 | ✅ | Octal and symbolic modes |
| chown | 109 | ✅ | Basic |

### Text Processing
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| cat | 275 | ✅ | Number lines, show ends |
| head | 295 | ✅ | Lines and bytes |
| tail | 538 | ⚠️ | -f partial for multi-file |
| grep | 525 | ✅ | -i, -v, -n, -c, -l |
| sed | 584 | ✅ | s/p/d commands |
| awk | 589 | ✅ | Basic AWK |
| cut | 535 | ✅ | -f, -d, -c |
| sort | 439 | ✅ | -r, -n, -u |
| uniq | 424 | ✅ | -c, -d, -u |
| wc | 320 | ✅ | -l, -w, -c |
| tr | 493 | ✅ | Translate/delete |
| diff | 489 | ⚠️ | Normal diff only |

### Directory/Path
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| ls | 675 | ✅ | -l, -a, -h, -R, -F |
| pwd | 29 | ✅ | Basic |
| cd | N/A | N/A | Shell built-in |
| find | 268 | ✅ | -name, -type |
| which | 84 | ✅ | PATH search |
| realpath | 133 | ✅ | Resolve symlinks |
| readlink | 67 | ✅ | Read symlink target |
| dirname | 176 | ✅ | Extract directory |
| basename | 223 | ✅ | Extract filename |

### System Info
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| uname | 201 | ✅ | -a, -s, -n, -r, -m |
| hostname | 33 | ✅ | Get hostname |
| uptime | 91 | ✅ | System uptime |
| date | 119 | ✅ | Current date/time |
| id | 43 | ✅ | UID/GID |
| whoami | 145 | ✅ | Current user |
| df | 453 | ✅ | Disk usage |
| du | 445 | ✅ | Directory sizes |
| free | 141 | ✅ | Memory info |
| ps | 342 | ✅ | Process list |
| dmesg | 37 | ✅ | Kernel messages |

### Process Control
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| kill | 95 | ✅ | Send signals |
| pgrep | 47 | ⚠️ | Needs procfs |
| pkill | 119 | ⚠️ | Needs procfs |
| nice | 119 | ⚠️ | Needs setpriority |
| nohup | 60 | ⚠️ | Signal handling |
| timeout | 91 | ⚠️ | Needs timer syscalls |

### Networking
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| ping | 577 | ✅ | ICMP ping |
| ifconfig | 427 | ✅ | Interface config |
| ip | 469 | ✅ | IP routing |
| netstat | 25 | ✅ | Basic |
| nc | 438 | ✅ | Netcat |
| wget | 474 | ⚠️ | IP only, no DNS |
| nslookup | 941 | ⚠️ | DNS queries |
| fw | 1135 | ✅ | Firewall config |

### Filesystem
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| mount | 357 | ⚠️ | Basic mounting |
| umount | 208 | ✅ | Unmount |
| stat | 212 | ✅ | File stats |
| file | 218 | ✅ | File type detection |
| mkfs_ext4 | 943 | ✅ | Create ext4 |

### Archives/Compression
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| tar | 697 | ✅ | Create/extract |
| gzip | 349 | ✅ | Compress |
| gunzip | 298 | ✅ | Decompress |

### Shell Utilities
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| echo | 232 | ✅ | -n, -e escapes |
| env | 30 | ✅ | Show environment |
| expr | 510 | ✅ | Expression eval |
| test | 384 | ✅ | Conditionals |
| true | 14 | ✅ | Exit 0 |
| false | 14 | ✅ | Exit 1 |
| yes | 32 | ✅ | Repeat string |
| seq | 408 | ✅ | Number sequence |
| sleep | 65 | ✅ | Delay |
| xargs | 536 | ✅ | Build commands |
| tee | 213 | ✅ | T-pipe |

### Terminal
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| clear | 13 | ✅ | Clear screen |
| reset | 36 | ✅ | Reset terminal |
| less | 641 | ✅ | Pager |
| more | 386 | ⚠️ | Search missing |
| loadkeys | 109 | ✅ | Keyboard layout |

### Testing/Debug
| Utility | Lines | Status | Notes |
|---------|-------|--------|-------|
| vttest | 31 | ✅ | VT100 test |
| testcolors | 155 | ✅ | Color test |
| fbtest | 519 | ✅ | Framebuffer test |
| fbperf | 76 | ⚠️ | FB performance |

---

## Comparison with GNU Coreutils

### Present in OXIDE but not typical coreutils:
- `fw` (firewall) - OXIDE-specific
- `ifconfig`, `ip`, `ping`, `nc`, `wget`, `nslookup` - Usually in net-tools/iproute2
- `loadkeys` - Usually in kbd package
- `mkfs_ext4` - Usually in e2fsprogs
- `fbtest`, `fbperf`, `vttest`, `testcolors` - OXIDE-specific testing

### Missing from OXIDE (common coreutils):
- `chgrp` - Change group (chown can do this)
- `install` - Copy with attributes
- `mkfifo` - Create named pipes
- `mknod` - Create device nodes
- `split` - Split files
- `join` - Join files on field
- `paste` - Merge lines
- `expand/unexpand` - Tab conversion
- `fold` - Wrap lines
- `fmt` - Format text
- `pr` - Paginate
- `nl` - Number lines (cat -n does this)
- `tsort` - Topological sort
- `factor` - Prime factors
- `shuf` - Shuffle lines
- `shred` - Secure delete
- `truncate` - Shrink/extend file
- `tty` - Print terminal name
- `stty` - Terminal settings (termios covers this)
- `nproc` - CPU count
- `printenv` - Print environment (env does this)
- `users` - Logged in users
- `groups` - User groups
- `logname` - Login name

---

## Kernel Dependencies for Fixes

### Required for nice/timeout/pgrep/pkill:
1. `setpriority()` syscall
2. `alarm()` or `timer_create()` syscalls
3. Enhanced `/proc` filesystem with:
   - `/proc/[pid]/cmdline`
   - `/proc/[pid]/status`

### Required for wget DNS:
1. DNS resolution in libc (`gethostbyname` or `getaddrinfo`)
2. Or use existing `nslookup` + parse output

---

## Recommended Fixes (Priority Order)

1. **High Priority:**
   - Implement `setpriority` syscall → fixes `nice`
   - Implement `alarm` syscall → fixes `timeout`
   - Add `/proc/[pid]/cmdline` → fixes `pgrep`, `pkill`

2. **Medium Priority:**
   - Add unified diff format to `diff`
   - Add search to `more`
   - Add DNS to `wget` (or document IP-only usage)

3. **Low Priority:**
   - Multi-file `tail -f`
   - `nohup` signal handling
   - `fbperf` ioctl
