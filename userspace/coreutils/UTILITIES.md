# OXIDE Coreutils - Utilities Roadmap

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
| `cp` | Copy files | DONE |
| `mv` | Move/rename files | DONE |
| `touch` | Create file / update timestamp | DONE |
| `ln` | Create links | DONE |
| `rmdir` | Remove empty directories | DONE |
| `chmod` | Change file permissions | DONE |
| `chown` | Change file ownership | DONE (needs kernel support) |
| `stat` | Display file status | DONE |

## Priority 2: Text Processing

| Utility | Description | Status |
|---------|-------------|--------|
| `head` | Output first lines of file | DONE |
| `tail` | Output last lines of file | DONE |
| `wc` | Word/line/byte count | DONE |
| `grep` | Search for patterns | DONE |
| `sort` | Sort lines | DONE |
| `uniq` | Filter duplicate lines | DONE |
| `cut` | Remove sections from lines | DONE |
| `tr` | Translate characters | DONE |
| `tee` | Pipe to file and stdout | DONE |
| `sed` | Stream editor | DONE |
| `awk` | Pattern scanning and processing | DONE |

## Priority 2.5: Text Editors

| Utility | Description | Status |
|---------|-------------|--------|
| `vim` | Vi IMproved - modal text editor | DONE |
| `less` | Advanced file viewer | DONE |
| `more` | Simple file pager | DONE |

## Priority 3: System Utilities

| Utility | Description | Status |
|---------|-------------|--------|
| `date` | Print/set date and time | DONE |
| `sleep` | Pause for specified time | DONE |
| `id` | Print user/group IDs | DONE |
| `whoami` | Print current user | DONE |
| `hostname` | Print/set hostname | DONE |
| `uptime` | Show system uptime | DONE |
| `clear` | Clear terminal screen | DONE |
| `reset` | Reset terminal | DONE |

## Priority 3.5: User and Group Management

| Utility | Description | Status |
|---------|-------------|--------|
| `useradd` | Create new user account | DONE |
| `groupadd` | Create new group | DONE |
| `usermod` | Modify user account | TODO |
| `groupmod` | Modify group | TODO |
| `userdel` | Delete user account | TODO |
| `groupdel` | Delete group | TODO |

## Priority 4: Process Utilities

| Utility | Description | Status |
|---------|-------------|--------|
| `top` | Interactive process viewer | DONE (ncurses-based, all flags) |
| `pgrep` | Find processes by name | DONE (needs /proc) |
| `pkill` | Kill processes by name | DONE (needs /proc) |
| `nice` | Run with modified priority | DONE (needs kernel support) |
| `nohup` | Run immune to hangups | DONE (needs signal support) |
| `timeout` | Run with time limit | DONE (needs timer support) |

## Priority 5: Network Utilities

| Utility | Description | Status |
|---------|-------------|--------|
| `wget` | Download files from web | DONE (needs network stack) |
| `ping` | Send ICMP echo requests | DONE (needs ICMP support) |
| `ifconfig` | Configure network interfaces | DONE (needs network stack) |
| `netstat` | Network statistics | DONE (needs network stack) |
| `nc` | Netcat - network Swiss army knife | DONE (needs network stack) |

## Priority 6: Archive/Compression

| Utility | Description | Status |
|---------|-------------|--------|
| `tar` | Archive utility | DONE (needs full implementation) |
| `gzip` | Compress files | DONE (needs DEFLATE algorithm) |
| `gunzip` | Decompress files | DONE (needs INFLATE algorithm) |

## Priority 7: Advanced Utilities

| Utility | Description | Status |
|---------|-------------|--------|
| `find` | Search for files | DONE |
| `xargs` | Build command lines from stdin | DONE |
| `which` | Locate command | DONE |
| `seq` | Print sequence of numbers | DONE |
| `yes` | Output string repeatedly | DONE |
| `expr` | Evaluate expressions | DONE |
| `basename` | Strip directory from path | DONE |
| `dirname` | Strip filename from path | DONE |
| `readlink` | Print resolved symlink | DONE |
| `realpath` | Print resolved path | DONE |
| `dd` | Convert and copy files | DONE |
| `hexdump` | Display file in hex | DONE |
| `od` | Octal dump | DONE |
| `diff` | Compare files | DONE |

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
