# User and Group Management Commands - Test Plan

## Overview
This document describes how to test the newly implemented `useradd` and `groupadd` commands in OXIDE OS.

## Prerequisites
- Build and run OXIDE OS in QEMU: `make run`
- Log in as root (default password: "root")

## Test Cases

### Test 1: Basic User Creation
```bash
# Create a simple user with default settings
useradd alice

# Verify user was added to /etc/passwd
cat /etc/passwd | grep alice

# Expected output: alice:x:1000:1000::/home/alice:/bin/esh
```

### Test 2: User Creation with Options
```bash
# Create user with specific UID, GID, home, and shell
useradd -u 1001 -g 1001 -d /home/bob -s /bin/esh -c "Bob Smith" bob

# Verify user details
cat /etc/passwd | grep bob

# Expected output: bob:x:1001:1001:Bob Smith:/home/bob:/bin/esh
```

### Test 3: User Creation with Home Directory
```bash
# Create user and home directory
useradd -m charlie

# Verify user was added
cat /etc/passwd | grep charlie

# Verify home directory was created
ls -ld /home/charlie

# Expected: directory exists with proper ownership
```

### Test 4: Duplicate User Prevention
```bash
# Try creating an existing user
useradd alice

# Expected output: useradd: user 'alice' already exists
# Expected exit code: 1
```

### Test 5: Permission Check
```bash
# Try running as non-root (if multi-user session available)
# This would require logging in as a different user first
# Expected: useradd: permission denied (must be root)
```

### Test 6: Basic Group Creation
```bash
# Create a simple group with default settings
groupadd developers

# Verify group was added to /etc/group
cat /etc/group | grep developers

# Expected output: developers:x:1000:
```

### Test 7: Group Creation with Specific GID
```bash
# Create group with specific GID
groupadd -g 2000 admins

# Verify group details
cat /etc/group | grep admins

# Expected output: admins:x:2000:
```

### Test 8: System Group Creation
```bash
# Create system group (GID < 1000)
groupadd -r service

# Verify group was added
cat /etc/group | grep service

# Expected: GID should be less than 1000
```

### Test 9: Duplicate Group Prevention
```bash
# Try creating an existing group
groupadd developers

# Expected output: groupadd: group 'developers' already exists
# Expected exit code: 1
```

### Test 10: Help and Usage
```bash
# Display help for useradd
useradd -h

# Display help for groupadd
groupadd -h

# Expected: usage information displayed
```

### Test 11: Integration Test - Complete User Setup
```bash
# Create a group
groupadd -g 5000 engineering

# Create a user in that group with home directory
useradd -u 5001 -g 5000 -m -c "Dave Engineer" -s /bin/esh dave

# Verify both
cat /etc/group | grep engineering
cat /etc/passwd | grep dave
ls -ld /home/dave

# Verify directory ownership
id dave  # Should show uid=5001 gid=5000
```

## Expected File Formats

### /etc/passwd Format
```
username:password:uid:gid:gecos:home:shell
```

Example:
```
root:root:0:0:root:/root:/bin/esh
alice:x:1000:1000::/home/alice:/bin/esh
bob:x:1001:1001:Bob Smith:/home/bob:/bin/esh
```

### /etc/group Format
```
groupname:password:gid:members
```

Example:
```
root:x:0:
developers:x:1000:
admins:x:2000:
engineering:x:5000:
```

## Known Limitations
1. Password field is set to 'x' (no actual password setting implemented)
2. Group membership list is empty (users not automatically added to groups)
3. No validation of shell or home directory paths
4. No /etc/shadow support (passwords stored in /etc/passwd)

## Troubleshooting

### Command Not Found
If `useradd` or `groupadd` are not found, ensure:
1. The system was built with `make build-full` or `make userspace`
2. The binaries are in the initramfs
3. Check with: `ls -l /bin/useradd /bin/groupadd`

### Permission Denied
- Commands require root privileges (UID 0)
- Use `id` to check current user
- Log in as root if necessary

### File Creation Issues
If /etc/passwd or /etc/group cannot be written:
- Check file permissions: `ls -l /etc/passwd /etc/group`
- Ensure /etc directory exists and is writable by root
- Check disk space (unlikely but possible)

## Manual Testing Session Example

```bash
# Boot system
make run

# Log in as root
login: root
password: root

# Run tests
useradd -h                           # Test 10
groupadd developers                   # Test 6
useradd -m alice                     # Test 3
cat /etc/passwd                      # Verify
cat /etc/group                       # Verify
ls -ld /home/alice                   # Check home dir
useradd alice                        # Test 4 (should fail)
echo $?                              # Should be 1
```

## Success Criteria
- All test cases pass as expected
- Files are properly formatted and parseable
- Error messages are clear and helpful
- Commands follow UNIX conventions
- No memory leaks or crashes
- Proper cleanup on errors
