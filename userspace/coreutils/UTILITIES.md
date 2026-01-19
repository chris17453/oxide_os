# EFFLUX Coreutils - Utilities Roadmap

## Currently Implemented

| Utility | Description |
|---------|-------------|
| `cat` | Concatenate and print files |
| `echo` | Print arguments |
| `env` | Print environment variables |
| `false` | Return false (exit 1) |
| `kill` | Send signal to process |
| `ls` | List directory contents |
| `mkdir` | Create directories |
| `ps` | List processes |
| `rm` | Remove files |
| `true` | Return true (exit 0) |
| `uname` | Print system information |

## Priority 1: Essential File Operations

| Utility | Description | Status |
|---------|-------------|--------|
| `cp` | Copy files | TODO |
| `mv` | Move/rename files | TODO |
| `touch` | Create file / update timestamp | TODO |
| `ln` | Create links | TODO |
| `rmdir` | Remove empty directories | TODO |
| `chmod` | Change file permissions | TODO |
| `chown` | Change file ownership | TODO |
| `stat` | Display file status | TODO |

## Priority 2: Text Processing

| Utility | Description | Status |
|---------|-------------|--------|
| `head` | Output first lines of file | TODO |
| `tail` | Output last lines of file | TODO |
| `wc` | Word/line/byte count | TODO |
| `grep` | Search for patterns | TODO |
| `sort` | Sort lines | TODO |
| `uniq` | Filter duplicate lines | TODO |
| `cut` | Remove sections from lines | TODO |
| `tr` | Translate characters | TODO |
| `tee` | Pipe to file and stdout | TODO |

## Priority 3: System Utilities

| Utility | Description | Status |
|---------|-------------|--------|
| `date` | Print/set date and time | TODO |
| `sleep` | Pause for specified time | TODO |
| `id` | Print user/group IDs | TODO |
| `whoami` | Print current user | TODO |
| `hostname` | Print/set hostname | TODO |
| `uptime` | Show system uptime | TODO |
| `clear` | Clear terminal screen | TODO |
| `reset` | Reset terminal | TODO |

## Priority 4: Process Utilities

| Utility | Description | Status |
|---------|-------------|--------|
| `pgrep` | Find processes by name | TODO |
| `pkill` | Kill processes by name | TODO |
| `nice` | Run with modified priority | TODO |
| `nohup` | Run immune to hangups | TODO |
| `timeout` | Run with time limit | TODO |

## Priority 5: Network Utilities

| Utility | Description | Status |
|---------|-------------|--------|
| `wget` | Download files from web | TODO |
| `ping` | Send ICMP echo requests | TODO |
| `ifconfig` | Configure network interfaces | TODO |
| `netstat` | Network statistics | TODO |
| `nc` | Netcat - network Swiss army knife | TODO |

## Priority 6: Archive/Compression

| Utility | Description | Status |
|---------|-------------|--------|
| `tar` | Archive utility | TODO |
| `gzip` | Compress files | TODO |
| `gunzip` | Decompress files | TODO |

## Priority 7: Advanced Utilities

| Utility | Description | Status |
|---------|-------------|--------|
| `find` | Search for files | TODO |
| `xargs` | Build command lines from stdin | TODO |
| `which` | Locate command | TODO |
| `seq` | Print sequence of numbers | TODO |
| `yes` | Output string repeatedly | TODO |
| `expr` | Evaluate expressions | TODO |
| `basename` | Strip directory from path | TODO |
| `dirname` | Strip filename from path | TODO |
| `readlink` | Print resolved symlink | TODO |
| `realpath` | Print resolved path | TODO |
| `dd` | Convert and copy files | TODO |
| `hexdump` | Display file in hex | TODO |
| `od` | Octal dump | TODO |
| `diff` | Compare files | TODO |

## Implementation Notes

### Dependencies

Some utilities require kernel support:
- Network utilities need TCP/IP stack
- `ping` needs raw sockets / ICMP
- `chmod`/`chown` need permission syscalls
- `date` needs RTC / time syscalls

### Syscalls Needed

| Syscall | Used By |
|---------|---------|
| `SYS_LINK` | ln |
| `SYS_SYMLINK` | ln -s |
| `SYS_READLINK` | readlink |
| `SYS_CHMOD` | chmod |
| `SYS_CHOWN` | chown |
| `SYS_STAT` | stat, ls -l |
| `SYS_RENAME` | mv |
| `SYS_UTIME` | touch |
| `SYS_NANOSLEEP` | sleep |
| `SYS_GETTIMEOFDAY` | date |
| `SYS_SOCKET` | network utilities |
